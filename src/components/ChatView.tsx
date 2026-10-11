import { useEffect, useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import type { ChatRow, DoneEvent, MessageRow, ModelInfo } from "../types";
import { Markdown } from "./Markdown";
import { ThinkingBlock } from "./ThinkingBlock";

interface Props {
  chat: ChatRow | null;
  messages: MessageRow[];
  stream: { content: string; reasoning: string } | null;
  busy: boolean;
  models: ModelInfo[];
  error: string | null;
  usage: DoneEvent | null;
  onSend: (text: string) => void;
  onModelChange: (model: string, thinking: boolean, effort: string) => void;
}

const EFFORTS = ["low", "high", "max"];

export function ChatView({
  chat,
  messages,
  stream,
  busy,
  models,
  error,
  usage,
  onSend,
  onModelChange,
}: Props) {
  const [draft, setDraft] = useState("");
  const bottomRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [messages, stream?.content, stream?.reasoning]);

  const submit = () => {
    const text = draft.trim();
    if (!text || busy) return;
    setDraft("");
    onSend(text);
  };

  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      submit();
    }
  };

  useEffect(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 200)}px`;
  }, [draft]);

  return (
    <div className="flex h-full min-w-0 flex-1 flex-col">
      <header className="flex h-12 shrink-0 items-center gap-3 border-b border-neutral-800 px-4">
        <span className="truncate text-sm font-medium text-neutral-300">
          {chat ? chat.title : "DeskSeek"}
        </span>
        <div className="ml-auto flex items-center gap-2">
          {chat && (
            <>
              <select
                value={chat.model}
                onChange={(e) => onModelChange(e.target.value, chat.thinking, chat.effort)}
                className="rounded-md border border-neutral-800 bg-neutral-900 px-2 py-1 text-xs text-neutral-300 outline-none"
              >
                {models.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.label}
                  </option>
                ))}
              </select>
              <button
                onClick={() => onModelChange(chat.model, !chat.thinking, chat.effort)}
                className={`rounded-md border px-2 py-1 text-xs transition ${
                  chat.thinking
                    ? "border-sky-800 bg-sky-500/10 text-sky-300"
                    : "border-neutral-800 text-neutral-500 hover:text-neutral-300"
                }`}
                title="Toggle thinking mode"
              >
                Thinking
              </button>
              {chat.thinking && (
                <select
                  value={chat.effort}
                  onChange={(e) => onModelChange(chat.model, chat.thinking, e.target.value)}
                  className="rounded-md border border-neutral-800 bg-neutral-900 px-2 py-1 text-xs text-neutral-300 outline-none"
                >
                  {EFFORTS.map((e) => (
                    <option key={e} value={e}>
                      {e}
                    </option>
                  ))}
                </select>
              )}
            </>
          )}
        </div>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-4">
        <div className="mx-auto flex max-w-3xl flex-col gap-5 py-6">
          {messages.length === 0 && !stream && (
            <div className="mt-24 text-center text-neutral-600">
              <p className="text-lg text-neutral-500">
                {chat?.kind === "coding" ? "Coding section" : "Start a conversation"}
              </p>
              <p className="mt-1 text-xs">
                {chat?.kind === "coding"
                  ? "The coding agent arrives in a later milestone — chat works today."
                  : "Your API key stays in the OS keychain. Nothing is sent anywhere else."}
              </p>
            </div>
          )}

          {messages.map((m) => <Bubble key={m.id} message={m} />)}

          {stream && (
            <div className="flex flex-col">
              <Role role="assistant" />
              <ThinkingBlock text={stream.reasoning} streaming={busy} />
              {stream.content ? (
                <Markdown>{stream.content}</Markdown>
              ) : (
                <span className="text-neutral-600">…</span>
              )}
            </div>
          )}

          <div ref={bottomRef} />
        </div>
      </div>

      {error && (
        <div className="mx-4 mb-2 rounded-lg border border-red-900/60 bg-red-950/40 px-3 py-2 text-xs text-red-300">
          {error}
        </div>
      )}

      <div className="shrink-0 px-4 pb-4">
        {usage && (
          <div className="mx-auto max-w-3xl pb-1 text-[11px] text-neutral-600">
            {usage.usage.prompt_tokens.toLocaleString()} in ·{" "}
            {usage.usage.completion_tokens.toLocaleString()} out
            {usage.costUsd != null && ` · $${usage.costUsd.toFixed(5)}`}
          </div>
        )}
        <div className="mx-auto flex max-w-3xl items-end gap-2 rounded-xl border border-neutral-800 bg-neutral-900 p-2 focus-within:border-neutral-700">
          <textarea
            ref={textareaRef}
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={onKeyDown}
            rows={1}
            placeholder="Send a message…  (Enter to send, Shift+Enter for a new line)"
            className="max-h-[200px] flex-1 resize-none bg-transparent px-2 py-1.5 text-sm text-neutral-100 outline-none placeholder:text-neutral-600"
          />
          <button
            onClick={submit}
            disabled={busy || !draft.trim()}
            className="rounded-lg bg-sky-600 px-3 py-1.5 text-sm font-medium text-white transition hover:bg-sky-500 disabled:cursor-not-allowed disabled:bg-neutral-800 disabled:text-neutral-600"
          >
            {busy ? "…" : "Send"}
          </button>
        </div>
      </div>
    </div>
  );
}

function Role({ role }: { role: string }) {
  return (
    <div className="mb-1 text-[11px] font-medium uppercase tracking-wide text-neutral-600">
      {role === "assistant" ? "DeskSeek" : role}
    </div>
  );
}

function Bubble({ message }: { message: MessageRow }) {
  if (message.role === "user") {
    return (
      <div className="flex flex-col items-end">
        <div className="max-w-[85%] rounded-2xl rounded-br-sm bg-neutral-800 px-4 py-2.5 text-sm whitespace-pre-wrap">
          {message.content}
        </div>
      </div>
    );
  }
  if (message.role === "tool") {
    return (
      <details className="rounded-lg border border-neutral-800 bg-neutral-900/50 px-3 py-2">
        <summary className="cursor-pointer text-xs text-neutral-500">tool result</summary>
        <pre className="mt-2 overflow-x-auto text-xs text-neutral-400">{message.content}</pre>
      </details>
    );
  }
  return (
    <div className="flex flex-col">
      <Role role="assistant" />
      {message.reasoning_content && (
        <ThinkingBlock text={message.reasoning_content} streaming={false} />
      )}
      <Markdown>{message.content ?? ""}</Markdown>
    </div>
  );
}
