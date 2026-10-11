import { useEffect, useState } from "react";
import { api } from "../api";
import type { FactRow, MemoryProposal } from "../types";

interface Props {
  chatId: string | null;
  onClose: () => void;
  onChanged: () => void;
}

export function MemoryPanel({ chatId, onClose, onChanged }: Props) {
  const [body, setBody] = useState("");
  const [path, setPath] = useState("");
  const [proposals, setProposals] = useState<MemoryProposal[]>([]);
  const [facts, setFacts] = useState<FactRow[]>([]);
  const [draft, setDraft] = useState("");
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const reload = async () => {
    setBody((await api.getMemory()) ?? "");
    setPath(await api.memoryPath());
    if (chatId) setProposals(await api.listProposals(chatId, "pending"));
    else setProposals([]);
    setFacts(await api.listFacts());
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
        setStatus(`Kept ${written} entr${written === 1 ? "y" : "ies"}.`);
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

  const forget = async (id: string) => {
    setBusy(true);
    try {
      await api.deleteFact(id);
      await reload();
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

  // memory.md rides along with every single request, so a nudge when it grows is
  // worth more than a hard limit: the user may still write whatever they like.
  const profileEntries = body.split("\n").filter((line) => line.trim().startsWith("- ")).length;
  const profileHeavy = profileEntries > 50 || body.length > 2_000;

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
                <div className="mt-2 flex items-center gap-2">
                  <button
                    onClick={() => decide([p.id], true)}
                    disabled={busy}
                    className="rounded bg-emerald-600/90 px-2 py-1 text-[11px] text-ok-fg hover:bg-emerald-500 disabled:opacity-50"
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
                  {/* Where Accept will file it. A profile entry is sent with
                      every request, so it is worth seeing before agreeing. */}
                  <span
                    className="ml-auto text-[10px] text-neutral-500"
                    title={
                      p.target === "profile"
                        ? "Goes into memory.md, which is sent with every request"
                        : "Kept as a fact and looked up only when it is relevant"
                    }
                  >
                    {p.target === "profile" ? "→ always sent" : "→ facts"}
                  </span>
                </div>
              </div>
            ))}
          </div>
        </section>

        <section>
          <h3 className="mb-2 text-xs font-semibold uppercase tracking-wide text-neutral-500">
            Kept facts
          </h3>
          {facts.length === 0 && (
            <p className="text-xs text-neutral-600">
              Nothing yet. Facts are looked up only when they match what you ask, so this list can
              grow without costing anything on the turns where none of it is relevant.
            </p>
          )}
          <div className="space-y-1">
            {facts.map((f) => (
              <div
                key={f.id}
                className="flex items-start gap-2 rounded-lg border border-neutral-800 bg-neutral-900/60 px-2 py-1.5 text-xs"
              >
                <span className="min-w-0 flex-1 text-neutral-300">{f.text}</span>
                <span className="shrink-0 text-[10px] text-neutral-600" title={f.scope}>
                  {f.scope === "global" ? "everywhere" : f.scope.replace(/^project:/, "")}
                </span>
                <button
                  onClick={() => forget(f.id)}
                  disabled={busy}
                  className="shrink-0 text-neutral-500 hover:text-red-400 disabled:opacity-50"
                  title="Forget this"
                >
                  ×
                </button>
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
              className="rounded-lg bg-sky-600 px-3 text-xs font-medium text-accent-fg hover:bg-sky-500 disabled:bg-neutral-800 disabled:text-neutral-600"
            >
              Add
            </button>
          </div>
        </section>

        <section>
          <div className="mb-2 flex items-center justify-between">
            <h3 className="text-xs font-semibold uppercase tracking-wide text-neutral-500">
              memory.md — sent with every request
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
          {profileHeavy && (
            <p className="mt-1 rounded-md border border-amber-900/60 bg-amber-950/30 px-2 py-1 text-[11px] text-amber-300">
              {profileEntries} entries here, and all of it is sent with every message. Keep this
              file to who the user is and how they want to be worked with; everything else belongs
              in the kept facts above, where it is only looked up when it is relevant.
            </p>
          )}
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
