import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ChatRow,
  DeltaEvent,
  DoneEvent,
  ErrorEvent,
  MessageRow,
  ModelInfo,
  SectionKind,
  Settings,
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

  listChats: (kind: SectionKind) => invoke<ChatRow[]>("list_chats", { kind }),
  createChat: (kind: SectionKind, workspace: string | null = null) =>
    invoke<ChatRow>("create_chat", { kind, workspace }),
  getMessages: (chatId: string) => invoke<MessageRow[]>("get_messages", { chatId }),
  renameChat: (chatId: string, title: string) => invoke<void>("rename_chat", { chatId, title }),
  deleteChat: (chatId: string) => invoke<void>("delete_chat", { chatId }),
  setChatModel: (chatId: string, model: string, thinking: boolean, effort: string) =>
    invoke<void>("set_chat_model", { chatId, model, thinking, effort }),

  sendMessage: (chatId: string, text: string) => invoke<void>("send_message", { chatId, text }),
};

export function onDelta(handler: (e: DeltaEvent) => void): Promise<UnlistenFn> {
  return listen<DeltaEvent>("chat-delta", (event) => handler(event.payload));
}

export function onDone(handler: (e: DoneEvent) => void): Promise<UnlistenFn> {
  return listen<DoneEvent>("chat-done", (event) => handler(event.payload));
}

export function onError(handler: (e: ErrorEvent) => void): Promise<UnlistenFn> {
  return listen<ErrorEvent>("chat-error", (event) => handler(event.payload));
}
