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
  /**
   * The folder this chat owns for its own files. Decided when the chat first
   * touches a document, so it is null until then — `chatFolder` in the API
   * answers with the real path either way.
   */
  folder: string | null;
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
  /**
   * This chat does not feed memory: `save_memory` writes nothing from it and its
   * prompt carries no memory at all.
   */
  incognito: boolean;
  /** The project this chat belongs to, or null. Membership only. */
  project_id: string | null;
  created_at: number;
  updated_at: number;
}

/**
 * A named container: a folder to work in, and the chats that belong to it.
 * Deleting one never deletes its chats — they are only unassigned.
 */
export interface ProjectRow {
  id: string;
  name: string;
  folder: string | null;
  /**
   * Rules every chat in the project inherits — set on the project rather than
   * copied into each chat, so one edit reaches all of them. Empty means none.
   */
  instructions: Instruction[];
  created_at: number;
  updated_at: number;
}

/** All-time spend across a project's chats, from `project_spend`. */
export interface ProjectSpend {
  projectId: string;
  costUsd: number;
  turns: number;
}

/**
 * One recurring task: a prompt and when to run it. The fields that describe the
 * schedule are `freq` and `time`, plus `weekday`/`nth` for the weekly and monthly
 * shapes, so "the first Wednesday of the month at 08:00" is one description
 * rather than a timestamp that has to be recomputed on every edit.
 */
export interface ScheduledTask {
  id: string;
  title: string;
  prompt: string;
  /** The chat it runs in, or null to start a fresh one each time. */
  chat_id: string | null;
  project_id: string | null;
  /** `hourly` | `daily` | `weekdays` | `weekly` | `monthly` */
  freq: string;
  /** Local wall-clock "HH:MM". An hourly task reads only the minute. */
  time: string;
  /** 0 = Monday. Read only by the weekly and monthly shapes. */
  weekday: number;
  /** 1 = first … 4 = fourth. Read only by the monthly shape. */
  nth: number;
  enabled: boolean;
  next_run_at: number;
  last_run_at: number | null;
  created_at: number;
}

/**
 * One run of a scheduled task, as the panel lists them.
 *
 * Read from the same `turn_usage` rows the spend view sums. A run that spent
 * nothing is not here — it never produced a priced round — which is the same
 * rule the spend view follows rather than a gap of its own.
 */
export interface TaskRun {
  run_id: string;
  chat_id: string;
  /** The chat it landed in, joined; empty when that chat has since been deleted. */
  chat_title: string;
  model: string;
  rounds: number;
  prompt_tokens: number;
  completion_tokens: number;
  cost_usd: number;
  created_at: number;
}

/** A `notify_user` ping, or a run finishing while the user was away. */
export interface NotifyEvent {
  chatId: string | null;
  title: string;
  body: string;
}

/** A scheduled task fired, so the sidebar and tabs need reloading. */
export interface ScheduledFiredEvent {
  taskId: string;
  chatId: string;
}

/** One stored "always allow" grant: a tool, in one folder. */
export interface ToolGrant {
  workspace: string;
  tool: string;
}

/**
 * One line of the approval audit trail: a decision the app made on the user's
 * behalf, with the hash of the arguments it was made about.
 */
export interface ApprovalRecord {
  id: string;
  chatId: string;
  /** The chat's title, joined rather than copied. */
  chatTitle: string;
  tool: string;
  /** `allowed` | `denied` | `blocked`. */
  decision: string;
  /** How or why: `once`, `session`, `always`, `stopped`, the guard's reason… */
  note: string;
  argsDigest: string;
  createdAt: number;
}

/** One message that matched a full-text search, with its match in context. */
export interface MessageHit {
  chatId: string;
  chatTitle: string;
  chatKind: SectionKind;
  messageId: string;
  role: string;
  /** The matched window, with the search terms bracketed: `the [pars]er lives`. */
  snippet: string;
}

/**
 * One page of search results. `more` is the backend saying another page exists,
 * so the list can offer it without knowing how many hits there are in total.
 */
export interface MessagePage {
  hits: MessageHit[];
  more: boolean;
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
  /** This turn has an answer in the discarded pile that can be brought back. */
  discarded?: boolean;
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
  /** Master switch for OS notifications. Off silences the toast, not the notice. */
  notifications: boolean;
  /** Raise a toast only when the window is not in front of the user. */
  notifyOnlyWhenUnfocused: boolean;
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

/** What an imported memory file did: notes newly written, and where they landed. */
export interface MemoryImport {
  added: number;
  categories: string[];
}

/**
 * A stored style reference, in its own store rather than the skills folder.
 *
 * Deliberately not a `SkillView`: there is no `default_on` (a design system is
 * opted into a chat deliberately) and no description of its own.
 */
export interface DesignView {
  id: string;
  name: string;
  /** Switched on for the chat the hub is looking at. */
  enabled: boolean;
}

export interface SkillView {
  id: string;
  name: string;
  description: string;
  /** Switched on for the chat the panel is looking at. */
  enabled: boolean;
  /**
   * `default: on` in the skill's own file: a *new* chat starts with it switched
   * on. A different question from `enabled`, which is about this chat.
   */
  default_on: boolean;
}

/** What an import folder did: what landed, and what was already here. */
export interface SkillImport {
  imported: string[];
  skipped: string[];
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
  /** A caveat about the call itself, shown on its card before it runs. */
  note?: string | null;
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

/** One choice the model offered when it asked a question. */
export interface QuestionOption {
  label: string;
  description: string;
}

/** One question the model asked mid-run, via the `ask_user` tool. */
export interface Question {
  /** A short label, at most 12 characters, e.g. "Framework". */
  header: string;
  question: string;
  options: QuestionOption[];
  multiSelect: boolean;
}

/** The model asked the user something; the run is parked until it is answered. */
export interface QuestionEvent {
  chatId: string;
  requestId: string;
  questions: Question[];
}

/** One answer, tied to its question by index so a sparse reply still lines up. */
export interface QuestionAnswer {
  index: number;
  selected: string[];
  freeform: string;
}

/** How a parked request stands. `lost` is a park the last session left behind. */
export type ParkedStatus = "pending" | "lost";

interface ParkedBase {
  id: string;
  chatId: string;
  /** The chat's title, joined rather than copied. */
  chatTitle: string;
  status: ParkedStatus;
  createdAt: number;
}

/**
 * A scheduled run parked on a decision, as the Scheduled panel lists it.
 *
 * `payload` is the event the chat itself received, so the panel draws the same
 * prompt the composer would — one description of the request rather than two
 * that could disagree. A `lost` row belongs to a run that died with the process:
 * it can be read and dismissed, but there is nothing left to answer.
 */
export type ParkedRequest =
  | (ParkedBase & { kind: "approval"; payload: ApprovalEvent })
  | (ParkedBase & { kind: "question"; payload: QuestionEvent });

/**
 * What a database sweep did, and how big the database is now.
 */
export interface Maintenance {
  /** Rows nothing can reach any more, across both pruned tables. */
  pruned: number;
  bytes: number;
}

/**
 * A turn the previous session left in flight. Read once at startup: the marker
 * exists for exactly as long as a run does, so anything found here belongs to a
 * process that died mid-turn and never got to clear it.
 */
export interface InterruptedRun {
  chatId: string;
  runId: string;
  startedAt: number;
  /** True when that run left file pre-images, so *Put its files back* would do something. */
  canRewind: boolean;
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
export type ArtifactKind = "html" | "svg" | "mermaid" | "chart" | "markdown" | "code";

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

/**
 * Spend figures. The Rust structs behind these are
 * `#[serde(rename_all = "camelCase")]`, like every other payload this app
 * receives. A snake_case field here is `undefined` at runtime — a type cannot
 * check the wire — and the panel then takes the whole window down formatting
 * it. `db.rs` has a test that asserts these keys.
 */
export interface DaySpend {
  day: string;
  costUsd: number;
  turns: number;
  promptTokens: number;
  completionTokens: number;
}

export interface ModelSpend {
  model: string;
  costUsd: number;
  turns: number;
}

export interface ChatSpend {
  chatId: string;
  title: string;
  costUsd: number;
  turns: number;
}

/**
 * One section's share of the tokens.
 *
 * The two kinds behave differently: a coding chat reading files burns input, a
 * conversation burns output — the same total, a different problem, and only one
 * of them is fixed by a cheaper prompt. Together the sections *are* the report's
 * own totals, which is what lets the panel draw them as slices of it.
 */
export interface SectionTokens {
  /** `chat` | `coding` */
  kind: string;
  turns: number;
  promptTokens: number;
  completionTokens: number;
}

/**
 * What a window of turns cost, grouped the four ways the panel shows it. One
 * shape for one round trip, so the panel cannot show a total from one moment and
 * a breakdown from another.
 */
export interface SpendReport {
  /** The window these figures cover, as a Unix timestamp. */
  since: number;
  totalUsd: number;
  turns: number;
  promptTokens: number;
  completionTokens: number;
  /** Newest day first, and only days that actually had spend. */
  days: DaySpend[];
  /** Dearest first. */
  models: ModelSpend[];
  chats: ChatSpend[];
  sections: SectionTokens[];
}

/**
 * A saved prompt: a request kept as a markdown file, for the composer rather
 * than for the model.
 */
export interface PromptView {
  /** The file's stem, a bare slug — `/p:<id>` is how it is reached. */
  id: string;
  name: string;
  /** The whole prompt, because the composer's job is to put it in the box. */
  body: string;
}

/** What an imported prompts folder did: what landed, and what was already saved. */
export interface PromptImport {
  imported: string[];
  skipped: string[];
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
  /**
   * A caveat about the call itself, from the backend. `run_command` carries one
   * because a shell command can change files `/rewind` cannot put back.
   */
  note?: string;
}
