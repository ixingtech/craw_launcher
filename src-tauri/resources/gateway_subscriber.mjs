import fs from "node:fs";
import { pathToFileURL } from "node:url";

function emit(event) {
  process.stdout.write(`${JSON.stringify(event)}\n`);
}

function readPayload(payloadPath) {
  const text = fs.readFileSync(payloadPath, "utf8").replace(/^\uFEFF/, "");
  return JSON.parse(text);
}

function extractAssistantText(message) {
  if (!message || message.role !== "assistant") {
    return "";
  }
  if (typeof message.content === "string" && message.content.trim()) {
    return message.content.trim();
  }
  if (!Array.isArray(message.content)) {
    return "";
  }
  const parts = [];
  for (const block of message.content) {
    if (block?.type === "text" && typeof block.text === "string" && block.text.trim()) {
      parts.push(block.text.trim());
    }
  }
  return parts.join("\n\n");
}

function collectSessionKeys(value, output = new Set(), depth = 0) {
  if (depth > 5 || value == null) {
    return output;
  }
  if (typeof value === "string") {
    if (value.startsWith("agent:")) {
      output.add(value);
    }
    return output;
  }
  if (Array.isArray(value)) {
    for (const item of value) {
      collectSessionKeys(item, output, depth + 1);
    }
    return output;
  }
  if (typeof value === "object") {
    for (const item of Object.values(value)) {
      collectSessionKeys(item, output, depth + 1);
    }
  }
  return output;
}

function collectHealthSessionKeys(payload, output = new Set()) {
  const recentGroups = [];
  if (Array.isArray(payload?.agents)) {
    for (const agent of payload.agents) {
      if (Array.isArray(agent?.sessions?.recent)) {
        recentGroups.push(agent.sessions.recent);
      }
    }
  }
  if (Array.isArray(payload?.sessions?.recent)) {
    recentGroups.push(payload.sessions.recent);
  }
  for (const group of recentGroups) {
    for (const item of group) {
      if (typeof item?.key === "string" && item.key.startsWith("agent:")) {
        output.add(item.key);
      }
    }
  }
  return output;
}

async function main() {
  const payloadPath = process.argv[2];
  if (!payloadPath) {
    throw new Error("missing payload path");
  }

  const payload = readPayload(payloadPath);
  const wsmod = await import(pathToFileURL(payload.wsModulePath).href);
  const WebSocket = wsmod.default;

  let requestId = 0;
  let ws = null;
  let manualClose = false;
  const seen = new Map();
  const recentSessionKeys = new Set();

  function nextId(prefix) {
    requestId += 1;
    return `${prefix}-${requestId}`;
  }

  function send(method, params) {
    const id = nextId("req");
    ws.send(JSON.stringify({ type: "req", id, method, params }));
    return id;
  }

  function requestHistory(sessionKey, pending) {
    if (!sessionKey || typeof sessionKey !== "string") {
      return;
    }
    recentSessionKeys.add(sessionKey);
    const id = nextId("history");
    pending.set(id, { type: "history", sessionKey });
    ws.send(
      JSON.stringify({
        type: "req",
        id,
        method: "chat.history",
        params: { sessionKey, limit: 6 },
      }),
    );
  }

  function enqueueEventHistory(frame, pending) {
    const sessionKeys = collectSessionKeys(frame.payload);
    for (const sessionKey of sessionKeys) {
      requestHistory(sessionKey, pending);
    }

    const agentIds = new Set();
    if (typeof frame.payload?.agentId === "string" && frame.payload.agentId.trim()) {
      agentIds.add(frame.payload.agentId.trim());
    }
    for (const agentId of agentIds) {
      requestHistory(`agent:${agentId}:main`, pending);
      for (const sessionKey of recentSessionKeys) {
        if (sessionKey.startsWith(`agent:${agentId}:cron:`)) {
          requestHistory(sessionKey, pending);
        }
      }
    }
  }

  function connect() {
    ws = new WebSocket(payload.gatewayUrl);
    const pending = new Map();
    let connectSent = false;

    ws.on("open", () => {
      emit({ type: "status", status: "connecting" });
    });

    ws.on("message", (raw) => {
      let frame;
      try {
        frame = JSON.parse(String(raw));
      } catch {
        return;
      }

      if (frame.type === "event" && frame.event === "connect.challenge" && !connectSent) {
        connectSent = true;
        const id = nextId("connect");
        pending.set(id, { type: "connect" });
        ws.send(
          JSON.stringify({
            type: "req",
            id,
            method: "connect",
            params: {
              minProtocol: 3,
              maxProtocol: 3,
              client: {
                id: "cli",
                mode: "cli",
                version: payload.clientVersion || "0.1.1",
                platform: process.platform,
              },
              caps: [],
              role: "operator",
              scopes: ["operator.admin"],
              auth: payload.gatewayToken
                ? { token: payload.gatewayToken }
                : payload.gatewayPassword
                  ? { password: payload.gatewayPassword }
                  : undefined,
            },
          }),
        );
        return;
      }

      if (frame.type === "event" && frame.event === "health") {
        collectHealthSessionKeys(frame.payload, recentSessionKeys);
      }

      if (frame.type === "event" && frame.event === "chat") {
        const sessionKey = frame.payload?.sessionKey;
        if (!sessionKey || frame.payload?.state !== "final") {
          return;
        }
        requestHistory(sessionKey, pending);
        return;
      }

      if (frame.type === "event" && (frame.event === "cron" || frame.event === "heartbeat" || frame.event === "agent")) {
        enqueueEventHistory(frame, pending);
        return;
      }

      if (frame.type !== "res") {
        return;
      }

      const pendingRequest = pending.get(frame.id);
      if (!pendingRequest) {
        return;
      }
      pending.delete(frame.id);

      if (!frame.ok) {
        emit({ type: "error", error: frame.error?.message || "gateway request failed" });
        return;
      }

      if (pendingRequest.type === "connect") {
        collectHealthSessionKeys(frame.payload?.snapshot?.health, recentSessionKeys);
        emit({ type: "status", status: "connected" });
        return;
      }

      if (pendingRequest.type === "history") {
        const messages = Array.isArray(frame.payload?.messages) ? frame.payload.messages : [];
        const latestAssistant = [...messages].reverse().find((message) => message?.role === "assistant");
        const text = extractAssistantText(latestAssistant);
        const timestamp = latestAssistant?.timestamp ? new Date(latestAssistant.timestamp).toISOString() : new Date().toISOString();
        if (!text) {
          return;
        }
        const signature = `${timestamp}|${text}`;
        if (seen.get(pendingRequest.sessionKey) === signature) {
          return;
        }
        seen.set(pendingRequest.sessionKey, signature);
        emit({
          type: "message",
          sessionKey: pendingRequest.sessionKey,
          text,
          timestamp,
        });
      }
    });

    ws.on("close", () => {
      emit({ type: "status", status: "disconnected" });
      if (!manualClose) {
        setTimeout(connect, 1000);
      }
    });

    ws.on("error", (error) => {
      emit({ type: "error", error: String(error) });
    });
  }

  connect();

  process.on("SIGTERM", () => {
    manualClose = true;
    ws?.close();
    process.exit(0);
  });
  process.on("SIGINT", () => {
    manualClose = true;
    ws?.close();
    process.exit(0);
  });
}

main().catch((error) => {
  emit({ type: "error", error: String(error) });
  process.exit(1);
});
