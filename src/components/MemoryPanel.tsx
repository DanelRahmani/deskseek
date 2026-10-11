import { useEffect, useState } from "react";
import { api } from "../api";
import type { MemoryProposal } from "../types";

interface Props {
  chatId: string | null;
  onClose: () => void;
  onChanged: () => void;
}

export function MemoryPanel({ chatId, onClose, onChanged }: Props) {
  const [body, setBody] = useState("");
  const [path, setPath] = useState("");
  const [proposals, setProposals] = useState<MemoryProposal[]>([]);
  const [draft, setDraft] = useState("");
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const reload = async () => {
    setBody((await api.getMemory()) ?? "");
    setPath(await api.memoryPath());
    if (chatId) setProposals(await api.listProposals(chatId, "pending"));
    else setProposals([]);
  };

  useEffect(() => {
    reload().catch((e) => setStatus(String(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [chatId]);

  const save = async () => {
    setBusy(true);
    try {
      await api.setMemory(body);
      setStatus("Saved.");
      onChanged();
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  };

  const addTyped = async () => {
    const entry = draft.trim();
    if (!entry) return;
    setBusy(true);
    try {
      await api.appendMemory([entry]);
      setDraft("");
      await reload();
      setStatus("Added to memory.");
      onChanged();
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  };

  const decide = async (ids: string[], accept: boolean) => {
    setBusy(true);
    try {
      if (accept) {
        const written = await api.acceptProposals(ids);
        setStatus(`Added ${written} entr${written === 1 ? "y" : "ies"} to memory.md.`);
      } else {
        await api.rejectProposals(ids);
      }
      await reload();
      onChanged();
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  };

  const summarize = async () => {
    if (!chatId) return;
    setBusy(true);
    setStatus("Summarising…");
    try {
      const raw = await api.summarizeChat(chatId);
      const [summary, count, topic, logPath] = raw.split("\u001f");
      setStatus(
        `${summary} — ${count} suggestion(s) below.` +
          (logPath ? ` Saved CHATLOG/${logPath.split(/[\\/]/).pop()} (${topic}).` : ""),
      );
      await reload();
      onChanged();
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <aside className="flex h-full w-96 shrink-0 flex-col border-l border-neutral-800 bg-neutral-950">
      <header className="flex items-center justify-between border-b border-neutral-800 px-4 py-3">
        <h2 className="text-sm font-semibold">Memory</h2>
        <button onClick={onClose} className="text-neutral-500 hover:text-neutral-200">
          ×
        </button>
      </header>

      <div className="flex-1 space-y-5 overflow-y-auto p-4">
        <section>
          <div className="mb-2 flex items-center justify-between">
            <h3 className="text-xs font-semibold uppercase tracking-wide text-neutral-500">
              Suggestions
            </h3>
            {proposals.length > 0 && (
              <button
                onClick={() => decide(proposals.map((p) => p.id), true)}
                disabled={busy}
                className="text-[11px] text-sky-400 hover:text-sky-300 disabled:opacity-50"
              >
                Accept all
              </button>
            )}
          </div>
          {!chatId && <p className="text-xs text-neutral-600">Open a chat first.</p>}
          {chatId && proposals.length === 0 && (
            <p className="text-xs text-neutral-600">
              Nothing pending. Use <span className="text-neutral-400">Summarise chat</span> to
              review what is worth remembering.
            </p>
          )}
          <div className="space-y-2">
            {proposals.map((p) => (
              <div
                key={p.id}
                className="rounded-lg border border-neutral-800 bg-neutral-900/60 p-2 text-xs"
              >
                <p className="text-neutral-300">{p.text}</p>
                <div className="mt-2 flex gap-2">
                  <button
                    onClick={() => decide([p.id], true)}
                    disabled={busy}
                    className="rounded bg-emerald-600/90 px-2 py-1 text-[11px] text-white hover:bg-emerald-500 disabled:opacity-50"
                  >
                    Accept
                  </button>
                  <button
                    onClick={() => decide([p.id], false)}
                    disabled={busy}
                    className="rounded border border-neutral-700 px-2 py-1 text-[11px] text-neutral-300 hover:border-neutral-600 disabled:opacity-50"
                  >
                    Reject
                  </button>
                </div>
              </div>
            ))}
          </div>
        </section>

        <section>
          <h3 className="mb-2 text-xs font-semibold uppercase tracking-wide text-neutral-500">
            Remember something
          </h3>
          <div className="flex gap-2">
            <input
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && addTyped()}
              placeholder="e.g. I write in British English"
              className="flex-1 rounded-lg border border-neutral-800 bg-neutral-900 px-2 py-1.5 text-xs text-neutral-200 outline-none focus:border-neutral-700"
            />
            <button
              onClick={addTyped}
              disabled={busy || !draft.trim()}
              className="rounded-lg bg-sky-600 px-3 text-xs font-medium text-white hover:bg-sky-500 disabled:bg-neutral-800 disabled:text-neutral-600"
            >
              Add
            </button>
          </div>
        </section>

        <section>
          <div className="mb-2 flex items-center justify-between">
            <h3 className="text-xs font-semibold uppercase tracking-wide text-neutral-500">
              memory.md
            </h3>
            <button
              onClick={save}
              disabled={busy}
              className="text-[11px] text-sky-400 hover:text-sky-300 disabled:opacity-50"
            >
              Save
            </button>
          </div>
          <textarea
            value={body}
            onChange={(e) => setBody(e.target.value)}
            spellCheck={false}
            className="h-52 w-full resize-y rounded-lg border border-neutral-800 bg-neutral-900 p-2 font-mono text-[11px] text-neutral-300 outline-none focus:border-neutral-700"
          />
          <p className="mt-1 break-all text-[10px] text-neutral-600">{path}</p>
        </section>
      </div>

      <footer className="flex items-center gap-2 border-t border-neutral-800 p-3">
        <button
          onClick={summarize}
          disabled={busy || !chatId}
          className="rounded-lg border border-neutral-800 px-3 py-1.5 text-xs text-neutral-300 hover:border-neutral-700 disabled:opacity-50"
        >
          Summarise chat
        </button>
        <span className="flex-1 truncate text-[11px] text-neutral-500">{status}</span>
      </footer>
    </aside>
  );
}
