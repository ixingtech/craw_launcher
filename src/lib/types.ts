export type GatewayMode = "auto" | "manual";

export interface ValidationInput {
  executablePath?: string | null;
  dataDir?: string | null;
}

export interface ValidationResult {
  executablePath?: string | null;
  installDir?: string | null;
  inferredDataDir?: string | null;
  supportsProfileSwitch: boolean;
  isValid: boolean;
  issues: string[];
}

export interface PathCandidate {
  executablePath: string;
  dataDir?: string | null;
  source: string;
  score: number;
  runtimeKind: "windows" | "wsl";
  wslDistro?: string | null;
  validation: ValidationResult;
}

export interface RuntimeTargetConfig {
  kind: "windows" | "wsl";
  wslDistro?: string | null;
  wslOpenclawPath?: string | null;
  wslDataDir?: string | null;
}

export interface LaunchRecord {
  profileId: string;
  profileName: string;
  launchedAt: string;
}

export interface GatewayConfig {
  mode: GatewayMode;
  command?: string | null;
  args: string[];
  url: string;
  healthEndpoint: string;
}

export interface AppSettings {
  openclawExecutablePath?: string | null;
  openclawDataDir?: string | null;
  runtimeTarget: RuntimeTargetConfig;
  profilesRoot?: string | null;
  closeLaunchedProfilesOnExit: boolean;
  gatewayConfig: GatewayConfig;
  recentProfileId?: string | null;
  recentLaunches: LaunchRecord[];
}

export interface PackageMeta {
  packageName: string;
  zipPath: string;
  sourceDir: string;
  exportedAt: string;
  fileCount: number;
  includeMemory: boolean;
  includeAccountInfo: boolean;
}

export interface ExportProfileRequest {
  sourceDir: string;
  zipPath?: string | null;
  packageName?: string | null;
  includeMemory?: boolean;
  includeAccountInfo?: boolean;
}

export interface ManagedProfile {
  id: string;
  name: string;
  path: string;
  importedFrom?: string | null;
  createdAt: string;
  lastUsedAt?: string | null;
}

export interface ImportVerification {
  valid: boolean;
  packageName?: string | null;
  exportedAt?: string | null;
  issues: string[];
}

export interface ProfileListItem {
  id: string;
  title: string;
  subtitle: string;
  updatedAt?: string | null;
}

export interface ProfileItemPreview {
  title: string;
  subtitle: string;
  content: string;
  updatedAt?: string | null;
}

export interface ProfileInventory {
  settingDocuments: ProfileListItem[];
  skills: ProfileListItem[];
  cronJobs: ProfileListItem[];
  memories: ProfileListItem[];
  accounts: ProfileListItem[];
}

export interface NotificationItem {
  id: string;
  title: string;
  subtitle: string;
  content: string;
  createdAt: string;
  kind: string;
}

export interface LaunchHandle {
  pid?: number | null;
  startedAt: string;
  profileId: string;
  profileName: string;
  executablePath: string;
  args: string[];
  connectionMessage?: string | null;
}

export interface GatewayStatus {
  mode: GatewayMode;
  url: string;
  profileId?: string | null;
  running: boolean;
  pid?: number | null;
  startedAt?: string | null;
  healthy: boolean;
  lastError?: string | null;
  logTail: string[];
}

export interface ChatMessage {
  id: string;
  role: "system" | "user" | "assistant";
  content: string;
  createdAt: string;
}

export interface Conversation {
  id: string;
  title: string;
  createdAt: string;
  updatedAt: string;
  messages: ChatMessage[];
}

export interface ConversationSummary {
  id: string;
  title: string;
  createdAt: string;
  updatedAt: string;
}

export interface ChatRequest {
  conversationId: string;
  content: string;
  profileId?: string | null;
  model?: string | null;
  params?: Record<string, unknown> | null;
}

export interface ChatDeltaEvent {
  conversationId: string;
  content: string;
}

export interface ChatDoneEvent {
  conversationId: string;
  conversation: Conversation;
}

export interface ChatConversationEvent {
  conversationId: string;
  conversation: Conversation;
}

export interface ChatErrorEvent {
  conversationId: string;
  error: string;
}
