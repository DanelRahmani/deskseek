import { useCallback, useEffect, useRef, useState } from "react";
import { api, onDelta, onDone, onError } from "./api";
import type { ChatRow, DoneEvent, MessageRow, ModelInfo, SectionKind, Settings } from "./types";
import { Sidebar } from "./components/Sidebar";
import { ChatView } from "./components/ChatView";
import { SettingsDialog } from "./components/SettingsDialog";

export default function App() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [keyMask, setKeyMask] = useState<string | null>(null);
  const [kind, setKind] = useState<SectionKind>("chat");
  const [chats, setChats] = useState<ChatRow[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [messages, setMessages] = useState<MessageRow[]>([]);
  const [stream, setStream] = useState<{ content: string; reasoning: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [usage, setUsage] = useState<DoneEvent | null>(null);

  // Read inside event handlers without re-subscribing on every chat switch.
  const activeIdRef = useRef<string | null>(null);
  activeIdRef.current = activeId;

  useEffect(() => {
    (async () => {
      const [s, m, k] = await Promise.all([
        api.getSettings(),
        api.listModels(),
        api.maskedApiKey(),
      ]);
      setSettings(s);
      setModels(m);
      setKeyMask(k);
      if (!k) setShowSettings(true);
    })().catch((e) => setError(String(e)));
  }, []);

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

  const refreshChats = useCallback(async (k: SectionKind) => {
    const list = await api.listChats(k);
    setChats(list);
    return list;
  }, []);

  useEffect(() => {
    refreshChats(kind).catch((e) => setError(String(e)));
  }, [kind, refreshChats]);

  const selectChat = useCallback(async (id: string) => {
    setActiveId(id);
    setStream(null);
    setError(null);
    setUsage(null);
    setMessages(await api.getMessages(id));
  }, []);

  const newChat = useCallback(async () => {
    const chat = await api.createChat(kind);
    await refreshChats(kind);
    await selectChat(chat.id);
  }, [kind, refreshChats, selectChat]);

  const deleteChat = useCallback(
    async (id: string) => {
      await api.deleteChat(id);
      const list = await refreshChats(kind);
      if (activeIdRef.current === id) {
        setActiveId(null);
        setMessages([]);
        if (list[0]) await selectChat(list[0].id);
      }
    },
    [kind, refreshChats, selectChat],
  );

  const send = useCallback(
    async (text: string) => {
      setError(null);
      let chatId = activeIdRef.current;
      try {
        if (!chatId) {
          const created = await api.createChat(kind);
          chatId = created.id;
          setActiveId(chatId);
          await refreshChats(kind);
        }
        const optimistic: MessageRow = {
          id: `tmp-${Date.now()}`,
          chat_id: chatId,
          seq: Number.MAX_SAFE_INTEGER,
          role: "user",
          content: text,
          reasoning_content: null,
          tool_calls_json: null,
          tool_call_id: null,
          created_at: Math.floor(Date.now() / 1000),
        };
        setMessages((m) => [...m, optimistic]);
        setStream({ content: "", reasoning: "" });
        setBusy(true);

        await api.sendMessage(chatId, text);

        // The user may have switched chats while this was streaming; only
        // refresh the view if this chat is still the one on screen.
        const [msgs] = await Promise.all([api.getMessages(chatId), refreshChats(kind)]);
        if (activeIdRef.current === chatId) setMessages(msgs);
      } catch (e) {
        if (activeIdRef.current === chatId) setError(String(e));
      } finally {
        setBusy(false);
        setStream(null);
      }
    },
    [kind, refreshChats],
  );

  const changeModel = useCallback(
    async (model: string, thinking: boolean, effort: string) => {
      const chat = activeIdRef.current;
      if (!chat) return;
      await api.setChatModel(chat, model, thinking, effort);
      await refreshChats(kind);
    },
    [kind, refreshChats],
  );

  const activeChat = chats.find((c) => c.id === activeId) ?? null;

  return (
    <div className="flex h-full">
      <Sidebar
        kind={kind}
        onKind={setKind}
        chats={chats}
        activeId={activeId}
        onSelect={selectChat}
        onNew={newChat}
        onDelete={deleteChat}
        onSettings={() => setShowSettings(true)}
        keyMask={keyMask}
      />
      <ChatView
        chat={activeChat}
        messages={messages}
        stream={stream}
        busy={busy}
        models={models}
        error={error}
        usage={usage}
        onSend={send}
        onModelChange={changeModel}
      />
      {showSettings && settings && (
        <SettingsDialog
          settings={settings}
          keyMask={keyMask}
          models={models}
          onClose={() => setShowSettings(false)}
          onSaved={(s, mask) => {
            setSettings(s);
            setKeyMask(mask);
          }}
        />
      )}
    </div>
  );
}
