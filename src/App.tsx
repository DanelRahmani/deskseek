import { Fragment, useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";
import {
  api,
  onApproval,
  onArtifact,
  onComputer,
  onDelta,
  onDone,
  onDragDrop,
  onError,
  onGuard,
  onMcpReady,
  onNotify,
  onNotifyOpen,
  onQuestion,
  onQuickSent,
  onScheduledFired,
  onShot,
  onTool,
  onTray,
} from "./api";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { pathToAttachment, toAttachment } from "./lib/attachments";
import type {
  ApprovalEvent,
  ArtifactDetail,
  Attachment,
  ChatRow,
  DoneEvent,
  Egress,
  InterruptedRun,
  MessageHit,
  MessageRow,
  ModelInfo,
  NotebookRow,
  PermissionMode,
  ProjectRow,
  PromptView,
  QuestionEvent,
  SectionKind,
  Settings,
  ToolRun,
} from "./types";
import { Sidebar } from "./components/Sidebar";
import { ChatView } from "./components/ChatView";
import { NotebookShell } from "./components/NotebookShell";
import { AGENT_MODES } from "./components/ChatView";
import { SettingsDialog } from "./components/SettingsDialog";
import type { SettingsSection } from "./components/SettingsDialog";
import { CustomiseDialog } from "./components/CustomiseDialog";
import type { CustomiseSection } from "./components/CustomiseDialog";
import { ProjectsPanel } from "./components/ProjectsPanel";
import { answerFrom } from "./components/QuestionPrompt";
import { ArtifactsGallery } from "./components/ArtifactsGallery";
import { ScheduledPanel } from "./components/ScheduledPanel";
import { DEFAULT_SPEND_DAYS, SpendPanel } from "./components/SpendPanel";
import { SetupPanel } from "./components/SetupPanel";
import { FilesPanel } from "./components/FilesPanel";
import { ArtifactPanel } from "./components/ArtifactPanel";
import { ContextPanel } from "./components/ContextPanel";
import { TabStrip } from "./components/TabStrip";
import { CommandPalette } from "./components/CommandPalette";
import { ContextMenu } from "./components/ContextMenu";
import { ShortcutSheet } from "./components/ShortcutSheet";
import { AudioOverviewDialog } from "./components/AudioOverviewDialog";
import {
  loadShell,
  openChatIn,
  openInOtherPane,
  pruneShell,
  saveShell,
  setPaneChat,
  toggleSplitPanes,
} from "./lib/shell";
import type { Shell } from "./lib/shell";
import { resolveShortcut } from "./lib/shortcuts";
import { buildPalette } from "./lib/commands";
import { resolveMode, resolveTheme } from "./lib/theme";
import { tokensFromChars } from "./lib/context";

type Panel =
  | "none"
  | "files"
  | "artifact"
  | "context"
  | "projects"
  | "artifacts"
  | "scheduled"
  | "spend";

/** Everything that belongs to one conversation's live turn. */
interface SessionRuntime {
  busy: boolean;
  stream: { content: string; reasoning: string } | null;
  tools: ToolRun[];
  approvals: ApprovalEvent[];
  /** Questions the model is waiting on for this chat. */
  questions: QuestionEvent[];
  usage: DoneEvent | null;
  /** Last failure for this chat, cleared when a new turn starts. */
  error: string | null;
}

const IDLE: SessionRuntime = {
  busy: false,
  stream: null,
  tools: [],
  approvals: [],
  questions: [],
  usage: null,
  error: null,
};

const HELP =
  "/mode ask|standard|plan|agent · /summarise review what is worth remembering · /skill switch a skill on · /compact summarise and reset the context · /files · /memory · /skills · /mcp · /prompts · /projects · /artifacts · /scheduled · /spend · /setup · /folder · /new · /export · /audio · /rewind · /help";

/** Bytes as something a person reads: whole KB below a megabyte, one decimal above. */
function fileSize(bytes: number): string {
  const kb = bytes / 1024;
  return kb < 1024 ? `${Math.max(1, Math.round(kb))} KB` : `${(kb / 1024).toFixed(1)} MB`;
}

/**
 * A notebook's name as every list draws it: its symbol, then its title.
 *
 * One function rather than a concatenation at each call site, because the tab, the
 * sidebar and the pane's own picker have to agree about what a notebook is called —
 * and "🦠 Biology" is the name when it has a symbol, not two things that happen to be
 * printed next to each other.
 */
function notebookLabel(notebook: NotebookRow | undefined): string {
  if (!notebook) return "Notebook";
  return notebook.symbol ? `${notebook.symbol} ${notebook.title}` : notebook.title;
}

export default function App() {
  // A crash on demand, so the error boundary can be eyeballed without editing
  // code: `localStorage.setItem("deskseek.crash", "1")` then reload. It clears
  // itself, so *Try again* recovers and the draft-preservation check is real.
  if (localStorage.getItem("deskseek.crash")) {
    localStorage.removeItem("deskseek.crash");
    throw new Error("Deliberate crash: deskseek.crash was set.");
  }

  const [settings, setSettings] = useState<Settings | null>(null);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [keyMask, setKeyMask] = useState<string | null>(null);
  const [documentsDir, setDocumentsDir] = useState("");
  const [kind, setKind] = useState<SectionKind>("chat");
  // Whether the sidebar is listing notebooks instead of conversations. A *view* rather
  // than a section: opening a notebook does not make the composer a notebook's.
  const [notebooksView, setNotebooksView] = useState(false);
  const [chats, setChats] = useState<ChatRow[]>([]);
  // Every chat seen so far, by id: tabs and panes outlive the sidebar's current
  // section, so they need rows of their own to render from.
  const [chatsById, setChatsById] = useState<Record<string, ChatRow>>({});
  /** Notebooks, for the same reason and in the same shape: a tab id is one or the
   *  other, and the pane asks which. */
  const [notebooksById, setNotebooksById] = useState<Record<string, NotebookRow>>({});
  const [shell, setShell] = useState<Shell>(loadShell);
  // Transcripts and drafts are per chat, so two panes can show two
  // conversations at once and switching back does not refetch.
  const [messagesByChat, setMessagesByChat] = useState<Record<string, MessageRow[]>>({});
  const [attachmentsByChat, setAttachmentsByChat] = useState<Record<string, Attachment[]>>({});
  // True while files are being dragged over the window, for the drop overlay.
  const [dropping, setDropping] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  // Which rail entry the dialog opens on, so Ctrl+M and `/memory` land on Memory
  // rather than on whatever was looked at last.
  const [settingsSection, setSettingsSection] = useState<SettingsSection>("appearance");
  const [showSetup, setShowSetup] = useState(false);
  const [panel, setPanel] = useState<Panel>("none");
  // The Customise hub, and which of its entries it opens on: `/skills` and
  // `/mcp` are two doors into the one room.
  const [customise, setCustomise] = useState<{ open: boolean; section: CustomiseSection }>({
    open: false,
    section: "skills",
  });
  const [palette, setPalette] = useState(false);
  // The shortcut list, on top of everything: `?` is the one chord nothing else
  // in the UI advertises.
  const [sheet, setSheet] = useState(false);
  /**
   * The chat an audio overview is being made for, or null. Opened by `/audio`
   * rather than by a button: it is a chat-level action like `/export` and
   * `/compact`, and the composer already is where a chat's actions live.
   */
  const [audioOverview, setAudioOverview] = useState<string | null>(null);
  // A right-clicked conversation's menu, at the cursor. `fixed` at the root so
  // the scrolling sidebar cannot clip it.
  const [menu, setMenu] = useState<{ id: string; x: number; y: number } | null>(null);
  // The row whose title is being edited in the sidebar. Owned here because the
  // menu that starts it lives at the root, above the list.
  const [renamingId, setRenamingId] = useState<string | null>(null);
  // Bumped by the window chord, not read as a flag: the open chat scrolls to its
  // newest message, and a second press has to mean something too.
  const [jumpToLatest, setJumpToLatest] = useState(0);
  // Saved prompts, so the composer's menu and the palette can offer them without
  // a round trip each time one is opened.
  const [prompts, setPrompts] = useState<PromptView[]>([]);
  // Text the palette asked to be put in the composer. The counter is what makes
  // picking the same prompt twice land twice.
  const [insert, setInsert] = useState<{ text: string; seq: number } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  // Full-text results for the sidebar's search box. The query itself lives in
  // the Sidebar; this side debounces it and owns the results.
  const [hits, setHits] = useState<MessageHit[]>([]);
  // Live turn state, keyed by chat — so a run in a background chat keeps
  // streaming while the user reads a different conversation.
  const [sessions, setSessions] = useState<Record<string, SessionRuntime>>({});
  // Chats that finished while they were not the one on screen. Not part of
  // `sessions`: it is about whether the user has *looked*, which outlives a turn.
  const [unread, setUnread] = useState<Record<string, boolean>>({});
  // Turns the last session left in flight, by chat. Loaded once at startup: the
  // marker only exists while a run does, so anything here is from a process that
  // died mid-turn.
  const [interrupted, setInterrupted] = useState<Record<string, InterruptedRun>>({});
  // Process-wide, not per chat: the kill switch stops everything, so one banner
  // in every pane is the truth rather than a per-chat guess.
  const [computer, setComputer] = useState<{ paused: boolean; reason: string | null }>({
    paused: false,
    reason: null,
  });
  // The artifact currently in the panel. One at a time: the panel is a reading
  // surface, not a workspace.
  const [artifact, setArtifact] = useState<{ chatId: string; detail: ArtifactDetail } | null>(null);
  // Not ready until the first `mcp-ready` arrives: at launch the model genuinely
  // has no MCP tools yet, and the header should say so rather than imply zero.
  const [mcp, setMcp] = useState({ ready: false, toolCount: 0, failedCount: 0 });

  const patch = useCallback((chatId: string, fn: (rt: SessionRuntime) => SessionRuntime) => {
    setSessions((prev) => ({ ...prev, [chatId]: fn(prev[chatId] ?? IDLE) }));
  }, []);

  // Text arrives token by token and every commit re-renders the transcript, so
  // the deltas are held here and committed once a frame instead. A fast stream
  // then costs a render per frame rather than one per token, which is the whole
  // reason a long chat used to stutter.
  const pendingDeltas = useRef<Record<string, { content: string; reasoning: string }>>({});
  const deltaFrame = useRef<number | null>(null);

  const flushDeltas = useCallback(() => {
    deltaFrame.current = null;
    const batch = pendingDeltas.current;
    pendingDeltas.current = {};
    const ids = Object.keys(batch);
    if (ids.length === 0) return;
    setSessions((prev) => {
      const next = { ...prev };
      for (const id of ids) {
        const rt = next[id];
        // No stream means the turn ended between the delta and this frame; the
        // text is stored by then and the transcript is about to be reloaded, so
        // dropping it here loses nothing. Keeping it would put a dead turn's
        // tokens at the head of the next one.
        if (!rt?.stream) continue;
        next[id] = {
          ...rt,
          stream: {
            content: rt.stream.content + batch[id].content,
            reasoning: rt.stream.reasoning + batch[id].reasoning,
          },
        };
      }
      return next;
    });
  }, []);

  const { tabs, panes, focus, splitAt } = shell;
  const activeId = panes[focus] ?? null;

  /** The notebook list changed under us — a copy, a delete — so read it again. */
  const refreshNotebooks = useCallback(async () => {
    const list = await api.listNotebooks().catch(() => [] as NotebookRow[]);
    setNotebooksById(Object.fromEntries(list.map((n) => [n.id, n])));
  }, []);

  // Tab and pane bookkeeping lives in lib/shell.ts so its rules can be checked
  // without a browser.
  const showChat = useCallback((id: string) => setShell((s) => openChatIn(s, id)), []);
  const pickPane = useCallback(
    (pane: number, id: string | null) => setShell((s) => setPaneChat(s, pane, id)),
    [],
  );
  const closeTab = useCallback((id: string) => setShell((s) => pruneShell(s, (t) => t === id)), []);
  const toggleSplit = useCallback(() => setShell(toggleSplitPanes), []);

  const loadMessages = useCallback(async (id: string) => {
    const msgs = await api.getMessages(id);
    setMessagesByChat((prev) => ({ ...prev, [id]: msgs }));
  }, []);

  /** Show a chat beside what is already open: split if needed, other pane. */
  const openInSplit = useCallback(
    (id: string) => {
      setShell((s) => openInOtherPane(s, id));
      if (!(id in messagesByChat)) loadMessages(id).catch((e) => setError(String(e)));
    },
    [messagesByChat, loadMessages],
  );

  /**
   * Open a notebook in the focused pane, making one when there is none.
   *
   * A notebook is a pane like a chat is, so this is the same `showChat` — nothing
   * here loads a transcript, because a notebook's conversation is not a chat's.
   */
  const openNotebook = useCallback(
    async (id?: string) => {
      try {
        const list = id ? null : await api.listNotebooks();
        const target =
          id ??
          list?.find((n) => n.id in notebooksById)?.id ??
          list?.[0]?.id ??
          (await api.createNotebook("New notebook")).id;
        if (!(target in notebooksById)) {
          const fresh = await api.listNotebooks();
          setNotebooksById(Object.fromEntries(fresh.map((n) => [n.id, n])));
        }
        showChat(target);
      } catch (e) {
        setError(String(e));
      }
    },
    [notebooksById, showChat],
  );

  /**
   * A notebook where there was none.
   *
   * `openNotebook` opens one that is already open, which is right for a button that
   * says *Notebooks* and wrong for one that says *New*: the whole reason this exists is
   * that nothing in the app could make a second notebook. The name counts the ones that
   * exist, so a list of them can be told apart before each is renamed.
   */
  const newNotebook = useCallback(async () => {
    try {
      const books = await api.listNotebooks().catch(() => [] as NotebookRow[]);
      const book = await api.createNotebook(`Notebook ${books.length + 1}`);
      setNotebooksById((prev) => ({ ...prev, [book.id]: book }));
      showChat(book.id);
    } catch (e) {
      setError(String(e));
    }
  }, [showChat]);

  const activeIdRef = useRef<string | null>(null);
  activeIdRef.current = activeId;
  const kindRef = useRef<SectionKind>("chat");
  kindRef.current = kind;

  // A native drop gives paths, not Files. Each one is read on the Rust side,
  // run through the same downscale a picked file gets, and added to the
  // conversation the focused pane is showing.
  const attachPaths = useCallback(async (paths: string[]) => {
    if (paths.length === 0) return;
    const key = activeIdRef.current ?? "";
    const added: Attachment[] = [];
    for (const path of paths) {
      try {
        added.push(await pathToAttachment(await api.attachPath(activeIdRef.current, path)));
      } catch (e) {
        setError(String(e));
      }
    }
    if (added.length === 0) return;
    setAttachmentsByChat((prev) => ({
      ...prev,
      [key]: [...(prev[key] ?? []), ...added].slice(0, 8),
    }));
  }, []);

  // Startup only: load settings and every chat, then restore the saved shell.
  useEffect(() => {
    (async () => {
      const [s, m, k, docs, chatList, codingList, paused, mcpStatus, interruptedRuns] =
        await Promise.all([
          api.getSettings(),
          api.listModels(),
          api.maskedApiKey(),
          api.documentsDir(),
          api.listChats("chat"),
          api.listChats("coding"),
          // The hotkey may have been pressed before the window was listening.
          api.computerState(),
          // MCP may already have finished starting, in which case `mcp-ready` has
          // been and gone; ask rather than wait forever for an event we missed.
          api.mcpStatus(),
          // An interrupted turn is worth a banner, not a failed startup, so a
          // read that goes wrong just means no banner.
          api.interruptedRuns().catch(() => [] as InterruptedRun[]),
        ]);
      setSettings(s);
      setModels(m);
      setKeyMask(k);
      setDocumentsDir(docs);
      setComputer({ paused, reason: paused ? "Computer control is stopped." : null });
      setInterrupted(Object.fromEntries(interruptedRuns.map((r) => [r.chatId, r])));
      // A configured, enabled server that is neither running nor failed is still
      // starting. Anything else means discovery is over, event or no event.
      setMcp({
        ready: !mcpStatus.servers.some((s) => s.enabled && !s.running && s.error === null),
        toolCount: mcpStatus.toolCount,
        failedCount: mcpStatus.servers.filter((s) => s.enabled && s.error !== null).length,
      });

      // Both sections, so a tab restored from the last session has a title even
      // when it belongs to the other one.
      const all = [...chatList, ...codingList];
      setChatsById(Object.fromEntries(all.map((c) => [c.id, c])));
      // Notebooks are panes too, so a tab restored from the last session may be
      // one of them — and a notebook deleted while the app was closed must not
      // come back as a tab, which is the same rule as a chat's.
      const books = await api.listNotebooks().catch(() => [] as NotebookRow[]);
      setNotebooksById(Object.fromEntries(books.map((n) => [n.id, n])));
      // Chats and notebooks deleted while the app was closed must not come back
      // as tabs.
      const known = new Set([...all.map((c) => c.id), ...books.map((n) => n.id)]);
      setShell((prev) => pruneShell(prev, (id) => !known.has(id)));
      // A restored pane needs its transcript, which lives in the database — and
      // only a chat has one.
      await Promise.all(
        panes
          .filter((p): p is string => p !== null && all.some((c) => c.id === p))
          .map(loadMessages),
      );

      // First run: show setup. It covers the API key too, so the settings
      // dialog is not stacked on top of it.
      if (!s.setupDone) setShowSetup(true);
      else if (!k) setShowSettings(true);
    })().catch((e) => setError(String(e)));
  }, []);

  // The tokens written to <html> last time, so that clearing an override takes
  // the inline property away instead of leaving it pinned to a value the user
  // has just removed.
  const appliedTokens = useRef<Set<string>>(new Set());

  // The appearance is cosmetic and the settings only arrive after start-up, so
  // it is applied in an effect rather than during render. This is the only place
  // that writes to <html>, and it also caches the resolved tokens where the
  // inline script in index.html can find them before the app runs.
  useEffect(() => {
    if (!settings) return;
    const apply = () => {
      const systemDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
      const tokens = resolveTheme(
        {
          theme: settings.theme,
          mode: settings.themeMode,
          accent: settings.accent,
          fontUi: settings.fontUi,
          fontCode: settings.fontCode,
          fontSize: settings.fontSize,
        },
        systemDark,
      );
      const root = document.documentElement;
      const previous = appliedTokens.current;
      for (const name of previous) if (!(name in tokens)) root.style.removeProperty(name);
      previous.clear();
      for (const [name, value] of Object.entries(tokens)) {
        root.style.setProperty(name, value);
        previous.add(name);
      }
      const mode = resolveMode(settings.themeMode, systemDark);
      root.dataset.theme = settings.theme;
      root.dataset.mode = mode;
      root.dataset.reduceMotion = String(settings.reduceMotion);
      // Native scrollbars and form controls follow this, not our tokens.
      root.style.colorScheme = mode;
      try {
        localStorage.setItem("deskseek.theme", JSON.stringify(tokens));
      } catch {
        // A full or disabled store must not stop the theme applying.
      }
    };
    apply();
    // Following the OS means reacting when it changes while the app is open.
    if (settings.themeMode === "dark" || settings.themeMode === "light") return;
    const query = window.matchMedia("(prefers-color-scheme: dark)");
    query.addEventListener("change", apply);
    return () => query.removeEventListener("change", apply);
  }, [
    settings?.theme,
    settings?.themeMode,
    settings?.accent,
    settings?.fontUi,
    settings?.fontCode,
    settings?.fontSize,
    settings?.reduceMotion,
  ]);

  useEffect(() => {
    saveShell(shell);
  }, [shell]);

  useEffect(() => {
    if (!notice) return;
    const timer = setTimeout(() => setNotice(null), 6000);
    return () => clearTimeout(timer);
  }, [notice]);

  /** Open the Customise hub, optionally on one of its entries. */
  const openCustomise = useCallback((section: CustomiseSection = "skills") => {
    setCustomise({ open: true, section });
  }, []);

  /** The saved prompts the composer's menu and the palette both draw on. */
  const refreshPrompts = useCallback(async () => {
    try {
      setPrompts(await api.listPrompts());
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void refreshPrompts();
  }, [refreshPrompts]);

  /** Re-read one section's chats and fold them into the maps the UI reads. */
  const refreshChats = useCallback(async (which: SectionKind) => {
    const list = await api.listChats(which);
    if (kindRef.current === which) setChats(list);
    setChatsById((prev) => {
      const next = { ...prev };
      // Drop this section's chats that are gone (deleted elsewhere), keep the rest.
      for (const [id, chat] of Object.entries(next)) {
        if (chat.kind === which && !list.some((c) => c.id === id)) delete next[id];
      }
      for (const chat of list) next[chat.id] = chat;
      return next;
    });
    return list;
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlisten: Array<() => void> = [];
    Promise.all([
      onDelta((e) => {
        const held = (pendingDeltas.current[e.chatId] ??= { content: "", reasoning: "" });
        if (e.kind === "content") held.content += e.text;
        else held.reasoning += e.text;
        if (deltaFrame.current === null) deltaFrame.current = requestAnimationFrame(flushDeltas);
      }),
      onTool((e) => {
        patch(e.chatId, (rt) => {
          const at = rt.tools.findIndex((t) => t.id === e.id);
          const tools = [...rt.tools];
          if (e.phase === "requested") {
            const run: ToolRun = {
              id: e.id,
              tool: e.tool,
              args: e.args,
              status: "running",
              note: e.note ?? undefined,
            };
            if (at >= 0) tools[at] = run;
            else tools.push(run);
            return { ...rt, tools };
          }
          const base: ToolRun =
            tools[at] ?? { id: e.id, tool: e.tool, args: e.args, status: "ok" };
          const finished: ToolRun = {
            ...base,
            tool: e.tool,
            status: e.ok ? "ok" : "error",
            output: e.output,
          };
          if (at >= 0) tools[at] = finished;
          else tools.push(finished);
          return { ...rt, tools };
        });
      }),
      onApproval((e) => {
        patch(e.chatId, (rt) => ({
          ...rt,
          approvals: [...rt.approvals.filter((a) => a.requestId !== e.requestId), e],
        }));
      }),
      onQuestion((e) => {
        patch(e.chatId, (rt) => ({
          ...rt,
          questions: [...rt.questions.filter((q) => q.requestId !== e.requestId), e],
        }));
      }),
      onShot((e) => {
        // The picture arrives on its own event and is pinned to the tool call
        // that took it, so a screenshot stays with the step it belongs to.
        patch(e.chatId, (rt) => ({
          ...rt,
          tools: rt.tools.map((t) => (t.id === e.callId ? { ...t, shot: e.dataUrl } : t)),
        }));
      }),
      onComputer((e) => setComputer({ paused: e.paused, reason: e.reason })),
      onMcpReady((e) =>
        setMcp({ ready: true, toolCount: e.toolCount, failedCount: e.failedCount }),
      ),
      onArtifact((e) => {
        // The whole document arrives on the event, so the panel opens without a
        // second round trip. A model that publishes in the background chat
        // still gets read: the panel is not tied to the selected tab.
        setArtifact({ chatId: e.chatId, detail: e.artifact });
        setPanel("artifact");
      }),
      onDone((e) => {
        patch(e.chatId, (rt) => ({ ...rt, busy: false, usage: e }));
        // A turn that finished in a conversation the user is not reading leaves
        // a dot; the sidebar is how they notice without hunting for it.
        if (e.chatId !== activeIdRef.current) {
          setUnread((prev) => ({ ...prev, [e.chatId]: true }));
        }
      }),
      onNotify((e) => {
        // The model said something is done. Only worth a notice when it is not
        // the chat on screen — there, the answer itself is right there.
        if (!e.chatId || e.chatId !== activeIdRef.current) {
          setNotice(`${e.title}: ${e.body}`);
        }
        const id = e.chatId;
        if (id) setUnread((prev) => ({ ...prev, [id]: true }));
      }),
      onScheduledFired(() => {
        // The run lands in a chat the sidebar may not know about yet — a task
        // that starts a fresh chat every time makes a new one.
        void refreshChats(kindRef.current);
        setNotice("A scheduled task started a run.");
      }),
      onError((e) => {
        patch(e.chatId, (rt) => ({ ...rt, busy: false, error: e.message }));
      }),
      onGuard((e) => {
        // Held on the chat, not shown as a global notice: with two panes open
        // the refusal belongs to the conversation it happened in.
        patch(e.chatId, (rt) => ({
          ...rt,
          error: e.escalated
            ? e.reason
            : `Safety guard refused ${e.tool ?? "a command"}: ${e.reason}`,
        }));
      }),
      onDragDrop((e) => {
        // `enter`/`over` raise the overlay; `drop` also hands over the paths;
        // everything else clears it.
        if (e.type === "enter" || e.type === "over") setDropping(true);
        else setDropping(false);
        if (e.type === "drop") void attachPaths(e.paths);
      }),
    ]).then((fns) => {
      if (disposed) fns.forEach((f) => f());
      else unlisten = fns;
    });
    return () => {
      disposed = true;
      unlisten.forEach((f) => f());
      if (deltaFrame.current !== null) cancelAnimationFrame(deltaFrame.current);
    };
  }, [patch, attachPaths, refreshChats, flushDeltas]);

  /**
   * Open settings, optionally on one section. Memory used to be a side panel of
   * its own; it is a settings screen now, so every route to it goes through here
   * and the dialog is the one place that decides where it opens.
   */
  const openSettings = useCallback((section: SettingsSection = "appearance") => {
    setSettingsSection(section);
    setShowSettings(true);
  }, []);

  useEffect(() => {
    refreshChats(kind).catch((e) => setError(String(e)));
  }, [kind, refreshChats]);

  /**
   * Archive a chat or bring it back. Nothing is deleted, which is why this can be
   * a hover button on a row rather than something behind a confirmation.
   */
  const setArchived = useCallback(
    async (id: string, archived: boolean) => {
      try {
        await api.setChatArchived(id, archived);
        await refreshChats(kindRef.current);
        setNotice(archived ? "Archived — it is under Archived in the sidebar." : "Restored.");
      } catch (e) {
        setError(String(e));
      }
    },
    [refreshChats],
  );

  // The chat's own runtime is left alone: a run in a background chat keeps its
  // stream, tools and pending approvals while the user reads another one.
  const selectChat = useCallback(
    async (id: string) => {
      setError(null);
      // Looking at it is what clears the dot, not the run finishing.
      setUnread((prev) => {
        if (!prev[id]) return prev;
        const next = { ...prev };
        delete next[id];
        return next;
      });
      showChat(id);
      try {
        await loadMessages(id);
      } catch (e) {
        setError(String(e));
      }
    },
    [showChat, loadMessages],
  );

  /**
   * A toast that was clicked: the plugin reports no click of its own, so the
   * window's focus handler reads a focus arriving hot on a toast's heels as one
   * and tells us which conversation the toast was about.
   *
   * Its own effect rather than part of the listeners above, because it is
   * registered after `selectChat` exists — the same reason `kindRef` is a ref.
   */
  useEffect(() => {
    let stop: (() => void) | undefined;
    void onNotifyOpen((e) => {
      void selectChat(e.chatId);
    }).then((fn) => {
      stop = fn;
    });
    return () => stop?.();
  }, [selectChat]);

  /**
   * A question asked in the quick-entry window.
   *
   * That window is a separate webview, so the conversation it chose may be one
   * this window has never listed — and even when it is on screen, the answer
   * streams into a chat nobody is looking at. Refresh the sidebar first so the
   * row exists, open it, then come to the front: the point of asking from
   * another application is that you get taken back to the answer.
   */
  useEffect(() => {
    let stop: (() => void) | undefined;
    void onQuickSent((e) => {
      void (async () => {
        try {
          await refreshChats(kindRef.current);
          await selectChat(e.chatId);
          if (e.error) {
            // The turn never started, and the window that would have said so is
            // already hidden — this is the only place left to say it.
            setError(e.error);
            return;
          }
          await getCurrentWindow().setFocus();
        } catch (err) {
          setError(String(err));
        }
      })();
    }).then((fn) => {
      stop = fn;
    });
    return () => stop?.();
  }, [refreshChats, selectChat]);

  const newChat = useCallback(async () => {
    const chat = await api.createChat(kindRef.current);
    await refreshChats(kindRef.current);
    await selectChat(chat.id);
  }, [refreshChats, selectChat]);

  /** A chat that starts inside a project: its folder, and its membership. */
  const newChatInProject = useCallback(
    async (project: ProjectRow) => {
      try {
        const chat = await api.createChat(kindRef.current, project.folder ?? null);
        await api.setChatProject(chat.id, project.id);
        await refreshChats(kindRef.current);
        setPanel("none");
        await selectChat(chat.id);
        setNotice(`New chat in "${project.name}".`);
      } catch (e) {
        setError(String(e));
      }
    },
    [refreshChats, selectChat],
  );

  /** Open one artifact in the panel, from the gallery or anywhere else. */
  const openArtifact = useCallback(async (artifactId: string) => {
    try {
      const detail = await api.getArtifact(artifactId);
      setArtifact({ chatId: detail.artifact.chat_id, detail });
      setPanel("artifact");
    } catch (e) {
      setError(String(e));
    }
  }, []);

  /** Pick the folder a coding chat works in; `pane` is the pane that asked. */
  const pickWorkspace = useCallback(
    async (chatId: string | null, pane: number) => {
      try {
        const { open: openDialog } = await import("@tauri-apps/plugin-dialog");
        const selected = await openDialog({
          directory: true,
          multiple: false,
          title: "Choose a folder DeskSeek may work in",
        });
        if (typeof selected !== "string") return;
        if (chatId) {
          await api.setChatWorkspace(chatId, selected);
        } else {
          chatId = (await api.createChat("coding", selected)).id;
          pickPane(pane, chatId);
        }
        await refreshChats("coding");
        await loadMessages(chatId);
        setNotice(`Working in ${selected}`);
      } catch (e) {
        setError(String(e));
      }
    },
    [pickPane, refreshChats, loadMessages],
  );

  const send = useCallback(
    async (text: string, chatId: string | null, pane: number) => {
      setError(null);
      setNotice(null);

      const command = text.trim().split(/\s+/)[0];
      if (text.trim().startsWith("/")) {
        switch (command) {
          case "/help":
            setNotice(HELP);
            return;
          case "/memory":
            openSettings("memory");
            return;
          case "/skills":
          case "/customise":
            openCustomise("skills");
            return;
          case "/mcp":
            openCustomise("connectors");
            return;
          case "/prompts":
            openCustomise("prompts");
            return;
          case "/projects":
            setPanel((current) => (current === "projects" ? "none" : "projects"));
            return;
          case "/notebook":
          case "/notebooks":
            // The list, not a notebook: opening one for them is how a second notebook
            // stayed invisible.
            setNotebooksView(true);
            return;
          case "/artifacts":
            setPanel((current) => (current === "artifacts" ? "none" : "artifacts"));
            return;
          case "/scheduled":
          case "/schedule":
            setPanel((current) => (current === "scheduled" ? "none" : "scheduled"));
            return;
          case "/spend":
          case "/usage":
            setPanel((current) => (current === "spend" ? "none" : "spend"));
            return;
          case "/setup":
            setShowSetup(true);
            return;
          case "/files":
            setPanel((current) => (current === "files" ? "none" : "files"));
            return;
          case "/settings":
            openSettings();
            return;
          case "/folder":
            await pickWorkspace(chatId, pane);
            return;
          case "/new":
            await newChat();
            return;
          case "/export":
            if (chatId) await exportChat(chatId);
            else setError("Open a chat to export.");
            return;
          case "/audio":
            if (chatId) setAudioOverview(chatId);
            else setError("Open a chat to make an audio overview of.");
            return;
          case "/rewind":
            if (chatId) await rewind(chatId);
            else setError("Open a chat to undo.");
            return;
          case "/summarise":
          case "/summarize": {
            if (!chatId) {
              setError("Open a chat to summarise.");
              return;
            }
            patch(chatId, (rt) => ({ ...rt, busy: true }));
            setNotice("Summarising the conversation…");
            try {
              const raw = await api.summarizeChat(chatId);
              const [summary, count, topic, logPath] = raw.split("\u001f");
              setNotice(
                `${summary} — ${count} suggestion(s).` +
                  (logPath ? ` Saved CHATLOG/${logPath.split(/[\\/]/).pop()} (${topic}).` : ""),
              );
              // Straight to where they are reviewed, but only when there is
              // something to review: a modal over an empty list helps nobody.
              if (Number(count) > 0) openSettings("memory");
            } catch (e) {
              setError(String(e));
            } finally {
              patch(chatId, (rt) => ({ ...rt, busy: false }));
            }
            return;
          }
          case "/mode": {
            if (!chatId) {
              setError("Open a chat first.");
              return;
            }
            const wanted = text.trim().split(/\s+/).slice(1).join(" ").toLowerCase();
            if (!wanted) {
              setNotice(
                `${AGENT_MODES.map((m) => `${m.id} (${m.label})`).join(" · ")} — /mode <name>, or Shift+Tab to cycle.`,
              );
              return;
            }
            if (!AGENT_MODES.some((m) => m.id === wanted)) {
              setNotice(
                `Unknown mode "${wanted}". One of: ${AGENT_MODES.map((m) => m.id).join(", ")}.`,
              );
              return;
            }
            // Written out rather than reused from the header handler: this keeps
            // `send` free of a dependency on a callback defined after it.
            await api.setChatMode(chatId, wanted);
            await refreshChats(kindRef.current);
            setNotice(
              wanted === "ask"
                ? "Ask mode: no tools."
                : wanted === "plan"
                  ? "Plan mode: it will read and propose, and change nothing."
                  : wanted === "agent"
                    ? "Agent mode: it runs unattended until the job is done."
                    : "Standard mode: writes and commands follow your approval setting.",
            );
            return;
          }
          case "/skill": {
            if (!chatId) {
              setError("Open a chat first.");
              return;
            }
            const installed = await api.listSkills(chatId);
            const wanted = text.trim().split(/\s+/).slice(1).join(" ").toLowerCase();
            if (!wanted) {
              setNotice(
                installed.length === 0
                  ? "No skills installed. /skills adds one, or ask the assistant to save one from the conversation."
                  : `Skills: ${installed
                      .map((s) => `${s.id}${s.enabled ? " ✓" : ""}`)
                      .join(", ")} — /skill <id> switches one on or off.`,
              );
              return;
            }
            const match = installed.find(
              (s) => s.id.toLowerCase() === wanted || s.name.toLowerCase() === wanted,
            );
            if (!match) {
              setNotice(
                `No skill called "${wanted}". Installed: ${installed
                  .map((s) => s.id)
                  .join(", ") || "none"}.`,
              );
              return;
            }
            // The whole enabled set, toggled — the command has to send the list,
            // so it must not drop the skills that are already on.
            const next = installed
              .filter((s) => (s.id === match.id ? !match.enabled : s.enabled))
              .map((s) => s.id);
            await api.setChatSkills(chatId, next);
            setNotice(`"${match.name}" ${match.enabled ? "switched off" : "switched on"}.`);
            return;
          }
          case "/compact": {
            if (!chatId) {
              setError("Nothing to compact yet — this conversation is still short.");
              return;
            }
            patch(chatId, (rt) => ({ ...rt, busy: true }));
            setNotice("Compacting the conversation…");
            try {
              await api.compactChat(chatId);
              await loadMessages(chatId);
              setNotice("Conversation compacted into a summary.");
            } catch (e) {
              setError(String(e));
            } finally {
              patch(chatId, (rt) => ({ ...rt, busy: false }));
            }
            return;
          }
          default:
            setNotice(`Unknown command ${command}. ${HELP}`);
            return;
        }
      }

      // The draft belongs to whatever the user was looking at, which is not
      // necessarily a chat yet — send() is what creates one.
      const draftKey = chatId ?? "";
      const outgoing = attachmentsByChat[draftKey] ?? [];

      try {
        if (!chatId) {
          chatId = (await api.createChat(kindRef.current)).id;
          await refreshChats(kindRef.current);
          pickPane(pane, chatId);
        }

        const list = await api.listChats(kindRef.current);
        const chat = list.find((c) => c.id === chatId);
        if (chat?.kind === "coding" && !chat.workspace) {
          setError("Choose a folder for this coding task first.");
          await pickWorkspace(chatId, pane);
          return;
        }

        setAttachmentsByChat((prev) => (prev[draftKey]?.length ? { ...prev, [draftKey]: [] } : prev));
        patch(chatId, () => ({
          ...IDLE,
          busy: true,
          stream: { content: "", reasoning: "" },
        }));
        setMessagesByChat((prev) => ({
          ...prev,
          [chatId as string]: [
            ...(prev[chatId as string] ?? []),
            {
              id: `tmp-${Date.now()}`,
              chat_id: chatId as string,
              seq: Number.MAX_SAFE_INTEGER,
              role: "user",
              content: text,
              reasoning_content: null,
              tool_calls_json: null,
              tool_call_id: null,
              attachments_json: outgoing.length ? JSON.stringify(outgoing) : null,
              created_at: Math.floor(Date.now() / 1000),
            },
          ],
        }));
        await api.sendMessage(chatId, text, outgoing);

        const [msgs] = await Promise.all([
          api.getMessages(chatId),
          refreshChats(kindRef.current),
        ]);
        if (chatId) setMessagesByChat((prev) => ({ ...prev, [chatId as string]: msgs }));
      } catch (e) {
        if (chatId) {
          patch(chatId, (rt) => ({ ...rt, error: rt.error ?? String(e) }));
          // A run that died mid-answer leaves the part that arrived as a stored
          // message. Fetching it is what puts that text on screen instead of the
          // error clearing the stream away with it.
          await loadMessages(chatId);
        }
      } finally {
        if (chatId) patch(chatId, (rt) => ({ ...rt, busy: false, stream: null }));
      }
    },
    [attachmentsByChat, loadMessages, newChat, openCustomise, pickPane, pickWorkspace, refreshChats],
  );

  const changeModel = useCallback(
    async (chatId: string, model: string, thinking: boolean, effort: string) => {
      // Patch the row first so the header updates without a round trip; the
      // refresh below only exists to keep the sidebar list in step.
      setChatsById((prev) =>
        prev[chatId] ? { ...prev, [chatId]: { ...prev[chatId], model, thinking, effort } } : prev,
      );
      try {
        await api.setChatModel(chatId, model, thinking, effort);
        await refreshChats(kindRef.current);
      } catch (e) {
        setError(String(e));
      }
    },
    [refreshChats],
  );

  const changeMode = useCallback(
    async (chatId: string, mode: PermissionMode) => {
      setChatsById((prev) =>
        prev[chatId] ? { ...prev, [chatId]: { ...prev[chatId], permission_mode: mode } } : prev,
      );
      try {
        await api.setChatPermissionMode(chatId, mode);
        await refreshChats(kindRef.current);
      } catch (e) {
        setError(String(e));
      }
    },
    [refreshChats],
  );

  // Whether this chat feeds memory. Privacy, so the row is patched only after
  // the write succeeds — the switch must never *look* on while it is off.
  const changeIncognito = useCallback(
    async (chatId: string, incognito: boolean) => {
      try {
        await api.setChatIncognito(chatId, incognito);
        setChatsById((prev) =>
          prev[chatId] ? { ...prev, [chatId]: { ...prev[chatId], incognito } } : prev,
        );
        await refreshChats(kindRef.current);
        setNotice(
          incognito
            ? "This chat is now off the record — nothing said here is remembered, and no memory is shown."
            : "This chat feeds memory again.",
        );
      } catch (e) {
        setError(String(e));
      }
    },
    [refreshChats],
  );

  // What a chat may send out. A declared policy is a refusal rather than a
  // prompt, so the notice says so plainly — and says the honest limit with it.
  const changeEgress = useCallback(
    async (chatId: string, egress: Egress) => {
      try {
        await api.setChatEgress(chatId, egress);
        setChatsById((prev) =>
          prev[chatId] ? { ...prev, [chatId]: { ...prev[chatId], egress } } : prev,
        );
        setNotice(
          egress === "none"
            ? "This chat now reaches no network at all. It is a policy, not a sandbox — a program it runs can still open a socket by itself."
            : egress === "read-only"
              ? "This chat may read the web but not send to it. A policy, not a sandbox."
              : "This chat has no declared network policy again.",
        );
      } catch (e) {
        setError(String(e));
      }
    },
    [],
  );

  const stop = useCallback(async (chatId: string) => {
    try {
      await api.stopChat(chatId);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  // How much rope a chat has. Stored as the mode's own id.
  const changeAgentMode = useCallback(
    async (chatId: string, mode: string) => {
      setChatsById((prev) =>
        prev[chatId] ? { ...prev, [chatId]: { ...prev[chatId], agent_mode: mode } } : prev,
      );
      try {
        await api.setChatMode(chatId, mode);
        await refreshChats(kindRef.current);
        setNotice(
          mode === "ask"
            ? "Ask mode: no tools."
            : mode === "plan"
              ? "Plan mode: it will read and propose, and change nothing."
              : mode === "agent"
                ? "Agent mode: it runs unattended until the job is done."
                : "Standard mode: writes and commands follow your approval setting.",
        );
      } catch (e) {
        setError(String(e));
      }
    },
    [refreshChats],
  );

  const decideApproval = useCallback(
    async (requestId: string, decision: "once" | "session" | "always" | "deny") => {
      // The prompt can belong to any chat, so clear it from whichever holds it.
      setSessions((prev) =>
        Object.fromEntries(
          Object.entries(prev).map(([id, rt]) => [
            id,
            { ...rt, approvals: rt.approvals.filter((a) => a.requestId !== requestId) },
          ]),
        ),
      );
      try {
        await api.respondApproval(requestId, decision);
      } catch (e) {
        setError(String(e));
      }
    },
    [],
  );

  /** Take a pending question off whichever chat holds it, and send the answer. */
  const clearQuestion = useCallback((requestId: string) => {
    setSessions((prev) =>
      Object.fromEntries(
        Object.entries(prev).map(([id, rt]) => [
          id,
          { ...rt, questions: rt.questions.filter((q) => q.requestId !== requestId) },
        ]),
      ),
    );
  }, []);

  const answerQuestion = useCallback(
    async (requestId: string, selected: string[][], free: string[]) => {
      clearQuestion(requestId);
      try {
        await api.respondQuestion(requestId, {
          answers: answerFrom(selected, free),
          cancelled: false,
        });
      } catch (e) {
        setError(String(e));
      }
    },
    [clearQuestion],
  );

  const skipQuestion = useCallback(
    async (requestId: string) => {
      clearQuestion(requestId);
      try {
        await api.respondQuestion(requestId, { answers: [], cancelled: true });
      } catch (e) {
        setError(String(e));
      }
    },
    [clearQuestion],
  );

  const deleteChat = useCallback(
    async (id: string) => {
      await api.deleteChat(id);
      await refreshChats(kindRef.current);
      setMessagesByChat((prev) => {
        if (!(id in prev)) return prev;
        const next = { ...prev };
        delete next[id];
        return next;
      });
      // The tab goes too; a pane showing it refills from the remaining tabs.
      setShell((s) => pruneShell(s, (t) => t === id));
    },
    [refreshChats],
  );

  const openMenu = useCallback((id: string, x: number, y: number) => setMenu({ id, x, y }), []);

  /**
   * A copy of a conversation, and of its folder when it owns one. The original is
   * untouched, and the copy starts at no spend — nothing was paid for it.
   */
  const duplicateChat = useCallback(
    async (chatId: string) => {
      try {
        const copy = await api.duplicateChat(chatId);
        await refreshChats(kindRef.current);
        setNotice(`Duplicated as "${copy.title}".`);
        await selectChat(copy.id);
      } catch (e) {
        setError(String(e));
      }
    },
    [refreshChats, selectChat],
  );

  const exportChatJson = useCallback(async (chatId: string) => {
    try {
      const path = await api.exportChatJson(chatId);
      const name = path.split(/[\\/]/).pop() ?? path;
      setNotice(`Wrote ${name} into this conversation's folder.`);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  // Importing is additive — a half-typed file cannot cost a chat — and the copy
  // is the transcript only, because the attachments live as files in the folder
  // the file came from. Say so rather than let the gaps look like a bug.
  const importChatJson = useCallback(async () => {
    try {
      const { open: openDialog } = await import("@tauri-apps/plugin-dialog");
      const selected = await openDialog({
        multiple: false,
        filters: [{ name: "DeskSeek conversation", extensions: ["json"] }],
        title: "Choose a conversation to import",
      });
      if (typeof selected !== "string") return;
      const chat = await api.importChatJson(selected);
      if (chat.kind !== kindRef.current) setKind(chat.kind);
      await refreshChats(kindRef.current);
      setNotice(`Imported "${chat.title}" — the transcript only, without its files.`);
      await selectChat(chat.id);
    } catch (e) {
      setError(String(e));
    }
  }, [refreshChats, selectChat]);

  /**
   * Save a hand-typed title. A `null` title is the sidebar saying the edit was
   * abandoned (Escape), which is a cancel and not a blank name.
   */
  const renameChat = useCallback(
    async (id: string, title: string | null) => {
      setRenamingId(null);
      if (title === null) return;
      try {
        await api.renameChat(id, title);
        await refreshChats(kindRef.current);
      } catch (e) {
        setError(String(e));
      }
    },
    [refreshChats],
  );

  const attach = useCallback(async (chatId: string | null, files: FileList | File[]) => {
    // Keyed by the draft, not the chat: files can be attached before a chat
    // exists, and send() picks them up from the same empty-string slot.
    const key = chatId ?? "";
    const added: Attachment[] = [];
    for (const file of Array.from(files)) {
      // One at a time, so a file too large to move reports itself instead of
      // taking the rest of the drop down with it.
      try {
        added.push(await toAttachment(file, chatId));
      } catch (e) {
        setError(String(e));
      }
    }
    if (added.length === 0) return;
    setAttachmentsByChat((prev) => ({
      ...prev,
      [key]: [...(prev[key] ?? []), ...added].slice(0, 8),
    }));
  }, []);

  const removeAttachment = useCallback((chatId: string | null, index: number) => {
    const key = chatId ?? "";
    setAttachmentsByChat((prev) => ({
      ...prev,
      [key]: (prev[key] ?? []).filter((_, i) => i !== index),
    }));
  }, []);

  // Show the user where this chat keeps its own files. The folder is worked out
  // on the Rust side the first time it is asked for, so the header never has to
  // guess a path.
  const revealChatFolder = useCallback(async (chatId: string) => {
    try {
      await api.revealPath(await api.chatFolder(chatId));
    } catch (e) {
      setError(String(e));
    }
  }, []);

  // Write the whole conversation to the documents folder. The file name comes
  // back so the notice can name it rather than say "somewhere".
  const exportChat = useCallback(async (chatId: string, asPdf = false) => {
    try {
      const path = asPdf ? await api.exportChatPdf(chatId) : await api.exportChat(chatId);
      const name = path.split(/[\\/]/).pop() ?? path;
      setNotice(`Exported to ${name} in your documents folder.`);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  // Put back the files the agent's last run changed. One run at a time, and it
  // names what it restored rather than claiming success silently.
  const rewind = useCallback(async (chatId: string) => {
    try {
      const paths = await api.rewindLastRun(chatId);
      if (paths.length === 0) {
        setNotice("Nothing to undo — this chat has no recorded file changes.");
        return;
      }
      const names = paths.map((path) => path.split(/[\\/]/).pop() ?? path);
      setNotice(
        `Put back ${names.length} file${names.length === 1 ? "" : "s"}: ${names.join(", ")}`,
      );
    } catch (e) {
      setError(String(e));
    }
  }, []);

  // Ask again from the last turn: the same question (regenerate) or an edited
  // one. The backend drops everything that followed it, so the model never sees
  // the answer it is being asked to replace.
  const resend = useCallback(async (chatId: string, text: string | null) => {
    patch(chatId, () => ({ ...IDLE, busy: true, stream: { content: "", reasoning: "" } }));
    try {
      await api.resendLastTurn(chatId, text);
      await loadMessages(chatId);
    } catch (e) {
      patch(chatId, (rt) => ({ ...rt, error: rt.error ?? String(e) }));
      // Same reason as `send`: the half that arrived is stored, so it has to be
      // fetched back before the stream is thrown away.
      await loadMessages(chatId);
    } finally {
      patch(chatId, (rt) => ({ ...rt, busy: false, stream: null }));
    }
  }, [loadMessages]);

  /**
   * A turn the last session left half-done. Two ways to put it right, and both
   * end by re-reading the markers rather than assuming: *Ask again* runs the turn
   * through the normal path (which drops the dangling tool calls a killed run
   * leaves, so the chat is usable again), and *Put its files back* is the
   * existing rewind. A second read is what clears the banner honestly — if the
   * action failed before the run started, the marker is still there and the
   * banner stays rather than lying about it.
   */
  const refreshInterrupted = useCallback(async () => {
    try {
      const runs = await api.interruptedRuns();
      setInterrupted(Object.fromEntries(runs.map((r) => [r.chatId, r])));
    } catch {
      // A banner is not worth an error of its own; the next launch re-reads it.
    }
  }, []);

  const retryInterrupted = useCallback(
    async (chatId: string) => {
      await resend(chatId, null);
      await refreshInterrupted();
    },
    [resend, refreshInterrupted],
  );

  const rewindInterrupted = useCallback(
    async (chatId: string) => {
      await rewind(chatId);
      await refreshInterrupted();
    },
    [rewind, refreshInterrupted],
  );

  const dismissInterrupted = useCallback(async (chatId: string) => {
    setInterrupted((prev) => {
      if (!(chatId in prev)) return prev;
      const next = { ...prev };
      delete next[chatId];
      return next;
    });
    try {
      await api.dismissInterrupted(chatId);
    } catch {
      // The banner is gone; a row that survived only reappears next launch.
    }
  }, []);

  /**
   * Swap back the answer a resend replaced. No model call and no busy state: both
   * answers are already recorded, so this is a reload of what is on screen.
   */
  const restoreTail = useCallback(
    async (chatId: string, afterSeq: number) => {
      try {
        await api.restoreDiscardedTail(chatId, afterSeq);
        await loadMessages(chatId);
      } catch (e) {
        setError(String(e));
      }
    },
    [loadMessages],
  );

  // A backup is a file, so the API key is not in it: the key lives in the OS
  // keychain, which is not a file this app can hand out.
  const backupProfile = useCallback(async () => {
    try {
      const path = await api.backupProfile();
      const name = path.split(/[\\/]/).pop() ?? path;
      setNotice(`Backed up the whole profile to ${name} in your documents folder.`);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  /**
   * Restoring replaces the database the running window is reading, so the honest
   * end is a reload: every open chat, transcript and stored shell belongs to the
   * profile that was just swapped out, and re-fetching all of it by hand is a
   * longer path to the same place.
   */
  const restoreProfile = useCallback(async () => {
    try {
      const { open: openDialog } = await import("@tauri-apps/plugin-dialog");
      const selected = await openDialog({
        multiple: false,
        filters: [{ name: "DeskSeek backup", extensions: ["zip"] }],
        title: "Choose a DeskSeek backup",
      });
      if (typeof selected !== "string") return;
      await api.restoreProfile(selected);
      window.location.reload();
    } catch (e) {
      setError(String(e));
    }
  }, []);

  // Sweeps what nothing can reach any more, and reports the size either way: the
  // number is the point as much as the pruning is, and it costs one read.
  const maintainDb = useCallback(async () => {
    try {
      const { pruned, bytes } = await api.maintainDb();
      const size = fileSize(bytes);
      setNotice(
        pruned === 0
          ? `The database is ${size}; there was nothing to prune.`
          : `Pruned ${pruned} unreachable row${pruned === 1 ? "" : "s"} — the database is now ${size}.`,
      );
    } catch (e) {
      setError(String(e));
    }
  }, []);

  // What was allowed and what it cost, as one file. The window is the spend
  // panel's, so the file and the screen say the same thing.
  const exportAudit = useCallback(async () => {
    try {
      const path = await api.exportAudit(DEFAULT_SPEND_DAYS);
      const name = path.split(/[\\/]/).pop() ?? path;
      setNotice(`Exported the decision log and the spend to ${name} in your documents folder.`);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  // The sidebar's search box is a full-text lookup over every message. The box
  // state lives in the Sidebar, so this debounces the keystrokes and owns the
  // results. A search is ranked by relevance, not by recency, so a common word
  // puts its most useful hits first — and has more of them than fit, which is
  // what `moreHits` is for.
  const searchTimer = useRef<number | null>(null);
  const [moreHits, setMoreHits] = useState(false);
  // Read by the paging below, which needs how many are already shown without
  // being a reason to re-render.
  const hitsRef = useRef<MessageHit[]>([]);
  hitsRef.current = hits;
  // The query the list on screen belongs to. A page that lands after the box has
  // changed must be dropped, not appended to a different search's results.
  const searchedFor = useRef("");
  const searchMessages = useCallback((query: string) => {
    if (searchTimer.current !== null) window.clearTimeout(searchTimer.current);
    const needle = query.trim();
    if (needle.length < 2) {
      searchedFor.current = "";
      setHits([]);
      setMoreHits(false);
      return;
    }
    searchTimer.current = window.setTimeout(() => {
      api
        .searchMessages(needle)
        .then((page) => {
          searchedFor.current = needle;
          setHits(page.hits);
          setMoreHits(page.more);
        })
        .catch(() => {
          searchedFor.current = "";
          setHits([]);
          setMoreHits(false);
        });
    }, 180);
  }, []);

  /** The next page of the search already on screen, appended to what is there. */
  const loadMoreHits = useCallback((query: string) => {
    const needle = query.trim();
    const offset = hitsRef.current.length;
    if (!needle || offset === 0 || needle !== searchedFor.current) return;
    api
      .searchMessages(needle, offset)
      .then((page) => {
        if (needle !== searchedFor.current) return;
        setHits((latest) => [...latest, ...page.hits]);
        setMoreHits(page.more);
      })
      .catch(() => setMoreHits(false));
  }, []);

  // The palette is global — both sections' chats, most recently used first — so
  // Ctrl+K finds a coding task while you are in chat and the reverse. The hint
  // already names the section, and picking one switches to it below.
  const paletteItems = useMemo(() => {
    const all = Object.values(chatsById).sort((a, b) => b.updated_at - a.updated_at);
    const items = buildPalette(all, !!activeId, activeId ? !!chatsById[activeId]?.archived : false);
    // Appended rather than folded into `buildPalette`: that function is the
    // built-in list, and a user's own prompts must not change what it answers.
    return [
      ...items,
      ...prompts.map((prompt) => ({
        id: `prompt:${prompt.id}`,
        label: prompt.name,
        hint: "Prompt — puts it in the composer",
      })),
    ];
  }, [chatsById, activeId, prompts]);
  const runPalette = useCallback(
    (id: string) => {
      setPalette(false);
      if (id.startsWith("prompt:")) {
        // Into the box, not sent: a saved prompt is text to edit first.
        const prompt = prompts.find((p) => p.id === id.slice("prompt:".length));
        if (prompt) setInsert((current) => ({ text: prompt.body, seq: (current?.seq ?? 0) + 1 }));
        return;
      }
      if (id.startsWith("chat:")) {
        const chatId = id.slice("chat:".length);
        const target = chatsById[chatId];
        // A chat from the other section: follow it there, or the sidebar and
        // the tab strip would still be showing the one you left.
        if (target && target.kind !== kindRef.current) setKind(target.kind);
        void selectChat(chatId);
        return;
      }
      switch (id) {
        case "new-chat":
          void newChat();
          return;
        case "quick-entry":
          // The palette lives inside the main window, so this is the one route to
          // the quick window that is not the chord.
          void api.showQuickEntry().catch((e) => setError(String(e)));
          return;
        case "settings":
          openSettings();
          return;
        case "memory":
          openSettings("memory");
          return;
        case "customise":
          openCustomise("skills");
          return;
        case "projects":
          setPanel((current) => (current === "projects" ? "none" : "projects"));
          return;
        case "artifacts":
          setPanel((current) => (current === "artifacts" ? "none" : "artifacts"));
          return;
        case "scheduled":
          setPanel((current) => (current === "scheduled" ? "none" : "scheduled"));
          return;
        case "spend":
          setPanel((current) => (current === "spend" ? "none" : "spend"));
          return;
        case "files":
          setPanel((current) => (current === "files" ? "none" : "files"));
          return;
        case "split":
          toggleSplit();
          return;
        case "backup":
          void backupProfile();
          return;
        case "restore":
          void restoreProfile();
          return;
        case "maintain":
          void maintainDb();
          return;
        case "export-audit":
          void exportAudit();
          return;
        case "export":
          if (activeIdRef.current) void exportChat(activeIdRef.current);
          return;
        case "export-pdf":
          if (activeIdRef.current) void exportChat(activeIdRef.current, true);
          return;
        case "export-json":
          if (activeIdRef.current) void exportChatJson(activeIdRef.current);
          return;
        case "import-chat":
          void importChatJson();
          return;
        case "duplicate":
          if (activeIdRef.current) void duplicateChat(activeIdRef.current);
          return;
        case "rewind":
          if (activeIdRef.current) void rewind(activeIdRef.current);
          return;
        case "archive":
          if (activeIdRef.current) void setArchived(activeIdRef.current, true);
          return;
        case "unarchive":
          if (activeIdRef.current) void setArchived(activeIdRef.current, false);
          return;
      }
    },
    [
      backupProfile,
      chatsById,
      exportAudit,
      exportChat,
      maintainDb,
      newChat,
      openCustomise,
      restoreProfile,
      rewind,
      selectChat,
      setArchived,
      toggleSplit,
    ],
  );

  // One window listener, resolved by the pure map in lib/shortcuts.ts. Every
  // action below is the same state change its button makes, so the keyboard can
  // never do something the UI cannot.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement | null;
      const typing =
        !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable);
      const action = resolveShortcut(e, typing);
      if (!action) return;
      e.preventDefault();
      switch (action) {
        case "new-chat":
          void newChat();
          return;
        case "close-tab":
          if (activeId) closeTab(activeId);
          return;
        case "rename-chat":
          // The editor is the sidebar's row, so this renames the open chat where
          // that row is. A tab from the other section has no row on screen, which
          // is the same limit the context menu's Rename already has.
          if (activeId) setRenamingId(activeId);
          return;
        case "toggle-files":
          setPanel((current) => (current === "files" ? "none" : "files"));
          return;
        case "open-memory":
          openSettings("memory");
          return;
        case "open-settings":
          openSettings();
          return;
        case "command-palette":
          setPalette((open) => !open);
          return;
        case "cheatsheet":
          setSheet((open) => !open);
          return;
        case "toggle-split":
          toggleSplit();
          return;
        case "focus-pane-0":
        case "focus-pane-1": {
          const pane = action === "focus-pane-0" ? 0 : 1;
          setShell((s) => (pane < s.panes.length ? { ...s, focus: pane } : s));
          return;
        }
        case "jump-to-latest":
          setJumpToLatest((n) => n + 1);
          return;
        case "dismiss":
          // Topmost first: the sheet and the palette sit above a dialog, which
          // covers a panel.
          if (menu) setMenu(null);
          else if (sheet) setSheet(false);
          else if (palette) setPalette(false);
          else if (customise.open) setCustomise((c) => ({ ...c, open: false }));
          else if (showSettings) setShowSettings(false);
          else if (showSetup) setShowSetup(false);
          else setPanel("none");
          return;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [activeId, closeTab, customise.open, menu, newChat, palette, runPalette, sheet, showSettings, showSetup, toggleSplit]);

  /**
   * A tray menu item the window has to run.
   *
   * Straight into `runPalette`, which is the same switch the command palette
   * uses — so the tray's "New chat" *is* the ＋ button and the palette's, rather
   * than a third thing that happens to look like it. Rust keeps the two items it
   * must own (Show and Quit) and never sends them here.
   */
  useEffect(() => {
    let stop: (() => void) | undefined;
    void onTray((e) => runPalette(e.id)).then((fn) => {
      stop = fn;
    });
    return () => stop?.();
  }, [runPalette]);

  const runningIds = useMemo(
    () => Object.keys(sessions).filter((id) => sessions[id].busy),
    [sessions],
  );
  const unreadIds = useMemo(() => Object.keys(unread), [unread]);

  /** Rough context estimate for one chat, using the app's one token rule. */
  const tokensFor = useCallback(
    (id: string) => {
      const chars =
        (messagesByChat[id] ?? []).reduce(
          (total, m) => total + (m.content?.length ?? 0) + (m.reasoning_content?.length ?? 0),
          0,
        ) + (settings?.systemPrompt.length ?? 0);
      return tokensFromChars(chars);
    },
    [messagesByChat, settings],
  );

  const panesRef = useRef<HTMLDivElement>(null);
  const dragging = useRef(false);

  const startDrag = (e: ReactPointerEvent<HTMLDivElement>) => {
    dragging.current = true;
    e.currentTarget.setPointerCapture(e.pointerId);
  };
  const dragDivider = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (!dragging.current || !panesRef.current) return;
    const rect = panesRef.current.getBoundingClientRect();
    if (rect.width === 0) return;
    const pct = ((e.clientX - rect.left) / rect.width) * 100;
    setShell((s) => ({ ...s, splitAt: Math.min(80, Math.max(20, pct)) }));
  };
  const endDrag = (e: ReactPointerEvent<HTMLDivElement>) => {
    dragging.current = false;
    if (e.currentTarget.hasPointerCapture(e.pointerId)) {
      e.currentTarget.releasePointerCapture(e.pointerId);
    }
  };

  return (
    <div className="flex h-full">
      <Sidebar
        view={notebooksView ? "notebooks" : kind}
        onView={(next) => {
          setNotebooksView(next === "notebooks");
          // The section is left alone when notebooks are showing: a notebook is not a
          // third kind of chat, so coming back to Chat returns to the one you were in.
          if (next !== "notebooks") setKind(next);
        }}
        chats={chats}
        activeId={activeId}
        hits={hits}
        moreHits={moreHits}
        onSearch={searchMessages}
        onMoreHits={loadMoreHits}
        onSelect={selectChat}
        onNew={newChat}
        onDelete={deleteChat}
        onSettings={() => openSettings()}
        onArchive={setArchived}
        onContextMenu={openMenu}
        renamingId={renamingId}
        onRename={renameChat}
        onMemory={() => openSettings("memory")}
        onCustomise={() => openCustomise("skills")}
        onProjects={() =>
          setPanel((current) => (current === "projects" ? "none" : "projects"))
        }
        notebooks={Object.values(notebooksById).sort((a, b) => b.updatedAt - a.updatedAt)}
        onOpenNotebook={(id) => void openNotebook(id)}
        onNewNotebook={() => void newNotebook()}
        onArtifacts={() =>
          setPanel((current) => (current === "artifacts" ? "none" : "artifacts"))
        }
        onScheduled={() =>
          setPanel((current) => (current === "scheduled" ? "none" : "scheduled"))
        }
        onSpend={() => setPanel((current) => (current === "spend" ? "none" : "spend"))}
        keyMask={keyMask}
        runningIds={runningIds}
        unreadIds={unreadIds}
      />

      {panel === "files" && (
        // Keyed by chat: switching conversations must not show the last
        // chat's tree, and remounting is what re-watches the folder.
        <FilesPanel
          key={activeId ?? "none"}
          chatId={activeId}
          onClose={() => setPanel("none")}
        />
      )}

      <div className="flex min-w-0 flex-1 flex-col">
        <TabStrip
          tabs={tabs.map((id) => ({
            id,
            title: chatsById[id]?.title ?? notebookLabel(notebooksById[id]),
            notebook: !chatsById[id] && !!notebooksById[id],
          }))}
          panes={panes}
          focus={focus}
          split={panes.length > 1}
          runningIds={runningIds}
          onSelect={selectChat}
          onClose={closeTab}
          onToggleSplit={toggleSplit}
        />

        <div ref={panesRef} className="flex min-h-0 flex-1">
          {panes.map((id, pane) => {
            // Each pane reads its own chat's live turn, so a background run keeps
            // streaming into the pane that is not focused.
            const rt = (id && sessions[id]) || IDLE;
            const row = id ? (chatsById[id] ?? null) : null;
            // A tab id is one or the other, and this is the one place that asks.
            const notebook = id ? (notebooksById[id] ?? null) : null;
            const max = models.find((m) => m.id === row?.model)?.context ?? 1_000_000;
            return (
              <Fragment key={pane}>
                {pane > 0 && (
                  <div
                    onPointerDown={startDrag}
                    onPointerMove={dragDivider}
                    onPointerUp={endDrag}
                    onPointerCancel={endDrag}
                    className="w-1 shrink-0 cursor-col-resize bg-neutral-800 transition hover:bg-sky-600"
                    title="Drag to resize"
                  />
                )}
                <div
                  // Clicking anywhere in a pane aims the sidebar, the tab strip
                  // and the file tree at it.
                  onPointerDownCapture={() =>
                    setShell((s) => (s.focus === pane ? s : { ...s, focus: pane }))
                  }
                  className="flex min-h-0 min-w-0 overflow-hidden"
                  style={
                    panes.length > 1
                      ? { flexGrow: pane === 0 ? splitAt : 100 - splitAt, flexBasis: 0 }
                      : { flexGrow: 1, flexBasis: 0 }
                  }
                >
                  {notebook ? (
                    <NotebookShell
                      notebookId={notebook.id}
                      notebooks={Object.values(notebooksById)}
                      models={models}
                      onOpen={showChat}
                      onChanged={() => void refreshNotebooks()}
                      onDeleted={(gone) => {
                        setShell((s) => pruneShell(s, (t) => t === gone));
                        void refreshNotebooks();
                      }}
                    />
                  ) : (
                    <ChatView
                      chat={row}
                    messages={id ? (messagesByChat[id] ?? []) : []}
                    stream={rt.stream}
                    tools={rt.tools}
                    approvals={rt.approvals}
                    questions={rt.questions}
                    attachments={attachmentsByChat[id ?? ""] ?? []}
                    busy={rt.busy}
                    models={models}
                    error={rt.error ?? (pane === focus ? error : null)}
                    notice={pane === focus ? notice : null}
                    usage={rt.usage}
                    computerPaused={computer.paused}
                    computerReason={computer.reason}
                    onResumeComputer={() => api.setComputerPaused(false)}
                    interrupted={id ? (interrupted[id] ?? null) : null}
                    onRetryInterrupted={() => id && void retryInterrupted(id)}
                    onRewindInterrupted={() => id && void rewindInterrupted(id)}
                    onDismissInterrupted={() => id && void dismissInterrupted(id)}
                    mcp={mcp}
                    onMcp={() => openCustomise("connectors")}
                    contextTokens={id ? tokensFor(id) : 0}
                    contextMax={max}
                    onSend={(text) => send(text, id, pane)}
                    onResend={(text) => id && resend(id, text)}
                    onRestoreTail={(afterSeq) => id && void restoreTail(id, afterSeq)}
                    onStop={() => id && stop(id)}
                    onFiles={() =>
                      setPanel((current) => (current === "files" ? "none" : "files"))
                    }
                    onModelChange={(model, thinking, effort) =>
                      id && changeModel(id, model, thinking, effort)
                    }
                    onModeChange={(mode) => id && changeMode(id, mode)}
                    onAgentMode={(mode) => id && changeAgentMode(id, mode)}
                    onIncognito={(on) => id && changeIncognito(id, on)}
                    onEgress={(egress) => id && changeEgress(id, egress)}
                    onPickWorkspace={() => pickWorkspace(id, pane)}
                    onRevealFolder={() => id && void revealChatFolder(id)}
                    onContext={() => setPanel((current) => (current === "context" ? "none" : "context"))}
                    onAttach={(files) => attach(id, files)}
                    onRemoveAttachment={(index) => removeAttachment(id, index)}
                    onDecideApproval={decideApproval}
                    onAnswerQuestion={answerQuestion}
                    onSkipQuestion={skipQuestion}
                    prompts={prompts}
                    insert={pane === focus ? insert : null}
                    // Only the focused pane answers the chord; zero is "never",
                    // the same way the insert above is nulled for the other pane.
                    jumpToLatest={pane === focus ? jumpToLatest : 0}
                  />
                )}
                </div>
              </Fragment>
            );
          })}
        </div>
      </div>

      {panel === "projects" && (
        <ProjectsPanel
          chats={chats}
          activeId={activeId}
          onSelectChat={selectChat}
          onNewChatInProject={newChatInProject}
          onChanged={() => void refreshChats(kindRef.current)}
          onClose={() => setPanel("none")}
        />
      )}
      {panel === "artifacts" && (
        <ArtifactsGallery
          chatsById={chatsById}
          onOpen={openArtifact}
          onClose={() => setPanel("none")}
        />
      )}
      {panel === "scheduled" && (
        <ScheduledPanel
          onClose={() => setPanel("none")}
          onChanged={() => void refreshChats(kindRef.current)}
          onOpenChat={(id) => {
            setPanel("none");
            void selectChat(id);
          }}
        />
      )}
      {panel === "spend" && (
        <SpendPanel
          onClose={() => setPanel("none")}
          onOpenChat={(id) => {
            setPanel("none");
            void selectChat(id);
          }}
        />
      )}
      {panel === "context" && activeId && (
        <ContextPanel
          chatId={activeId}
          limit={models.find((m) => m.id === chatsById[activeId]?.model)?.context ?? 1_000_000}
          onClose={() => setPanel("none")}
        />
      )}
      {panel === "artifact" && artifact && (
        <ArtifactPanel
          // Re-mounted per version: a new one resets the version picker and the
          // editor, which is exactly what a fresh document should do.
          key={`${artifact.detail.artifact.id}@${artifact.detail.artifact.current_version}`}
          detail={artifact.detail}
          onClose={() => setPanel("none")}
          onChange={(detail) => setArtifact({ chatId: artifact.chatId, detail })}
        />
      )}

      {showSetup && settings && (
        <SetupPanel
          settings={settings}
          onOpenSettings={() => {
            setShowSetup(false);
            openSettings();
          }}
          onSaved={setSettings}
          onClose={() => setShowSetup(false)}
        />
      )}

      {customise.open && (
        <CustomiseDialog
          chatId={activeId}
          section={customise.section}
          /* A skill is built by talking about it, so the hub hands the user a
             chat rather than growing an editor of its own. */
          onAsk={async (text) => {
            try {
              const chat = await api.createChat(kindRef.current);
              await refreshChats(kindRef.current);
              setCustomise((c) => ({ ...c, open: false }));
              await selectChat(chat.id);
              await send(text, chat.id, focus);
            } catch (e) {
              setError(String(e));
            }
          }}
          onSaved={setSettings}
          onClose={() => {
            // The hub is where prompts are added, so what it may have changed is
            // re-read on the way out rather than left until a restart.
            void refreshPrompts();
            setCustomise((c) => ({ ...c, open: false }));
          }}
        />
      )}

      {showSettings && settings && (
        <SettingsDialog
          settings={settings}
          keyMask={keyMask}
          models={models}
          documentsDir={documentsDir}
          section={settingsSection}
          chatId={activeId}
          onClose={() => setShowSettings(false)}
          onMemoryChanged={() => {
            if (activeIdRef.current) loadMessages(activeIdRef.current).catch(() => {});
          }}
          onSaved={(saved, mask) => {
            setSettings(saved);
            setKeyMask(mask);
          }}
        />
      )}

      {palette && (
        <CommandPalette
          items={paletteItems}
          onRun={runPalette}
          onClose={() => setPalette(false)}
        />
      )}

      {sheet && <ShortcutSheet onClose={() => setSheet(false)} />}

      {audioOverview && (
        <AudioOverviewDialog
          chatId={audioOverview}
          onNotice={setNotice}
          onClose={() => setAudioOverview(null)}
        />
      )}

      {menu &&
        (() => {
          const row = chatsById[menu.id];
          return (
            <ContextMenu
              x={menu.x}
              y={menu.y}
              onClose={() => setMenu(null)}
              items={[
                { label: "Open", onSelect: () => void selectChat(menu.id) },
                { label: "Open in split view", onSelect: () => openInSplit(menu.id) },
                { label: "Rename", onSelect: () => setRenamingId(menu.id) },
                { label: "Duplicate", onSelect: () => void duplicateChat(menu.id) },
                {
                  label: row?.archived ? "Take out of the archive" : "Archive",
                  onSelect: () => void setArchived(menu.id, !row?.archived),
                },
                { label: "Delete", danger: true, onSelect: () => void deleteChat(menu.id) },
              ]}
            />
          );
        })()}

      {dropping && (
        // Inert on purpose: the drop is handled natively by the window, so the
        // overlay only has to be seen, not to receive anything.
        <div className="pointer-events-none fixed inset-0 z-50 flex items-center justify-center bg-neutral-950/70">
          <div className="rounded-xl border-2 border-dashed border-sky-600 bg-neutral-900/80 px-10 py-8 text-center">
            <p className="text-sm font-medium text-neutral-100">Drop to attach</p>
            <p className="mt-1 text-xs text-neutral-400">
              Files and images are added to this conversation.
            </p>
          </div>
        </div>
      )}
    </div>
  );
}
