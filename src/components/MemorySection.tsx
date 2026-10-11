/**
 * The memory screen: what the assistant keeps, and where it keeps it.
 *
 * It lives in the settings dialog rather than in a panel beside the conversation
 * because memory is something you visit occasionally to prune, not something to
 * watch while you work. Everything it shows is read back off disk, so a file
 * edited by hand shows up here as edited — the files are the store.
 */
import { useEffect, useState } from "react";
import { api } from "../api";
import { Icon } from "./Icon";
import type { MemoryCategoryView, MemoryProposal } from "../types";

interface Props {
  /** The open chat: whose suggestions these are, and what Summarise acts on. */
  chatId: string | null;
  /**
   * Told when memory changed. The profile is sent with every request, so the
   * transcript's cost and context estimate are stale the moment it is edited.
   */
  onChanged: () => void;
}

const BTN =
  "rounded-lg border border-neutral-800 px-3 py-1.5 text-xs text-neutral-300 transition hover:border-neutral-700 disabled:opacity-50";
const INPUT =
  "rounded-lg border border-neutral-800 bg-neutral-900 px-2 py-1.5 text-xs text-neutral-200 outline-none focus:border-neutral-700";
const LINK = "text-[11px] text-sky-400 hover:text-sky-300 disabled:opacity-50";

export function MemorySection({ chatId, onChanged }: Props) {
  const [body, setBody] = useState("");
  const [path, setPath] = useState("");
  const [proposals, setProposals] = useState<MemoryProposal[]>([]);
  const [store, setStore] = useState<MemoryCategoryView[]>([]);
  const [draft, setDraft] = useState("");
  // Where a hand-typed line is filed. The profile is the default because a line
  // someone types here is usually about themselves, but it is also the one file
  // that is sent with every request, so the choice is visible rather than implied.
  const [target, setTarget] = useState("profile");
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const reload = async () => {
    setBody((await api.getMemory()) ?? "");
    setPath(await api.memoryPath());
    setStore(await api.memoryOverview());
    if (chatId) setProposals(await api.listProposals(chatId, "pending"));
    else setProposals([]);
  };

  useEffect(() => {
    reload().catch((e) => setStatus(String(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [chatId]);

  const run = async (what: () => Promise<void>) => {
    setBusy(true);
    try {
      await what();
      await reload();
      onChanged();
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  };

  const saveProfile = () =>
    run(async () => {
      await api.setMemory(body);
      setStatus("Profile saved.");
    });

  const addTyped = () => {
    const entry = draft.trim();
    if (!entry) return;
    return run(async () => {
      await api.appendMemory(target, [entry]);
      setDraft("");
      setStatus(`Added to ${target}.`);
    });
  };

  const decide = (ids: string[], accept: boolean) =>
    run(async () => {
      if (accept) {
        const written = await api.acceptProposals(ids);
        setStatus(`Kept ${written} entr${written === 1 ? "y" : "ies"}.`);
      } else {
        await api.rejectProposals(ids);
      }
    });

  const summarize = () =>
    run(async () => {
      if (!chatId) return;
      setStatus("Summarising…");
      const raw = await api.summarizeChat(chatId);
      const [summary, count, topic, logPath] = raw.split("\u001f");
      setStatus(
        `${summary} — ${count} suggestion(s) below.` +
          (logPath ? ` Saved CHATLOG/${logPath.split(/[\\/]/).pop()} (${topic}).` : ""),
      );
    });

  // The profile rides along with every single request, so a nudge when it grows
  // is worth more than a hard limit: the user may still write whatever they like.
  const profileEntries = body.split("\n").filter((line) => line.trim().startsWith("- ")).length;
  const profileHeavy = profileEntries > 50 || body.length > 2_000;
  const filed = store.filter((c) => c.entries.length > 0);
  const empty = store.filter((c) => c.entries.length === 0);

  return (
    <>
      {status && (
        <p className="rounded-lg border border-neutral-800 bg-neutral-900/60 px-3 py-2 text-[11px] text-neutral-400">
          {status}
        </p>
      )}

      <section>
        <div className="mb-2 flex items-center justify-between">
          <label className="text-xs text-neutral-500">
            Suggestions for the open chat
          </label>
          <button
            onClick={() => Promise.resolve(summarize())}
            disabled={busy || !chatId}
            className={LINK}
            title={
              chatId
                ? "Read the conversation and propose what is worth remembering"
                : "Open a chat first"
            }
          >
            Summarise this chat
          </button>
        </div>
        {!chatId && <p className="text-xs text-neutral-600">Open a chat first.</p>}
        {chatId && proposals.length === 0 && (
          <p className="text-xs text-neutral-600">
            Nothing to review. <span className="text-neutral-400">Summarise this chat</span> reads
            the conversation and proposes entries; nothing is stored until you accept it. You can
            also type <span className="text-neutral-400">/summarise</span> in the composer.
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
                  className={BTN}
                >
                  Reject
                </button>
                {/* Where Accept will file it. A profile entry is sent with every
                    request, so it is worth seeing before agreeing. */}
                <span
                  className="ml-auto text-[10px] text-neutral-500"
                  title={
                    p.target === "profile"
                      ? "Goes into MEMORY.md, which is sent with every request"
                      : `Goes into memory/${p.target}.md, looked up only when it is relevant`
                  }
                >
                  {p.target === "profile" ? "→ always sent" : `→ ${p.target}`}
                </span>
              </div>
            </div>
          ))}
        </div>
      </section>

      <section>
        <label className="block text-xs text-neutral-500">Kept memory</label>
        <p className="mt-1 mb-2 text-[11px] text-neutral-600">
          One file per category. These are looked up only when they match what you ask, so they can
          grow without costing anything on the turns where none of it is relevant.
        </p>
        {filed.length === 0 ? (
          <p className="text-xs text-neutral-600">Nothing filed yet.</p>
        ) : (
          <div className="space-y-3">
            {filed.map((c) => (
              <div key={c.category}>
                <div className="mb-1 flex items-baseline gap-2">
                  <span className="shrink-0 text-[11px] font-medium text-neutral-300">
                    {c.category}
                  </span>
                  <span className="truncate text-[10px] text-neutral-600">{c.about}</span>
                </div>
                <div className="space-y-1">
                  {c.entries.map((e) => (
                    <div
                      key={`${c.category}-${e.date}-${e.text}`}
                      className="flex items-start gap-2 rounded-lg border border-neutral-800 bg-neutral-900/60 px-2 py-1.5 text-xs"
                    >
                      <span className="min-w-0 flex-1 text-neutral-300">{e.text}</span>
                      <span className="shrink-0 text-[10px] text-neutral-600">{e.date}</span>
                      <button
                        onClick={() =>
                          run(async () => {
                            await api.deleteMemoryEntry(c.category, e.text);
                          })
                        }
                        disabled={busy}
                        className="shrink-0 text-neutral-500 hover:text-red-400 disabled:opacity-50"
                        title="Forget this"
                      >
                        <Icon name="close" />
                      </button>
                    </div>
                  ))}
                </div>
              </div>
            ))}
            {empty.length > 0 && (
              <p className="text-[10px] text-neutral-600">
                Nothing yet under {empty.map((c) => c.category).join(", ")}.
              </p>
            )}
          </div>
        )}
      </section>

      <section>
        <label className="block text-xs text-neutral-500">Remember something</label>
        <div className="mt-2 flex gap-2">
          <select
            value={target}
            onChange={(e) => setTarget(e.target.value)}
            className={`${INPUT} min-w-0 flex-1`}
          >
            <option value="profile">profile — sent with every request</option>
            {store.map((c) => (
              <option key={c.category} value={c.category}>
                {c.category} — {c.about}
              </option>
            ))}
          </select>
          <input
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void addTyped();
            }}
            placeholder="e.g. I write in British English"
            className={`${INPUT} min-w-0 flex-[2]`}
          />
          <button onClick={() => void addTyped()} disabled={busy || !draft.trim()} className={BTN}>
            Add
          </button>
        </div>
      </section>

      <section>
        <div className="mb-2 flex items-center justify-between">
          <label className="text-xs text-neutral-500">
            Profile — sent with every request
          </label>
          <span className="flex items-center gap-3">
            <button
              onClick={() => void api.revealPath(path).catch((e) => setStatus(String(e)))}
              className={LINK}
              title="Open the folder these files live in"
            >
              Show folder
            </button>
            <button onClick={() => void saveProfile()} disabled={busy} className={LINK}>
              Save
            </button>
          </span>
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
            {profileEntries} entries here, and all of it is sent with every message. Keep this file
            to who the user is and how they want to be worked with; everything else belongs in the
            categories above, where it is only looked up when it is relevant.
          </p>
        )}
      </section>
    </>
  );
}
