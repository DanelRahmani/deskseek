export type SectionKind = "chat" | "coding";

export interface Attachment {
  name: string;
  mime: string;
  /** Inline `data:` URL, set for images only — the frontend downscales them. */
  data_url: string;
  /** Decoded contents, set for text files only (the model is text-only). */
  text?: string;
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
  /** Accumulated USD spent across this chat's turns. */
  cost_usd: number;
  /** How much rope this chat has: `ask`, `standard`, `plan` or `agent`. */
  agent_mode: string;
  /** Out of the sidebar's way. Nothing else about the chat changes. */
  archived: boolean;
  created_at: number;
  updated_at: number;
}

/** One stored "always allow" grant: a tool, in one folder. */
export interface ToolGrant {
  workspace: string;
  tool: string;
}

/** One message that matched a full-text search, with its match in context. */
export interface MessageHit {
  chatId: string;
  chatTitle: string;
  chatKind: SectionKind;
  messageId: string;
  role: string;
  /** The matched window, with the search terms bracketed. */
  snippet: string;
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
  /** `system` follows the OS; `dark` and `light` override it. */
  themeMode: string;
  /** Accent colour as `#rrggbb`, replacing the theme's own. Empty keeps it. */
  accent: string;
  /** UI font family. Empty uses the built-in stack. */
  fontUi: string;
  /** Code font family. Empty uses the built-in stack. */
  fontCode: string;
  /** Base UI font size in px. 0 uses the default. */
  fontSize: number;
  /** Suppress transitions and animations. */
  reduceMotion: boolean;
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
  /** Stop a run after it has spent this many USD. 0 disables the limit. */
  costLimitUsd: number;
  /** Where shell commands run. `auto` takes the strongest rung this machine can reach. */
  sandbox: SandboxMode;
  /** Image the Docker rung runs commands in. It must carry the workspace's toolchain. */
  sandboxImage: string;
  /** Master switch for reading the desktop (`ui_snapshot`). Off by default. */
  computerUse: boolean;
  /** Set once the first-run setup screen has been finished or dismissed. */
  setupDone: boolean;
}

export type SandboxMode = "auto" | "host" | "docker" | "wsl";

/** Where a shell command actually runs. */
export type SandboxRung = "host" | "docker" | "wsl";

/** One measured piece of a request, in characters. */
export interface ContextPart {
  label: string;
  /** Which slice of the bar this belongs to. */
  group: string;
  chars: number;
}

export interface ContextBreakdown {
  parts: ContextPart[];
  /** Pictures in the history. Charged per picture, so counted rather than measured. */
  images: number;
}

/** What the first-run setup screen reports about this machine. */
export interface EnvProbe {
  hasApiKey: boolean;
  /** `docker --version` — the CLI is on PATH. */
  dockerCli: boolean;
  /** `docker info` — the engine answers. False when Docker Desktop is stopped. */
  dockerRunning: boolean;
  /** What `docker --version` printed, when the CLI answered at all. */
  dockerVersion: string;
  /** Docker Desktop's launcher was found, so the engine can be started here. */
  dockerDesktop: boolean;
  wsl: boolean;
  /** The distribution commands would run in, when one is installed. */
  wslDistro: string;
  /** The rung the current setting resolves to — what will really happen. */
  sandboxRung: SandboxRung;
  /** Set when a pinned rung could not be honoured, and why. */
  sandboxNote: string | null;
  /** "Edge" / "Chrome" / "Chromium", when a browser was found. */
  browserName: string;
  /** The executable browser_read will drive. Empty when none was found. */
  browserPath: string;
  searxngUrl: string;
  searxngOk: boolean;
}

/** One MCP server, as stored in mcp.json. */
export interface McpServer {
  name: string;
  /** A local server: the executable to launch. Empty for a remote one. */
  command: string;
  args: string[];
  env: Record<string, string>;
  /** A remote server: the streamable-HTTP endpoint, set instead of `command`. */
  url?: string;
  /** Extra HTTP headers for a remote server, e.g. an Authorization bearer. */
  headers: Record<string, string>;
  enabled: boolean;
  /** Run this server's readOnlyHint tools without asking. Off by default. */
  trustReadOnly: boolean;
}

/** One line in a server's log: a tool call we made, or a line it wrote to stderr. */
export interface ToolLog {
  server: string;
  at: string;
  kind: "call" | "stderr";
  tool: string;
  /** The redacted arguments, calls only. */
  args: string;
  /** A stderr line, or why the call failed. */
  detail: string;
  ms: number;
  /** Characters returned, calls only. */
  size: number;
  ok: boolean;
}

/** Emitted once discovery finishes, so the UI knows what the model can reach. */
export interface McpReadyEvent {
  toolCount: number;
  runningCount: number;
  failedCount: number;
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
  /** `profile` (MEMORY.md) or a category name — `work`, `interests`, and so on. */
  target: string;
  created_at: number;
}

/** One remembered line, as it is written into a category file. */
export interface MemoryEntryView {
  /** `YYYY-MM-DD`, as recorded in the file. */
  date: string;
  text: string;
}

/**
 * One memory file: the category, what it is for, and what is in it. The profile is
 * not one of these — `MEMORY.md` has its own editor, because it is sent with every
 * request rather than looked up.
 */
export interface MemoryCategoryView {
  /** A category id, or `project-<name>` for a file a project folder added. */
  category: string;
  about: string;
  entries: MemoryEntryView[];
}

export interface SkillView {
  id: string;
  name: string;
  description: string;
  enabled: boolean;
}

export type ToolClassName = "read" | "internal" | "write" | "command" | "computer";

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

/**
 * A picture taken by `browser_screenshot`, on its own event. The tool result the
 * model receives stays text — this is for the user's eyes.
 */
export interface ShotEvent {
  chatId: string;
  /** The tool call it belongs to, so it lands on the right card. */
  callId: string;
  dataUrl: string;
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

/** Emitted when computer control is stopped or started again. */
export interface ComputerStateEvent {
  paused: boolean;
  /** Why it stopped, or null when it is running. */
  reason: string | null;
}

/** What an artifact's content is, which picks its renderer. */
export type ArtifactKind = "html" | "svg" | "mermaid" | "markdown" | "code";

export interface Artifact {
  id: string;
  chat_id: string;
  message_id: string | null;
  kind: ArtifactKind;
  title: string;
  current_version: number;
  created_at: number;
  updated_at: number;
}

export interface ArtifactVersion {
  version: number;
  /** `assistant` when the model wrote it, `user` when it was edited here. */
  created_by: string;
  created_at: number;
}

/** An artifact and one version of its content — the current one unless asked. */
export interface ArtifactDetail {
  artifact: Artifact;
  content: string;
  /** Newest first, which is the order the dropdown shows. */
  versions: ArtifactVersion[];
}

/** Emitted when the model publishes or updates an artifact. */
export interface ArtifactEvent {
  chatId: string;
  artifact: ArtifactDetail;
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
  /** A `data:` URL picture, for `browser_screenshot`. */
  shot?: string;
}
