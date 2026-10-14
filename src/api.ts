import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebview, type DragDropEvent } from "@tauri-apps/api/webview";
import type {
  ApprovalEvent,
  ApprovalRecord,
  Appshot,
  Artifact,
  ArtifactDetail,
  ArtifactEvent,
  Attachment,
  AudioClip,
  AudioScript,
  ChatRow,
  ComputerStateEvent,
  ContextBreakdown,
  DeltaEvent,
  DesignView,
  DoneEvent,
  Egress,
  ErrorEvent,
  EnvProbe,
  FilePreview,
  FsEvent,
  GuardEvent,
  Instruction,
  InterruptedRun,
  Maintenance,
  MemoryCategoryView,
  MemoryImport,
  MemoryEntryView,
  MemoryProposal,
  McpReadyEvent,
  McpServer,
  McpStatus,
  MessagePage,
  MessageRow,
  ModelInfo,
  NotifyEvent,
  NotifyOpenEvent,
  ParkedRequest,
  PermissionMode,
  ProjectRow,
  ProjectSpend,
  PromptImport,
  PromptView,
  QuestionAnswer,
  QuestionEvent,
  QuickSentEvent,
  Recap,
  ScheduledFiredEvent,
  ScheduledTask,
  SectionKind,
  Settings,
  ShotEvent,
  SkillImport,
  SkillView,
  SpeechEstimate,
  SpendReport,
  TaskRun,
  ToolGrant,
  ToolEvent,
  ToolLog,
  TreeEntry,
  TreeListing,
  TrayEvent,
} from "./types";

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  /** Writes the whole profile to one zip; returns the path. The API key is never in it. */
  backupProfile: () => invoke<string>("backup_profile"),
  /** Unpacks a profile over this one. Reload the window afterwards. */
  restoreProfile: (path: string) => invoke<void>("restore_profile", { path }),
  /** Raise the quick-entry window — the tray's and the palette's way in. */
  showQuickEntry: () => invoke<void>("show_quick_entry"),
  /** Photograph the window that was in front when the quick window was raised. */
  captureFocusedWindow: () => invoke<Appshot>("capture_focused_window"),
  /** Sweeps the tables that grow for ever; returns what went and the size now. */
  maintainDb: () => invoke<Maintenance>("maintain_db"),

  hasApiKey: () => invoke<boolean>("has_api_key"),
  maskedApiKey: () => invoke<string | null>("masked_api_key"),
  setApiKey: (key: string) => invoke<void>("set_api_key", { key }),
  clearApiKey: () => invoke<void>("clear_api_key"),
  testConnection: () => invoke<void>("test_connection"),
  /** Check a saved endpoint by id — including one that is not the default. */
  testEndpoint: (id: string) => invoke<void>("test_endpoint", { id }),

  /** The models the default endpoint offers. */
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
  searchMessages: (query: string, offset = 0) =>
    invoke<MessagePage>("search_messages", { query, offset }),
  /** Undo the last agent run's file changes; returns the paths put back. */
  rewindLastRun: (chatId: string) => invoke<string[]>("rewind_last_run", { chatId }),
  /** Ask again from the last user turn; `text` edits it first. */
  resendLastTurn: (chatId: string, text: string | null) =>
    invoke<void>("resend_last_turn", { chatId, text }),
  /** Swap back the answer a resend replaced, at that turn. No model call. */
  restoreDiscardedTail: (chatId: string, afterSeq: number) =>
    invoke<void>("restore_discarded_tail", { chatId, afterSeq }),
  renameChat: (chatId: string, title: string) => invoke<void>("rename_chat", { chatId, title }),
  deleteChat: (chatId: string) => invoke<void>("delete_chat", { chatId }),
  /** A copy of the conversation, and of its folder when it owns one. */
  duplicateChat: (chatId: string) => invoke<ChatRow>("duplicate_chat", { chatId }),
  /** The transcript as a portable JSON file; returns the path written. */
  exportChatJson: (chatId: string) => invoke<string>("export_chat_json", { chatId }),
  /** Reads one back as a new conversation. Additive: nothing is overwritten. */
  importChatJson: (path: string) => invoke<ChatRow>("import_chat_json", { path }),
  /** Moves a chat to the sidebar's archive, or back out of it. */
  setChatArchived: (chatId: string, archived: boolean) =>
    invoke<void>("set_chat_archived", { chatId, archived }),
  /**
   * Turns a chat's "does not feed memory" switch on or off. With it on, nothing
   * said here is written to memory and the prompt carries none.
   */
  setChatIncognito: (chatId: string, incognito: boolean) =>
    invoke<void>("set_chat_incognito", { chatId, incognito }),
  setChatModel: (chatId: string, model: string, thinking: boolean, effort: string) =>
    invoke<void>("set_chat_model", { chatId, model, thinking, effort }),
  setChatPermissionMode: (chatId: string, mode: PermissionMode) =>
    invoke<void>("set_chat_permission_mode", { chatId, mode }),
  /** What this chat may send out. A declared policy holds in every mode. */
  setChatEgress: (chatId: string, egress: Egress) =>
    invoke<void>("set_chat_egress", { chatId, egress }),
  /** Chat mode only: how much rope this conversation has. */
  setChatMode: (chatId: string, mode: string) =>
    invoke<void>("set_chat_mode", { chatId, mode }),
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
  /**
   * Import a memory file another assistant exported: a heading names a category
   * and its lines become that category's notes. Additive — nothing already
   * written is overwritten, and an unknown heading falls back to `other`.
   */
  importMemory: (path: string) => invoke<MemoryImport>("import_memory", { path }),
  summarizeChat: (chatId: string) => invoke<string>("summarize_chat", { chatId }),
  compactChat: (chatId: string) => invoke<string>("compact_chat", { chatId }),

  listSkills: (chatId?: string) => invoke<SkillView[]>("list_skills", { chatId: chatId ?? null }),
  /**
   * Marks a skill as one a new chat starts with. Written into the skill's own
   * SKILL.md, so it applies to chats that do not exist yet — and leaves this one
   * exactly as it is.
   */
  setSkillDefault: (id: string, defaultOn: boolean) =>
    invoke<void>("set_skill_default", { id, defaultOn }),
  setChatSkills: (chatId: string, skillIds: string[]) =>
    invoke<void>("set_chat_skills", { chatId, skillIds }),
  skillsDir: () => invoke<string>("skills_dir"),
  installSkill: (path: string) => invoke<string>("install_skill", { path }),
  /** Install a skill from pasted text; the text names itself. */
  installSkillText: (text: string) => invoke<string>("install_skill_text", { text }),
  /** Put back any bundled example the user is missing; returns what it added. */
  installExamples: () => invoke<string[]>("install_examples"),
  /**
   * Import the `SKILL.md` folders in a folder the user picked: one skill
   * directory, or a folder of them. A name already installed is skipped, not
   * replaced, and the report names both lists.
   */
  importSkills: (path: string) => invoke<SkillImport>("import_skills", { path }),
  /** Write every skill out as `<id>/SKILL.md`; returns the ids written. */
  exportSkills: (path: string) => invoke<string[]>("export_skills", { path }),
  deleteSkill: (id: string) => invoke<void>("delete_skill", { id }),

  respondApproval: (requestId: string, decision: string) =>
    invoke<boolean>("respond_approval", { requestId, decision }),
  /** Answer the question the model asked mid-run, which resumes the run. */
  respondQuestion: (requestId: string, answer: { answers: QuestionAnswer[]; cancelled: boolean }) =>
    invoke<boolean>("respond_question", { requestId, answer }),
  listToolPermissions: (workspace: string) =>
    invoke<string[]>("list_tool_permissions", { workspace }),
  revokeToolPermission: (workspace: string, tool: string, target: string) =>
    invoke<void>("revoke_tool_permission", { workspace, tool, target }),
  /** Every stored "always allow" grant, so it can be seen and taken back. */
  toolGrants: () => invoke<ToolGrant[]>("tool_grants"),
  /** The last decisions the app made on the user's behalf, newest first. */
  recentDecisions: () => invoke<ApprovalRecord[]>("recent_decisions"),

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
  /** Bring the local SearXNG container up and point web_search at it. */
  installSearxng: () => invoke<string>("install_searxng"),

  listArtifacts: (chatId: string) => invoke<Artifact[]>("list_artifacts", { chatId }),
  /** Every artifact, from every chat — what the gallery lists. */
  listAllArtifacts: () => invoke<Artifact[]>("list_all_artifacts"),
  getArtifact: (artifactId: string, version?: number) =>
    invoke<ArtifactDetail>("get_artifact", { artifactId, version: version ?? null }),
  saveArtifactVersion: (artifactId: string, content: string) =>
    invoke<ArtifactDetail>("save_artifact_version", { artifactId, content }),
  /**
   * Rewrite one selected passage of an artifact and answer with the replacement
   * text alone. One model call, not a chat turn; the panel patches it in.
   */
  reviseSelection: (input: ReviseRequest) =>
    invoke<string>("revise_selection", { ...input }),
  /**
   * Write the script for an audio overview of a chat, and price it. Two calls
   * rather than one because the price comes before the money is spent.
   */
  audioScript: (chatId: string) => invoke<AudioScript>("audio_script", { chatId }),
  /**
   * What speaking this script costs. Pure on the other side, and called as the
   * script is edited — the price has to follow the words.
   */
  audioEstimate: (script: string) => invoke<SpeechEstimate>("audio_estimate", { script }),
  /** Read a script out loud and save the clip in the chat's folder. */
  audioOverview: (chatId: string, script: string) =>
    invoke<AudioClip>("audio_overview", { chatId, script }),
  /**
   * A recording as words. `name` carries the extension the endpoint sniffs; the
   * bytes travel because the webview is where they were recorded.
   */
  transcribeAudio: (dataUrl: string, mime: string, name: string) =>
    invoke<string>("transcribe_audio", { dataUrl, mime, name }),
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
  attachPath: (chatId: string | null, path: string) =>
    invoke<Attachment>("attach_path", { chatId, path }),
  /**
   * The same for a file the webview holds as a `File` rather than a path: its
   * bytes travel instead, and the Rust side puts it in the chat's folder.
   */
  attachBytes: (chatId: string | null, name: string, dataUrl: string) =>
    invoke<Attachment>("attach_bytes", { chatId, name, dataUrl }),
  /** Where this chat keeps its own files, worked out on demand. */
  chatFolder: (chatId: string) => invoke<string>("chat_folder", { chatId }),
  stopChat: (chatId: string) => invoke<boolean>("stop_chat", { chatId }),
  runningChats: () => invoke<string[]>("running_chats"),
  computerState: () => invoke<boolean>("computer_state"),
  setComputerPaused: (paused: boolean) => invoke<boolean>("set_computer_paused", { paused }),

  listTree: (chatId: string, path: string | null = null) =>
    invoke<TreeListing>("list_tree", { chatId, path }),
  /** Files anywhere under the chat's folder whose name contains `query`. */
  findInTree: (chatId: string, query: string) =>
    invoke<TreeEntry[]>("find_in_tree", { chatId, query }),
  readPreview: (chatId: string, path: string) =>
    invoke<FilePreview>("read_preview", { chatId, path }),
  watchWorkspace: (chatId: string) => invoke<string>("watch_workspace", { chatId }),
  unwatchWorkspace: (chatId: string) => invoke<void>("unwatch_workspace", { chatId }),

  listProjects: () => invoke<ProjectRow[]>("list_projects"),
  createProject: (name: string, folder: string | null = null) =>
    invoke<ProjectRow>("create_project", { name, folder }),
  renameProject: (id: string, name: string) => invoke<void>("rename_project", { id, name }),
  setProjectFolder: (id: string, folder: string | null) =>
    invoke<void>("set_project_folder", { id, folder }),
  deleteProject: (id: string) => invoke<void>("delete_project", { id }),
  setChatProject: (chatId: string, projectId: string | null) =>
    invoke<void>("set_chat_project", { chatId, projectId }),
  /** Scheduled runs parked on an approval or a question, answerable from here. */
  listParked: () => invoke<ParkedRequest[]>("list_parked"),
  dismissParked: (id: string) => invoke<void>("dismiss_parked", { id }),
  /** Turns a previous session left in flight — read once, at startup. */
  interruptedRuns: () => invoke<InterruptedRun[]>("interrupted_runs"),
  dismissInterrupted: (chatId: string) => invoke<void>("dismiss_interrupted", { chatId }),
  setProjectInstructions: (id: string, instructions: Instruction[]) =>
    invoke<void>("set_project_instructions", { id, instructions }),
  projectSpend: () => invoke<ProjectSpend[]>("project_spend"),
  projectMemory: (projectId: string) =>
    invoke<MemoryEntryView[]>("project_memory", { projectId }),
  /** Adds one entry to a project's own memory file, from the panel that shows it. */
  appendProjectMemory: (projectId: string, text: string) =>
    invoke<number>("append_project_memory", { projectId, text }),
  /** Forgets one entry from a project's own memory file. */
  deleteProjectMemory: (projectId: string, text: string) =>
    invoke<boolean>("delete_project_memory", { projectId, text }),

  /** Design systems: their own store and their own switch, so neither list has
   *  to know the other one to avoid switching it off. */
  listDesignSystems: (chatId?: string) =>
    invoke<DesignView[]>("list_design_systems", { chatId: chatId ?? null }),
  saveDesignSystem: (name: string, body: string) =>
    invoke<string>("save_design_system", { name, body }),
  designSystemFromProject: (folder: string) =>
    invoke<string>("design_system_from_project", { folder }),
  deleteDesignSystem: (id: string) => invoke<void>("delete_design_system", { id }),
  setChatDesigns: (chatId: string, designIds: string[]) =>
    invoke<void>("set_chat_designs", { chatId, designIds }),
  designDir: () => invoke<string>("design_dir"),

  /** Recurring tasks and their internal calendar. */
  listScheduled: () => invoke<ScheduledTask[]>("list_scheduled"),
  createScheduled: (task: ScheduledDraft) => invoke<ScheduledTask>("create_scheduled", task),
  updateScheduled: (id: string, task: ScheduledDraft & { enabled: boolean }) =>
    invoke<void>("update_scheduled", { id, ...task }),
  deleteScheduled: (id: string) => invoke<void>("delete_scheduled", { id }),
  /** Run a task now without moving when it next runs on its own. */
  runScheduledNow: (id: string) => invoke<string>("run_scheduled_now", { id }),
  /**
   * A task's recent runs, newest first — read from the same `turn_usage` rows the
   * spend view sums, so what a task has done and what it cost cannot disagree.
   */
  taskRuns: (taskId: string, limit?: number) =>
    invoke<TaskRun[]>("task_runs", { taskId, limit: limit ?? null }),

  /** What the last `days` of turns cost, grouped by day, model and chat. */
  spendReport: (days: number) => invoke<SpendReport>("spend_report", { days }),
  /** The same window's shape: days, hours and conversations. */
  recap: (days: number) => invoke<Recap>("recap", { days }),
  /**
   * The decision log and the bill as one JSON file in the documents folder.
   * Returns the path it wrote, so the notice can name the file.
   */
  exportAudit: (days: number) => invoke<string>("export_audit", { days }),

  /** Saved prompts: one markdown file each, in a folder of their own. */
  listPrompts: () => invoke<PromptView[]>("list_prompts"),
  savePrompt: (name: string, body: string) => invoke<string>("save_prompt", { name, body }),
  deletePrompt: (id: string) => invoke<void>("delete_prompt", { id }),
  promptsDir: () => invoke<string>("prompts_dir"),
  /** Import a folder of prompt markdown files; a name already saved is skipped. */
  importPrompts: (path: string) => invoke<PromptImport>("import_prompts", { path }),
};

/**
 * Everything a task's form collects. `nextRunAt` is computed on the Rust side.
 *
 * A type alias rather than an interface on purpose: `invoke` takes an indexable
 * argument, and only an alias gets the implicit index signature for it.
 */
export type ScheduledDraft = {
  title: string;
  prompt: string;
  chat_id: string | null;
  project_id: string | null;
  freq: string;
  time: string;
  weekday: number;
  nth: number;
  /**
   * What makes the task due. `"cadence"` runs it on the calendar above; `"file"`
   * and `"git"` make it wait for something to change, and then `target` is what
   * is watched and `filter` says which change counts.
   */
  kind: string;
  target: string;
  filter: string;
  interval_secs: number;
  /** The model it runs on, or `""` for the chat's own. */
  model: string;
};

/**
 * One in-place revision: the selected text, a little of what surrounds it, and
 * what should happen to it. An alias rather than an interface for the same
 * reason as `ScheduledDraft` — `invoke` takes an indexable argument.
 *
 * `chatId` rides along so the call can be priced onto the conversation that
 * asked for it: it is one model call the user did not type and does pay for.
 */
export type ReviseRequest = {
  chatId: string;
  before: string;
  selection: string;
  after: string;
  instruction: string;
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

/** The model asked the user a question mid-run, via `ask_user`. */
export function onQuestion(handler: (e: QuestionEvent) => void): Promise<UnlistenFn> {
  return listen<QuestionEvent>("chat-question", (event) => handler(event.payload));
}

/** Something done — a `notify_user` call, or a run finishing while away. */
export function onNotify(handler: (e: NotifyEvent) => void): Promise<UnlistenFn> {
  return listen<NotifyEvent>("chat-notify", (event) => handler(event.payload));
}

/** A toast was clicked, as far as the app can tell: open this conversation. */
export function onNotifyOpen(handler: (e: NotifyOpenEvent) => void): Promise<UnlistenFn> {
  return listen<NotifyOpenEvent>("notify-open", (event) => handler(event.payload));
}

/** A scheduled task fired, in a chat the sidebar may not know about yet. */
export function onScheduledFired(handler: (e: ScheduledFiredEvent) => void): Promise<UnlistenFn> {
  return listen<ScheduledFiredEvent>("scheduled-fired", (event) => handler(event.payload));
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

/** A question was asked in the quick window and handed to a conversation. */
export function onQuickSent(handler: (e: QuickSentEvent) => void): Promise<UnlistenFn> {
  return listen<QuickSentEvent>("quick-sent", (event) => handler(event.payload));
}

/** A tray menu item the window has to run — the same ids the palette uses. */
export function onTray(handler: (e: TrayEvent) => void): Promise<UnlistenFn> {
  return listen<TrayEvent>("tray-action", (event) => handler(event.payload));
}
