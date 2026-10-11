import { useEffect, useRef, useState } from "react";
import type { ClipboardEvent, KeyboardEvent } from "react";
import type {
  ApprovalEvent,
  Attachment,
  ChatRow,
  DoneEvent,
  MessageRow,
  ModelInfo,
  PermissionMode,
  ToolRun,
} from "../types";
import { Markdown } from "./Markdown";
import { ThinkingBlock } from "./ThinkingBlock";
import { ToolCallCard } from "./ToolCallCard";
import { ApprovalPrompt } from "./ApprovalPrompt";

interface Props {
  chat: ChatRow | null;
  messages: MessageRow[];
  stream: { content: string; reasoning: string } | null;
  tools: ToolRun[];
  approvals: ApprovalEvent[];
  attachments: Attachment[];
  busy: boolean;
  models: ModelInfo[];
  error: string | null;
  notice: string | null;
  usage: DoneEvent | null;
  contextTokens: number;
  contextMax: number;
  onSend: (text: string) => void;
  onStop: () => void;
  onModelChange: (model: string, thinking: boolean, effort: string) => void;
  onModeChange: (mode: PermissionMode) => void;
  onPickWorkspace: () => void;
  onAttach: (files: FileList | File[]) => void;
  onRemoveAttachment: (index: number) => void;
  onDecideApproval: (requestId: string, decision: "once" | "session" | "always" | "deny") => void;
}

const EFFORTS = ["low", "high", "max"];

/// The permission modes, in increasing order of autonomy. The labels are what
/// the user picks; the values are what the backend stores.
const MODES: { id: PermissionMode; label: string; hint: string }[] = [
  { id: "manual", label: "Ask", hint: "Pause for approval before every write and command" },
  { id: "auto", label: "Auto", hint: "Run without asking; a guard refuses dangerous commands" },
  { id: "skip", label: "Skip", hint: "Never ask; only catastrophic commands are refused" },
];

/** Kept in sync with the switch in App.tsx's send(). */
const COMMANDS = [
  { name: "/compact", description: "Summarise the chat and reset the context" },
  { name: "/memory", description: "Open the memory panel" },
  { name: "/skills", description: "Choose skills for this chat" },
  { name: "/mcp", description: "Manage MCP servers" },
  { name: "/setup", description: "Reopen the setup screen" },
  { name: "/folder", description: "Pick the folder for a coding task" },
  { name: "/settings", description: "Open settings" },
  { name: "/new", description: "Start a new chat" },
  { name: "/help", description: "List the commands" },
];

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return String(n);
}

export function ChatView({
  chat,
  messages,
  stream,
  tools,
  approvals,
  attachments,
  busy,
  models,
  error,
  notice,
  usage,
  contextTokens,
  contextMax,
  onSend,
  onStop,
  onModelChange,
  onModeChange,
  onPickWorkspace,
  onAttach,
  onRemoveAttachment,
  onDecideApproval,
}: Props) {
  const [draft, setDraft] = useState("");
  const [cmdIndex, setCmdIndex] = useState(0);
  // Tracks which draft the user dismissed the menu for, so Esc stays dismissed
  // until they type something new.
  const [dismissedFor, setDismissedFor] = useState<string | null>(null);
  const bottomRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  const commandQuery = draft.startsWith("/") && !draft.includes(" ") ? draft : null;
  const filtered = commandQuery
    ? COMMANDS.filter((command) => command.name.startsWith(commandQuery.toLowerCase()))
    : [];
  const menuOpen = filtered.length > 0 && dismissedFor !== draft;

  const runCommand = (name: string) => {
    setDraft("");
    setDismissedFor(null);
    setCmdIndex(0);
    onSend(name);
  };

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [messages, stream?.content, stream?.reasoning, tools.length, approvals.length]);

  const submit = () => {
    const text = draft.trim();
    if ((!text && attachments.length === 0) || busy) return;
    setDraft("");
    onSend(text);
  };

  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (menuOpen) {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setCmdIndex((i) => (i + 1) % filtered.length);
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        setCmdIndex((i) => (i - 1 + filtered.length) % filtered.length);
        return;
      }
      if (e.key === "Tab" || e.key === "Enter") {
        e.preventDefault();
        runCommand(filtered[Math.min(cmdIndex, filtered.length - 1)].name);
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        setDismissedFor(draft);
        return;
      }
    }
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      submit();
    }
  };

  const onPaste = (e: ClipboardEvent<HTMLTextAreaElement>) => {
    const files = Array.from(e.clipboardData.files);
    if (files.length > 0) {
      e.preventDefault();
      onAttach(files);
    }
  };

  useEffect(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 200)}px`;
  }, [draft]);

  const needsFolder = chat?.kind === "coding" && !chat.workspace;
  const usedFraction = contextMax > 0 ? Math.min(1, contextTokens / contextMax) : 0;

  return (
    <div className="flex h-full min-w-0 flex-1 flex-col">
      <header className="flex min-h-12 shrink-0 flex-wrap items-center gap-2 border-b border-neutral-800 px-4 py-1.5">
        <span className="truncate text-sm font-medium text-neutral-300">
          {chat ? chat.title : "DeskSeek"}
        </span>

        {chat?.kind === "coding" && (
          <button
            onClick={onPickWorkspace}
            className={`rounded-md border px-2 py-1 text-xs transition ${
              needsFolder
                ? "border-amber-800 bg-amber-950/40 text-amber-300 hover:border-amber-700"
                : "border-neutral-800 text-neutral-400 hover:border-neutral-700"
            }`}
            title={chat.workspace ?? "Choose a folder to work in"}
          >
            {needsFolder
              ? "Choose folder…"
              : `📁 ${chat.workspace?.split(/[\\/]/).filter(Boolean).pop()}`}
          </button>
        )}

        <div className="ml-auto flex items-center gap-2">
          {chat && (
            <div
              className="flex items-center gap-0.5 rounded-md border border-neutral-800 p-0.5"
              title={MODES.find((m) => m.id === chat.permission_mode)?.hint}
            >
              {MODES.map((mode) => {
                const active = chat.permission_mode === mode.id;
                const tone =
                  mode.id === "skip"
                    ? "border-red-700 bg-red-500/15 text-red-300"
                    : "border-sky-800 bg-sky-500/15 text-sky-300";
                return (
                  <button
                    key={mode.id}
                    onClick={() => onModeChange(mode.id)}
                    className={`rounded px-1.5 py-0.5 text-[11px] transition ${
                      active ? tone : "border border-transparent text-neutral-500 hover:text-neutral-300"
                    }`}
                  >
                    {mode.label}
                  </button>
                );
              })}
            </div>
          )}

          <div
            className="flex items-center gap-1.5"
            title={`Approximately ${contextTokens.toLocaleString()} of ${contextMax.toLocaleString()} tokens used (chars ÷ 3.6)`}
          >
            <div className="h-1.5 w-16 overflow-hidden rounded-full bg-neutral-800">
              <div
                className={`h-full rounded-full ${
                  usedFraction > 0.8
                    ? "bg-red-500"
                    : usedFraction > 0.5
                      ? "bg-amber-500"
                      : "bg-sky-600"
                }`}
                style={{ width: `${Math.max(2, usedFraction * 100)}%` }}
              />
            </div>
            <span className="text-[10px] tabular-nums text-neutral-600">
              ~{formatTokens(contextTokens)}/{formatTokens(contextMax)}
            </span>
          </div>

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

      {chat && chat.permission_mode !== "manual" && (
        <div
          className={`shrink-0 px-4 py-1 text-[11px] ${
            chat.permission_mode === "skip"
              ? "bg-red-950/40 text-red-300"
              : "bg-sky-950/30 text-sky-300"
          }`}
        >
          {chat.permission_mode === "skip"
            ? "Skip mode: writes and commands run without asking. Only catastrophic commands (disk formatting, forced history rewrites, publishing) are refused."
            : "Auto mode: writes and commands run without asking. A safety guard refuses catastrophic commands and uploads of local data."}
        </div>
      )}

      <div className="min-h-0 flex-1 overflow-y-auto px-4">
        <div className="mx-auto flex max-w-3xl flex-col gap-5 py-6">
          {messages.length === 0 && !stream && (
            <div className="mt-24 text-center text-neutral-600">
              <p className="text-lg text-neutral-500">
                {chat?.kind === "coding" ? "Coding section" : "Start a conversation"}
              </p>
              <p className="mt-1 text-xs">
                {chat?.kind === "coding"
                  ? "Point it at a folder, then ask for a change. Every write and command asks first."
                  : "Attach an image or ask for a document — chat can read and write files in your documents folder."}
              </p>
              <p className="mt-3 text-xs text-neutral-700">
                Commands: /compact /memory /skills /mcp /setup /folder /new /help
              </p>
            </div>
          )}

          {messages.map((m) => (
            <Bubble key={m.id} message={m} />
          ))}

          {tools.map((run) => (
            <ToolCallCard key={run.id} run={run} />
          ))}

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
        <div className="mx-auto max-w-3xl">
          {approvals.map((request) => (
            <ApprovalPrompt key={request.requestId} request={request} onDecide={onDecideApproval} />
          ))}

          {attachments.length > 0 && (
            <div className="mb-2 flex flex-wrap gap-2">
              {attachments.map((file, index) => (
                <div
                  key={`${file.name}-${index}`}
                  className="flex items-center gap-2 rounded-lg border border-neutral-800 bg-neutral-900 px-2 py-1 text-[11px] text-neutral-300"
                >
                  {file.mime.startsWith("image/") ? (
                    <img src={file.data_url} alt="" className="h-8 w-8 rounded object-cover" />
                  ) : (
                    <span>📎</span>
                  )}
                  <span className="max-w-40 truncate">{file.name}</span>
                  <button
                    onClick={() => onRemoveAttachment(index)}
                    className="text-neutral-500 hover:text-red-400"
                  >
                    ×
                  </button>
                </div>
              ))}
            </div>
          )}

          {usage && (
            <div className="pb-1 text-[11px] text-neutral-600">
              {usage.usage?.prompt_tokens.toLocaleString()} in ·{" "}
              {usage.usage?.completion_tokens.toLocaleString()} out
              {usage.costUsd != null && ` · $${usage.costUsd.toFixed(5)}`}
            </div>
          )}

          {notice && <div className="pb-1 text-[11px] text-sky-400">{notice}</div>}

          {menuOpen && (
            <div className="mb-2 overflow-hidden rounded-xl border border-neutral-800 bg-neutral-900">
              {filtered.map((command, index) => (
                <button
                  key={command.name}
                  onMouseDown={(e) => {
                    e.preventDefault();
                    runCommand(command.name);
                  }}
                  onMouseEnter={() => setCmdIndex(index)}
                  className={`flex w-full items-baseline gap-3 px-3 py-2 text-left text-xs transition ${
                    index === cmdIndex ? "bg-neutral-800 text-neutral-100" : "text-neutral-400"
                  }`}
                >
                  <span className="font-mono">{command.name}</span>
                  <span className="truncate text-neutral-500">{command.description}</span>
                </button>
              ))}
            </div>
          )}

          <div className="flex items-end gap-2 rounded-xl border border-neutral-800 bg-neutral-900 p-2 focus-within:border-neutral-700">
            <input
              ref={fileRef}
              type="file"
              multiple
              hidden
              onChange={(e) => {
                if (e.target.files) onAttach(e.target.files);
                e.target.value = "";
              }}
            />
            <button
              onClick={() => fileRef.current?.click()}
              className="rounded-lg px-2 py-1.5 text-neutral-500 hover:text-neutral-200"
              title="Attach files or images"
            >
              📎
            </button>
            <textarea
              ref={textareaRef}
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={onKeyDown}
              onPaste={onPaste}
              rows={1}
              placeholder="Send a message, or type / for commands…"
              className="max-h-[200px] flex-1 resize-none bg-transparent px-1 py-1.5 text-sm text-neutral-100 outline-none placeholder:text-neutral-600"
            />
            <button
              onClick={submit}
              disabled={busy || (!draft.trim() && attachments.length === 0)}
              className={`rounded-lg px-3 py-1.5 text-sm font-medium text-white transition disabled:cursor-not-allowed disabled:bg-neutral-800 disabled:text-neutral-600 ${
                busy ? "hidden" : "bg-sky-600 hover:bg-sky-500"
              }`}
            >
              Send
            </button>
            {busy && (
              <button
                onClick={onStop}
                className="rounded-lg border border-neutral-700 px-3 py-1.5 text-sm text-neutral-200 transition hover:border-red-800 hover:bg-red-950/40 hover:text-red-300"
                title="Stop this run"
              >
                Stop
              </button>
            )}
          </div>
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
  let attachments: Attachment[] = [];
  if (message.attachments_json) {
    try {
      attachments = JSON.parse(message.attachments_json) as Attachment[];
    } catch {
      attachments = [];
    }
  }

  if (message.role === "user") {
    return (
      <div className="flex flex-col items-end gap-1">
        {attachments.length > 0 && (
          <div className="flex flex-wrap justify-end gap-2">
            {attachments.map((file, index) =>
              file.mime.startsWith("image/") ? (
                <img
                  key={index}
                  src={file.data_url}
                  alt={file.name}
                  className="max-h-40 rounded-lg border border-neutral-800 object-contain"
                />
              ) : (
                <span
                  key={index}
                  className="rounded-lg border border-neutral-800 px-2 py-1 text-[11px] text-neutral-400"
                >
                  📎 {file.name}
                </span>
              ),
            )}
          </div>
        )}
        {message.content && (
          <div className="max-w-[85%] rounded-2xl rounded-br-sm bg-neutral-800 px-4 py-2.5 text-sm whitespace-pre-wrap">
            {message.content}
          </div>
        )}
      </div>
    );
  }

  if (message.role === "tool") {
    return (
      <details className="rounded-lg border border-neutral-800 bg-neutral-900/50 px-3 py-2">
        <summary className="cursor-pointer text-xs text-neutral-500">tool result</summary>
        <pre className="mt-2 overflow-x-auto whitespace-pre-wrap text-xs text-neutral-400">
          {message.content}
        </pre>
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
