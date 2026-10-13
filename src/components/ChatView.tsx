import { memo, useEffect, useMemo, useRef, useState } from "react";
import type { ClipboardEvent, KeyboardEvent } from "react";
import type {
  ApprovalEvent,
  Attachment,
  ChatRow,
  DoneEvent,
  Egress,
  InterruptedRun,
  MessageRow,
  ModelInfo,
  PermissionMode,
  PromptView,
  QuestionEvent,
  ToolRun,
  TreeEntry,
} from "../types";
import { api } from "../api";
import { composerOffer, matchMentions } from "../lib/commands";
import type { PaletteItem } from "../lib/commands";
import { loadDraft, saveDraft } from "../lib/draft";
import { tasksFrom } from "../lib/tasks";
import { buildTranscript, windowTranscript, TRANSCRIPT_WINDOW } from "../lib/transcript";
import { CopyButton, Markdown } from "./Markdown";
import { IconButton } from "./Icon";
import { formatTokens } from "../lib/context";
import { ThinkingBlock } from "./ThinkingBlock";
import { ToolStepGroup } from "./ToolStepGroup";
import { ApprovalPrompt } from "./ApprovalPrompt";
import { QuestionPrompt } from "./QuestionPrompt";
import { TaskList } from "./TaskList";

interface Props {
  chat: ChatRow | null;
  messages: MessageRow[];
  stream: { content: string; reasoning: string } | null;
  tools: ToolRun[];
  approvals: ApprovalEvent[];
  /** Questions the model is waiting on for this chat. */
  questions: QuestionEvent[];
  attachments: Attachment[];
  busy: boolean;
  models: ModelInfo[];
  error: string | null;
  notice: string | null;
  usage: DoneEvent | null;
  /** Process-wide: computer control is stopped until the user starts it again. */
  computerPaused: boolean;
  computerReason: string | null;
  onResumeComputer: () => void;
  /** The turn a previous session left in flight here, if any. */
  interrupted: InterruptedRun | null;
  onRetryInterrupted: () => void;
  onRewindInterrupted: () => void;
  onDismissInterrupted: () => void;
  /** What the MCP servers offer. `ready: false` means discovery is still running. */
  mcp: { ready: boolean; toolCount: number; failedCount: number };
  onMcp: () => void;
  contextTokens: number;
  contextMax: number;
  /** Opens the context breakdown for this chat. */
  onContext: () => void;
  onSend: (text: string) => void;
  /** Ask again from the last user turn; `text` edits it first, null repeats it. */
  onResend: (text: string | null) => void;
  /**
   * Put back the answer a resend replaced, on the turn whose `seq` this is. The
   * two answers swap, so the button is a toggle rather than a one-way door.
   */
  onRestoreTail: (afterSeq: number) => void;
  onStop: () => void;
  onFiles: () => void;
  onModelChange: (model: string, thinking: boolean, effort: string) => void;
  /** Ask, Standard, Plan or Agent — see AGENT_MODES. */
  onAgentMode: (mode: string) => void;
  onModeChange: (mode: PermissionMode) => void;
  /**
   * Turns this chat's "does not feed memory" switch on or off. With it on, no
   * memory is shown here and nothing said here is remembered.
   */
  onIncognito: (incognito: boolean) => void;
  /**
   * What this chat may send out. A declared policy is a refusal rather than a
   * prompt, so it holds in every mode — see EGRESS.
   */
  onEgress: (egress: Egress) => void;
  onPickWorkspace: () => void;
  /** Show where this chat keeps its own files. */
  onRevealFolder: () => void;
  onAttach: (files: FileList | File[]) => void;
  onRemoveAttachment: (index: number) => void;
  onDecideApproval: (requestId: string, decision: "once" | "session" | "always" | "deny") => void;
  /** Answer a question the model asked; `selected` and `free` are per question. */
  onAnswerQuestion: (requestId: string, selected: string[][], free: string[]) => void;
  onSkipQuestion: (requestId: string) => void;
  /** Saved prompts, offered in the slash menu as `/p:<id>` — see runCommand. */
  prompts: PromptView[];
  /**
   * Text to drop into the composer from outside it, with a counter so picking the
   * same prompt twice lands twice. The palette is the caller.
   */
  insert: { text: string; seq: number } | null;
  /** Bumped by the window chord; each new value scrolls to the newest message. */
  jumpToLatest: number;
}

const EFFORTS = ["low", "high", "max"];

/**
 * The four modes, as a ladder: each rung may do more than the one before.
 *
 * This is the only "how much rope" control in the app. The approval control
 * beside it is shown in Standard alone, because in Ask and Plan there is nothing
 * to approve and in Agent mode the answer is "nothing asks".
 */
export const AGENT_MODES: { id: string; label: string; hint: string }[] = [
  {
    id: "ask",
    label: "Ask",
    hint: "No tools: answers from what the model knows, and says when something would need a file, the web or a command",
  },
  {
    id: "standard",
    label: "Standard",
    hint: "Every tool; writes and commands follow the approval mode beside this",
  },
  {
    id: "plan",
    label: "Plan",
    hint: "Reads only: read files, look things up on the web, and propose a plan — it changes nothing",
  },
  {
    id: "agent",
    label: "Agent",
    hint: "Runs unattended until the job is done; only catastrophic commands are refused",
  },
];

/** The rung after this one, for Shift+Tab. Wraps, so the cycle always moves. */
export function nextAgentMode(current: string): string {
  const at = AGENT_MODES.findIndex((m) => m.id === current);
  return AGENT_MODES[(at + 1) % AGENT_MODES.length].id;
}

/// The permission modes, in increasing order of autonomy. The labels are what
/// the user picks; the values are what the backend stores. `manual` is
/// "Ask first" rather than "Ask" so it cannot be confused with the Ask *agent
/// mode*, which sits beside it in the header.
export const MODES: { id: PermissionMode; label: string; hint: string }[] = [
  { id: "manual", label: "Ask first", hint: "Pause for approval before every write and command" },
  { id: "auto", label: "Auto", hint: "Run without asking; a guard refuses dangerous commands" },
  { id: "skip", label: "Skip", hint: "Never ask; only catastrophic commands are refused" },
];

/// What a chat may send out. Three rungs, and `all` is not a policy at all —
/// nothing is declared, so the guard's own heuristic decides what it always did.
/// A declared rung is a refusal rather than a prompt, which is why it is a
/// switch beside the approval mode rather than one more thing to approve.
export const EGRESS: { id: Egress; label: string; hint: string }[] = [
  { id: "all", label: "Network: on", hint: "Say nothing about the network; the safety guard decides as it always has" },
  { id: "read-only", label: "Read only", hint: "It may read the web; it may not send anything out" },
  { id: "none", label: "No network", hint: "Nothing reaches out: no fetching, no searching, no command that talks to a remote" },
];

export const EGRESS_NOTE =
  "A policy, not a sandbox: a program the agent runs can still open a socket by itself. It refuses \
what this app sends out — a fetch, an upload, a browser action";

/** Kept in sync with the switch in App.tsx's send(). */
export const COMMANDS = [
  { name: "/mode", description: "Ask, Standard, Plan or Agent — or cycle with Shift+Tab" },
  { name: "/compact", description: "Summarise the chat and reset the context" },
  { name: "/summarise", description: "Propose memories from this chat" },
  { name: "/memory", description: "Open the memory settings" },
  { name: "/skill", description: "List skills, or switch one on for this chat" },
  { name: "/skills", description: "Skills, connectors, instructions and design systems" },
  { name: "/mcp", description: "Manage MCP servers (connectors)" },
  { name: "/prompts", description: "Saved prompts: the requests you retype" },
  { name: "/projects", description: "Projects: a folder and the chats in it" },
  { name: "/artifacts", description: "Every artifact, from every chat" },
  { name: "/scheduled", description: "Recurring prompts on a calendar" },
  { name: "/spend", description: "What the last turns cost, by day and model" },
  { name: "/setup", description: "Reopen the setup screen" },
  { name: "/files", description: "Show the file tree" },
  { name: "/folder", description: "Pick the folder for a coding task" },
  { name: "/settings", description: "Open settings" },
  { name: "/new", description: "Start a new chat" },
  { name: "/export", description: "Write this chat to a Markdown file" },
  { name: "/rewind", description: "Undo the agent's last file changes" },
  { name: "/help", description: "List the commands" },
];

export function ChatView({
  chat,
  messages,
  stream,
  tools,
  approvals,
  questions,
  attachments,
  busy,
  models,
  error,
  notice,
  usage,
  computerPaused,
  computerReason,
  onResumeComputer,
  interrupted,
  onRetryInterrupted,
  onRewindInterrupted,
  onDismissInterrupted,
  mcp,
  onMcp,
  contextTokens,
  contextMax,
  onContext,
  onSend,
  onResend,
  onRestoreTail,
  onStop,
  onFiles,
  onModelChange,
  onAgentMode,
  onModeChange,
  onIncognito,
  onEgress,
  onPickWorkspace,
  onRevealFolder,
  onAttach,
  onRemoveAttachment,
  onDecideApproval,
  onAnswerQuestion,
  onSkipQuestion,
  prompts,
  insert,
  jumpToLatest,
}: Props) {
  // Seeded from sessionStorage, not "", so a render throw — which unmounts this
  // whole tree — does not take a half-typed message with it. See lib/draft.ts.
  const [draft, setDraftState] = useState(() => loadDraft(chat?.id));
  const setDraft = (text: string) => {
    setDraftState(text);
    saveDraft(chat?.id, text);
  };
  const [cmdIndex, setCmdIndex] = useState(0);
  // Derived from the tool calls rather than plumbed through as its own prop:
  // `update_tasks` already carries the plan, and only the newest one counts.
  const tasks = useMemo(() => tasksFrom(tools), [tools]);
  // Messages and the turn in flight, folded so a run of tool calls draws as one
  // row the user can open instead of a column of cards.
  const transcript = useMemo(() => buildTranscript(messages, tools), [messages, tools]);
  // Only the newest user turn is editable: an older one has answers after it
  // that a re-run would silently discard, and offering that would be a surprise.
  const lastUserId = useMemo(() => {
    for (let i = messages.length - 1; i >= 0; i -= 1) {
      if (messages[i].role === "user") return messages[i].id;
    }
    return null;
  }, [messages]);

  // The newest answer, which is the only one a retry can replace. Walking in
  // order resets at each user turn, so an answer that is already followed by a
  // newer question is not the one offered: retrying it would throw that question
  // and this reply away. A tool-call turn carries no content and is not an answer
  // either.
  const retryId = useMemo(() => {
    let answer: string | null = null;
    for (const message of messages) {
      if (message.role === "user") answer = null;
      else if (message.role === "assistant" && message.content) answer = message.id;
    }
    return answer;
  }, [messages]);

  // "The agent is operating your desktop" is derived, not reported: if a
  // clicking tool is in flight, it is true, and it stops being true the moment
  // the call returns. Nothing to keep in sync.
  const acting = tools.some(
    (t) => t.status === "running" && (t.tool === "ui_click" || t.tool === "ui_type"),
  );
  // Tracks which draft the user dismissed the menu for, so Esc stays dismissed
  // until they type something new.
  const [dismissedFor, setDismissedFor] = useState<string | null>(null);
  // How much of the transcript is drawn. It only ever grows, so revealing older
  // turns cannot lose the reader's place, and it resets when the chat does —
  // otherwise a window opened in a 500-turn chat would follow you to a new one.
  const [shownLimit, setShownLimit] = useState(TRANSCRIPT_WINDOW);
  useEffect(() => {
    setShownLimit(TRANSCRIPT_WINDOW);
  }, [chat?.id]);
  const { shown, hidden } = useMemo(
    () => windowTranscript(transcript, shownLimit),
    [transcript, shownLimit],
  );
  const bottomRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  const offer = composerOffer(draft);
  const scope = offer?.scope ?? null;
  const query = offer?.query ?? "";
  // The `@` and `$` lists are read when their menu opens, not on every render:
  // one is a walk of the folder, the other a round trip to the backend. `scope`
  // in the state is what keeps the file list from flashing in the skill menu
  // while the other one is still loading.
  const [mentions, setMentions] = useState<{ scope: string; items: PaletteItem[] } | null>(null);
  useEffect(() => {
    if (!chat || scope === null || scope === "command") return;
    let alive = true;
    const timer = window.setTimeout(
      () => {
        const file = (entry: TreeEntry): PaletteItem => ({
          id: entry.path,
          label: entry.path,
          hint: entry.isDir ? "Folder" : "File",
        });
        // A file is searched by name over the whole folder, so one three levels
        // down is reachable without walking to it; a one-letter query is not
        // worth a walk of the tree, so it lists the top of it instead.
        const read: Promise<PaletteItem[]> =
          scope === "skill"
            ? api.listSkills(chat.id).then((list) =>
                list.map((skill) => ({
                  id: skill.id,
                  label: skill.id,
                  hint: `${skill.name} — ${skill.description}`,
                })),
              )
            : (query.trim().length >= 2
                ? api.findInTree(chat.id, query.trim())
                : api.listTree(chat.id, null).then((listing) => listing.entries)
              ).then((entries) => entries.map(file));
        read
          .then((items) => {
            if (alive) setMentions({ scope, items });
          })
          // An unreadable folder is an empty menu rather than a banner over the
          // composer: a mention is a convenience, and the box still works.
          .catch(() => {
            if (alive) setMentions({ scope, items: [] });
          });
      },
      // Long enough that typing a path does not search the disk on every letter.
      scope === "skill" ? 0 : 150,
    );
    return () => {
      alive = false;
      window.clearTimeout(timer);
    };
  }, [chat, scope, query]);

  // Saved prompts join the built-in commands in the one menu, and are told apart
  // by the `/p:` prefix. `COMMANDS` itself is the built-in list — Settings shows
  // it, so a user's own prompts must not end up in it.
  //
  // All three menus are the same rows — a mono name and a description — so the
  // only difference between them is what `runCommand` does with the name.
  const filtered = useMemo(() => {
    const mark = scope === "file" ? "@" : "$";
    if (scope === "file" || scope === "skill") {
      const list = mentions?.scope === scope ? mentions.items : [];
      return matchMentions(list, query).map((item) => ({
        name: `${mark}${item.id}`,
        description: item.hint,
      }));
    }
    if (scope === "command") {
      const choices = [
        ...COMMANDS,
        ...prompts.map((prompt) => ({ name: `/p:${prompt.id}`, description: prompt.name })),
      ];
      return choices.filter((command) => command.name.startsWith(query));
    }
    return [];
  }, [scope, query, prompts, mentions]);
  const menuOpen = filtered.length > 0 && dismissedFor !== draft;

  /**
   * A chosen menu entry. A built-in command acts and is sent; a saved prompt and
   * a mention only fill the box, because both are text the user is about to edit
   * — sending `read @App.tsx` unread would make the mention a worse version of
   * typing the path, the same way it would a prompt.
   */
  const runCommand = (name: string) => {
    setDismissedFor(null);
    setCmdIndex(0);
    if (scope === "file" || scope === "skill") {
      // The trailing space ends the mention, which closes this menu and lets the
      // next one start. ponytail: a path with a space in it inserts unquoted, so
      // the menu closes a word early; the path still reads as a path.
      setDraft(`${draft.slice(0, draft.lastIndexOf(" ") + 1)}${name} `);
      textareaRef.current?.focus();
      return;
    }
    const prompt = name.startsWith("/p:") ? prompts.find((p) => `/p:${p.id}` === name) : undefined;
    if (prompt) {
      setDraft(prompt.body);
      textareaRef.current?.focus();
      return;
    }
    setDraft("");
    onSend(name);
  };

  useEffect(() => {
    if (!insert) return;
    setDraft(insert.text);
    textareaRef.current?.focus();
  }, [insert]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [messages, stream?.content, stream?.reasoning, tools.length, approvals.length]);

  // Ctrl+G. Separate from the effect above on purpose: that one follows the
  // transcript, this one follows the counter, so it can pull the user back down
  // without waiting for something new to arrive.
  useEffect(() => {
    if (!jumpToLatest) return;
    bottomRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [jumpToLatest]);

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
      return;
    }
    // Shift+Tab cycles the mode, the way every other agent UI does it. Checking
    // the menu first is what keeps Tab-for-completion working.
    if (e.key === "Tab" && e.shiftKey && chat) {
      e.preventDefault();
      onAgentMode(nextAgentMode(chat.agent_mode));
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
            className={`rounded-md px-2 py-1 text-xs transition duration-150 ${
              needsFolder
                ? "border border-amber-800 bg-amber-950/40 text-amber-300 hover:border-amber-700"
                : "text-neutral-400 hover:bg-neutral-800 hover:text-neutral-200"
            }`}
            title={chat.workspace ?? "Choose a folder to work in"}
          >
            {needsFolder
              ? "Choose folder…"
              : `📁 ${chat.workspace?.split(/[\\/]/).filter(Boolean).pop()}`}
          </button>
        )}

        {/* A chat-mode chat keeps a folder of its own, made the first time it
            needs one — so there is nothing to choose, only something to show. */}
        {chat && chat.kind !== "coding" && (
          <button
            onClick={onRevealFolder}
            aria-label="Show this chat's folder"
            className="rounded-md px-2 py-1 text-xs text-neutral-500 transition duration-150 hover:bg-neutral-800 hover:text-neutral-300"
            title="This chat keeps its own files in a folder — click to show it"
          >
            📁
          </button>
        )}

        {(!mcp.ready || mcp.toolCount > 0 || mcp.failedCount > 0) && (
          <button
            onClick={onMcp}
            className={`rounded-md px-2 py-1 text-xs transition duration-150 ${
              mcp.failedCount > 0
                ? "border border-amber-800 bg-amber-950/40 text-amber-300 hover:border-amber-700"
                : "text-neutral-500 hover:bg-neutral-800 hover:text-neutral-300"
            }`}
            title={
              !mcp.ready
                ? "MCP servers are still starting; their tools are not offered yet"
                : `${mcp.toolCount} tool(s) offered to the model` +
                  (mcp.failedCount > 0 ? `, ${mcp.failedCount} server(s) failed to start` : "")
            }
          >
            {!mcp.ready
              ? "MCP starting…"
              : mcp.toolCount > 0
                ? `🔌 ${mcp.toolCount} MCP tool${mcp.toolCount === 1 ? "" : "s"}`
                : `MCP: ${mcp.failedCount} failed`}
          </button>
        )}

        <div className="ml-auto flex items-center gap-2">
          {chat && (
            <button
              onClick={onFiles}
              className="rounded-md px-2 py-1 text-xs text-neutral-400 transition duration-150 hover:bg-neutral-800 hover:text-neutral-200"
              title="Show or hide the file tree"
            >
              Files
            </button>
          )}

          <button
            onClick={onContext}
            aria-label={`Context: about ${contextTokens.toLocaleString()} of ${contextMax.toLocaleString()} tokens used. Show the breakdown.`}
            className="flex items-center gap-1.5 rounded px-1 py-0.5 transition hover:bg-neutral-800"
            title={`Approximately ${contextTokens.toLocaleString()} of ${contextMax.toLocaleString()} tokens used (characters ÷ 3.6). Click for the breakdown.`}
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
          </button>

          {chat && (
            <>
              <select
                value={chat.model}
                onChange={(e) => onModelChange(e.target.value, chat.thinking, chat.effort)}
                aria-label="Model"
                className="rounded-md border border-neutral-800 bg-neutral-900 px-2 py-1 text-xs text-neutral-300 outline-hidden focus:border-neutral-700"
              >
                {models.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.label}
                  </option>
                ))}
              </select>
              <button
                onClick={() => onModelChange(chat.model, !chat.thinking, chat.effort)}
                className={`rounded-md px-2 py-1 text-xs transition duration-150 ${
                  chat.thinking
                    ? "bg-sky-500/15 text-sky-300"
                    : "text-neutral-500 hover:bg-neutral-800 hover:text-neutral-300"
                }`}
                title="Toggle thinking mode"
              >
                Thinking
              </button>
              {chat.thinking && (
                <select
                  value={chat.effort}
                  onChange={(e) => onModelChange(chat.model, chat.thinking, e.target.value)}
                  aria-label="Thinking effort"
                  className="rounded-md border border-neutral-800 bg-neutral-900 px-2 py-1 text-xs text-neutral-300 outline-hidden focus:border-neutral-700"
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

      {/* A `log` is a live region whose additions are announced, which is what a
          transcript is. `aria-busy` is the part that makes it usable: streaming
          rewrites this subtree every frame, and a screen reader that read each
          frame would be unusable. Busy holds the announcements back until the
          turn settles, then says the answer once. */}
      <div
        role="log"
        aria-label="Conversation"
        aria-busy={busy}
        className="min-h-0 flex-1 overflow-y-auto px-4"
      >
        <div className="mx-auto flex max-w-3xl flex-col gap-6 py-6">
          {messages.length === 0 && !stream && (
            <div className="mt-24 text-center">
              <p className="text-[15px] text-neutral-300">
                {chat?.kind === "coding" ? "Point it at a folder" : "Start a conversation"}
              </p>
              <p className="mx-auto mt-2 max-w-sm text-xs leading-relaxed text-neutral-500">
                {chat?.kind === "coding"
                  ? "Choose a folder in the header, then ask for a change. Every write and command asks first."
                  : "Ask anything, attach an image, or ask for a document — it can read and write files in your documents folder."}
              </p>
              <p className="mx-auto mt-5 max-w-md font-mono text-[11px] leading-relaxed text-neutral-600">
                {COMMANDS.map((command) => command.name).join("  ")}
              </p>
            </div>
          )}

          {hidden > 0 && (
            <button
              onClick={() => setShownLimit((n) => n + TRANSCRIPT_WINDOW)}
              className="mx-auto rounded-md px-3 py-1.5 text-xs text-neutral-500 transition hover:bg-neutral-800 hover:text-neutral-300"
            >
              Show {hidden} earlier {hidden === 1 ? "message" : "messages"}
            </button>
          )}

          {shown.map((item) =>
            item.kind === "message" ? (
              <Bubble
                key={item.message.id}
                message={item.message}
                onEdit={item.message.id === lastUserId ? onResend : undefined}
                onRetry={
                  item.message.id === retryId && !busy ? () => onResend(null) : undefined
                }
                onRestore={
                  item.message.role === "user" && item.message.discarded && !busy
                    ? () => onRestoreTail(item.message.seq)
                    : undefined
                }
              />
            ) : (
              <ToolStepGroup key={`steps-${item.steps[0].id}`} steps={item.steps} />
            ),
          )}

          {stream && (
            <div className="flex flex-col">
              <ThinkingBlock text={stream.reasoning} streaming={busy} />
              {stream.content ? (
                <div className="flex items-end gap-1">
                  <div className="min-w-0 flex-1">
                    <Markdown>{stream.content}</Markdown>
                  </div>
                  {busy && <span className="caret mb-1.5" aria-hidden />}
                </div>
              ) : (
                <span className="text-neutral-600">…</span>
              )}
            </div>
          )}

          <div ref={bottomRef} />
        </div>
      </div>

      {acting && (
        <div className="mx-4 mb-2 rounded-lg border border-sky-900/60 bg-sky-950/40 px-3 py-2 text-xs text-sky-300">
          The agent is operating your desktop. Press <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>Esc</kbd>{" "}
          to stop it.
        </div>
      )}

      {computerPaused && (
        <div className="mx-4 mb-2 flex items-center gap-3 rounded-lg border border-amber-900/60 bg-amber-950/40 px-3 py-2 text-xs text-amber-300">
          <span className="flex-1">{computerReason ?? "Computer control is stopped."}</span>
          <button
            onClick={onResumeComputer}
            className="shrink-0 rounded border border-amber-800/60 px-2 py-0.5 transition hover:border-amber-700 hover:text-amber-200"
          >
            Start it again
          </button>
        </div>
      )}

      {/* A turn the last session left half-done. It is a real problem, not just
          news: the history ends on a tool call with no result, which the next
          request cannot be built from — so *Ask again* is the fix as well as the
          retry. Shown only when the marker survived a restart, so a run that is
          live right now never draws it. */}
      {interrupted && (
        <div className="mx-4 mb-2 flex flex-wrap items-center gap-3 rounded-lg border border-amber-900/60 bg-amber-950/40 px-3 py-2 text-xs text-amber-300">
          <span className="flex-1">
            The last turn here was interrupted — the app closed while it was still running.
          </span>
          <button
            onClick={onRetryInterrupted}
            className="shrink-0 rounded border border-amber-800/60 px-2 py-0.5 transition hover:border-amber-700 hover:text-amber-200"
          >
            Ask again
          </button>
          {interrupted.canRewind && (
            <button
              onClick={onRewindInterrupted}
              className="shrink-0 rounded border border-amber-800/60 px-2 py-0.5 transition hover:border-amber-700 hover:text-amber-200"
              title="Put back the files that run changed, before trying again"
            >
              Put its files back
            </button>
          )}
          <button
            onClick={onDismissInterrupted}
            className="shrink-0 rounded px-2 py-0.5 text-amber-400/80 transition hover:text-amber-200"
          >
            Dismiss
          </button>
        </div>
      )}

      {/* The meter in the header turns amber then red; this is the same fact with
          the fix next to it. `/compact` is the existing command, so there is no
          second code path to keep in step. */}
      {usedFraction > 0.8 && !busy && (
        <div className="mx-4 mb-2 flex items-center gap-3 rounded-lg border border-amber-900/60 bg-amber-950/40 px-3 py-2 text-xs text-amber-300">
          <span className="flex-1">
            This conversation is about {Math.round(usedFraction * 100)}% of the model&apos;s context
            window.
          </span>
          <button
            onClick={() => onSend("/compact")}
            className="shrink-0 rounded border border-amber-800/60 px-2 py-0.5 transition hover:border-amber-700 hover:text-amber-200"
            title="Summarise the conversation and carry on from the summary"
          >
            Summarise it
          </button>
        </div>
      )}

      {error && (
        <div className="mx-4 mb-2 rounded-lg border border-red-900/60 bg-red-950/40 px-3 py-2 text-xs text-red-300">
          {error}
        </div>
      )}

      <div className="shrink-0 px-4 pb-4">
        <div className="mx-auto max-w-3xl">
          {tasks.length > 0 && <TaskList tasks={tasks} />}

          {questions.map((request) => (
            <QuestionPrompt
              key={request.requestId}
              request={request}
              onSubmit={onAnswerQuestion}
              onSkip={onSkipQuestion}
            />
          ))}

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
                    aria-label={`Remove ${file.name}`}
                    title={`Remove ${file.name}`}
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

          {chat && (
            <div className="mb-2 flex flex-wrap items-center gap-2">
              <div
                className="flex items-center gap-0.5 rounded-lg bg-neutral-900 p-0.5"
                title={AGENT_MODES.find((m) => m.id === chat.agent_mode)?.hint}
              >
                {AGENT_MODES.map((entry) => {
                  const active = chat.agent_mode === entry.id;
                  return (
                    <button
                      key={entry.id}
                      onClick={() => onAgentMode(entry.id)}
                      title={entry.hint}
                      className={`rounded-md px-2 py-0.5 text-[12px] transition duration-150 ${
                        active
                          ? "bg-sky-500/15 font-medium text-sky-300"
                          : "text-neutral-500 hover:text-neutral-300"
                      }`}
                    >
                      {entry.label}
                    </button>
                  );
                })}
              </div>

              {chat.agent_mode === "standard" && (
                <div
                  className="flex items-center gap-0.5 rounded-lg bg-neutral-900 p-0.5"
                  title={MODES.find((m) => m.id === chat.permission_mode)?.hint}
                >
                  {MODES.map((mode) => {
                    const active = chat.permission_mode === mode.id;
                    const tone =
                      mode.id === "skip"
                        ? "bg-red-500/15 text-red-300"
                        : "bg-sky-500/15 text-sky-300";
                    return (
                      <button
                        key={mode.id}
                        onClick={() => onModeChange(mode.id)}
                        title={mode.hint}
                        className={`rounded-md px-1.5 py-0.5 text-[12px] transition duration-150 ${
                          active ? `${tone} font-medium` : "text-neutral-500 hover:text-neutral-300"
                        }`}
                      >
                        {mode.label}
                      </button>
                    );
                  })}
                </div>
              )}

              <button
                onClick={() => onIncognito(!chat.incognito)}
                title={
                  chat.incognito
                    ? "This chat does not feed memory. Click to let it remember again."
                    : "Off the record: nothing said here is remembered, and no memory is shown"
                }
                className={`rounded-lg px-2 py-1 text-[12px] transition duration-150 ${
                  chat.incognito
                    ? "bg-amber-500/15 font-medium text-amber-300"
                    : "text-neutral-500 hover:bg-neutral-900 hover:text-neutral-300"
                }`}
              >
                Incognito
              </button>

              <div
                className="flex items-center gap-0.5 rounded-lg bg-neutral-900 p-0.5"
                title={EGRESS.find((e) => e.id === chat.egress)?.hint}
              >
                {EGRESS.map((rung) => {
                  const active = chat.egress === rung.id;
                  return (
                    <button
                      key={rung.id}
                      onClick={() => onEgress(rung.id)}
                      title={rung.hint}
                      className={`rounded-md px-1.5 py-0.5 text-[12px] transition duration-150 ${
                        active
                          ? "bg-amber-500/15 font-medium text-amber-300"
                          : "text-neutral-500 hover:text-neutral-300"
                      }`}
                    >
                      {rung.label}
                    </button>
                  );
                })}
              </div>

              <span className="ml-auto hidden text-[11px] text-neutral-600 sm:block">
                Shift+Tab to change mode
              </span>

              {chat.incognito && (
                <span className="w-full text-[11px] text-amber-300/80">
                  This chat does not feed memory — nothing here is remembered, and no memory is
                  shown to it.
                </span>
              )}

              {chat.egress !== "all" && (
                <span className="w-full text-[11px] text-amber-300/80">
                  {chat.egress === "none"
                    ? "This chat reaches no network at all":
                    "This chat may read the web but send nothing out"}
                  {" — "}
                  {EGRESS_NOTE}
                </span>
              )}
            </div>
          )}

          <div className="flex items-end gap-2 rounded-2xl border border-neutral-800/70 bg-neutral-900 p-2 transition duration-200 focus-within:border-neutral-600">
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
              aria-label="Attach files or images"
              className="rounded-lg px-2 py-1.5 text-neutral-500 transition duration-150 hover:bg-neutral-800 hover:text-neutral-200"
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
              aria-label="Message"
              className="max-h-[200px] flex-1 resize-none bg-transparent px-1 py-1.5 text-sm text-neutral-100 outline-hidden"
            />
            <button
              onClick={submit}
              disabled={busy || (!draft.trim() && attachments.length === 0)}
              className={`rounded-xl px-3 py-1.5 text-sm font-medium text-accent-fg transition duration-150 disabled:cursor-not-allowed disabled:bg-neutral-800 disabled:text-neutral-600 ${
                busy ? "hidden" : "bg-sky-600 hover:bg-sky-500"
              }`}
            >
              Send
            </button>
            {busy && (
              <button
                onClick={onStop}
                className="rounded-xl border border-neutral-700 px-3 py-1.5 text-sm text-neutral-200 transition duration-150 hover:border-red-800 hover:bg-red-950/40 hover:text-red-300"
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

/**
 * One stored row. Memoised because a streaming turn re-renders this whole view
 * every frame; only the message whose props actually changed — the live answer
 * is drawn outside the transcript, so normally none of them — is rendered
 * again. The callbacks are `undefined` for almost every row, which is what
 * makes the default shallow compare enough.
 */
const Bubble = memo(function Bubble({
  message,
  onEdit,
  onRetry,
  onRestore,
}: {
  message: MessageRow;
  /** Set on the newest user turn, which can be edited and sent again. */
  onEdit?: (text: string) => void;
  /** Set on a user turn whose answer was replaced, to swap it back. */
  onRestore?: () => void;
  /** Set on the newest answer, which can be asked for again. */
  onRetry?: () => void;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  let attachments: Attachment[] = [];
  if (message.attachments_json) {
    try {
      attachments = JSON.parse(message.attachments_json) as Attachment[];
    } catch {
      attachments = [];
    }
  }

  if (message.role === "user") {
    if (draft !== null && onEdit) {
      return (
        <div className="flex flex-col items-end gap-2">
          <textarea
            autoFocus
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") setDraft(null);
              if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) onEdit(draft);
            }}
            rows={3}
            className="w-[85%] resize-y rounded-2xl rounded-br-lg border border-neutral-700 bg-neutral-800 px-4 py-2.5 text-sm whitespace-pre-wrap text-neutral-100 outline-hidden focus:border-neutral-500"
          />
          <div className="flex items-center gap-2">
            <span className="text-[11px] text-neutral-600">Ctrl+Enter to resend</span>
            <button
              onClick={() => setDraft(null)}
              className="rounded-md px-2 py-1 text-[11px] text-neutral-500 transition hover:text-neutral-300"
            >
              Cancel
            </button>
            <button
              onClick={() => onEdit(draft)}
              disabled={!draft.trim()}
              className="rounded-md bg-sky-600 px-2 py-1 text-[11px] font-medium text-accent-fg transition hover:bg-sky-500 disabled:opacity-50"
            >
              Save &amp; resend
            </button>
          </div>
        </div>
      );
    }
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
          <div className="max-w-[85%] rounded-2xl rounded-br-lg bg-neutral-800 px-4 py-2.5 text-sm leading-relaxed whitespace-pre-wrap text-neutral-100">
            {message.content}
          </div>
        )}
        {/* Always in the DOM, never hover-only: an affordance that appears when
            pointed at is invisible to the keyboard and to anyone who has not
            found it yet. It is quiet instead — 70% until you reach for it. */}
        <div className="flex items-center gap-0.5 opacity-70 transition duration-150 hover:opacity-100">
          {message.content && (
            <CopyButton text={message.content} label="Copy this message" />
          )}
          {onEdit && (
            <IconButton
              icon="edit"
              label="Edit this message and ask again — the answers after it are replaced"
              onClick={() => setDraft(message.content ?? "")}
            />
          )}
          {onRestore && (
            <IconButton
              icon="restore"
              label="Bring back the answer this replaced — the current one is kept, so it is a swap"
              onClick={onRestore}
            />
          )}
        </div>
      </div>
    );
  }

  if (message.role === "tool") {
    return (
      <details className="rounded-xl border border-neutral-800/80 bg-neutral-900/40 px-3 py-2">
        <summary className="cursor-pointer text-xs text-neutral-500">tool result</summary>
        <pre className="mt-2 overflow-x-auto whitespace-pre-wrap text-xs text-neutral-400">
          {message.content}
        </pre>
      </details>
    );
  }

  return (
    <div className="flex flex-col">
      {message.reasoning_content && (
        <ThinkingBlock text={message.reasoning_content} streaming={false} />
      )}
      <Markdown>{message.content ?? ""}</Markdown>
      {message.content && (
        <div className="mt-1 flex items-center gap-0.5 opacity-70 transition duration-150 hover:opacity-100">
          <CopyButton text={message.content} label="Copy this answer as Markdown" />
          {onRetry && (
            <IconButton
              icon="retry"
              label="Ask the same question again, replacing this answer"
              onClick={onRetry}
            />
          )}
        </div>
      )}
    </div>
  );
});
