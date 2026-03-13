import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import ReactMarkdown from "react-markdown";
import lobsterIcon from "./icon.svg";
import { api } from "./lib/api";
import { isEnglish, t, translateBackendText } from "./lib/i18n";
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
const IS_MAC = typeof navigator !== "undefined" && /Mac|iPhone|iPad|iPod/.test(navigator.userAgent);
const DEFAULT_OPENCLAW_PATH = IS_MAC ? "~/.openclaw" : `C:\\Users\\${isEnglish ? "your-name" : "你的用户名"}\\.openclaw`;
const DEFAULT_EXECUTABLE_PATH = IS_MAC
  ? "/Applications/OpenClaw.app/Contents/MacOS/OpenClaw"
  : `C:\\Users\\${isEnglish ? "your-name" : "你的用户名"}\\AppData\\Roaming\\npm\\openclaw.cmd`;

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

type ConfirmState = {
  open: boolean;
  title: string;
  message: string;
  details: string[];
  confirmLabel: string;
  cancelLabel: string;
  confirmTone: "primary" | "ghost";
  resolve?: (confirmed: boolean) => void;
};

type InventorySection = "settingDocuments" | "skills" | "cronJobs" | "memories" | "accounts";

const defaultSettings: AppSettings = {
  openclawExecutablePath: "",
  openclawDataDir: "",
  runtimeTarget: {
    kind: "windows",
    wslDistro: "",
    wslOpenclawPath: "",
    wslDataDir: ""
  },
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
  const [autoDetectionAttempted, setAutoDetectionAttempted] = useState(false);
  const [settingsDraft, setSettingsDraft] = useState<AppSettings>(defaultSettings);
  const [chatDraft, setChatDraft] = useState("");
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [lobsterSearch, setLobsterSearch] = useState("");
  const [status, setStatus] = useState<{ message: string; tone: StatusTone } | null>(null);
  const [currentVersion, setCurrentVersion] = useState("");
  const [updateSummary, setUpdateSummary] = useState<{ version: string; notes?: string | null } | null>(null);
  const [checkingUpdates, setCheckingUpdates] = useState(false);
  const [installingUpdate, setInstallingUpdate] = useState(false);
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
  const [confirmState, setConfirmState] = useState<ConfirmState>({
    open: false,
    title: t("confirmTitle"),
    message: "",
    details: [],
    confirmLabel: t("confirm"),
    cancelLabel: t("cancel"),
    confirmTone: "primary"
  });
  const [exportState, setExportState] = useState<ExportState>({
    open: false,
    sourceDir: "",
    packageName: t("myLobster"),
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
    document.title = t("appTitle");
  }, []);

  useEffect(() => {
    void getVersion().then(setCurrentVersion).catch(() => setCurrentVersion(""));
  }, []);

  useEffect(() => {
    if (!docEditing) {
      setDocDraft(readmeQuery.data?.content || "");
    }
  }, [docEditing, readmeQuery.data?.content]);

  useEffect(() => {
    if (settingsQuery.data) setSettingsDraft(settingsQuery.data);
  }, [settingsQuery.data]);

  useEffect(() => {
    const current = settingsQuery.data;
    if (!current || autoDetectionAttempted) {
      return;
    }

    const windowsReady = Boolean(current.openclawExecutablePath && current.openclawDataDir);
    const wslReady = Boolean(
      current.runtimeTarget.wslDistro &&
      current.runtimeTarget.wslOpenclawPath &&
      current.runtimeTarget.wslDataDir
    );
    const currentRuntimeReady = current.runtimeTarget.kind === "wsl" ? wslReady : windowsReady;

    if (currentRuntimeReady) {
      setAutoDetectionAttempted(true);
      return;
    }

    setAutoDetectionAttempted(true);
    void detectQuery.refetch()
      .then((result) => {
        const candidates = result.data ?? [];
        const preferredRuntime = current.runtimeTarget.kind;
        const preferred =
          candidates.find((item) => item.runtimeKind === preferredRuntime && item.validation.isValid) ||
          candidates.find((item) => item.runtimeKind === preferredRuntime) ||
          candidates.find((item) => item.validation.isValid) ||
          candidates[0];

        if (preferred) {
          onApplyDetection(preferred, current);
        }
      })
      .catch(() => {});
  }, [autoDetectionAttempted, detectQuery, settingsQuery.data]);

  useEffect(() => {
    let cancelled = false;
    void check()
      .then((update) => {
        if (cancelled || !update) return;
        setUpdateSummary({ version: update.version, notes: update.body });
      })
      .catch(() => {
        if (!cancelled) setUpdateSummary(null);
      });
    return () => {
      cancelled = true;
    };
  }, []);

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
  const activeLobsterName = activeLobster?.name ?? translateBackendText(activeLaunchRecord?.profileName) ?? t("defaultLocalLobster");
  const notificationsQuery = useQuery({
    queryKey: ["notifications", activeLobsterId],
    queryFn: () => api.listNotifications(activeLobsterId),
    enabled: store.page === "notifications",
    refetchInterval: store.page === "notifications" ? 5000 : false,
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
  const needsSetup = settingsDraft.runtimeTarget.kind === "wsl"
    ? !settingsDraft.runtimeTarget.wslDistro || !settingsDraft.runtimeTarget.wslOpenclawPath || !settingsDraft.runtimeTarget.wslDataDir
    : !settingsDraft.openclawExecutablePath || !settingsDraft.openclawDataDir;
  const recentLaunch = settingsQuery.data?.recentLaunches?.[0];

  const saveSettingsMutation = useMutation({
    mutationFn: api.saveSettings,
    onSuccess: async (settings) => {
      setSettingsDraft(settings);
      setStatus({ message: t("setttingsSaved"), tone: "success" });
      await queryClient.invalidateQueries({ queryKey: ["settings"] });
    },
    onError: (error) => setStatus({ message: readableError(error, t("saveSettingsFailed")), tone: "error" })
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
      setStatus({ message: t("importedLobsterStatus", { name: profile.name }), tone: "success" });
      await queryClient.invalidateQueries({ queryKey: ["profiles"] });
      await queryClient.invalidateQueries({ queryKey: ["profile-inventory"] });
    },
    onError: (error) => setStatus({ message: readableError(error, t("importLobsterPackageFailed")), tone: "error" })
  });

  const exportMutation = useMutation({
    mutationFn: (request: ExportProfileRequest) => api.exportProfile(request),
    onSuccess: (result) => setStatus({ message: t("exportedLobsterPackageStatus", { path: result.zipPath }), tone: "success" }),
    onError: (error) => setStatus({ message: readableError(error, t("exportLobsterPackageFailed")), tone: "error" })
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
      setStatus({ message: t("deletedLobsterStatus"), tone: "success" });
      await queryClient.invalidateQueries({ queryKey: ["profiles"] });
      await queryClient.invalidateQueries({ queryKey: ["profile-inventory"] });
    },
    onError: (error) => setStatus({ message: readableError(error, t("deleteLobsterFailed")), tone: "error" })
  });
  const renameProfileMutation = useMutation({
    mutationFn: ({ profileId, name }: { profileId: string; name: string }) => api.renameProfile(profileId, name),
    onSuccess: async (profile) => {
      queryClient.setQueryData<Awaited<ReturnType<typeof api.listProfiles>>>(["profiles"], (current = []) =>
        current.map((item) => item.id === profile.id ? profile : item)
      );
      setStatus({ message: t("renamedLobsterStatus", { name: profile.name }), tone: "success" });
      await queryClient.invalidateQueries({ queryKey: ["profiles"] });
      await queryClient.invalidateQueries({ queryKey: ["settings"] });
    },
    onError: (error) => setStatus({ message: readableError(error, t("renameLobsterFailed")), tone: "error" })
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
      setStatus({ message: t("launchedStatus", { name: translateBackendText(handle.profileName) }), tone: "success" });
      await queryClient.invalidateQueries({ queryKey: ["settings"] });
      await queryClient.invalidateQueries({ queryKey: ["profiles"] });
      await queryClient.invalidateQueries({ queryKey: ["gateway-status"] });
    },
    onError: (error) => setStatus({ message: readableError(error, t("launchLobsterFailed")), tone: "error" })
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
      setStatus({ message: t("docsSaved"), tone: "success" });
      await readmeQuery.refetch();
    },
    onError: (error) => setStatus({ message: readableError(error, t("saveDocsFailed")), tone: "error" })
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
      setStatus({ message: readableError(error, t("sendMessageFailed")), tone: "error" });
    }
  });

  const onApplyDetection = (candidate?: PathCandidate, baseSettings: AppSettings = settingsDraft) => {
    const picked = candidate || detectQuery.data?.find((item) => item.validation.isValid) || detectQuery.data?.[0];
    if (!picked) {
      setStatus({ message: t("noExecutableFound"), tone: "warning" });
      return;
    }
    const next: AppSettings = picked.runtimeKind === "wsl"
      ? {
          ...baseSettings,
          runtimeTarget: {
            kind: "wsl",
            wslDistro: picked.wslDistro ?? "",
            wslOpenclawPath: picked.executablePath,
            wslDataDir: picked.dataDir ?? baseSettings.runtimeTarget.wslDataDir
          }
        }
      : {
          ...baseSettings,
          openclawExecutablePath: picked.executablePath,
          openclawDataDir: picked.dataDir ?? baseSettings.openclawDataDir,
          runtimeTarget: {
            ...baseSettings.runtimeTarget,
            kind: "windows"
          }
        };
    setSettingsDraft(next);
    saveSettingsMutation.mutate(next);
  };

  const onCheckForUpdates = async () => {
    setCheckingUpdates(true);
    try {
      const update = await check();
      if (!update) {
        setUpdateSummary(null);
        setStatus({ message: t("noUpdateAvailable"), tone: "success" });
        return;
      }
      setUpdateSummary({ version: update.version, notes: update.body });
      setStatus({ message: t("updateAvailableStatus", { version: update.version }), tone: "success" });
    } catch (error) {
      setStatus({ message: readableError(error, t("checkUpdatesFailed")), tone: "error" });
    } finally {
      setCheckingUpdates(false);
    }
  };

  const onInstallUpdate = async () => {
    setInstallingUpdate(true);
    try {
      const update = await check();
      if (!update) {
        setUpdateSummary(null);
        setStatus({ message: t("noUpdateAvailable"), tone: "success" });
        return;
      }
      await update.downloadAndInstall();
      setStatus({ message: t("updaterRestarting"), tone: "success" });
      await relaunch();
    } catch (error) {
      setStatus({ message: readableError(error, t("installUpdateFailed")), tone: "error" });
    } finally {
      setInstallingUpdate(false);
    }
  };

  const onDetectOpenclaw = async () => {
    try {
      const result = await detectQuery.refetch();
      const candidates = result.data ?? [];
      if (candidates.length === 0) {
        setStatus({ message: t("noExecutableFound"), tone: "warning" });
        return;
      }
      onApplyDetection(candidates.find((item) => item.validation.isValid) || candidates[0]);
    } catch (error) {
      setStatus({ message: readableError(error, t("autoDetectFailed")), tone: "error" });
    }
  };

  const onCreateConversation = () => {
    const now = new Date().toISOString();
    api.saveConversation({ id: buildConversationId(activeLobsterId), title: t("newConversation"), createdAt: now, updatedAt: now, messages: [] })
      .then((conversation) => {
        store.setSelectedConversationId(conversation.id);
        queryClient.setQueryData(["conversation", conversation.id], conversation);
        queryClient.setQueryData<ConversationSummary[]>(["conversation-summaries"], (current = []) =>
          mergeConversationSummary(current, conversation)
        );
      })
      .catch((error) => setStatus({ message: readableError(error, t("createConversationFailed")), tone: "error" }));
  };

  const onSend = () => {
    if (!store.selectedConversationId || !chatDraft.trim()) return;
    sendMutation.mutate({ conversationId: store.selectedConversationId, content: chatDraft.trim(), profileId: activeLobsterId });
  };

  const onImport = async () => {
    const zipPath = await api.pickZipFile();
    if (!zipPath) return;
    const requestedName = window.prompt(t("importNamePrompt"), "");
    const verification = await api.verifyImportPackage(zipPath);
    if (!verification.valid) {
      const confirmed = await openConfirmDialog({
        message: t("importTamperedWarning"),
        details: verification.issues.map((issue) => translateBackendText(issue)),
        confirmLabel: t("continueImport"),
        confirmTone: "ghost"
      });
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
      packageName: viewingLobster?.name?.trim() || t("myLobster"),
      exportDir: desktopPathQuery.data || "",
      includeMemory: false,
      includeAccountInfo: false
    });
  };

  const openConfirmDialog = ({
    title = t("confirmTitle"),
    message,
    details = [],
    confirmLabel = t("confirm"),
    cancelLabel = t("cancel"),
    confirmTone = "primary"
  }: {
    title?: string;
    message: string;
    details?: string[];
    confirmLabel?: string;
    cancelLabel?: string;
    confirmTone?: "primary" | "ghost";
  }) =>
    new Promise<boolean>((resolve) => {
      setConfirmState({
        open: true,
        title,
        message,
        details,
        confirmLabel,
        cancelLabel,
        confirmTone,
        resolve
      });
    });

  const closeConfirmDialog = (confirmed: boolean) => {
    setConfirmState((current) => {
      current.resolve?.(confirmed);
      return {
        open: false,
        title: t("confirmTitle"),
        message: "",
        details: [],
        confirmLabel: t("confirm"),
        cancelLabel: t("cancel"),
        confirmTone: "primary"
      };
    });
  };

  const onPreviewItem = async (section: InventorySection, item: ProfileListItem) => {
    setPreviewState({
      open: true,
      loading: true,
      title: translateBackendText(item.title),
      subtitle: translateBackendText(item.subtitle),
      content: t("loadingPreview"),
      updatedAt: item.updatedAt
    });
    try {
      const preview = await api.previewProfileItem(viewingLobsterId, section, item.id);
      setPreviewState({ open: true, loading: false, ...preview });
    } catch (error) {
      setPreviewState((current) => ({ ...current, loading: false }));
      setStatus({ message: readableError(error, t("previewFailed")), tone: "error" });
    }
  };

  const onRefreshLobsters = async () => {
    try {
      await Promise.all([
        profilesQuery.refetch(),
        inventoryQuery.refetch(),
        settingsQuery.refetch()
      ]);
      setStatus({ message: t("lobstersRefreshed"), tone: "success" });
    } catch (error) {
      setStatus({ message: readableError(error, t("refreshLobstersFailed")), tone: "error" });
    }
  };

  const onOpenControlWeb = async (profileId: string) => {
    try {
      await api.openControlWeb(profileId);
      setStatus({ message: t("controlWebOpened"), tone: "success" });
    } catch (error) {
      setStatus({ message: readableError(error, t("openControlWebFailed")), tone: "error" });
    }
  };

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark plain"><img className="lobster-icon-image" src={lobsterIcon} alt={t("appName")} /></div>
          <div className="brand-copy"><h1>{t("appName")}</h1><p>{t("appTagline")}</p></div>
        </div>
        <nav className="nav-list">
          {(["overview", "profiles", "chat", "notifications", "docs", "settings"] as PageId[]).map((page) => (
            <button key={page} className={`nav-button ${store.page === page ? "active" : ""}`} onClick={() => store.setPage(page)}>{navLabel(page)}</button>
          ))}
        </nav>
        <section className="sidebar-card">
          <span className="section-tag">{t("currentStatus")}</span>
          <StatusPill tone={needsSetup ? "warn" : activeLobsterLaunching || activeLobsterRunning ? "good" : "muted"} label={needsSetup ? t("needSetup") : activeLobsterLaunching ? t("launching") : activeLobsterRunning ? t("launched") : t("canLaunch")} />
          <DetailRow label={t("currentLobster")} value={activeLobsterName} />
        </section>
      </aside>

      <main className="main-content">
        {store.page === "overview" ? (
          <section className="page-stack scroll-page profiles-page">
            <section className="hero-card">
              <div className="hero-copy"><h2>{t("heroTitle")}</h2><p>{t("heroDescription", { name: activeLobsterName, path: DEFAULT_OPENCLAW_PATH.split("\\").join("\\") })}</p></div>
              <div className="hero-action">
                <button className="button primary large" onClick={() => launchMutation.mutate(activeLobsterId)} disabled={needsSetup || activeLobsterLaunching}>{activeLobsterLaunching ? t("launching") : activeLobsterRunning ? t("launched") : t("launchLobster")}</button>
                <button className="button ghost" onClick={() => store.setPage("profiles")}>{t("switchLobster")}</button>
                <button className="button ghost" onClick={() => onOpenControlWeb(activeLobsterId)} disabled={needsSetup}>{t("openControlWeb")}</button>
                <div className="hero-note"><div className="hero-note-row"><span>{t("lastLaunch")}</span><strong>{activeLaunchRecord ? formatTime(activeLaunchRecord.launchedAt) : recentLaunch ? formatTime(recentLaunch.launchedAt) : t("noLaunchRecordYet")}</strong></div></div>
              </div>
            </section>
            <div className="build-signature">2026/3/13 0.1.4 @ixing</div>
          </section>
        ) : null}
        {store.page === "profiles" ? (
          <section className="page-stack scroll-page">
            <section className="page-header profiles-page-header">
              <div>{t("profilesTag").trim() ? <span className="section-tag">{t("profilesTag")}</span> : null}<h2>{t("profilesTitle")}</h2><p className="muted">{t("profilesDescription")}</p></div>
              <div className="button-row wrap"><button className="button ghost" onClick={() => onRefreshLobsters()}>{t("refreshList")}</button><button className="button ghost" onClick={() => onExport()}>{t("exportLobsterPackage")}</button><button className="button primary" onClick={() => onImport()}>{t("importLobsterPackage")}</button></div>
            </section>
            <section className="profiles-layout">
              <div className="panel profile-list-shell">
                <label className="input-group compact-input">
                  <input value={lobsterSearch} placeholder={t("searchLobsterPlaceholder")} onChange={(event) => setLobsterSearch(event.target.value)} />
                </label>
                <div className="profile-scroll-list">
                  <button className={`profile-card compact ${viewingLobsterId === LOCAL_PROFILE_ID ? "active" : ""}`} onClick={() => store.setViewingProfileId(LOCAL_PROFILE_ID)}><strong>{t("defaultLocalLobster")}</strong></button>
                  <div className="stack">
                    {filteredProfiles.map((profile) => (
                      <button key={profile.id} className={`profile-card compact ${viewingLobsterId === profile.id ? "active" : ""}`} onClick={() => store.setViewingProfileId(profile.id)}><strong>{profile.name}</strong></button>
                    ))}
                  </div>
                </div>
              </div>
              <div className="panel profile-detail-panel">
                <div className="panel-header">
                  <div><h3>{viewingLobster?.name || t("defaultLocalLobster")}</h3><p className="muted">{viewingLobster ? t("importedLobsterDescription") : t("localLobsterDescription")}</p></div>
                  <div className="button-row wrap">
                    <button className="button primary" onClick={() => launchMutation.mutate(viewingLobsterId)} disabled={viewingLobsterLaunching}>{viewingLobsterLaunching ? t("launching") : viewingLobsterRunning ? t("launched") : t("launch")}</button>
                    {viewingLobster ? <button className="button ghost" onClick={() => {
                      const nextName = window.prompt(t("enterNewLobsterName"), viewingLobster.name);
                      if (!nextName || nextName.trim() === viewingLobster.name) return;
                      renameProfileMutation.mutate({ profileId: viewingLobster.id, name: nextName.trim() });
                    }}>{t("rename")}</button> : null}
                    {viewingLobster ? <button className="button ghost" onClick={async () => {
                      const confirmed = await openConfirmDialog({
                        message: t("deleteLobsterConfirm", { name: viewingLobster.name }),
                        confirmLabel: t("confirmDelete"),
                        confirmTone: "ghost"
                      });
                      if (!confirmed) return;
                      deleteProfileMutation.mutate(viewingLobster.id);
                    }}>{t("delete")}</button> : null}
                  </div>
                </div>
                <div className="detail-list">
                  <DetailRow label={t("lobsterName")} value={viewingLobster?.name || t("defaultLocalLobster")} />
                  <DetailRow label={t("lobsterDirectory")} value={viewingLobster?.path || settingsDraft.openclawDataDir || DEFAULT_OPENCLAW_PATH} />
                  <DetailRow label={t("lobsterSource")} value={viewingLobster?.importedFrom || t("currentMachineDefaultDirectory")} />
                  <DetailRow label={t("createdAt")} value={viewingLobster ? formatTime(viewingLobster.createdAt) : t("systemDefault")} />
                  <DetailRow label={t("recentlyLaunched")} value={viewingLobster?.lastUsedAt ? formatTime(viewingLobster.lastUsedAt) : t("noRecordYet")} />
                </div>
                <section className="inventory-grid">
                  <InventoryBlock title={t("settingDocuments")} section="settingDocuments" items={inventoryQuery.data?.settingDocuments ?? []} emptyText={t("noSettingDocuments")} onPreview={onPreviewItem} />
                  <InventoryBlock title={t("skills")} section="skills" items={inventoryQuery.data?.skills ?? []} emptyText={t("noWorkspaceSkills")} onPreview={onPreviewItem} />
                  <InventoryBlock title={t("cronJobs")} section="cronJobs" items={inventoryQuery.data?.cronJobs ?? []} emptyText={t("noCronJobs")} onPreview={onPreviewItem} />
                  <InventoryBlock title={t("memories")} section="memories" items={inventoryQuery.data?.memories ?? []} emptyText={t("noMemories")} onPreview={onPreviewItem} />
                  <InventoryBlock title={t("accounts")} section="accounts" items={inventoryQuery.data?.accounts ?? []} emptyText={t("noAccounts")} onPreview={onPreviewItem} />
                </section>
              </div>
            </section>
          </section>
        ) : null}

        {store.page === "chat" ? (
          <section className="page-stack scroll-page chat-page">
            <section className="page-header">
              <div><h2>{t("chattingWithLobster")}</h2><p className="muted">{t("currentlyUsing", { name: activeLobsterName })}</p></div>
              <StatusPill tone={gatewayTone(gatewayQuery.data)} label={gatewayLabel(gatewayQuery.data)} />
            </section>
            <section className="chat-layout">
              <div className="panel conversation-panel">
                <div className="panel-header"><h3>{t("conversationList")}</h3><button className="button ghost" onClick={() => onCreateConversation()}>{t("newConversation")}</button></div>
                <div className="conversation-list">
                  {sortedChatConversations.map((conversation) => (
                    <div key={conversation.id} className={`conversation-card ${store.selectedConversationId === conversation.id ? "active" : ""}`}>
                      <button className="conversation-select" onClick={() => store.setSelectedConversationId(conversation.id)}><strong>{translateBackendText(conversation.title)}</strong><span className="muted">{formatTime(conversation.updatedAt)}</span></button>
                      {store.selectedConversationId === conversation.id ? <div className="inline-actions"><button className="mini-button" onClick={() => {
                        const nextTitle = window.prompt(t("renameConversationPrompt"), conversation.title);
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
                          .catch((error) => setStatus({ message: readableError(error, t("renameConversationFailed")), tone: "error" }));
                      }}>{t("rename")}</button><button className="mini-button danger" onClick={() => api.deleteConversation(conversation.id).then(() => {
                        store.setSelectedConversationId(undefined);
                        queryClient.removeQueries({ queryKey: ["conversation", conversation.id] });
                        queryClient.setQueryData<ConversationSummary[]>(["conversation-summaries"], (current = []) =>
                          current.filter((item) => item.id !== conversation.id)
                        );
                      }).catch((error) => setStatus({ message: readableError(error, t("deleteConversationFailed")), tone: "error" }))}>{t("delete")}</button></div> : null}
                    </div>
                  ))}
                </div>
              </div>
              <div className="panel chat-panel">
                {selectedConversation ? (
                  <>
                    <div className="panel-header"><div><h3>{translateBackendText(selectedConversation.title)}</h3><p className="muted">{t("chattingWithName", { name: activeLobsterName })}</p></div></div>
                    <div className="messages">
                      {sortedSelectedMessages.map((message) => (
                        <article key={message.id} className={`message ${message.role}`}><div className="message-head"><div className="message-role">{message.role === "user" ? t("me") : message.role === "assistant" ? activeLobsterName : t("system")}</div><time className="message-time">{formatChatTime(message.createdAt)}</time></div><MessageContent content={message.content} /></article>
                      ))}
                      {store.selectedConversationId && store.streamingBuffer[store.selectedConversationId] ? <article className="message assistant"><div className="message-head"><div className="message-role">{activeLobsterName}</div><time className="message-time">{t("generating")}</time></div><MessageContent content={store.streamingBuffer[store.selectedConversationId]} /></article> : null}
                      {store.selectedConversationId && waitingForReply && !store.streamingBuffer[store.selectedConversationId] ? <article className="message assistant loading"><div className="message-head"><div className="message-role">{activeLobsterName}</div><time className="message-time">{t("processing")}</time></div><div className="loading-reply"><span className="loading-spinner" aria-hidden="true" /><span>{t("thinkingMessage")}</span></div></article> : null}
                    </div>
                    <div className="composer"><textarea value={chatDraft} onChange={(event) => setChatDraft(event.target.value)} placeholder={t("composePlaceholder", { name: activeLobsterName })} /><div className="button-row"><button className="button primary" onClick={() => onSend()}>{t("sendToName", { name: activeLobsterName })}</button></div></div>
                  </>
                ) : <div className="empty-state">{t("selectConversationEmpty")}</div>}
              </div>
            </section>
          </section>
        ) : null}
        {store.page === "notifications" ? (
          <section className="page-stack">
            <section className="page-header">
              <div><h2>{t("notificationsTitle")}</h2><p className="muted">{t("notificationsDescription")}</p></div>
              <StatusPill tone={gatewayTone(gatewayQuery.data)} label={gatewayLabel(gatewayQuery.data)} />
            </section>
            <div className="page-scroll-body notifications-scroll-body">
              <section className="panel notifications-panel">
                {(notificationsQuery.data ?? []).length === 0 ? (
                  <div className="empty-state">{t("notificationsEmpty")}</div>
                ) : (
                  <div className="stack">
                    {(notificationsQuery.data ?? []).map((item: NotificationItem) => (
                      <section key={item.id} className="notification-card">
                        <div className="panel-header">
                          <div><h3>{translateBackendText(item.title)}</h3><p className="muted">{translateBackendText(item.subtitle)} · {formatTime(item.createdAt)}</p></div>
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
              <div><h2>{t("docsTitle")}</h2><p className="muted">{t("docsDescription")}</p></div>
              <div className="button-row wrap">
                <button className="button ghost" onClick={() => readmeQuery.refetch()}>{t("refreshDocs")}</button>
                {docEditing ? (
                  <>
                    <button className="button ghost" onClick={() => { setDocEditing(false); setDocDraft(readmeQuery.data?.content || ""); }}>{t("cancel")}</button>
                    <button
                      className="button primary"
                      onClick={() => saveReadmeMutation.mutate({
                        profileId: viewingLobsterId,
                        content: docDraft
                      })}
                    >
                      {t("save")}
                    </button>
                  </>
                ) : (
                  <button className="button ghost" onClick={() => { setDocDraft(readmeQuery.data?.content || ""); setDocEditing(true); }}>{t("edit")}</button>
                )}
              </div>
            </section>
            <div className="page-scroll-body">
              <section className="panel doc-panel">
                <div className="panel-header">
                  <div><h3>{viewingLobster?.name || t("defaultLocalLobster")}</h3><p className="muted">{translateBackendText(readmeQuery.data?.subtitle) || t("selectedLobsterDirectory")}</p></div>
                </div>
                {readmeQuery.isLoading ? (
                  <div className="empty-state">{t("loadingDocs")}</div>
                ) : docEditing ? (
                  <div className="doc-editor-shell">
                    <textarea
                      className="doc-editor"
                      value={docDraft}
                      onChange={(event) => setDocDraft(event.target.value)}
                      placeholder={t("editReadmePlaceholder")}
                    />
                  </div>
                ) : readmeQuery.data ? (
                  <article className="doc-content"><ReactMarkdown>{readmeQuery.data.content}</ReactMarkdown></article>
                ) : (
                  <div className="empty-state">{t("missingReadme")}</div>
                )}
              </section>
            </div>
          </section>
        ) : null}
        {store.page === "settings" ? (
          <section className="settings-page">
            <section className="page-header"><div><h2>{t("settingsTitle")}</h2><p className="muted">{t("settingsDescription")}</p></div><button className="button primary" onClick={() => saveSettingsMutation.mutate(settingsDraft)}>{t("saveSettings")}</button></section>
            <section className="panel">
              <div className="panel-header settings-header"><div><h3>{t("basicSettings")}</h3><p className="muted">{t("defaultExecutableIs", { path: DEFAULT_EXECUTABLE_PATH })}</p><p className="muted">{t("defaultDirectoryIs", { path: DEFAULT_OPENCLAW_PATH })}</p></div><button className="button secondary" onClick={() => void onDetectOpenclaw()}>{detectQuery.isFetching ? t("detecting") : t("autoDetect")}</button></div>
              <div className="setting-fields">
                <label className="input-group">
                  <span>{isEnglish ? "Runtime Target" : "运行环境"}</span>
                  <select value={settingsDraft.runtimeTarget.kind} onChange={(event) => setSettingsDraft((current) => ({ ...current, runtimeTarget: { ...current.runtimeTarget, kind: event.target.value as "windows" | "wsl" } }))}>
                    <option value="windows">Windows</option>
                    <option value="wsl">WSL</option>
                  </select>
                </label>
                {settingsDraft.runtimeTarget.kind === "wsl" ? (
                  <>
                    <InputGroup label={isEnglish ? "WSL Distro" : "WSL 发行版"} value={settingsDraft.runtimeTarget.wslDistro || ""} placeholder="Ubuntu" onChange={(value) => setSettingsDraft((current) => ({ ...current, runtimeTarget: { ...current.runtimeTarget, wslDistro: value } }))} />
                    <InputGroup label={isEnglish ? "WSL OpenClaw Path" : "WSL OpenClaw 路径"} value={settingsDraft.runtimeTarget.wslOpenclawPath || ""} placeholder="/home/user/.local/bin/openclaw" onChange={(value) => setSettingsDraft((current) => ({ ...current, runtimeTarget: { ...current.runtimeTarget, wslOpenclawPath: value } }))} />
                    <InputGroup label={isEnglish ? "WSL Data Directory" : "WSL 数据目录"} value={settingsDraft.runtimeTarget.wslDataDir || ""} placeholder="/home/user/.openclaw" onChange={(value) => setSettingsDraft((current) => ({ ...current, runtimeTarget: { ...current.runtimeTarget, wslDataDir: value } }))} />
                  </>
                ) : (
                  <>
                    <InputGroup label={t("executablePath")} value={settingsDraft.openclawExecutablePath || ""} placeholder={DEFAULT_EXECUTABLE_PATH} onChange={(value) => setSettingsDraft((current) => ({ ...current, openclawExecutablePath: value }))} />
                    <InputGroup label={isEnglish ? t("localLobsterDirectory") : "默认龙虾目录"} value={settingsDraft.openclawDataDir || ""} placeholder={DEFAULT_OPENCLAW_PATH} onChange={(value) => setSettingsDraft((current) => ({ ...current, openclawDataDir: value }))} />
                  </>
                )}
              </div>
              {settingsDraft.runtimeTarget.kind === "windows" ? <div className="button-row wrap"><button className="button ghost" onClick={() => api.pickOpenclawExecutable().then((value) => { if (value) setSettingsDraft((current) => ({ ...current, openclawExecutablePath: value })); })}>{IS_MAC ? t("chooseAppOrExecutable") : t("chooseExecutable")}</button><button className="button ghost" onClick={() => api.pickDirectory().then((value) => { if (value) setSettingsDraft((current) => ({ ...current, openclawDataDir: value })); })}>{t("chooseLocalDirectory")}</button></div> : null}
              {(detectQuery.data ?? []).map((candidate) => <div key={`${candidate.runtimeKind}:${candidate.wslDistro || "windows"}:${candidate.executablePath}`} className="candidate-card"><DetailRow label={isEnglish ? "Runtime" : "运行环境"} value={candidate.runtimeKind === "wsl" ? `WSL${candidate.wslDistro ? ` (${candidate.wslDistro})` : ""}` : "Windows"} /><DetailRow label={t("source")} value={translateBackendText(candidate.source)} /><DetailRow label={t("executablePath")} value={candidate.executablePath} /><DetailRow label={t("lobsterDirectory")} value={candidate.dataDir || t("unknown")} /><button className="button ghost" onClick={() => onApplyDetection(candidate)}>{t("useThisResult")}</button></div>)}
            </section>
            <section className="panel">
              <div className="panel-header settings-header">
                <div>
                  <h3>{t("updatesTitle")}</h3>
                  <p className="muted">{t("updatesDescription")}</p>
                </div>
                <button className="button secondary" onClick={() => void onCheckForUpdates()} disabled={checkingUpdates || installingUpdate}>
                  {checkingUpdates ? t("checkingForUpdates") : t("checkForUpdates")}
                </button>
              </div>
              <div className="candidate-card">
                <DetailRow label={t("currentVersionLabel")} value={currentVersion || "0.1.4"} />
                {updateSummary ? (
                  <>
                    <DetailRow label={t("checkForUpdates")} value={updateSummary.version} />
                    {updateSummary.notes ? <div className="detail-row"><span>{t("releaseNotesLabel")}</span><span>{updateSummary.notes}</span></div> : null}
                    <div className="button-row wrap">
                      <button className="button primary" onClick={() => void onInstallUpdate()} disabled={checkingUpdates || installingUpdate}>
                        {installingUpdate ? t("installingUpdate") : t("installUpdate")}
                      </button>
                    </div>
                  </>
                ) : <p className="muted">{t("updaterUnavailable")}</p>}
              </div>
            </section>
            <details className="panel advanced-panel" open={advancedOpen} onToggle={(event) => setAdvancedOpen((event.currentTarget as HTMLDetailsElement).open)}>
              <summary>{advancedOpen ? t("collapseAdvancedSettings") : t("expandAdvancedSettings")}</summary>
              <div className="advanced-grid">
                <section className="sub-panel"><h3>{t("lobsterDirectorySection")}</h3><InputGroup label={t("importedLobsterSaveLocation")} value={settingsDraft.profilesRoot || ""} placeholder={t("appDefaultDirectory")} onChange={(value) => setSettingsDraft((current) => ({ ...current, profilesRoot: value }))} /><button className="button ghost" onClick={() => api.pickDirectory().then((value) => { if (value) setSettingsDraft((current) => ({ ...current, profilesRoot: value })); })}>{t("chooseDirectory")}</button></section>
                <section className="sub-panel"><h3>{t("connectionService")}</h3><InputGroup label={t("connectionAddress")} value={settingsDraft.gatewayConfig.url} onChange={(value) => setSettingsDraft((current) => ({ ...current, gatewayConfig: { ...current.gatewayConfig, url: value } }))} /><InputGroup label={t("healthEndpoint")} value={settingsDraft.gatewayConfig.healthEndpoint} onChange={(value) => setSettingsDraft((current) => ({ ...current, gatewayConfig: { ...current.gatewayConfig, healthEndpoint: value } }))} /><div className="button-row wrap"><button className="button ghost" onClick={() => api.startGateway(settingsDraft.gatewayConfig.mode, settingsDraft.gatewayConfig).then(() => queryClient.invalidateQueries({ queryKey: ["gateway-status"] }))}>{t("startConnectionService")}</button><button className="button ghost" onClick={() => api.stopGateway().then(() => queryClient.invalidateQueries({ queryKey: ["gateway-status"] }))}>{t("stopConnectionService")}</button></div></section>
              </div>
              <div className="button-row start advanced-actions"><button className="button ghost" onClick={() => setSettingsDraft((current) => ({ ...current, profilesRoot: defaultSettings.profilesRoot, gatewayConfig: { ...defaultSettings.gatewayConfig } }))}>{t("resetAdvancedSettings")}</button></div>
            </details>
          </section>
        ) : null}

        {exportState.open ? <ExportDialog state={exportState} onChange={setExportState} onClose={() => setExportState((current) => ({ ...current, open: false }))} onPickDirectory={() => api.pickDirectory().then((value) => { if (value) setExportState((current) => ({ ...current, exportDir: value })); })} onConfirm={async () => { const packageName = exportState.packageName.trim() || t("myLobster"); const exportDir = exportState.exportDir.trim() || desktopPathQuery.data || ""; if (!exportDir) return setStatus({ message: t("chooseExportLocationFirst"), tone: "warning" }); if (exportState.includeMemory && !await openConfirmDialog({ message: t("exportMemoryRisk"), confirmLabel: t("continueExport"), confirmTone: "ghost" })) return; if (exportState.includeAccountInfo && !await openConfirmDialog({ message: t("exportAccountRisk"), confirmLabel: t("continueExport"), confirmTone: "ghost" })) return; exportMutation.mutate({ sourceDir: exportState.sourceDir, zipPath: joinPath(exportDir, `${packageName}.claw`), packageName, includeMemory: exportState.includeMemory, includeAccountInfo: exportState.includeAccountInfo }); setExportState((current) => ({ ...current, open: false })); }} /> : null}
        {previewState.open ? <PreviewDialog state={previewState} onClose={() => setPreviewState((current) => ({ ...current, open: false }))} /> : null}
        {confirmState.open ? <ConfirmDialog state={confirmState} onClose={closeConfirmDialog} /> : null}
        {status ? <div className="status-dock"><div className={`status-banner bottom ${status.tone}`}><div className="status-text">{status.message}</div><button className="status-close" onClick={() => setStatus(null)}>{t("close")}</button></div></div> : null}
      </main>
    </div>
  );
}

function ExportDialog({ state, onChange, onClose, onPickDirectory, onConfirm }: { state: ExportState; onChange: React.Dispatch<React.SetStateAction<ExportState>>; onClose: () => void; onPickDirectory: () => void; onConfirm: () => void | Promise<void>; }) {
  return <div className="modal-backdrop"><div className="modal-card"><div className="panel-header"><div><h3>{t("exportDialogTitle")}</h3><p className="muted">{t("exportDialogDescription")}</p></div><button className="button ghost" onClick={() => onClose()}>{t("cancel")}</button></div><div className="modal-form"><InputGroup label={t("lobsterPackageName")} value={state.packageName} placeholder={t("myLobster")} onChange={(value) => onChange((current) => ({ ...current, packageName: value }))} /><InputGroup label={t("exportLocation")} value={state.exportDir} placeholder={t("desktop")} onChange={(value) => onChange((current) => ({ ...current, exportDir: value }))} /><div className="button-row start"><button className="button ghost" onClick={() => onPickDirectory()}>{t("chooseExportLocation")}</button></div><div className="toggle-list"><label className="toggle-card"><input type="checkbox" checked={state.includeMemory} onChange={(event) => onChange((current) => ({ ...current, includeMemory: event.target.checked }))} /><div><strong>{t("exportMemoryAndHistory")}</strong><p className="muted">{t("exportMemoryDescription")}</p></div></label><label className="toggle-card"><input type="checkbox" checked={state.includeAccountInfo} onChange={(event) => onChange((current) => ({ ...current, includeAccountInfo: event.target.checked }))} /><div><strong>{t("exportAccountAndDevice")}</strong><p className="muted">{t("exportAccountDescription")}</p></div></label></div></div><div className="button-row wrap"><button className="button primary" onClick={() => onConfirm()}>{t("confirmExport")}</button></div></div></div>;
}

function PreviewDialog({ state, onClose }: { state: PreviewState; onClose: () => void }) {
  return <div className="modal-backdrop"><div className="modal-card preview-modal"><div className="panel-header"><div><h3>{translateBackendText(state.title)}</h3><p className="muted">{translateBackendText(state.subtitle)}</p>{state.updatedAt ? <p className="muted">{t("updatedAt", { time: formatTime(state.updatedAt) })}</p> : null}</div><button className="button ghost" onClick={() => onClose()}>{t("close")}</button></div><pre className="preview-content">{state.loading ? t("loadingPreview") : state.content || t("noPreviewContent")}</pre></div></div>;
}

function ConfirmDialog({ state, onClose }: { state: ConfirmState; onClose: (confirmed: boolean) => void }) {
  return <div className="modal-backdrop"><div className="modal-card confirm-modal"><div className="panel-header"><div><h3>{state.title}</h3><p className="muted confirm-message">{state.message}</p></div><button className="button ghost" onClick={() => onClose(false)}>{state.cancelLabel}</button></div>{state.details.length ? <div className="confirm-detail-list">{state.details.map((detail) => <div key={detail} className="confirm-detail-item">{detail}</div>)}</div> : null}<div className="button-row wrap"><button className={`button ${state.confirmTone}`} onClick={() => onClose(true)}>{state.confirmLabel}</button></div></div></div>;
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
  const hideGenericSubtitle = section === "settingDocuments" || section === "skills";
  return <section className="panel inventory-panel"><button className="inventory-toggle" onClick={() => setOpen((current) => !current)}><div><h3>{title}</h3><span className="muted">{t("itemCount", { count: items.length })}</span></div><span className="muted">{open ? t("collapse") : t("expand")}</span></button>{open ? items.length === 0 ? <div className="empty-state">{emptyText}</div> : <div className="stack">{items.map((item) => <div key={item.id} className="inventory-item"><div className="inventory-item-copy"><strong>{translateBackendText(item.title)}</strong>{!hideGenericSubtitle && item.subtitle ? <span className="muted value-text">{translateBackendText(item.subtitle)}</span> : null}{item.updatedAt ? <span className="muted">{formatTime(item.updatedAt)}</span> : null}</div><button className="mini-button" onClick={() => onPreview(section, item)}>{t("preview")}</button></div>)}</div> : null}</section>;
}

function StatusPill({ tone, label }: { tone: "good" | "warn" | "muted"; label: string }) {
  return <div className={`status-pill ${tone}`}>{label}</div>;
}
function navLabel(page: PageId) {
  return { overview: t("overview"), profiles: t("profiles"), chat: t("chat"), notifications: t("notifications"), docs: t("docs"), settings: t("settings") }[page];
}

function readableError(error: unknown, fallback: string) {
  if (error instanceof Error && error.message) return translateBackendText(error.message);
  if (typeof error === "string" && error.trim()) return translateBackendText(error);
  return fallback;
}

function gatewayLabel(status?: GatewayStatus) {
  if (!status) return t("gatewayUnchecked");
  if (status.healthy) return t("gatewayHealthy");
  if (status.running) return t("gatewayConnecting");
  return t("gatewayOffline");
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
  if (!value) return t("unknown");
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


