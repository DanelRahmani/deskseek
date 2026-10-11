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
  onShot,
  onTool,
} from "./api";
import { filesToAttachments, pathToAttachment } from "./lib/attachments";
import type {
  ApprovalEvent,
  ArtifactDetail,
  Attachment,
  ChatRow,
  DoneEvent,
  MessageHit,
  MessageRow,
  ModelInfo,
  PermissionMode,
  SectionKind,
  Settings,
  ToolRun,
} from "./types";
import { Sidebar } from "./components/Sidebar";
import { ChatView } from "./components/ChatView";
import { SettingsDialog } from "./components/SettingsDialog";
import { MemoryPanel } from "./components/MemoryPanel";
import { McpPanel } from "./components/McpPanel";
import { SkillsPanel } from "./components/SkillsPanel";
import { SetupPanel } from "./components/SetupPanel";
import { FilesPanel } from "./components/FilesPanel";
import { ArtifactPanel } from "./components/ArtifactPanel";
import { ContextPanel } from "./components/ContextPanel";
import { TabStrip } from "./components/TabStrip";
import { CommandPalette } from "./components/CommandPalette";
import { loadShell, openChatIn, pruneShell, saveShell, setPaneChat, toggleSplitPanes } from "./lib/shell";
import type { Shell } from "./lib/shell";
import { resolveShortcut } from "./lib/shortcuts";
import { buildPalette } from "./lib/commands";
import { resolveMode, resolveTheme } from "./lib/theme";
import { tokensFromChars } from "./lib/context";

type Panel = "none" | "memory" | "skills" | "mcp" | "files" | "artifact" | "context";

/** Everything that belongs to one conversation's live turn. */
interface SessionRuntime {
  busy: boolean;
  stream: { content: string; reasoning: string } | null;
  tools: ToolRun[];
  approvals: ApprovalEvent[];
  usage: DoneEvent | null;
  /** Last failure for this chat, cleared when a new turn starts. */
  error: string | null;
}

const IDLE: SessionRuntime = {
  busy: false,
  stream: null,
  tools: [],
  approvals: [],
  usage: null,
  error: null,
};

const HELP =
  "/compact summarise and reset the context · /files · /memory · /skills · /mcp · /setup · /folder · /new · /export · /rewind · /help";

export default function App() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [keyMask, setKeyMask] = useState<string | null>(null);
  const [documentsDir, setDocumentsDir] = useState("");
  const [kind, setKind] = useState<SectionKind>("chat");
  const [chats, setChats] = useState<ChatRow[]>([]);
  // Every chat seen so far, by id: tabs and panes outlive the sidebar's current
  // section, so they need rows of their own to render from.
  const [chatsById, setChatsById] = useState<Record<string, ChatRow>>({});
  const [shell, setShell] = useState<Shell>(loadShell);
  // Transcripts and drafts are per chat, so two panes can show two
  // conversations at once and switching back does not refetch.
  const [messagesByChat, setMessagesByChat] = useState<Record<string, MessageRow[]>>({});
  const [attachmentsByChat, setAttachmentsByChat] = useState<Record<string, Attachment[]>>({});
  // True while files are being dragged over the window, for the drop overlay.
  const [dropping, setDropping] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [showSetup, setShowSetup] = useState(false);
  const [panel, setPanel] = useState<Panel>("none");
  const [palette, setPalette] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  // Full-text results for the sidebar's search box. The query itself lives in
  // the Sidebar; this side debounces it and owns the results.
  const [hits, setHits] = useState<MessageHit[]>([]);
  // Live turn state, keyed by chat — so a run in a background chat keeps
  // streaming while the user reads a different conversation.
  const [sessions, setSessions] = useState<Record<string, SessionRuntime>>({});
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

  const { tabs, panes, focus, splitAt } = shell;
  const activeId = panes[focus] ?? null;

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
        added.push(await pathToAttachment(await api.attachPath(path)));
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
      const [s, m, k, docs, chatList, codingList, paused, mcpStatus] = await Promise.all([
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
      ]);
      setSettings(s);
      setModels(m);
      setKeyMask(k);
      setDocumentsDir(docs);
      setComputer({ paused, reason: paused ? "Computer control is stopped." : null });
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
      // Chats deleted while the app was closed must not come back as tabs.
      const known = new Set(all.map((c) => c.id));
      setShell((prev) => pruneShell(prev, (id) => !known.has(id)));
      // A restored pane needs its transcript, which lives in the database.
      await Promise.all(
        panes.filter((p): p is string => p !== null && known.has(p)).map(loadMessages),
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

  useEffect(() => {
    let disposed = false;
    let unlisten: Array<() => void> = [];
    Promise.all([
      onDelta((e) => {
        patch(e.chatId, (rt) =>
          rt.stream
            ? {
                ...rt,
                stream: {
                  content: rt.stream.content + (e.kind === "content" ? e.text : ""),
                  reasoning: rt.stream.reasoning + (e.kind === "reasoning" ? e.text : ""),
                },
              }
            : rt,
        );
      }),
      onTool((e) => {
        patch(e.chatId, (rt) => {
          const at = rt.tools.findIndex((t) => t.id === e.id);
          const tools = [...rt.tools];
          if (e.phase === "requested") {
            const run: ToolRun = { id: e.id, tool: e.tool, args: e.args, status: "running" };
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
    };
  }, [patch, attachPaths]);

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
    refreshChats(kind).catch((e) => setError(String(e)));
  }, [kind, refreshChats]);

  // The chat's own runtime is left alone: a run in a background chat keeps its
  // stream, tools and pending approvals while the user reads another one.
  const selectChat = useCallback(
    async (id: string) => {
      setError(null);
      showChat(id);
      try {
        await loadMessages(id);
      } catch (e) {
        setError(String(e));
      }
    },
    [showChat, loadMessages],
  );

  const newChat = useCallback(async () => {
    const chat = await api.createChat(kindRef.current);
    await refreshChats(kindRef.current);
    await selectChat(chat.id);
  }, [refreshChats, selectChat]);

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
            setPanel("memory");
            return;
          case "/skills":
            setPanel("skills");
            return;
          case "/mcp":
            setPanel("mcp");
            return;
          case "/setup":
            setShowSetup(true);
            return;
          case "/files":
            setPanel((current) => (current === "files" ? "none" : "files"));
            return;
          case "/settings":
            setShowSettings(true);
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
          case "/rewind":
            if (chatId) await rewind(chatId);
            else setError("Open a chat to undo.");
            return;
          case "/compact": {
            if (!chatId) {
              setError("Nothing to compact yet.");
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
        if (chatId) patch(chatId, (rt) => ({ ...rt, error: rt.error ?? String(e) }));
      } finally {
        if (chatId) patch(chatId, (rt) => ({ ...rt, busy: false, stream: null }));
      }
    },
    [attachmentsByChat, loadMessages, newChat, pickPane, pickWorkspace, refreshChats],
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

  const stop = useCallback(async (chatId: string) => {
    try {
      await api.stopChat(chatId);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  // Chat mode only. Off means the next request carries no tools at all, so the
  // conversation loses its file access until it is turned back on.
  const changeTools = useCallback(
    async (chatId: string, enabled: boolean) => {
      setChatsById((prev) =>
        prev[chatId] ? { ...prev, [chatId]: { ...prev[chatId], tools_enabled: enabled } } : prev,
      );
      try {
        await api.setChatTools(chatId, enabled);
        await refreshChats(kindRef.current);
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

  const attach = useCallback(async (chatId: string | null, files: FileList | File[]) => {
    // Keyed by the draft, not the chat: files can be attached before a chat
    // exists, and send() picks them up from the same empty-string slot.
    const key = chatId ?? "";
    try {
      const added = await filesToAttachments(files);
      setAttachmentsByChat((prev) => ({
        ...prev,
        [key]: [...(prev[key] ?? []), ...added].slice(0, 8),
      }));
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const removeAttachment = useCallback((chatId: string | null, index: number) => {
    const key = chatId ?? "";
    setAttachmentsByChat((prev) => ({
      ...prev,
      [key]: (prev[key] ?? []).filter((_, i) => i !== index),
    }));
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
    } finally {
      patch(chatId, (rt) => ({ ...rt, busy: false, stream: null }));
    }
  }, [loadMessages]);

  // The sidebar's search box is a full-text lookup over every message. The box
  // state lives in the Sidebar, so this debounces the keystrokes and owns the
  // results.
  const searchTimer = useRef<number | null>(null);
  const searchMessages = useCallback((query: string) => {
    if (searchTimer.current !== null) window.clearTimeout(searchTimer.current);
    const needle = query.trim();
    if (needle.length < 2) {
      setHits([]);
      return;
    }
    searchTimer.current = window.setTimeout(() => {
      api.searchMessages(needle).then(setHits).catch(() => setHits([]));
    }, 180);
  }, []);

  // The palette is global — both sections' chats, most recently used first — so
  // Ctrl+K finds a coding task while you are in chat and the reverse. The hint
  // already names the section, and picking one switches to it below.
  const paletteItems = useMemo(() => {
    const all = Object.values(chatsById).sort((a, b) => b.updated_at - a.updated_at);
    return buildPalette(all, !!activeId);
  }, [chatsById, activeId]);
  const runPalette = useCallback(
    (id: string) => {
      setPalette(false);
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
        case "settings":
          setShowSettings(true);
          return;
        case "memory":
          setPanel("memory");
          return;
        case "skills":
          setPanel("skills");
          return;
        case "mcp":
          setPanel("mcp");
          return;
        case "files":
          setPanel((current) => (current === "files" ? "none" : "files"));
          return;
        case "split":
          toggleSplit();
          return;
        case "export":
          if (activeIdRef.current) void exportChat(activeIdRef.current);
          return;
        case "export-pdf":
          if (activeIdRef.current) void exportChat(activeIdRef.current, true);
          return;
        case "rewind":
          if (activeIdRef.current) void rewind(activeIdRef.current);
          return;
      }
    },
    [chatsById, exportChat, newChat, rewind, selectChat, toggleSplit],
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
        case "toggle-files":
          setPanel((current) => (current === "files" ? "none" : "files"));
          return;
        case "open-memory":
          setPanel("memory");
          return;
        case "open-settings":
          setShowSettings(true);
          return;
        case "command-palette":
          setPalette((open) => !open);
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
        case "dismiss":
          // Topmost first: the palette sits above a dialog, which covers a panel.
          if (palette) setPalette(false);
          else if (showSettings) setShowSettings(false);
          else if (showSetup) setShowSetup(false);
          else setPanel("none");
          return;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [activeId, closeTab, newChat, palette, runPalette, showSettings, showSetup, toggleSplit]);

  const runningIds = useMemo(
    () => Object.keys(sessions).filter((id) => sessions[id].busy),
    [sessions],
  );

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
        kind={kind}
        onKind={setKind}
        chats={chats}
        activeId={activeId}
        hits={hits}
        onSearch={searchMessages}
        onSelect={selectChat}
        onNew={newChat}
        onDelete={async (id) => {
          await api.deleteChat(id);
          await refreshChats(kind);
          setMessagesByChat((prev) => {
            if (!(id in prev)) return prev;
            const next = { ...prev };
            delete next[id];
            return next;
          });
          // The tab goes too; a pane showing it refills from the remaining tabs.
          setShell((s) => pruneShell(s, (t) => t === id));
        }}
        onSettings={() => setShowSettings(true)}
        onMemory={() => setPanel("memory")}
        onSkills={() => setPanel("skills")}
        onMcp={() => setPanel("mcp")}
        keyMask={keyMask}
        runningIds={runningIds}
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
          tabs={tabs.map((id) => chatsById[id]).filter((c): c is ChatRow => !!c)}
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
                  <ChatView
                    chat={row}
                    messages={id ? (messagesByChat[id] ?? []) : []}
                    stream={rt.stream}
                    tools={rt.tools}
                    approvals={rt.approvals}
                    attachments={attachmentsByChat[id ?? ""] ?? []}
                    busy={rt.busy}
                    models={models}
                    error={rt.error ?? (pane === focus ? error : null)}
                    notice={pane === focus ? notice : null}
                    usage={rt.usage}
                    computerPaused={computer.paused}
                    computerReason={computer.reason}
                    onResumeComputer={() => api.setComputerPaused(false)}
                    mcp={mcp}
                    onMcp={() => setPanel("mcp")}
                    contextTokens={id ? tokensFor(id) : 0}
                    contextMax={max}
                    onSend={(text) => send(text, id, pane)}
                    onResend={(text) => id && resend(id, text)}
                    onStop={() => id && stop(id)}
                    onFiles={() =>
                      setPanel((current) => (current === "files" ? "none" : "files"))
                    }
                    onModelChange={(model, thinking, effort) =>
                      id && changeModel(id, model, thinking, effort)
                    }
                    onModeChange={(mode) => id && changeMode(id, mode)}
                    onToolsChange={(enabled) => id && changeTools(id, enabled)}
                    onPickWorkspace={() => pickWorkspace(id, pane)}
                    onContext={() => setPanel((current) => (current === "context" ? "none" : "context"))}
                    onAttach={(files) => attach(id, files)}
                    onRemoveAttachment={(index) => removeAttachment(id, index)}
                    onDecideApproval={decideApproval}
                  />
                </div>
              </Fragment>
            );
          })}
        </div>
      </div>

      {panel === "memory" && (
        <MemoryPanel
          chatId={activeId}
          onClose={() => setPanel("none")}
          onChanged={() => {
            if (activeIdRef.current) loadMessages(activeIdRef.current).catch(() => {});
          }}
        />
      )}
      {panel === "skills" && <SkillsPanel chatId={activeId} onClose={() => setPanel("none")} />}
      {panel === "context" && activeId && (
        <ContextPanel
          chatId={activeId}
          limit={models.find((m) => m.id === chatsById[activeId]?.model)?.context ?? 1_000_000}
          onClose={() => setPanel("none")}
        />
      )}
      {panel === "mcp" && <McpPanel onClose={() => setPanel("none")} />}
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
            setShowSettings(true);
          }}
          onSaved={setSettings}
          onClose={() => setShowSetup(false)}
        />
      )}

      {showSettings && settings && (
        <SettingsDialog
          settings={settings}
          keyMask={keyMask}
          models={models}
          documentsDir={documentsDir}
          onClose={() => setShowSettings(false)}
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
