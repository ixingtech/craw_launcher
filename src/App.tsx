import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import ReactMarkdown from "react-markdown";
import { api } from "./lib/api";
import { useAppStore, type PageId } from "./lib/store";
import type {
  AppSettings,
  ChatDeltaEvent,
  ChatDoneEvent,
  ChatConversationEvent,
  ChatErrorEvent,
  ChatMessage,
  Conversation,
  ConversationSummary,
  ExportProfileRequest,
  GatewayStatus,
  NotificationItem,
  PathCandidate,
  ProfileItemPreview,
  ProfileListItem
} from "./lib/types";

const LOCAL_PROFILE_ID = "__local__";
const DEFAULT_OPENCLAW_PATH = "C:\\Users\\你的用户名\\.openclaw";
const DEFAULT_EXECUTABLE_PATH = "C:\\Users\\你的用户名\\AppData\\Roaming\\npm\\openclaw.cmd";

type StatusTone = "success" | "warning" | "error";

type ExportState = {
  open: boolean;
  sourceDir: string;
  packageName: string;
  exportDir: string;
  includeMemory: boolean;
  includeAccountInfo: boolean;
};

type PreviewState = {
  open: boolean;
  loading: boolean;
  title: string;
  subtitle: string;
  content: string;
  updatedAt?: string | null;
};

type InventorySection = "skills" | "cronJobs" | "memories" | "accounts";

const defaultSettings: AppSettings = {
  openclawExecutablePath: "",
  openclawDataDir: "",
  profilesRoot: "",
  gatewayConfig: {
    mode: "manual",
    command: "",
    args: [],
    url: "http://127.0.0.1:18789",
    healthEndpoint: "/health"
  },
  recentProfileId: "",
  recentLaunches: []
};

export default function App() {
  const queryClient = useQueryClient();
  const store = useAppStore();
  const needsProfiles = store.page === "profiles" || store.page === "chat" || store.page === "notifications" || store.page === "docs";
  const [settingsDraft, setSettingsDraft] = useState<AppSettings>(defaultSettings);
  const [chatDraft, setChatDraft] = useState("");
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [lobsterSearch, setLobsterSearch] = useState("");
  const [status, setStatus] = useState<{ message: string; tone: StatusTone } | null>(null);
  const [docDraft, setDocDraft] = useState("");
  const [docEditing, setDocEditing] = useState(false);
  const [previewState, setPreviewState] = useState<PreviewState>({
    open: false,
    loading: false,
    title: "",
    subtitle: "",
    content: "",
    updatedAt: null
  });
  const [exportState, setExportState] = useState<ExportState>({
    open: false,
    sourceDir: "",
    packageName: "我的龙虾",
    exportDir: "",
    includeMemory: false,
    includeAccountInfo: false
  });
  const shouldPollGateway =
    store.page === "chat" ||
    store.page === "notifications" ||
    store.page === "profiles" ||
    store.page === "docs";

  const settingsQuery = useQuery({
    queryKey: ["settings"],
    queryFn: api.loadSettings,
    staleTime: 30000,
    refetchOnWindowFocus: false
  });
  const desktopPathQuery = useQuery({
    queryKey: ["desktop-path"],
    queryFn: api.desktopPath,
    enabled: exportState.open
  });
  const detectQuery = useQuery({
    queryKey: ["detect-openclaw"],
    queryFn: api.detectOpenclaw,
    enabled: false,
    staleTime: 30000
  });
  const profilesQuery = useQuery({
    queryKey: ["profiles"],
    queryFn: api.listProfiles,
    enabled: needsProfiles,
    staleTime: 30000
  });
  const conversationSummariesQuery = useQuery({
    queryKey: ["conversation-summaries"],
    queryFn: api.listConversationSummaries,
    enabled: store.page === "chat",
    staleTime: 30000
  });
  const gatewayQuery = useQuery({
    queryKey: ["gateway-status"],
    queryFn: api.gatewayStatus,
    enabled: shouldPollGateway,
    refetchInterval: shouldPollGateway ? 5000 : false,
    refetchOnWindowFocus: false,
    staleTime: 15000
  });
  const inventoryQuery = useQuery({
    queryKey: ["profile-inventory", store.viewingProfileId || LOCAL_PROFILE_ID],
    queryFn: () => api.inspectProfile(store.viewingProfileId || LOCAL_PROFILE_ID),
    enabled: store.page === "profiles"
  });
  const readmeQuery = useQuery({
    queryKey: ["profile-readme", store.viewingProfileId || store.activeProfileId || settingsQuery.data?.recentProfileId || LOCAL_PROFILE_ID],
    queryFn: () =>
      api.readProfileReadme(store.viewingProfileId || store.activeProfileId || settingsQuery.data?.recentProfileId || LOCAL_PROFILE_ID),
    enabled: store.page === "docs"
  });

  useEffect(() => {
    if (!docEditing) {
      setDocDraft(readmeQuery.data?.content || "");
    }
  }, [docEditing, readmeQuery.data?.content]);

  useEffect(() => {
    if (settingsQuery.data) setSettingsDraft(settingsQuery.data);
  }, [settingsQuery.data]);

  useEffect(() => {
    if (!store.activeProfileId) store.setActiveProfileId(settingsQuery.data?.recentProfileId || LOCAL_PROFILE_ID);
    if (!store.viewingProfileId) store.setViewingProfileId(settingsQuery.data?.recentProfileId || LOCAL_PROFILE_ID);
  }, [settingsQuery.data?.recentProfileId, store]);

  useEffect(() => {
    if (desktopPathQuery.data && exportState.open && !exportState.exportDir) {
      setExportState((current) => ({ ...current, exportDir: desktopPathQuery.data || current.exportDir }));
    }
  }, [desktopPathQuery.data, exportState.exportDir, exportState.open]);

  useEffect(() => {
    let offA = () => {};
    let offB = () => {};
    let offC = () => {};
    let offD = () => {};
    void listen<ChatDeltaEvent>("chat://delta", (event) => {
      store.appendStreamingChunk(event.payload.conversationId, event.payload.content);
    }).then((fn) => (offA = fn));
    void listen<ChatDoneEvent>("chat://done", async (event) => {
      store.clearStreamingConversation(event.payload.conversationId);
      queryClient.setQueryData(["conversation", event.payload.conversationId], event.payload.conversation);
      queryClient.setQueryData<ConversationSummary[]>(["conversation-summaries"], (current = []) =>
        mergeConversationSummary(current, event.payload.conversation)
      );
      await queryClient.invalidateQueries({ queryKey: ["conversation-summaries"] });
      await queryClient.invalidateQueries({ queryKey: ["notifications"] });
    }).then((fn) => (offB = fn));
    void listen<ChatConversationEvent>("chat://conversation", async (event) => {
      queryClient.setQueryData(["conversation", event.payload.conversationId], event.payload.conversation);
      queryClient.setQueryData<ConversationSummary[]>(["conversation-summaries"], (current = []) =>
        mergeConversationSummary(current, event.payload.conversation)
      );
      await queryClient.invalidateQueries({ queryKey: ["conversation-summaries"] });
      await queryClient.invalidateQueries({ queryKey: ["notifications"] });
    }).then((fn) => (offD = fn));
    void listen<ChatErrorEvent>("chat://error", (event) => {
      setStatus({ message: event.payload.error, tone: "error" });
      store.clearStreamingConversation(event.payload.conversationId);
    }).then((fn) => (offC = fn));
    return () => {
      offA();
      offB();
      offC();
      offD();
    };
  }, [queryClient, store]);

  const profiles = profilesQuery.data ?? [];
  useEffect(() => {
    if (!profilesQuery.isFetched) {
      return;
    }
    if (store.activeProfileId && store.activeProfileId !== LOCAL_PROFILE_ID && !profiles.some((profile) => profile.id === store.activeProfileId)) {
      store.setActiveProfileId(LOCAL_PROFILE_ID);
    }
    if (store.viewingProfileId && store.viewingProfileId !== LOCAL_PROFILE_ID && !profiles.some((profile) => profile.id === store.viewingProfileId)) {
      store.setViewingProfileId(LOCAL_PROFILE_ID);
    }
  }, [profiles, profilesQuery.isFetched, store, store.activeProfileId, store.viewingProfileId]);
  const filteredProfiles = useMemo(() => {
    const keyword = lobsterSearch.trim().toLowerCase();
    if (!keyword) return profiles;
    return profiles.filter((profile) => `${profile.name} ${profile.path}`.toLowerCase().includes(keyword));
  }, [lobsterSearch, profiles]);

  const activeLobsterId = store.activeProfileId || settingsQuery.data?.recentProfileId || LOCAL_PROFILE_ID;
  const viewingLobsterId = store.viewingProfileId || activeLobsterId;
  const activeLaunchRecord = (settingsQuery.data?.recentLaunches ?? []).find((launch) => launch.profileId === activeLobsterId);
  const activeLobster = profiles.find((profile) => profile.id === activeLobsterId);
  const viewingLobster = profiles.find((profile) => profile.id === viewingLobsterId);
  const activeLobsterName = activeLobster?.name ?? activeLaunchRecord?.profileName ?? "默认本地龙虾";
  const notificationsQuery = useQuery({
    queryKey: ["notifications", activeLobsterId],
    queryFn: () => api.listNotifications(activeLobsterId),
    enabled: store.page === "notifications",
    refetchOnWindowFocus: false,
    staleTime: 15000
  });
  const gatewayOnline = Boolean(gatewayQuery.data?.running && gatewayQuery.data?.healthy);
  const isProfileRunning = (profileId: string) => gatewayOnline && activeLobsterId === profileId;
  const activeLobsterRunning = isProfileRunning(activeLobsterId);
  const viewingLobsterRunning = isProfileRunning(viewingLobsterId);
  const conversations = useMemo(
    () => (conversationSummariesQuery.data ?? []).filter((conversation) => conversationBelongsToProfile(conversation.id, activeLobsterId)),
    [conversationSummariesQuery.data, activeLobsterId]
  );
  const chatConversations = useMemo(
    () => conversations.filter((conversation) => !conversation.id.includes("--conv--push-")),
    [conversations]
  );
  const sortedChatConversations = useMemo(
    () => [...chatConversations].sort((left, right) => right.updatedAt.localeCompare(left.updatedAt) || right.id.localeCompare(left.id)),
    [chatConversations]
  );
  const selectedConversationSummary = sortedChatConversations.find((conversation) => conversation.id === store.selectedConversationId);
  const selectedConversationQuery = useQuery({
    queryKey: ["conversation", store.selectedConversationId],
    queryFn: () => api.getConversation(store.selectedConversationId!),
    enabled: store.page === "chat" && !!store.selectedConversationId,
    staleTime: 30000
  });
  const selectedConversation = selectedConversationQuery.data;
  const sortedSelectedMessages = useMemo(
    () => selectedConversation
      ? [...selectedConversation.messages].sort((left, right) => left.createdAt.localeCompare(right.createdAt) || left.id.localeCompare(right.id))
      : [],
    [selectedConversation]
  );
  useEffect(() => {
    if (chatConversations.length === 0) {
      if (store.selectedConversationId) {
        store.setSelectedConversationId(undefined);
      }
      return;
    }
    if (selectedConversationSummary) {
      return;
    }
    const fallbackConversation = sortedChatConversations[0];
    if (fallbackConversation && store.selectedConversationId !== fallbackConversation.id) {
      store.setSelectedConversationId(fallbackConversation.id);
    }
  }, [chatConversations, selectedConversationSummary, sortedChatConversations, store]);
  const waitingForReply = store.selectedConversationId ? !!store.waitingConversations[store.selectedConversationId] : false;
  const needsSetup = !settingsDraft.openclawExecutablePath || !settingsDraft.openclawDataDir;
  const recentLaunch = settingsQuery.data?.recentLaunches?.[0];

  const saveSettingsMutation = useMutation({
    mutationFn: api.saveSettings,
    onSuccess: async (settings) => {
      setSettingsDraft(settings);
      setStatus({ message: "设置已保存。", tone: "success" });
      await queryClient.invalidateQueries({ queryKey: ["settings"] });
    },
    onError: (error) => setStatus({ message: readableError(error, "保存设置失败。"), tone: "error" })
  });

  const importMutation = useMutation({
    mutationFn: ({
      zipPath,
      requestedName,
      ignoreVerification
    }: {
      zipPath: string;
      requestedName?: string | null;
      ignoreVerification?: boolean;
    }) => api.importProfile(zipPath, requestedName, ignoreVerification),
    onSuccess: async (profile) => {
      store.setViewingProfileId(profile.id);
      store.setPage("profiles");
      setStatus({ message: `已导入龙虾：${profile.name}`, tone: "success" });
      await queryClient.invalidateQueries({ queryKey: ["profiles"] });
      await queryClient.invalidateQueries({ queryKey: ["profile-inventory"] });
    },
    onError: (error) => setStatus({ message: readableError(error, "导入龙虾包失败。"), tone: "error" })
  });

  const exportMutation = useMutation({
    mutationFn: (request: ExportProfileRequest) => api.exportProfile(request),
    onSuccess: (result) => setStatus({ message: `龙虾包已导出到：${result.zipPath}`, tone: "success" }),
    onError: (error) => setStatus({ message: readableError(error, "导出龙虾包失败。"), tone: "error" })
  });

  const deleteProfileMutation = useMutation({
    mutationFn: api.deleteProfile,
    onSuccess: async (_, deletedProfileId) => {
      if (store.activeProfileId === deletedProfileId) store.setActiveProfileId(LOCAL_PROFILE_ID);
      if (store.viewingProfileId === deletedProfileId) store.setViewingProfileId(LOCAL_PROFILE_ID);
      queryClient.setQueryData<Awaited<ReturnType<typeof api.listProfiles>>>(["profiles"], (current = []) =>
        current.filter((profile) => profile.id !== deletedProfileId)
      );
      queryClient.removeQueries({ queryKey: ["profile-inventory", deletedProfileId] });
      setStatus({ message: "这只龙虾已经删除。", tone: "success" });
      await queryClient.invalidateQueries({ queryKey: ["profiles"] });
      await queryClient.invalidateQueries({ queryKey: ["profile-inventory"] });
    },
    onError: (error) => setStatus({ message: readableError(error, "删除龙虾失败。"), tone: "error" })
  });
  const renameProfileMutation = useMutation({
    mutationFn: ({ profileId, name }: { profileId: string; name: string }) => api.renameProfile(profileId, name),
    onSuccess: async (profile) => {
      queryClient.setQueryData<Awaited<ReturnType<typeof api.listProfiles>>>(["profiles"], (current = []) =>
        current.map((item) => item.id === profile.id ? profile : item)
      );
      setStatus({ message: `已将龙虾改名为：${profile.name}`, tone: "success" });
      await queryClient.invalidateQueries({ queryKey: ["profiles"] });
      await queryClient.invalidateQueries({ queryKey: ["settings"] });
    },
    onError: (error) => setStatus({ message: readableError(error, "修改龙虾名称失败。"), tone: "error" })
  });
  const launchMutation = useMutation({
    mutationFn: api.launchOpenclaw,
    onSuccess: async (handle) => {
      store.setActiveProfileId(handle.profileId);
      store.setViewingProfileId(handle.profileId);
      queryClient.setQueryData<GatewayStatus>(["gateway-status"], (current) => ({
        mode: current?.mode || "manual",
        url: current?.url || settingsDraft.gatewayConfig.url,
        running: true,
        healthy: true,
        pid: handle.pid ?? current?.pid ?? null,
        startedAt: handle.startedAt,
        lastError: null,
        logTail: current?.logTail ?? []
      }));
      setStatus({ message: `已启动：${handle.profileName}`, tone: "success" });
      await queryClient.invalidateQueries({ queryKey: ["settings"] });
      await queryClient.invalidateQueries({ queryKey: ["profiles"] });
      await queryClient.invalidateQueries({ queryKey: ["gateway-status"] });
    },
    onError: (error) => setStatus({ message: readableError(error, "启动龙虾失败。"), tone: "error" })
  });
  const launchingProfileId = launchMutation.isPending ? launchMutation.variables : undefined;
  const activeLobsterLaunching = launchingProfileId === activeLobsterId;
  const viewingLobsterLaunching = launchingProfileId === viewingLobsterId;

  const saveReadmeMutation = useMutation({
    mutationFn: ({ profileId, content }: { profileId: string; content: string }) => api.saveProfileReadme(profileId, content),
    onSuccess: async (preview, variables) => {
      setDocEditing(false);
      setDocDraft(preview.content);
      queryClient.setQueryData(["profile-readme", variables.profileId], preview);
      setStatus({ message: "文档已保存。", tone: "success" });
      await readmeQuery.refetch();
    },
    onError: (error) => setStatus({ message: readableError(error, "保存文档失败。"), tone: "error" })
  });

  const sendMutation = useMutation({
    mutationFn: ({ conversationId, content, profileId }: { conversationId: string; content: string; profileId?: string }) =>
      api.sendChatMessage(conversationId, { conversationId, content, profileId }),
    onMutate: async ({ conversationId, content }) => {
      const optimistic: ChatMessage = { id: `pending-${crypto.randomUUID()}`, role: "user", content, createdAt: new Date().toISOString() };
      store.setWaitingConversation(conversationId, true);
      queryClient.setQueryData<Conversation | undefined>(["conversation", conversationId], (current) =>
        appendMessageToLoadedConversation(current, optimistic)
      );
      queryClient.setQueryData<ConversationSummary[]>(["conversation-summaries"], (current = []) =>
        touchConversationSummary(current, conversationId, optimistic.createdAt)
      );
      setChatDraft("");
    },
    onError: (error, variables) => {
      store.clearStreamingConversation(variables.conversationId);
      setStatus({ message: readableError(error, "发送消息失败。"), tone: "error" });
    }
  });

  const onApplyDetection = (candidate?: PathCandidate) => {
    const picked = candidate || detectQuery.data?.[0];
    if (!picked) {
      setStatus({ message: "没有找到可用的 OpenClaw 启动入口。", tone: "warning" });
      return;
    }
    const next: AppSettings = { ...settingsDraft, openclawExecutablePath: picked.executablePath, openclawDataDir: picked.dataDir ?? settingsDraft.openclawDataDir };
    setSettingsDraft(next);
    saveSettingsMutation.mutate(next);
  };

  const onDetectOpenclaw = async () => {
    try {
      const result = await detectQuery.refetch();
      const candidates = result.data ?? [];
      if (candidates.length === 0) {
        setStatus({ message: "没有找到可用的 OpenClaw 启动入口。", tone: "warning" });
        return;
      }
      onApplyDetection(candidates[0]);
    } catch (error) {
      setStatus({ message: readableError(error, "自动识别失败。"), tone: "error" });
    }
  };

  const onCreateConversation = () => {
    const now = new Date().toISOString();
    api.saveConversation({ id: buildConversationId(activeLobsterId), title: "新对话", createdAt: now, updatedAt: now, messages: [] })
      .then((conversation) => {
        store.setSelectedConversationId(conversation.id);
        queryClient.setQueryData(["conversation", conversation.id], conversation);
        queryClient.setQueryData<ConversationSummary[]>(["conversation-summaries"], (current = []) =>
          mergeConversationSummary(current, conversation)
        );
      })
      .catch((error) => setStatus({ message: readableError(error, "新建对话失败。"), tone: "error" }));
  };

  const onSend = () => {
    if (!store.selectedConversationId || !chatDraft.trim()) return;
    sendMutation.mutate({ conversationId: store.selectedConversationId, content: chatDraft.trim(), profileId: activeLobsterId });
  };

  const onImport = async () => {
    const zipPath = await api.pickZipFile();
    if (!zipPath) return;
    const requestedName = window.prompt("给这只龙虾起个名字。留空则使用压缩包名字。", "");
    const verification = await api.verifyImportPackage(zipPath);
    if (!verification.valid) {
      const issueText = verification.issues.map((issue) => `- ${issue}`).join("\n");
      const confirmed = window.confirm(
        `你正尝试导入的龙虾被篡改过，是否无视安全风险继续导入？\n\n${issueText}`
      );
      if (!confirmed) return;
      importMutation.mutate({ zipPath, requestedName, ignoreVerification: true });
      return;
    }
    importMutation.mutate({ zipPath, requestedName });
  };

  const onExport = async () => {
    const sourceDir = viewingLobster?.path || settingsDraft.openclawDataDir || (await api.pickDirectory());
    if (!sourceDir) return;
    setExportState({
      open: true,
      sourceDir,
      packageName: viewingLobster?.name?.trim() || "我的龙虾",
      exportDir: desktopPathQuery.data || "",
      includeMemory: false,
      includeAccountInfo: false
    });
  };

  const onPreviewItem = async (section: InventorySection, item: ProfileListItem) => {
    setPreviewState({
      open: true,
      loading: true,
      title: item.title,
      subtitle: item.subtitle,
      content: "正在加载预览...",
      updatedAt: item.updatedAt
    });
    try {
      const preview = await api.previewProfileItem(viewingLobsterId, section, item.id);
      setPreviewState({ open: true, loading: false, ...preview });
    } catch (error) {
      setPreviewState((current) => ({ ...current, loading: false }));
      setStatus({ message: readableError(error, "预览内容失败。"), tone: "error" });
    }
  };

  const onRefreshLobsters = async () => {
    try {
      await Promise.all([
        profilesQuery.refetch(),
        inventoryQuery.refetch(),
        settingsQuery.refetch()
      ]);
      setStatus({ message: "龙虾列表已刷新。", tone: "success" });
    } catch (error) {
      setStatus({ message: readableError(error, "刷新龙虾列表失败。"), tone: "error" });
    }
  };

  const onOpenControlWeb = async (profileId: string) => {
    try {
      await api.openControlWeb(profileId);
      setStatus({ message: "控制网页已经打开。", tone: "success" });
    } catch (error) {
      setStatus({ message: readableError(error, "打开控制网页失败。"), tone: "error" });
    }
  };

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark plain"><span className="lobster-icon">🦞</span></div>
          <div className="brand-copy"><h1>小龙虾</h1><p>轻松玩转 OpenClaw</p></div>
        </div>
        <nav className="nav-list">
          {(["overview", "profiles", "chat", "notifications", "docs", "settings"] as PageId[]).map((page) => (
            <button key={page} className={`nav-button ${store.page === page ? "active" : ""}`} onClick={() => store.setPage(page)}>{navLabel(page)}</button>
          ))}
        </nav>
        <section className="sidebar-card">
          <span className="section-tag">当前状态</span>
          <StatusPill tone={needsSetup ? "warn" : activeLobsterLaunching || activeLobsterRunning ? "good" : "muted"} label={needsSetup ? "需要设置" : activeLobsterLaunching ? "启动中" : activeLobsterRunning ? "已启动" : "可以启动"} />
          <DetailRow label="当前龙虾" value={activeLobsterName} />
        </section>
      </aside>

      <main className="main-content">
        {store.page === "overview" ? (
          <section className="page-stack scroll-page">
            <section className="hero-card">
              <div className="hero-copy"><h2>启动你的龙虾</h2><p>当前使用 <strong>{activeLobsterName}</strong>。默认目录为 <code>{DEFAULT_OPENCLAW_PATH}</code>。</p></div>
              <div className="hero-action">
                <button className="button primary large" onClick={() => launchMutation.mutate(activeLobsterId)} disabled={needsSetup || activeLobsterLaunching}>{activeLobsterLaunching ? "启动中" : activeLobsterRunning ? "已启动" : "启动龙虾"}</button>
                <button className="button ghost" onClick={() => store.setPage("profiles")}>切换龙虾</button>
                <button className="button ghost" onClick={() => onOpenControlWeb(activeLobsterId)} disabled={needsSetup}>打开控制网页</button>
                <div className="hero-note"><div className="hero-note-row"><span>最近启动</span><strong>{activeLaunchRecord ? formatTime(activeLaunchRecord.launchedAt) : recentLaunch ? formatTime(recentLaunch.launchedAt) : "还没有启动记录"}</strong></div></div>
              </div>
            </section>
          </section>
        ) : null}
        {store.page === "profiles" ? (
          <section className="page-stack scroll-page">
            <section className="page-header">
              <div><span className="section-tag">龙虾</span><h2>切换、查看和管理龙虾</h2><p className="muted">每一只龙虾都代表一套独立的 OpenClaw agent 数据。</p></div>
              <div className="button-row wrap"><button className="button ghost" onClick={() => onRefreshLobsters()}>刷新列表</button><button className="button ghost" onClick={() => onExport()}>导出龙虾包</button><button className="button primary" onClick={() => onImport()}>导入龙虾包</button></div>
            </section>
            <section className="profiles-layout">
              <div className="panel profile-list-shell">
                <InputGroup label="搜索龙虾" value={lobsterSearch} placeholder="按名称或路径搜索" onChange={setLobsterSearch} />
                <div className="profile-scroll-list">
                  <button className={`profile-card compact ${viewingLobsterId === LOCAL_PROFILE_ID ? "active" : ""}`} onClick={() => store.setViewingProfileId(LOCAL_PROFILE_ID)}><strong>默认本地龙虾</strong><span className="muted value-text">{settingsDraft.openclawDataDir || DEFAULT_OPENCLAW_PATH}</span></button>
                  <div className="stack">
                    {filteredProfiles.map((profile) => (
                      <button key={profile.id} className={`profile-card compact ${viewingLobsterId === profile.id ? "active" : ""}`} onClick={() => store.setViewingProfileId(profile.id)}><strong>{profile.name}</strong><span className="muted value-text">{profile.path}</span></button>
                    ))}
                  </div>
                </div>
              </div>
              <div className="panel">
                <div className="panel-header">
                  <div><h3>{viewingLobster?.name || "默认本地龙虾"}</h3><p className="muted">{viewingLobster ? "这是导入后的龙虾，可以单独启动，也可以直接删除。" : "这是当前电脑上的默认本地龙虾。"}</p></div>
                  <div className="button-row wrap">
                    <button className="button primary" onClick={() => launchMutation.mutate(viewingLobsterId)} disabled={viewingLobsterLaunching}>{viewingLobsterLaunching ? "启动中" : viewingLobsterRunning ? "已启动" : "启动"}</button>
                    {viewingLobster ? <button className="button ghost" onClick={() => {
                      const nextName = window.prompt("输入新的龙虾名称。", viewingLobster.name);
                      if (!nextName || nextName.trim() === viewingLobster.name) return;
                      renameProfileMutation.mutate({ profileId: viewingLobster.id, name: nextName.trim() });
                    }}>改名</button> : null}
                    {viewingLobster ? <button className="button ghost" onClick={() => window.confirm(`确定删除龙虾“${viewingLobster.name}”吗？删除后无法恢复。`) && deleteProfileMutation.mutate(viewingLobster.id)}>删除</button> : null}
                  </div>
                </div>
                <div className="detail-list">
                  <DetailRow label="名称" value={viewingLobster?.name || "默认本地龙虾"} />
                  <DetailRow label="目录" value={viewingLobster?.path || settingsDraft.openclawDataDir || DEFAULT_OPENCLAW_PATH} />
                  <DetailRow label="来源" value={viewingLobster?.importedFrom || "当前本机默认目录"} />
                  <DetailRow label="创建时间" value={viewingLobster ? formatTime(viewingLobster.createdAt) : "系统默认"} />
                  <DetailRow label="最近启动" value={viewingLobster?.lastUsedAt ? formatTime(viewingLobster.lastUsedAt) : "还没有记录"} />
                </div>
                <section className="inventory-grid">
                  <InventoryBlock title="技能列表" section="skills" items={inventoryQuery.data?.skills ?? []} emptyText="还没有发现单独的技能文件。" onPreview={onPreviewItem} />
                  <InventoryBlock title="定时任务列表" section="cronJobs" items={inventoryQuery.data?.cronJobs ?? []} emptyText="这只龙虾还没有定时任务。" onPreview={onPreviewItem} />
                  <InventoryBlock title="记忆列表" section="memories" items={inventoryQuery.data?.memories ?? []} emptyText="还没有发现记忆条目。" onPreview={onPreviewItem} />
                  <InventoryBlock title="账号列表" section="accounts" items={inventoryQuery.data?.accounts ?? []} emptyText="还没有发现账号或设备信息。" onPreview={onPreviewItem} />
                </section>
              </div>
            </section>
          </section>
        ) : null}

        {store.page === "chat" ? (
          <section className="page-stack scroll-page">
            <section className="page-header">
              <div><h2>和龙虾聊天</h2><p className="muted">当前使用：{activeLobsterName}</p></div>
              <StatusPill tone={gatewayTone(gatewayQuery.data)} label={gatewayLabel(gatewayQuery.data)} />
            </section>
            <section className="chat-layout">
              <div className="panel conversation-panel">
                <div className="panel-header"><h3>对话列表</h3><button className="button ghost" onClick={() => onCreateConversation()}>新建对话</button></div>
                <div className="conversation-list">
                  {sortedChatConversations.map((conversation) => (
                    <div key={conversation.id} className={`conversation-card ${store.selectedConversationId === conversation.id ? "active" : ""}`}>
                      <button className="conversation-select" onClick={() => store.setSelectedConversationId(conversation.id)}><strong>{conversation.title}</strong><span className="muted">{formatTime(conversation.updatedAt)}</span></button>
                      <div className="inline-actions"><button className="mini-button" onClick={() => {
                        const nextTitle = window.prompt("输入新的对话名称。", conversation.title);
                        if (!nextTitle || nextTitle === conversation.title) return;
                        api.getConversation(conversation.id)
                          .then((fullConversation) =>
                            api.saveConversation({
                              ...fullConversation,
                              title: nextTitle,
                              updatedAt: new Date().toISOString()
                            })
                          )
                          .then((savedConversation) => {
                            queryClient.setQueryData(["conversation", savedConversation.id], savedConversation);
                            queryClient.setQueryData<ConversationSummary[]>(["conversation-summaries"], (current = []) =>
                              mergeConversationSummary(current, savedConversation)
                            );
                          })
                          .catch((error) => setStatus({ message: readableError(error, "修改对话名称失败。"), tone: "error" }));
                      }}>重命名</button><button className="mini-button danger" onClick={() => api.deleteConversation(conversation.id).then(() => {
                        store.setSelectedConversationId(undefined);
                        queryClient.removeQueries({ queryKey: ["conversation", conversation.id] });
                        queryClient.setQueryData<ConversationSummary[]>(["conversation-summaries"], (current = []) =>
                          current.filter((item) => item.id !== conversation.id)
                        );
                      }).catch((error) => setStatus({ message: readableError(error, "删除对话失败。"), tone: "error" }))}>删除</button></div>
                    </div>
                  ))}
                </div>
              </div>
              <div className="panel chat-panel">
                {selectedConversation ? (
                  <>
                    <div className="panel-header"><div><h3>{selectedConversation.title}</h3><p className="muted">正在和 {activeLobsterName} 聊天</p></div></div>
                    <div className="messages">
                      {sortedSelectedMessages.map((message) => (
                        <article key={message.id} className={`message ${message.role}`}><div className="message-head"><div className="message-role">{message.role === "user" ? "我" : message.role === "assistant" ? activeLobsterName : "系统"}</div><time className="message-time">{formatChatTime(message.createdAt)}</time></div><MessageContent content={message.content} /></article>
                      ))}
                      {store.selectedConversationId && store.streamingBuffer[store.selectedConversationId] ? <article className="message assistant"><div className="message-head"><div className="message-role">{activeLobsterName}</div><time className="message-time">生成中</time></div><MessageContent content={store.streamingBuffer[store.selectedConversationId]} /></article> : null}
                      {store.selectedConversationId && waitingForReply && !store.streamingBuffer[store.selectedConversationId] ? <article className="message assistant loading"><div className="message-head"><div className="message-role">{activeLobsterName}</div><time className="message-time">处理中</time></div><div className="loading-reply"><span className="loading-spinner" aria-hidden="true" /><span>正在思考你的消息…</span></div></article> : null}
                    </div>
                    <div className="composer"><textarea value={chatDraft} onChange={(event) => setChatDraft(event.target.value)} placeholder={`输入你想对 ${activeLobsterName} 说的话`} /><div className="button-row"><button className="button primary" onClick={() => onSend()}>发送给 {activeLobsterName}</button></div></div>
                  </>
                ) : <div className="empty-state">选择一个对话，或者先新建一个对话。</div>}
              </div>
            </section>
          </section>
        ) : null}
        {store.page === "notifications" ? (
          <section className="page-stack">
            <section className="page-header">
              <div><h2>通知</h2><p className="muted">这里集中显示当前龙虾收到的主动消息和提醒，只显示最新100条。</p></div>
              <StatusPill tone={gatewayTone(gatewayQuery.data)} label={gatewayLabel(gatewayQuery.data)} />
            </section>
            <div className="page-scroll-body">
              <section className="panel notifications-panel">
                {(notificationsQuery.data ?? []).length === 0 ? (
                  <div className="empty-state">当前还没有收到新的主动消息或提醒。</div>
                ) : (
                  <div className="stack">
                    {(notificationsQuery.data ?? []).map((item: NotificationItem) => (
                      <section key={item.id} className="notification-card">
                        <div className="panel-header">
                          <div><h3>{item.title}</h3><p className="muted">{item.subtitle} · {formatTime(item.createdAt)}</p></div>
                        </div>
                        <div className="messages compact">
                          <article className="message assistant">
                            <div className="message-head"><div className="message-role">{item.kind === "heartbeat" ? "Heartbeat" : activeLobsterName}</div><time className="message-time">{formatChatTime(item.createdAt)}</time></div>
                            <ReactMarkdown>{item.content}</ReactMarkdown>
                          </article>
                        </div>
                      </section>
                    ))}
                  </div>
                )}
              </section>
            </div>
          </section>
        ) : null}
        {store.page === "docs" ? (
          <section className="page-stack">
            <section className="page-header">
              <div><h2>文档</h2><p className="muted">显示当前选中龙虾目录中的 README.md。</p></div>
              <div className="button-row wrap">
                <button className="button ghost" onClick={() => readmeQuery.refetch()}>刷新文档</button>
                {docEditing ? (
                  <>
                    <button className="button ghost" onClick={() => { setDocEditing(false); setDocDraft(readmeQuery.data?.content || ""); }}>取消</button>
                    <button
                      className="button primary"
                      onClick={() => saveReadmeMutation.mutate({
                        profileId: viewingLobsterId,
                        content: docDraft
                      })}
                    >
                      保存
                    </button>
                  </>
                ) : (
                  <button className="button ghost" onClick={() => { setDocDraft(readmeQuery.data?.content || ""); setDocEditing(true); }}>编辑</button>
                )}
              </div>
            </section>
            <div className="page-scroll-body">
              <section className="panel doc-panel">
                <div className="panel-header">
                  <div><h3>{viewingLobster?.name || "默认本地龙虾"}</h3><p className="muted">{readmeQuery.data?.subtitle || "当前选中的龙虾目录"}</p></div>
                </div>
                {readmeQuery.isLoading ? (
                  <div className="empty-state">正在读取文档...</div>
                ) : docEditing ? (
                  <div className="doc-editor-shell">
                    <textarea
                      className="doc-editor"
                      value={docDraft}
                      onChange={(event) => setDocDraft(event.target.value)}
                      placeholder="在这里编辑 README.md"
                    />
                  </div>
                ) : readmeQuery.data ? (
                  <article className="doc-content"><ReactMarkdown>{readmeQuery.data.content}</ReactMarkdown></article>
                ) : (
                  <div className="empty-state">当前选中的龙虾目录里没有找到 README.md。</div>
                )}
              </section>
            </div>
          </section>
        ) : null}
        {store.page === "settings" ? (
          <section className="settings-page">
            <section className="page-header"><div><h2>检查启动入口和本地目录</h2><p className="muted">优先使用自动识别；不对时再手动修改。</p></div><button className="button primary" onClick={() => saveSettingsMutation.mutate(settingsDraft)}>保存设置</button></section>
            <section className="panel">
              <div className="panel-header"><div><h3>基础设置</h3><p className="muted">默认目录为 {DEFAULT_OPENCLAW_PATH}。</p><p className="muted">默认 OpenClaw 启动入口为 {DEFAULT_EXECUTABLE_PATH}。</p></div><button className="button secondary" onClick={() => void onDetectOpenclaw()}>{detectQuery.isFetching ? "识别中..." : "自动识别"}</button></div>
              <div className="setting-fields"><InputGroup label="OpenClaw 启动入口" value={settingsDraft.openclawExecutablePath || ""} placeholder={DEFAULT_EXECUTABLE_PATH} onChange={(value) => setSettingsDraft((current) => ({ ...current, openclawExecutablePath: value }))} /><InputGroup label="默认本地龙虾目录" value={settingsDraft.openclawDataDir || ""} placeholder={DEFAULT_OPENCLAW_PATH} onChange={(value) => setSettingsDraft((current) => ({ ...current, openclawDataDir: value }))} /></div>
              <div className="button-row wrap"><button className="button ghost" onClick={() => api.pickOpenclawExecutable().then((value) => { if (value) setSettingsDraft((current) => ({ ...current, openclawExecutablePath: value })); })}>选择启动入口</button><button className="button ghost" onClick={() => api.pickDirectory().then((value) => { if (value) setSettingsDraft((current) => ({ ...current, openclawDataDir: value })); })}>选择本地目录</button></div>
              {(detectQuery.data ?? []).map((candidate) => <div key={candidate.executablePath} className="candidate-card"><DetailRow label="来源" value={candidate.source} /><DetailRow label="启动入口" value={candidate.executablePath} /><DetailRow label="目录" value={candidate.dataDir || "未识别到"} /><button className="button ghost" onClick={() => onApplyDetection(candidate)}>使用这个结果</button></div>)}
            </section>
            <details className="panel advanced-panel" open={advancedOpen} onToggle={(event) => setAdvancedOpen((event.currentTarget as HTMLDetailsElement).open)}>
              <summary>{advancedOpen ? "收起高级设置" : "展开高级设置"}</summary>
              <div className="advanced-grid">
                <section className="sub-panel"><h3>龙虾目录</h3><InputGroup label="导入龙虾保存位置" value={settingsDraft.profilesRoot || ""} placeholder="应用默认目录" onChange={(value) => setSettingsDraft((current) => ({ ...current, profilesRoot: value }))} /><button className="button ghost" onClick={() => api.pickDirectory().then((value) => { if (value) setSettingsDraft((current) => ({ ...current, profilesRoot: value })); })}>选择目录</button></section>
                <section className="sub-panel"><h3>连接服务</h3><InputGroup label="连接地址" value={settingsDraft.gatewayConfig.url} onChange={(value) => setSettingsDraft((current) => ({ ...current, gatewayConfig: { ...current.gatewayConfig, url: value } }))} /><InputGroup label="健康检查路径" value={settingsDraft.gatewayConfig.healthEndpoint} onChange={(value) => setSettingsDraft((current) => ({ ...current, gatewayConfig: { ...current.gatewayConfig, healthEndpoint: value } }))} /><div className="button-row wrap"><button className="button ghost" onClick={() => api.startGateway(settingsDraft.gatewayConfig.mode, settingsDraft.gatewayConfig).then(() => queryClient.invalidateQueries({ queryKey: ["gateway-status"] }))}>启动连接服务</button><button className="button ghost" onClick={() => api.stopGateway().then(() => queryClient.invalidateQueries({ queryKey: ["gateway-status"] }))}>停止连接服务</button></div></section>
              </div>
            </details>
          </section>
        ) : null}

        {exportState.open ? <ExportDialog state={exportState} onChange={setExportState} onClose={() => setExportState((current) => ({ ...current, open: false }))} onPickDirectory={() => api.pickDirectory().then((value) => { if (value) setExportState((current) => ({ ...current, exportDir: value })); })} onConfirm={() => { const packageName = exportState.packageName.trim() || "我的龙虾"; const exportDir = exportState.exportDir.trim() || desktopPathQuery.data || ""; if (!exportDir) return setStatus({ message: "请先选择导出位置。", tone: "warning" }); if (exportState.includeMemory && !window.confirm("开启后，导出的龙虾包可能包含记忆和历史会话。继续导出有泄露风险，确认继续吗？")) return; if (exportState.includeAccountInfo && !window.confirm("开启后，导出的龙虾包可能包含账号和设备信息。继续导出有泄露风险，确认继续吗？")) return; exportMutation.mutate({ sourceDir: exportState.sourceDir, zipPath: joinPath(exportDir, `${packageName}.claw`), packageName, includeMemory: exportState.includeMemory, includeAccountInfo: exportState.includeAccountInfo }); setExportState((current) => ({ ...current, open: false })); }} /> : null}
        {previewState.open ? <PreviewDialog state={previewState} onClose={() => setPreviewState((current) => ({ ...current, open: false }))} /> : null}
        {status ? <div className="status-dock"><div className={`status-banner bottom ${status.tone}`}><div className="status-text">{status.message}</div><button className="status-close" onClick={() => setStatus(null)}>关闭</button></div></div> : null}
      </main>
    </div>
  );
}

function ExportDialog({ state, onChange, onClose, onPickDirectory, onConfirm }: { state: ExportState; onChange: React.Dispatch<React.SetStateAction<ExportState>>; onClose: () => void; onPickDirectory: () => void; onConfirm: () => void; }) {
  return <div className="modal-backdrop"><div className="modal-card"><div className="panel-header"><div><h3>导出龙虾包</h3><p className="muted">默认只导出安全内容，不包含记忆和账号信息。</p></div><button className="button ghost" onClick={() => onClose()}>取消</button></div><div className="modal-form"><InputGroup label="龙虾包名称" value={state.packageName} placeholder="我的龙虾" onChange={(value) => onChange((current) => ({ ...current, packageName: value }))} /><InputGroup label="导出位置" value={state.exportDir} placeholder="桌面" onChange={(value) => onChange((current) => ({ ...current, exportDir: value }))} /><div className="button-row start"><button className="button ghost" onClick={() => onPickDirectory()}>选择导出位置</button></div><div className="toggle-list"><label className="toggle-card"><input type="checkbox" checked={state.includeMemory} onChange={(event) => onChange((current) => ({ ...current, includeMemory: event.target.checked }))} /><div><strong>导出记忆和历史会话</strong><p className="muted">默认关闭。开启后可能暴露这只龙虾的记忆内容。</p></div></label><label className="toggle-card"><input type="checkbox" checked={state.includeAccountInfo} onChange={(event) => onChange((current) => ({ ...current, includeAccountInfo: event.target.checked }))} /><div><strong>导出账号和设备信息</strong><p className="muted">默认关闭。开启后可能暴露登录状态和设备信息。</p></div></label></div></div><div className="button-row wrap"><button className="button primary" onClick={() => onConfirm()}>确认导出</button></div></div></div>;
}

function PreviewDialog({ state, onClose }: { state: PreviewState; onClose: () => void }) {
  return <div className="modal-backdrop"><div className="modal-card preview-modal"><div className="panel-header"><div><h3>{state.title}</h3><p className="muted">{state.subtitle}</p>{state.updatedAt ? <p className="muted">更新时间：{formatTime(state.updatedAt)}</p> : null}</div><button className="button ghost" onClick={() => onClose()}>关闭</button></div><pre className="preview-content">{state.loading ? "正在加载..." : state.content || "没有可预览的内容。"}</pre></div></div>;
}

function Field({ label, value }: { label: string; value: string }) {
  return <div className="field"><span>{label}</span><strong>{value}</strong></div>;
}

function InputGroup({ label, value, placeholder, onChange }: { label: string; value: string; placeholder?: string; onChange: (value: string) => void; }) {
  return <label className="input-group"><span>{label}</span><input value={value} placeholder={placeholder} onChange={(event) => onChange(event.target.value)} /></label>;
}

function DetailRow({ label, value }: { label: string; value: string }) {
  return <div className="detail-row"><span>{label}</span><strong className="value-text">{value}</strong></div>;
}

function InventoryBlock({ title, section, items, emptyText, onPreview }: { title: string; section: InventorySection; items: ProfileListItem[]; emptyText: string; onPreview: (section: InventorySection, item: ProfileListItem) => void; }) {
  const [open, setOpen] = useState(false);
  return <section className="panel inventory-panel"><button className="inventory-toggle" onClick={() => setOpen((current) => !current)}><div><h3>{title}</h3><span className="muted">{items.length} 项</span></div><span className="muted">{open ? "收起" : "展开"}</span></button>{open ? items.length === 0 ? <div className="empty-state">{emptyText}</div> : <div className="stack">{items.map((item) => <div key={item.id} className="inventory-item"><div className="inventory-item-copy"><strong>{item.title}</strong><span className="muted value-text">{item.subtitle}</span>{item.updatedAt ? <span className="muted">{formatTime(item.updatedAt)}</span> : null}</div><button className="mini-button" onClick={() => onPreview(section, item)}>预览</button></div>)}</div> : null}</section>;
}

function StatusPill({ tone, label }: { tone: "good" | "warn" | "muted"; label: string }) {
  return <div className={`status-pill ${tone}`}>{label}</div>;
}
function navLabel(page: PageId) {
  return { overview: "首页", profiles: "龙虾", chat: "聊天", notifications: "通知", docs: "文档", settings: "设置" }[page];
}

function readableError(error: unknown, fallback: string) {
  if (error instanceof Error && error.message) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return fallback;
}

function gatewayLabel(status?: GatewayStatus) {
  if (!status) return "未检查";
  if (status.healthy) return "已连上";
  if (status.running) return "正在连接";
  return "未连上";
}

function gatewayTone(status?: GatewayStatus): "good" | "warn" | "muted" {
  if (!status) return "muted";
  if (status.healthy) return "good";
  if (status.running) return "warn";
  return "muted";
}

function toSystemDate(value?: string | null) {
  if (!value) return null;
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return null;
  return date;
}

function systemTimeZone() {
  return Intl.DateTimeFormat().resolvedOptions().timeZone;
}

function formatTime(value?: string | null) {
  if (!value) return "未知";
  const date = toSystemDate(value);
  if (!date) return value;
  return new Intl.DateTimeFormat(undefined, {
    hour12: false,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    timeZone: systemTimeZone()
  }).format(date);
}

function formatChatTime(value: string) {
  const date = toSystemDate(value);
  if (!date) return value;
  const now = new Date();
  const isToday =
    date.getFullYear() === now.getFullYear() &&
    date.getMonth() === now.getMonth() &&
    date.getDate() === now.getDate();
  return new Intl.DateTimeFormat(undefined, {
    hour12: false,
    year: isToday ? undefined : "numeric",
    month: isToday ? undefined : "2-digit",
    day: isToday ? undefined : "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: isToday ? "2-digit" : undefined,
    timeZone: systemTimeZone()
  }).format(date);
}

function MessageContent({ content }: { content: string }) {
  return looksLikeMarkdown(content)
    ? <ReactMarkdown>{content}</ReactMarkdown>
    : <p className="message-text">{content}</p>;
}

function looksLikeMarkdown(value: string) {
  return /[`*_#>\-\[\]\(\)\|]/.test(value) || value.includes("\n");
}

function joinPath(base: string, leaf: string) {
  return base.replace(/[\\/]+$/, "") + "\\" + leaf.replace(/^[\\/]+/, "");
}

function appendMessageToLoadedConversation(conversation: Conversation | undefined, message: ChatMessage) {
  if (!conversation) {
    return conversation;
  }
  return {
    ...conversation,
    updatedAt: message.createdAt,
    messages: [...conversation.messages, message]
  };
}

function mergeConversationSummary(summaries: ConversationSummary[], conversation: Conversation) {
  const nextSummary: ConversationSummary = {
    id: conversation.id,
    title: conversation.title,
    createdAt: conversation.createdAt,
    updatedAt: conversation.updatedAt
  };
  return [nextSummary, ...summaries.filter((item) => item.id !== conversation.id)]
    .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt) || right.id.localeCompare(left.id));
}

function touchConversationSummary(summaries: ConversationSummary[], conversationId: string, updatedAt: string) {
  return summaries
    .map((item) => item.id === conversationId ? { ...item, updatedAt } : item)
    .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt) || right.id.localeCompare(left.id));
}

function buildConversationId(profileId?: string) {
  return `${profileSessionKey(profileId)}--conv--${crypto.randomUUID()}`;
}

function conversationBelongsToProfile(conversationId: string, profileId?: string) {
  const prefix = `${profileSessionKey(profileId)}--conv--`;
  if (conversationId.startsWith(prefix)) return true;
  return (!profileId || profileId === LOCAL_PROFILE_ID) && !conversationId.includes("--conv--");
}

function profileSessionKey(profileId?: string) {
  return !profileId || profileId === LOCAL_PROFILE_ID ? "local" : profileId;
}
