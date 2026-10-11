import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebview, type DragDropEvent } from "@tauri-apps/api/webview";
import type {
  ApprovalEvent,
  Artifact,
  ArtifactDetail,
  ArtifactEvent,
  Attachment,
  ChatRow,
  ComputerStateEvent,
  ContextBreakdown,
  DeltaEvent,
  DoneEvent,
  ErrorEvent,
  EnvProbe,
  FilePreview,
  FsEvent,
  GuardEvent,
  MemoryCategoryView,
  MemoryProposal,
  McpReadyEvent,
  McpServer,
  McpStatus,
  MessageHit,
  MessageRow,
  ModelInfo,
  PermissionMode,
  SectionKind,
  Settings,
  ShotEvent,
  SkillView,
  ToolGrant,
  ToolEvent,
  ToolLog,
  TreeListing,
} from "./types";

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  /** Writes one JSON bundle of settings + MCP servers; returns the path. */
  backupConfig: () => invoke<string>("backup_config"),
  /** Reads a bundle back. The API key is never in one. */
  restoreConfig: (path: string) => invoke<void>("restore_config", { path }),

  hasApiKey: () => invoke<boolean>("has_api_key"),
  maskedApiKey: () => invoke<string | null>("masked_api_key"),
  setApiKey: (key: string) => invoke<void>("set_api_key", { key }),
  clearApiKey: () => invoke<void>("clear_api_key"),
  testConnection: () => invoke<void>("test_connection"),

  listModels: () => invoke<ModelInfo[]>("list_models"),

  probeEnvironment: () => invoke<EnvProbe>("probe_environment"),

  /** What the next request to this chat would be made of, in characters. */
  contextBreakdown: (chatId: string) =>
    invoke<ContextBreakdown>("context_breakdown", { chatId }),

  listChats: (kind: SectionKind) => invoke<ChatRow[]>("list_chats", { kind }),
  createChat: (kind: SectionKind, workspace: string | null = null) =>
    invoke<ChatRow>("create_chat", { kind, workspace }),
  getMessages: (chatId: string) => invoke<MessageRow[]>("get_messages", { chatId }),
  /** Search what was said, across every conversation. */
  searchMessages: (query: string) => invoke<MessageHit[]>("search_messages", { query }),
  /** Undo the last agent run's file changes; returns the paths put back. */
  rewindLastRun: (chatId: string) => invoke<string[]>("rewind_last_run", { chatId }),
  /** Ask again from the last user turn; `text` edits it first. */
  resendLastTurn: (chatId: string, text: string | null) =>
    invoke<void>("resend_last_turn", { chatId, text }),
  renameChat: (chatId: string, title: string) => invoke<void>("rename_chat", { chatId, title }),
  deleteChat: (chatId: string) => invoke<void>("delete_chat", { chatId }),
  setChatModel: (chatId: string, model: string, thinking: boolean, effort: string) =>
    invoke<void>("set_chat_model", { chatId, model, thinking, effort }),
  setChatPermissionMode: (chatId: string, mode: PermissionMode) =>
    invoke<void>("set_chat_permission_mode", { chatId, mode }),
  /** Chat mode only: tools on gives the model file access; off is a plain chat. */
  setChatTools: (chatId: string, enabled: boolean) =>
    invoke<void>("set_chat_tools", { chatId, enabled }),
  setChatWorkspace: (chatId: string, workspace: string | null) =>
    invoke<void>("set_chat_workspace", { chatId, workspace }),

  getMemory: () => invoke<string | null>("get_memory"),
  setMemory: (body: string) => invoke<void>("set_memory", { body }),
  memoryPath: () => invoke<string>("memory_path"),
  appendMemory: (category: string, entries: string[]) =>
    invoke<number>("append_memory", { category, entries }),

  listProposals: (chatId: string, status?: string) =>
    invoke<MemoryProposal[]>("list_proposals", { chatId, status: status ?? null }),
  acceptProposals: (ids: string[]) => invoke<number>("accept_proposals", { ids }),
  rejectProposals: (ids: string[]) => invoke<void>("reject_proposals", { ids }),
  /** Every memory file, so the panel can show and prune what was kept. */
  memoryOverview: () => invoke<MemoryCategoryView[]>("memory_overview"),
  /** Forgets one line, by the text the panel shows. */
  deleteMemoryEntry: (category: string, text: string) =>
    invoke<boolean>("delete_memory_entry", { category, text }),
  summarizeChat: (chatId: string) => invoke<string>("summarize_chat", { chatId }),
  compactChat: (chatId: string) => invoke<string>("compact_chat", { chatId }),

  listSkills: (chatId?: string) => invoke<SkillView[]>("list_skills", { chatId: chatId ?? null }),
  setChatSkills: (chatId: string, skillIds: string[]) =>
    invoke<void>("set_chat_skills", { chatId, skillIds }),
  skillsDir: () => invoke<string>("skills_dir"),
  installSkill: (path: string) => invoke<string>("install_skill", { path }),
  deleteSkill: (id: string) => invoke<void>("delete_skill", { id }),

  respondApproval: (requestId: string, decision: string) =>
    invoke<boolean>("respond_approval", { requestId, decision }),
  listToolPermissions: (workspace: string) =>
    invoke<string[]>("list_tool_permissions", { workspace }),
  revokeToolPermission: (workspace: string, tool: string) =>
    invoke<void>("revoke_tool_permission", { workspace, tool }),
  /** Every stored "always allow" grant, so it can be seen and taken back. */
  toolGrants: () => invoke<ToolGrant[]>("tool_grants"),

  documentsDir: () => invoke<string>("documents_dir"),
  /** Write a whole chat to a Markdown file in the documents folder. */
  exportChat: (chatId: string) => invoke<string>("export_chat", { chatId }),
  /** Same conversation as a PDF, printed by the browser on this machine. */
  exportChatPdf: (chatId: string) => invoke<string>("export_chat_pdf", { chatId }),
  /** Open a generated document in the app that owns its type. */
  openDocument: (path: string) => invoke<void>("open_document", { path }),
  /** Show a file or folder in the system file manager. */
  revealPath: (path: string) => invoke<void>("reveal_path", { path }),
  /** Best-effort launch of Docker Desktop; the engine answers much later. */
  startDocker: () => invoke<void>("start_docker"),

  listArtifacts: (chatId: string) => invoke<Artifact[]>("list_artifacts", { chatId }),
  getArtifact: (artifactId: string, version?: number) =>
    invoke<ArtifactDetail>("get_artifact", { artifactId, version: version ?? null }),
  saveArtifactVersion: (artifactId: string, content: string) =>
    invoke<ArtifactDetail>("save_artifact_version", { artifactId, content }),

  mcpStatus: () => invoke<McpStatus>("mcp_status"),
  mcpServers: () => invoke<McpServer[]>("mcp_servers"),
  mcpSaveServers: (servers: McpServer[]) => invoke<McpStatus>("mcp_save_servers", { servers }),
  mcpReload: () => invoke<McpStatus>("mcp_reload"),
  mcpRestart: (name: string) => invoke<McpStatus>("mcp_restart", { name }),
  mcpConfigPath: () => invoke<string>("mcp_config_path"),
  mcpLogs: () => invoke<ToolLog[]>("mcp_logs"),

  sendMessage: (chatId: string, text: string, attachments: Attachment[]) =>
    invoke<void>("send_message", { chatId, text, attachments }),
  /** Read a dropped file into an attachment: a native drop gives a path, not a File. */
  attachPath: (path: string) => invoke<Attachment>("attach_path", { path }),
  stopChat: (chatId: string) => invoke<boolean>("stop_chat", { chatId }),
  runningChats: () => invoke<string[]>("running_chats"),
  computerState: () => invoke<boolean>("computer_state"),
  setComputerPaused: (paused: boolean) => invoke<boolean>("set_computer_paused", { paused }),

  listTree: (chatId: string, path: string | null = null) =>
    invoke<TreeListing>("list_tree", { chatId, path }),
  readPreview: (chatId: string, path: string) =>
    invoke<FilePreview>("read_preview", { chatId, path }),
  watchWorkspace: (chatId: string) => invoke<string>("watch_workspace", { chatId }),
  unwatchWorkspace: (chatId: string) => invoke<void>("unwatch_workspace", { chatId }),
};

export function onDelta(handler: (e: DeltaEvent) => void): Promise<UnlistenFn> {
  return listen<DeltaEvent>("chat-delta", (event) => handler(event.payload));
}

export function onTool(handler: (e: ToolEvent) => void): Promise<UnlistenFn> {
  return listen<ToolEvent>("chat-tool", (event) => handler(event.payload));
}

/** A picture from `browser_screenshot`, for the user to see. */
export function onShot(handler: (e: ShotEvent) => void): Promise<UnlistenFn> {
  return listen<ShotEvent>("chat-shot", (event) => handler(event.payload));
}

/** Computer control was stopped or started again. */
export function onComputer(handler: (e: ComputerStateEvent) => void): Promise<UnlistenFn> {
  return listen<ComputerStateEvent>("computer-state", (event) => handler(event.payload));
}

/** The model published or updated an artifact, for the side panel. */
export function onArtifact(handler: (e: ArtifactEvent) => void): Promise<UnlistenFn> {
  return listen<ArtifactEvent>("chat-artifact", (event) => handler(event.payload));
}

/**
 * Files dragged from the OS onto the window. Tauri intercepts the drop, so this
 * is not an HTML drag event: it carries paths, which the shell reads back.
 */
export function onDragDrop(handler: (e: DragDropEvent) => void): Promise<UnlistenFn> {
  return getCurrentWebview().onDragDropEvent((event) => handler(event.payload));
}

/** One MCP tool call, or a line a server wrote to stderr. */
export function onMcpLog(handler: (e: ToolLog) => void): Promise<UnlistenFn> {
  return listen<ToolLog>("mcp-log", (event) => handler(event.payload));
}

/** MCP discovery finished, so the tool list is now what it will be. */
export function onMcpReady(handler: (e: McpReadyEvent) => void): Promise<UnlistenFn> {
  return listen<McpReadyEvent>("mcp-ready", (event) => handler(event.payload));
}

export function onApproval(handler: (e: ApprovalEvent) => void): Promise<UnlistenFn> {
  return listen<ApprovalEvent>("chat-approval", (event) => handler(event.payload));
}

export function onDone(handler: (e: DoneEvent) => void): Promise<UnlistenFn> {
  return listen<DoneEvent>("chat-done", (event) => handler(event.payload));
}

export function onError(handler: (e: ErrorEvent) => void): Promise<UnlistenFn> {
  return listen<ErrorEvent>("chat-error", (event) => handler(event.payload));
}

export function onGuard(handler: (e: GuardEvent) => void): Promise<UnlistenFn> {
  return listen<GuardEvent>("chat-guard", (event) => handler(event.payload));
}

export function onFsChanged(handler: (e: FsEvent) => void): Promise<UnlistenFn> {
  return listen<FsEvent>("fs-changed", (event) => handler(event.payload));
}
