import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ApprovalEvent,
  Attachment,
  ChatRow,
  DeltaEvent,
  DoneEvent,
  ErrorEvent,
  EnvProbe,
  FilePreview,
  FsEvent,
  GuardEvent,
  MemoryProposal,
  McpServer,
  McpStatus,
  MessageRow,
  ModelInfo,
  PermissionMode,
  SectionKind,
  Settings,
  SkillView,
  ToolEvent,
  TreeListing,
} from "./types";

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),

  hasApiKey: () => invoke<boolean>("has_api_key"),
  maskedApiKey: () => invoke<string | null>("masked_api_key"),
  setApiKey: (key: string) => invoke<void>("set_api_key", { key }),
  clearApiKey: () => invoke<void>("clear_api_key"),
  testConnection: () => invoke<void>("test_connection"),

  listModels: () => invoke<ModelInfo[]>("list_models"),

  probeEnvironment: () => invoke<EnvProbe>("probe_environment"),

  listChats: (kind: SectionKind) => invoke<ChatRow[]>("list_chats", { kind }),
  createChat: (kind: SectionKind, workspace: string | null = null) =>
    invoke<ChatRow>("create_chat", { kind, workspace }),
  getMessages: (chatId: string) => invoke<MessageRow[]>("get_messages", { chatId }),
  renameChat: (chatId: string, title: string) => invoke<void>("rename_chat", { chatId, title }),
  deleteChat: (chatId: string) => invoke<void>("delete_chat", { chatId }),
  setChatModel: (chatId: string, model: string, thinking: boolean, effort: string) =>
    invoke<void>("set_chat_model", { chatId, model, thinking, effort }),
  setChatPermissionMode: (chatId: string, mode: PermissionMode) =>
    invoke<void>("set_chat_permission_mode", { chatId, mode }),
  setChatWorkspace: (chatId: string, workspace: string | null) =>
    invoke<void>("set_chat_workspace", { chatId, workspace }),

  getMemory: () => invoke<string | null>("get_memory"),
  setMemory: (body: string) => invoke<void>("set_memory", { body }),
  memoryPath: () => invoke<string>("memory_path"),
  appendMemory: (entries: string[]) => invoke<void>("append_memory", { entries }),

  listProposals: (chatId: string, status?: string) =>
    invoke<MemoryProposal[]>("list_proposals", { chatId, status: status ?? null }),
  acceptProposals: (ids: string[]) => invoke<number>("accept_proposals", { ids }),
  rejectProposals: (ids: string[]) => invoke<void>("reject_proposals", { ids }),
  summarizeChat: (chatId: string) => invoke<string>("summarize_chat", { chatId }),
  compactChat: (chatId: string) => invoke<string>("compact_chat", { chatId }),

  listSkills: (chatId?: string) => invoke<SkillView[]>("list_skills", { chatId: chatId ?? null }),
  setChatSkills: (chatId: string, skillIds: string[]) =>
    invoke<void>("set_chat_skills", { chatId, skillIds }),

  respondApproval: (requestId: string, decision: string) =>
    invoke<boolean>("respond_approval", { requestId, decision }),
  listToolPermissions: (workspace: string) =>
    invoke<string[]>("list_tool_permissions", { workspace }),
  revokeToolPermission: (workspace: string, tool: string) =>
    invoke<void>("revoke_tool_permission", { workspace, tool }),

  documentsDir: () => invoke<string>("documents_dir"),

  mcpStatus: () => invoke<McpStatus>("mcp_status"),
  mcpServers: () => invoke<McpServer[]>("mcp_servers"),
  mcpSaveServers: (servers: McpServer[]) => invoke<McpStatus>("mcp_save_servers", { servers }),
  mcpReload: () => invoke<McpStatus>("mcp_reload"),
  mcpRestart: (name: string) => invoke<McpStatus>("mcp_restart", { name }),
  mcpConfigPath: () => invoke<string>("mcp_config_path"),

  sendMessage: (chatId: string, text: string, attachments: Attachment[]) =>
    invoke<void>("send_message", { chatId, text, attachments }),
  stopChat: (chatId: string) => invoke<boolean>("stop_chat", { chatId }),
  runningChats: () => invoke<string[]>("running_chats"),

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
