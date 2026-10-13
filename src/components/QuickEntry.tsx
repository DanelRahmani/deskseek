import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { emit } from "@tauri-apps/api/event";

import { api } from "../api";
import type { QuickTarget } from "../types";

/**
 * The quick-entry window: ask something without leaving the application you are
 * in.
 *
 * It is deliberately the same app underneath. A question goes through the
 * ordinary `create_chat` and `send_message` the main window calls, so a quick
 * question is not a second, quieter run path — it lands in a real conversation
 * and streams its answer into the main window like any other. This component's
 * whole job is to decide *which* conversation and then get out of the way.
 */
export function QuickEntry() {
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [target, setTarget] = useState<QuickTarget>("last");
  const box = useRef<HTMLTextAreaElement>(null);

  // The window is a transparent frame around the card below, but the app's own
  // stylesheet paints an opaque body — which would show as a grey square behind
  // it. Its own window, its own paint.
  useEffect(() => {
    const body = document.body;
    const root = document.getElementById("root");
    const previous = { body: body.style.background, root: root?.style.background ?? "" };
    body.style.background = "transparent";
    if (root) root.style.background = "transparent";
    return () => {
      body.style.background = previous.body;
      if (root) root.style.background = previous.root;
    };
  }, []);

  // The window is hidden rather than unmounted between questions, so focusing
  // the box on mount would only ever work for the first one. Focus follows the
  // window, which is also what the user expects after pressing the chord.
  useEffect(() => {
    const stop = getCurrentWindow().onFocusChanged(({ payload: focused }) => {
      if (focused) box.current?.focus();
    });
    return () => {
      void stop.then((off) => off());
    };
  }, []);

  useEffect(() => {
    api
      .getSettings()
      .then((s) => setTarget(s.quickTarget))
      .catch(() => {
        /* the default is a fine answer */
      });
  }, []);

  const close = useCallback(() => {
    setError(null);
    void getCurrentWindow().hide();
  }, []);

  const send = useCallback(async () => {
    const question = text.trim();
    if (!question || busy) return;
    setBusy(true);
    setError(null);
    try {
      // Asked here rather than left to `send_message`, because a missing key is
      // the one failure with no event behind it — the command rejects before a
      // run exists, and by then this window is already gone.
      if (!(await api.hasApiKey())) {
        throw new Error("No API key yet — add one in Settings → Access.");
      }
      const chatId = await resolve(target);
      // Deliberately **not** awaited: `send_message` resolves when the whole turn
      // has finished, and a quick window that stayed up for the length of an
      // answer would be a worse version of the main window. The reply streams
      // into the chat, and the chat is what the main window is told to open.
      void api.sendMessage(chatId, question, []).catch((e) => {
        void emit("quick-sent", { chatId, text: question, error: String(e) });
      });
      setText("");
      await emit("quick-sent", { chatId, text: question });
      void getCurrentWindow().hide();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, [text, busy, target]);

  return (
    <div
      className="flex h-screen w-screen items-center justify-center"
      onKeyDown={(e) => {
        if (e.key === "Escape") close();
      }}
    >
      <div className="w-full overflow-hidden rounded-xl border border-neutral-700 bg-neutral-900 shadow-2xl">
        <div className="flex items-start gap-2 p-2">
          <span aria-hidden className="mt-2 select-none text-neutral-500">
            ⚡
          </span>
          <textarea
            ref={box}
            value={text}
            rows={1}
            placeholder="Ask without leaving what you are doing…"
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                void send();
              }
            }}
            className="max-h-40 min-h-9 flex-1 resize-none rounded-md bg-transparent py-2 text-sm text-neutral-100 outline-hidden placeholder:text-neutral-600 focus:bg-neutral-800/60"
          />
          <button
            type="button"
            onClick={() => void send()}
            disabled={busy || text.trim().length === 0}
            className="mt-1 rounded-md bg-sky-500 px-3 py-1.5 text-xs font-medium text-black hover:bg-sky-400 disabled:opacity-40"
          >
            {busy ? "Asking…" : "Ask"}
          </button>
        </div>
        {error && (
          <p className="border-t border-neutral-800 px-3 py-1.5 text-[11px] text-red-300">
            {error}
          </p>
        )}
        <div className="flex items-center justify-between border-t border-neutral-800 px-3 py-1.5 text-[11px] text-neutral-500">
          <span>→ {target === "new" ? "a new chat" : "your most recent chat"}</span>
          <span>Enter to ask · Esc to close</span>
        </div>
      </div>
    </div>
  );
}

/**
 * Which conversation the question goes into.
 *
 * `last` is the honest reading of "the chat I was just in": the sidebar's own
 * order, newest first, so it is the same row the user would have clicked. With
 * nothing to reuse it makes one, which is what `new` always does.
 */
async function resolve(target: QuickTarget): Promise<string> {
  if (target !== "new") {
    const chats = await api.listChats("chat");
    const recent = chats.find((chat) => !chat.archived);
    if (recent) return recent.id;
  }
  return (await api.createChat("chat")).id;
}
