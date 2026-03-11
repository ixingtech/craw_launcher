import fs from "node:fs";
import { spawn } from "node:child_process";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { Writable, Readable } from "node:stream";

function emit(event) {
  process.stdout.write(`${JSON.stringify(event)}\n`);
}

function fail(message) {
  emit({ type: "error", error: String(message || "unknown error") });
  process.exit(1);
}

function windowsShell(executablePath) {
  return process.platform === "win32" && /\.(cmd|bat)$/i.test(executablePath);
}

function buildServerArgs(payload) {
  const args = [];
  if (payload.profileName) {
    args.push("--profile", payload.profileName);
  }
  args.push("acp");
  if (payload.sessionKey) {
    args.push("--session", payload.sessionKey);
  }
  return args;
}

async function main() {
  const payloadPath = process.argv[2];
  if (!payloadPath) {
    fail("missing payload path");
  }

  const payloadText = fs.readFileSync(payloadPath, "utf8").replace(/^\uFEFF/, "");
  const payload = JSON.parse(payloadText);
  const sdkPath = payload.sdkPath;
  if (!sdkPath || !fs.existsSync(sdkPath)) {
    fail("ACP SDK not found");
  }

  const acp = await import(pathToFileURL(sdkPath).href);
  const serverArgs = buildServerArgs(payload);
  const env = {
    ...process.env,
    ...(payload.gatewayUrl ? { OPENCLAW_GATEWAY_URL: payload.gatewayUrl } : {}),
    ...(payload.gatewayToken ? { OPENCLAW_GATEWAY_TOKEN: payload.gatewayToken } : {}),
    ...(payload.gatewayPassword ? { OPENCLAW_GATEWAY_PASSWORD: payload.gatewayPassword } : {}),
  };

  const server = spawn(payload.openclawPath, serverArgs, {
    stdio: ["pipe", "pipe", "pipe"],
    shell: windowsShell(payload.openclawPath),
    windowsHide: true,
    env,
    cwd: payload.cwd || process.cwd(),
  });

  let stderrText = "";
  server.stderr?.setEncoding("utf8");
  server.stderr?.on("data", (chunk) => {
    stderrText += chunk;
  });

  const stream = acp.ndJsonStream(Writable.toWeb(server.stdin), Readable.toWeb(server.stdout));
  let sessionId = null;
  let finished = false;

  const client = {
    async sessionUpdate(params) {
      const update = params.update;
      switch (update.sessionUpdate) {
        case "agent_message_chunk":
          if (update.content?.type === "text" && update.content.text) {
            emit({ type: "delta", content: update.content.text });
          }
          break;
        case "tool_call":
          emit({ type: "tool", title: update.title, status: update.status });
          break;
        case "tool_call_update":
          emit({ type: "tool-update", toolCallId: update.toolCallId, status: update.status });
          break;
        default:
          break;
      }
    },
    async requestPermission(params) {
      const allowOption = params.options.find((option) => option.kind === "allow");
      const rejectOption = params.options.find((option) => option.kind === "reject");
      const selected = allowOption ?? rejectOption ?? params.options[0];
      if (!selected) {
        return { outcome: { outcome: "cancelled" } };
      }
      return {
        outcome: {
          outcome: "selected",
          optionId: selected.optionId,
        },
      };
    },
    async writeTextFile() {
      return {};
    },
    async readTextFile() {
      return { content: "" };
    },
  };

  const connection = new acp.ClientSideConnection(() => client, stream);
  const timeout = setTimeout(() => {
    fail("ACP 流式连接超时，没有收到可用回复。");
  }, 12000);

  try {
    await connection.initialize({
      protocolVersion: acp.PROTOCOL_VERSION,
      clientCapabilities: {
        fs: {
          readTextFile: true,
          writeTextFile: true,
        },
      },
      clientInfo: {
        name: "openclaw-launcher-stream",
        version: "0.1.1",
      },
    });

    const sessionResult = await connection.newSession({
      cwd: payload.cwd || process.cwd(),
      mcpServers: [],
    });
    sessionId = sessionResult.sessionId;

    const promptResult = await connection.prompt({
      sessionId,
      prompt: [
        {
          type: "text",
          text: payload.message,
        },
      ],
    });

    finished = true;
    clearTimeout(timeout);
    emit({ type: "done", stopReason: promptResult.stopReason });
  } catch (error) {
    clearTimeout(timeout);
    fail(stderrText.trim() ? `${String(error)}\n${stderrText.trim()}` : String(error));
  } finally {
    try {
      if (!finished && sessionId && connection.cancel) {
        await connection.cancel({ sessionId });
      }
    } catch {}
    try {
      server.kill();
    } catch {}
  }
}

main().catch((error) => fail(error));
