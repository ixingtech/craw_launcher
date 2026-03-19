import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  ChatRequest,
  Conversation,
  ConversationSummary,
  ExportProfileRequest,
  GatewayConfig,
  GatewayMode,
  GatewayStatus,
  ImportVerification,
  LaunchHandle,
  ManagedProfile,
  NotificationItem,
  PackageMeta,
  ProfileInventory,
  ProfileItemPreview,
  PathCandidate,
  ValidationInput,
  ValidationResult
} from "./types";

export const api = {
  detectOpenclaw: () => invoke<PathCandidate[]>("detect_openclaw"),
  validateOpenclawPath: (input: ValidationInput) =>
    invoke<ValidationResult>("validate_openclaw_path", { input }),
  loadSettings: () => invoke<AppSettings>("load_settings"),
  saveSettings: (settings: AppSettings) =>
    invoke<AppSettings>("save_settings", { settings }),
  listProfiles: () => invoke<ManagedProfile[]>("list_profiles"),
  inspectProfile: (profileId: string) => invoke<ProfileInventory>("inspect_profile", { profileId }),
  listNotifications: (profileId: string) => invoke<NotificationItem[]>("list_notifications", { profileId }),
  previewProfileItem: (profileId: string, section: string, itemId: string) =>
    invoke<ProfileItemPreview>("preview_profile_item", { profileId, section, itemId }),
  readProfileReadme: (profileId: string) =>
    invoke<ProfileItemPreview | null>("read_profile_readme", { profileId }),
  saveProfileReadme: (profileId: string, content: string) =>
    invoke<ProfileItemPreview>("save_profile_readme", { profileId, content }),
  exportProfile: (request: ExportProfileRequest) =>
    invoke<PackageMeta>("export_profile", { request }),
  verifyImportPackage: (zipPath: string) =>
    invoke<ImportVerification>("verify_import_package", { zipPath }),
  importProfile: (zipPath: string, requestedName?: string | null, ignoreVerification?: boolean) =>
    invoke<ManagedProfile>("import_profile", { zipPath, requestedName, ignoreVerification }),
  renameProfile: (profileId: string, name: string) =>
    invoke<ManagedProfile>("rename_profile", { profileId, name }),
  deleteProfile: (profileId: string) => invoke<void>("delete_profile", { profileId }),
  launchOpenclaw: (profileId: string) =>
    invoke<LaunchHandle>("launch_openclaw", { profileId }),
  gatewayStatus: () => invoke<GatewayStatus>("gateway_status"),
  startGateway: (mode: GatewayMode, gatewayConfig: GatewayConfig) =>
    invoke<GatewayStatus>("start_gateway", { mode, gatewayConfig }),
  stopGateway: () => invoke<GatewayStatus>("stop_gateway"),
  listConversationSummaries: () => invoke<ConversationSummary[]>("list_conversation_summaries"),
  getConversation: (conversationId: string) => invoke<Conversation>("get_conversation", { conversationId }),
  saveConversation: (conversation: Conversation) =>
    invoke<Conversation>("save_conversation", { conversation }),
  deleteConversation: (conversationId: string) =>
    invoke<void>("delete_conversation", { conversationId }),
  sendChatMessage: (conversationId: string, request: ChatRequest) =>
    invoke<void>("send_chat_message", { conversationId, request }),
  openControlWeb: (profileId: string) => invoke<void>("open_control_web", { profileId }),
  openExternalUrl: (url: string) => invoke<void>("open_external_url", { url }),
  openLobsterTerminal: (profileId: string) => invoke<void>("open_lobster_terminal", { profileId }),
  pickOpenclawExecutable: () => invoke<string | null>("pick_openclaw_executable"),
  pickDirectory: () => invoke<string | null>("pick_directory"),
  desktopPath: () => invoke<string | null>("desktop_path"),
  pickZipFile: () => invoke<string | null>("pick_zip_file"),
  pickSaveZipPath: (defaultName?: string | null) =>
    invoke<string | null>("pick_save_zip_path", { defaultName })
};
