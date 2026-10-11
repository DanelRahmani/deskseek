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
  created_at: number;
  updated_at: number;
}

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

export type ToolClassName = "read" | "write" | "command";

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

/** A tool call the UI is rendering. */
export interface ToolRun {
  id: string;
  tool: string;
  args: unknown;
  status: "running" | "ok" | "error";
  output?: string;
}
