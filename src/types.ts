export type SectionKind = "chat" | "coding";

export interface Attachment {
  name: string;
  mime: string;
  /** Inline `data:` URL — the frontend downscales images before sending. */
  data_url: string;
}

export interface ChatRow {
  id: string;
  title: string;
  kind: SectionKind;
  workspace: string | null;
  model: string;
  thinking: boolean;
  effort: string;
  /** `manual` asks before every write and command; `auto` and `skip` do not. */
  permission_mode: PermissionMode;
  created_at: number;
  updated_at: number;
}

export type PermissionMode = "manual" | "auto" | "skip";

export interface MessageRow {
  id: string;
  chat_id: string;
  seq: number;
  role: "system" | "user" | "assistant" | "tool";
  content: string | null;
  reasoning_content: string | null;
  tool_calls_json: string | null;
  tool_call_id: string | null;
  attachments_json: string | null;
  created_at: number;
}

export interface ModelInfo {
  id: string;
  label: string;
  context: number;
  max_output: number;
}

export interface Instruction {
  id: string;
  name: string;
  /** `chat` | `coding` | `both` */
  scope: string;
  body: string;
  enabled: boolean;
}

export interface Settings {
  baseUrl: string;
  chatModel: string;
  codingModel: string;
  systemPrompt: string;
  thinking: boolean;
  effort: string;
  theme: string;
  documentsDir: string;
  instructions: Instruction[];
  /** Master switch for fetch_url / browser_read / web_search. */
  browsingEnabled: boolean;
  /** Base URL of a SearXNG instance with JSON output enabled. Empty disables web search. */
  searxngUrl: string;
  /** Explicit Chromium-family browser. Empty auto-detects Edge, then Chrome. */
  browserPath: string;
  /** Permission mode new chats start in. */
  permissionMode: PermissionMode;
  /** Set once the first-run setup screen has been finished or dismissed. */
  setupDone: boolean;
}

/** What the first-run setup screen reports about this machine. */
export interface EnvProbe {
  hasApiKey: boolean;
  /** `docker --version` — the CLI is on PATH. */
  dockerCli: boolean;
  /** `docker info` — the engine answers. False when Docker Desktop is stopped. */
  dockerRunning: boolean;
  wsl: boolean;
  searxngUrl: string;
  searxngOk: boolean;
}

/** One MCP server, as stored in mcp.json. */
export interface McpServer {
  name: string;
  command: string;
  args: string[];
  env: Record<string, string>;
  enabled: boolean;
  /** Run this server's readOnlyHint tools without asking. Off by default. */
  trustReadOnly: boolean;
}

export interface McpServerStatus {
  name: string;
  enabled: boolean;
  running: boolean;
  toolCount: number;
  /** Why the server is not running, when it is not. */
  error: string | null;
}

export interface McpStatus {
  servers: McpServerStatus[];
  configError: string | null;
  toolCount: number;
  runningCount: number;
  configPath: string;
}

export interface Usage {
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
  prompt_cache_hit_tokens: number;
  prompt_cache_miss_tokens: number;
}

export interface MemoryProposal {
  id: string;
  chat_id: string;
  text: string;
  status: string;
  created_at: number;
}

export interface SkillView {
  id: string;
  name: string;
  description: string;
  enabled: boolean;
}

export type ToolClassName = "read" | "internal" | "write" | "command";

export interface DeltaEvent {
  chatId: string;
  kind: "content" | "reasoning";
  text: string;
}

export interface ToolEvent {
  chatId: string;
  phase: "requested" | "finished";
  id: string;
  tool: string;
  class?: ToolClassName;
  args?: unknown;
  ok?: boolean;
  output?: string;
}

export interface ApprovalEvent {
  requestId: string;
  chatId: string;
  tool: string;
  class: ToolClassName;
  args: Record<string, unknown>;
  workspace: string;
}

export interface DoneEvent {
  chatId: string;
  messageId: string;
  usage: Usage | null;
  costUsd: number | null;
  finishReason: string | null;
}

export interface ErrorEvent {
  chatId: string;
  message: string;
}

/** One row of the project tree. */
export interface TreeEntry {
  name: string;
  /** Workspace-relative, `/` separated. */
  path: string;
  isDir: boolean;
  size: number;
}

/** One directory's worth of tree rows. */
export interface TreeListing {
  root: string;
  path: string;
  entries: TreeEntry[];
}

/** A read-only peek at one file. */
export interface FilePreview {
  path: string;
  size: number;
  text: string | null;
  /** Why there is no text: binary, or over the preview cap. */
  note: string | null;
}

/** Emitted when the watched workspace changes on disk. */
export interface FsEvent {
  paths: string[];
}

/** Emitted when the safety guard refuses a call, or drops a chat back to asking. */
export interface GuardEvent {
  chatId: string;
  tool?: string;
  args?: unknown;
  reason: string;
  mode?: PermissionMode;
  escalated?: boolean;
}

/** A tool call the UI is rendering. */
export interface ToolRun {
  id: string;
  tool: string;
  args: unknown;
  status: "running" | "ok" | "error";
  output?: string;
}
