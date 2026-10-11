import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, onApproval, onDelta, onDone, onError, onTool } from "./api";
import { filesToAttachments } from "./lib/attachments";
import type {
  ApprovalEvent,
  Attachment,
  ChatRow,
  DoneEvent,
  MessageRow,
  ModelInfo,
  SectionKind,
  Settings,
  ToolRun,
} from "./types";
import { Sidebar } from "./components/Sidebar";
import { ChatView } from "./components/ChatView";
import { SettingsDialog } from "./components/SettingsDialog";
import { MemoryPanel } from "./components/MemoryPanel";
import { SkillsPanel } from "./components/SkillsPanel";

type Panel = "none" | "memory" | "skills";

const HELP =
  "/compact summarise and reset the context · /memory · /skills · /folder · /new · /help";

export default function App() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [keyMask, setKeyMask] = useState<string | null>(null);
  const [documentsDir, setDocumentsDir] = useState("");
  const [kind, setKind] = useState<SectionKind>("chat");
  const [chats, setChats] = useState<ChatRow[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [messages, setMessages] = useState<MessageRow[]>([]);
  const [stream, setStream] = useState<{ content: string; reasoning: string } | null>(null);
  const [tools, setTools] = useState<ToolRun[]>([]);
  const [approvals, setApprovals] = useState<ApprovalEvent[]>([]);
  const [attachments, setAttachments] = useState<Attachment[]>([]);
  const [busy, setBusy] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [panel, setPanel] = useState<Panel>("none");
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [usage, setUsage] = useState<DoneEvent | null>(null);

  const activeIdRef = useRef<string | null>(null);
  activeIdRef.current = activeId;
  const kindRef = useRef<SectionKind>("chat");
  kindRef.current = kind;

  useEffect(() => {
    (async () => {
      const [s, m, k, docs] = await Promise.all([
        api.getSettings(),
        api.listModels(),
        api.maskedApiKey(),
        api.documentsDir(),
      ]);
      setSettings(s);
      setModels(m);
      setKeyMask(k);
      setDocumentsDir(docs);
      if (!k) setShowSettings(true);
    })().catch((e) => setError(String(e)));
  }, []);

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
        if (e.chatId !== activeIdRef.current) return;
        setStream((s) =>
          s
            ? {
                content: s.content + (e.kind === "content" ? e.text : ""),
                reasoning: s.reasoning + (e.kind === "reasoning" ? e.text : ""),
              }
            : s,
        );
      }),
      onTool((e) => {
        if (e.chatId !== activeIdRef.current) return;
        setTools((prev) => {
          const at = prev.findIndex((t) => t.id === e.id);
          if (e.phase === "requested") {
            const run: ToolRun = { id: e.id, tool: e.tool, args: e.args, status: "running" };
            if (at >= 0) {
              const copy = [...prev];
              copy[at] = run;
              return copy;
            }
            return [...prev, run];
          }
          const copy = [...prev];
          const base: ToolRun = copy[at] ?? { id: e.id, tool: e.tool, args: e.args, status: "ok" };
          const finished: ToolRun = {
            ...base,
            tool: e.tool,
            status: e.ok ? "ok" : "error",
            output: e.output,
          };
          if (at >= 0) copy[at] = finished;
          else copy.push(finished);
          return copy;
        });
      }),
      onApproval((e) => {
        if (e.chatId !== activeIdRef.current) return;
        setApprovals((prev) => [...prev.filter((a) => a.requestId !== e.requestId), e]);
      }),
      onDone((e) => {
        if (e.chatId !== activeIdRef.current) return;
        setUsage(e);
      }),
      onError((e) => {
        if (e.chatId !== activeIdRef.current) return;
        setError(e.message);
      }),
    ]).then((fns) => {
      if (disposed) fns.forEach((f) => f());
      else unlisten = fns;
    });
    return () => {
      disposed = true;
      unlisten.forEach((f) => f());
    };
  }, []);

  const refreshChats = useCallback(async (which: SectionKind) => {
    const list = await api.listChats(which);
    setChats(list);
    return list;
  }, []);

  useEffect(() => {
    refreshChats(kind).catch((e) => setError(String(e)));
  }, [kind, refreshChats]);

  const selectChat = useCallback(async (id: string) => {
    setActiveId(id);
    setStream(null);
    setTools([]);
    setApprovals([]);
    setAttachments([]);
    setError(null);
    setUsage(null);
    setMessages(await api.getMessages(id));
  }, []);

  const newChat = useCallback(async () => {
    const chat = await api.createChat(kindRef.current);
    await refreshChats(kindRef.current);
    await selectChat(chat.id);
  }, [refreshChats, selectChat]);

  const pickWorkspace = useCallback(async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Choose a folder DeskSeek may work in",
      });
      if (typeof selected !== "string") return;
      let chatId = activeIdRef.current;
      if (!chatId) {
        const created = await api.createChat("coding", selected);
        chatId = created.id;
        setActiveId(chatId);
      } else {
        await api.setChatWorkspace(chatId, selected);
      }
      await refreshChats("coding");
      await selectChat(chatId);
      setNotice(`Working in ${selected}`);
    } catch (e) {
      setError(String(e));
    }
  }, [refreshChats, selectChat]);

  const send = useCallback(
    async (text: string) => {
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
          case "/settings":
            setShowSettings(true);
            return;
          case "/folder":
            await pickWorkspace();
            return;
          case "/new":
            await newChat();
            return;
          case "/compact": {
            const id = activeIdRef.current;
            if (!id) {
              setError("Nothing to compact yet.");
              return;
            }
            setBusy(true);
            setNotice("Compacting the conversation…");
            try {
              await api.compactChat(id);
              setMessages(await api.getMessages(id));
              setNotice("Conversation compacted into a summary.");
            } catch (e) {
              setError(String(e));
            } finally {
              setBusy(false);
            }
            return;
          }
          default:
            setNotice(`Unknown command ${command}. ${HELP}`);
            return;
        }
      }

      let chatId = activeIdRef.current;
      try {
        if (!chatId) {
          const created = await api.createChat(kindRef.current);
          chatId = created.id;
          setActiveId(chatId);
          await refreshChats(kindRef.current);
        }

        const list = await api.listChats(kindRef.current);
        const chat = list.find((c) => c.id === chatId);
        if (chat?.kind === "coding" && !chat.workspace) {
          setError("Choose a folder for this coding task first.");
          await pickWorkspace();
          return;
        }

        const outgoing = attachments;
        setAttachments([]);
        setTools([]);
        setUsage(null);
        setMessages((m) => [
          ...m,
          {
            id: `tmp-${Date.now()}`,
            chat_id: chatId!,
            seq: Number.MAX_SAFE_INTEGER,
            role: "user",
            content: text,
            reasoning_content: null,
            tool_calls_json: null,
            tool_call_id: null,
            attachments_json: outgoing.length ? JSON.stringify(outgoing) : null,
            created_at: Math.floor(Date.now() / 1000),
          },
        ]);
        setStream({ content: "", reasoning: "" });
        setBusy(true);

        await api.sendMessage(chatId, text, outgoing);

        const [msgs] = await Promise.all([
          api.getMessages(chatId),
          refreshChats(kindRef.current),
        ]);
        if (activeIdRef.current === chatId) setMessages(msgs);
      } catch (e) {
        if (activeIdRef.current === chatId) setError(String(e));
      } finally {
        setBusy(false);
        setStream(null);
      }
    },
    [attachments, newChat, pickWorkspace, refreshChats],
  );

  const changeModel = useCallback(
    async (model: string, thinking: boolean, effort: string) => {
      const chat = activeIdRef.current;
      if (!chat) return;
      await api.setChatModel(chat, model, thinking, effort);
      await refreshChats(kindRef.current);
    },
    [refreshChats],
  );

  const decideApproval = useCallback(
    async (requestId: string, decision: "once" | "session" | "always" | "deny") => {
      setApprovals((prev) => prev.filter((a) => a.requestId !== requestId));
      try {
        await api.respondApproval(requestId, decision);
      } catch (e) {
        setError(String(e));
      }
    },
    [],
  );

  const attach = useCallback(async (files: FileList | File[]) => {
    try {
      const added = await filesToAttachments(files);
      setAttachments((prev) => [...prev, ...added].slice(0, 8));
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const activeChat = chats.find((c) => c.id === activeId) ?? null;

  const contextTokens = useMemo(() => {
    const chars =
      messages.reduce(
        (total, m) => total + (m.content?.length ?? 0) + (m.reasoning_content?.length ?? 0),
        0,
      ) + (settings?.systemPrompt.length ?? 0);
    // Rough English/code average: about 3.6 characters per token.
    return Math.ceil(chars / 3.6);
  }, [messages, settings]);

  const contextMax =
    models.find((m) => m.id === activeChat?.model)?.context ?? 1_000_000;

  return (
    <div className="flex h-full">
      <Sidebar
        kind={kind}
        onKind={setKind}
        chats={chats}
        activeId={activeId}
        onSelect={selectChat}
        onNew={newChat}
        onDelete={async (id) => {
          await api.deleteChat(id);
          const list = await refreshChats(kind);
          if (activeIdRef.current === id) {
            setActiveId(null);
            setMessages([]);
            if (list[0]) await selectChat(list[0].id);
          }
        }}
        onSettings={() => setShowSettings(true)}
        onMemory={() => setPanel("memory")}
        onSkills={() => setPanel("skills")}
        keyMask={keyMask}
      />

      <ChatView
        chat={activeChat}
        messages={messages}
        stream={stream}
        tools={tools}
        approvals={approvals}
        attachments={attachments}
        busy={busy}
        models={models}
        error={error}
        notice={notice}
        usage={usage}
        contextTokens={contextTokens}
        contextMax={contextMax}
        onSend={send}
        onModelChange={changeModel}
        onPickWorkspace={pickWorkspace}
        onAttach={attach}
        onRemoveAttachment={(index) =>
          setAttachments((prev) => prev.filter((_, i) => i !== index))
        }
        onDecideApproval={decideApproval}
      />

      {panel === "memory" && (
        <MemoryPanel
          chatId={activeId}
          onClose={() => setPanel("none")}
          onChanged={() => {
            if (activeIdRef.current) {
              api.getMessages(activeIdRef.current).then(setMessages).catch(() => {});
            }
          }}
        />
      )}
      {panel === "skills" && <SkillsPanel chatId={activeId} onClose={() => setPanel("none")} />}

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
    </div>
  );
}
