export type SectionKind = "chat" | "coding";

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
  created_at: number;
}

export interface ModelInfo {
  id: string;
  label: string;
  context: number;
  max_output: number;
}

export interface Settings {
  baseUrl: string;
  chatModel: string;
  codingModel: string;
  systemPrompt: string;
  thinking: boolean;
  effort: string;
  theme: string;
}

export interface Usage {
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
  prompt_cache_hit_tokens: number;
  prompt_cache_miss_tokens: number;
}

export interface DeltaEvent {
  chatId: string;
  kind: "content" | "reasoning";
  text: string;
}

export interface DoneEvent {
  chatId: string;
  messageId: string;
  usage: Usage;
  costUsd: number | null;
  finishReason: string | null;
}

export interface ErrorEvent {
  chatId: string;
  message: string;
}
