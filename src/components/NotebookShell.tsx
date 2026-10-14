import { useCallback, useEffect, useState } from "react";
import { api } from "../api";
import { citations } from "../lib/citation";
import { quantities } from "../lib/quiz";
import { when } from "../lib/time";
import { Icon, IconButton } from "./Icon";
import { MakeDialog } from "./MakeDialog";
import { Markdown } from "./Markdown";
import { StudySetDialog } from "./StudySetDialog";
import type {
  Generator,
  Instruction,
  Made,
  MakeOptions,
  NotebookOutputRow,
  NotebookRow,
  NotebookView,
  SourceView,
  StudySetRow,
} from "../types";

interface Props {
  /** The notebook this pane is showing. */
  notebookId: string;
  /** Every notebook, for the picker in the rail's header. */
  notebooks: NotebookRow[];
  /** The models an endpoint serves, for the pin beside the picker. */
  models: { id: string }[];
  /** Show another notebook in this pane. */
  onOpen: (id: string) => void;
  /** The notebook list changed — a copy, a delete — so the sidebar and the
   *  palette are stale. */
  onChanged: () => void;
  /** This notebook was deleted, so the pane showing it has to go somewhere. */
  onDeleted: (id: string) => void;
}

const quiet =
  "rounded-lg px-2 py-1 text-[12px] text-neutral-400 transition duration-150 hover:bg-neutral-800 hover:text-neutral-200";

/**
 * A notebook: its sources on the left, the conversation about them in the
 * middle, and what it has produced below.
 *
 * Three regions and no fourth, because the notebook *is* the surface — it is not
 * a panel beside a chat. The model is sent the catalogue (each source's name and
 * what it contains) and reaches the text with `source_search` and `read_source`,
 * so forty documents cost what one question costs rather than what forty
 * documents cost. A question's answer cites what it used by label, and those
 * labels come back as chips.
 *
 * The outputs are documents rather than artifacts, and deliberately: an artifact
 * belongs to a chat (`artifacts.chat_id` is a foreign key), and a notebook has no
 * chat — that is what keeps every surface that reads a chat from having to learn
 * about a second kind of conversation. So an output is a file in the notebook's
 * own folder, written by the same writers a chat uses.
 */
export function NotebookShell({
  notebookId,
  notebooks,
  models,
  onOpen,
  onChanged,
  onDeleted,
}: Props) {
  const [view, setView] = useState<NotebookView | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [question, setQuestion] = useState("");
  const [url, setUrl] = useState("");
  const [title, setTitle] = useState("");
  /** The menu of things this notebook can make, read from Rust rather than
   *  spelled again here. */
  const [generators, setGenerators] = useState<Generator[]>([]);
  /** The source whose full entry is open, if one is. */
  const [reading, setReading] = useState<SourceView | null>(null);
  /** The study set being taken, if one is. */
  const [taking, setTaking] = useState<StudySetRow | null>(null);
  /** The entry whose options are open, if one is. */
  const [making, setMaking] = useState<Generator | null>(null);

  const load = useCallback(async () => {
    const next = await api.openNotebook(notebookId);
    setView(next);
    setTitle(next.notebook.title);
    // Returned as well as stored, so a caller can see what changed: an answer that
    // made a study set is the one thing here the reader has to be *told* about,
    // because it appears in a list rather than in the reply.
    return next;
  }, [notebookId]);

  useEffect(() => {
    load().catch((e) => setStatus(String(e)));
  }, [load]);

  useEffect(() => {
    // Read once: the list is a constant in Rust, and a menu that changed under a
    // press would be a menu nobody could aim at.
    api.notebookGenerators().then(setGenerators).catch(() => setGenerators([]));
  }, []);

  /** True when the work finished, so a caller can decide what to do next — a form
   *  that closed on a refusal would take the reason away with it. */
  const act = async (work: () => Promise<unknown>, note?: string) => {
    setBusy(true);
    setStatus(null);
    try {
      await work();
      await load();
      if (note) setStatus(note);
      return true;
    } catch (e) {
      setStatus(String(e));
      return false;
    } finally {
      setBusy(false);
    }
  };

  const runEntry = async (options: MakeOptions) => {
    const entry = making;
    if (!entry) return;
    const done = await act(async () => {
      const made = await api.runMakeEntry(notebookId, entry.id, options);
      setStatus(whatWasMade(entry, made));
    });
    // Closed only on success: a refusal is something to read and adjust the form
    // against, and it lands in the status line where it stays visible.
    if (done) setMaking(null);
  };

  const addFile = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const picked = await open({
        multiple: false,
        title: "Add a document to this notebook",
      });
      if (typeof picked !== "string") return;
      await act(
        () => api.addSource(notebookId, picked, null),
        "Added. Its digest is written once — editing the copy and pressing Re-sync is what asks for a new one.",
      );
    } catch (e) {
      setStatus(String(e));
    }
  };

  const ask = async () => {
    const text = question.trim();
    if (!text || busy) return;
    setBusy(true);
    setStatus(null);
    try {
      const before = view?.sets.length ?? 0;
      const asked = await api.askNotebook(notebookId, text);
      setQuestion("");
      const next = await load();
      if (next.sets.length > before) {
        setStatus(`Made "${next.sets[0].title}" — it is in the Study list, where you take it.`);
      } else if (asked.unresolved.length > 0) {
        // A citation that names something the notebook does not hold is the one
        // failure this feature is supposed to make visible rather than hide.
        setStatus(
          `The answer cited ${asked.unresolved.join(", ")}, which this notebook does not hold.`,
        );
      }
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  };

  const save = (content: string, format: string) =>
    act(
      () => api.saveNotebookOutput(notebookId, view?.notebook.title ?? "Notebook", format, content),
      "Saved into the notebook's folder.",
    );

  const copy = async () => {
    setBusy(true);
    try {
      const made = await api.copyNotebook(notebookId);
      onChanged();
      onOpen(made.id);
      setStatus("Copied — its sources and outputs came with it, its questions did not.");
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    setBusy(true);
    try {
      await api.deleteNotebook(notebookId);
      onChanged();
      onDeleted(notebookId);
    } catch (e) {
      setStatus(String(e));
      setBusy(false);
    }
  };

  if (!view) {
    return (
      <div className="flex min-w-0 flex-1 items-center justify-center text-[13px] text-neutral-500">
        {status ?? "Opening the notebook…"}
      </div>
    );
  }

  const sourceNames = view.sources.map((s) => s.name);
  const notebook = view.notebook;

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col bg-neutral-950">
      <div className="flex shrink-0 items-center gap-2 border-b border-neutral-800/80 px-3 py-2">
        <Icon name="book" size={14} className="shrink-0 text-sky-400" />
        <input
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          onBlur={() => {
            if (title.trim() && title.trim() !== notebook.title) {
              void act(() => api.renameNotebook(notebookId, title.trim()));
            }
          }}
          aria-label="Notebook title"
          className="min-w-0 flex-1 bg-transparent text-[13px] text-neutral-100 outline-hidden"
        />
        <select
          value={notebookId}
          onChange={(e) => onOpen(e.target.value)}
          aria-label="Which notebook"
          className="max-w-48 rounded-lg bg-neutral-900 px-2 py-1 text-[12px] text-neutral-300 outline-hidden"
        >
          {notebooks.map((n) => (
            <option key={n.id} value={n.id}>
              {n.title}
            </option>
          ))}
        </select>
        <select
          value={notebook.model}
          onChange={(e) => void act(() => api.setNotebookModel(notebookId, e.target.value))}
          aria-label="The model questions run on"
          className="max-w-44 rounded-lg bg-neutral-900 px-2 py-1 text-[12px] text-neutral-300 outline-hidden"
        >
          <option value="">The chat's model</option>
          {models.map((m) => (
            <option key={m.id} value={m.id}>
              {m.id}
            </option>
          ))}
        </select>
        <button onClick={() => void copy()} disabled={busy} className={quiet}>
          Copy
        </button>
        <button
          onClick={() => void remove()}
          disabled={busy}
          className="rounded-lg px-2 py-1 text-[12px] text-neutral-400 transition duration-150 hover:bg-red-950/40 hover:text-red-300"
        >
          Delete
        </button>
      </div>

      <div className="flex min-h-0 flex-1">
        {/* The rail: what this notebook holds, and what the model may read. */}
        <div className="flex w-72 shrink-0 flex-col border-r border-neutral-800/80">
          <div className="flex items-center gap-1 border-b border-neutral-800/60 px-3 py-2">
            <span className="flex-1 text-[12px] tracking-wide text-neutral-500 uppercase">
              Sources
            </span>
            <button onClick={() => void addFile()} disabled={busy} className={quiet}>
              Add file
            </button>
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto p-2">
            {view.sources.length === 0 && (
              <p className="px-1 py-2 text-[12px] leading-relaxed text-neutral-500">
                Nothing here yet. Add a document or a page and it is copied into this notebook's
                own folder, listed with a line saying what it contains.
              </p>
            )}
            {view.sources.map((source) => (
              <div
                key={source.id}
                className={`mb-1 rounded-lg p-2 transition ${source.enabled ? "bg-neutral-900/60" : "opacity-55"}`}
              >
                <div className="flex items-center gap-2">
                  <input
                    type="checkbox"
                    checked={source.enabled}
                    onChange={(e) =>
                      void act(() => api.setSourceEnabled(source.id, e.target.checked))
                    }
                    aria-label={`Use ${source.name}`}
                    className="shrink-0"
                  />
                  <button
                    onClick={() => source.path && void api.revealPath(source.path)}
                    title="Show this file"
                    className="min-w-0 flex-1 truncate text-left text-[13px] text-neutral-200"
                  >
                    {source.name}
                  </button>
                  <State state={source.state} />
                </div>
                <p className="mt-1 line-clamp-3 text-[12px] leading-relaxed text-neutral-500">
                  {source.digest || "Not described yet."}
                </p>
                <div className="mt-1 flex items-center gap-1">
                  {source.chars === 0 && (
                    <span className="text-[11px] text-amber-500/80">No readable text</span>
                  )}
                  <span className="flex-1 text-[11px] text-neutral-600">
                    {source.chars > 0 ? `${source.chars.toLocaleString()} characters` : ""}
                  </span>
                  {source.digest !== "" && (
                    <button
                      onClick={() => setReading(source)}
                      title="Read the whole description of this source"
                      className={quiet}
                    >
                      Read
                    </button>
                  )}
                  <button
                    onClick={() =>
                      void act(
                        () => api.resyncSource(source.id),
                        "Read again — the digest is only rewritten if the text changed.",
                      )
                    }
                    disabled={busy}
                    className={quiet}
                  >
                    Re-sync
                  </button>
                  <IconButton
                    icon="close"
                    label={`Remove ${source.name}`}
                    onClick={() => void act(() => api.removeSource(source.id))}
                  />
                </div>
              </div>
            ))}
          </div>
          <div className="border-t border-neutral-800/60 p-2">
            <div className="flex items-center gap-1">
              <input
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                placeholder="https://…"
                aria-label="Add a page by URL"
                className="min-w-0 flex-1 rounded-lg bg-neutral-900 px-2 py-1.5 text-[12px] text-neutral-200 outline-hidden"
              />
              <button
                onClick={() => {
                  const target = url.trim();
                  if (!target) return;
                  setUrl("");
                  void act(
                    () => api.addSource(notebookId, null, target),
                    "Read once and kept as a file beside the others.",
                  );
                }}
                disabled={busy}
                className={quiet}
              >
                Add page
              </button>
            </div>
          </div>
        </div>

        {/* The conversation: answers cite what they used, by label. */}
        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
          <div className="min-h-0 flex-1 overflow-y-auto p-3">
            {view.messages.length === 0 && (
              <p className="px-1 py-2 text-[12px] leading-relaxed text-neutral-500">
                Ask something about the sources. The answer is written from them and says which one
                it used; if it was not in them, it says so rather than guessing.
              </p>
            )}
            {view.messages.map((message) => (
              <div key={message.id} className="mb-3">
                <div className="mb-1 text-[11px] tracking-wide text-neutral-600 uppercase">
                  {message.role === "user" ? "You" : "Notebook"}
                </div>
                <div
                  className={`rounded-xl px-3 py-2 text-[13px] leading-relaxed ${
                    message.role === "user"
                      ? "bg-neutral-900 text-neutral-200"
                      : "bg-neutral-900/40 text-neutral-100"
                  }`}
                >
                  <Markdown>{message.content}</Markdown>
                </div>
                {message.role === "assistant" && (
                  <div className="mt-1 flex flex-wrap items-center gap-1">
                    {citations(message.citations, sourceNames).map((cite) => (
                      <span
                        key={cite.label}
                        title={
                          cite.known
                            ? `${cite.name}${cite.heading ? ` → ${cite.heading}` : ""}`
                            : `${cite.name} is no longer in this notebook`
                        }
                        className={`rounded-md px-1.5 py-0.5 text-[11px] ${
                          cite.known
                            ? "bg-sky-500/10 text-sky-300"
                            : "bg-amber-500/10 text-amber-400"
                        }`}
                      >
                        {cite.name}
                        {cite.heading ? ` → ${cite.heading}` : ""}
                        {cite.known ? "" : " (gone)"}
                      </span>
                    ))}
                    <span className="flex-1" />
                    {["md", "docx", "pdf"].map((format) => (
                      <button
                        key={format}
                        onClick={() => void save(message.content, format)}
                        disabled={busy}
                        className={quiet}
                      >
                        Save .{format}
                      </button>
                    ))}
                  </div>
                )}
              </div>
            ))}
          </div>
          <div className="shrink-0 border-t border-neutral-800/60 p-2">
            <div className="flex items-end gap-2">
              <textarea
                value={question}
                onChange={(e) => setQuestion(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && !e.shiftKey) {
                    e.preventDefault();
                    void ask();
                  }
                }}
                rows={2}
                placeholder="Ask about these sources… (Enter to ask, Shift+Enter for a new line)"
                aria-label="Ask this notebook"
                className="min-h-0 min-w-0 flex-1 resize-none rounded-xl bg-neutral-900 px-3 py-2 text-[13px] text-neutral-100 outline-hidden"
              />
              <button
                onClick={() => void ask()}
                disabled={busy || !question.trim()}
                className="rounded-xl bg-sky-600 px-3 py-2 text-[13px] text-white transition duration-150 hover:bg-sky-500 disabled:opacity-40"
              >
                Ask
              </button>
            </div>
            {view.notebook.instructions.length > 0 && (
              <p className="mt-1 text-[11px] text-neutral-600">
                {view.notebook.instructions.length} rule
                {view.notebook.instructions.length === 1 ? "" : "s"} apply to every answer.
              </p>
            )}
          </div>
        </div>

        {/* What it produced. */}
        <div className="flex w-56 shrink-0 flex-col border-l border-neutral-800/80">
          <div className="flex items-center gap-1 border-b border-neutral-800/60 px-3 py-2">
            <span className="flex-1 text-[12px] tracking-wide text-neutral-500 uppercase">
              Make
            </span>
          </div>
          <div className="border-b border-neutral-800/60 p-2">
            {generators.map((entry) => (
              <button
                key={entry.id}
                onClick={() => setMaking(entry)}
                disabled={busy}
                className="mb-0.5 w-full rounded-lg px-2 py-1 text-left text-[12px] text-neutral-300 transition duration-150 hover:bg-neutral-800 hover:text-neutral-100 disabled:opacity-40"
              >
                {entry.title}
                {entry.formats.length > 0 && (
                  // The format the entry really writes rather than a promise about
                  // it: an entry that writes no file has none to name.
                  <span className="ml-1 text-[11px] text-neutral-600">.{entry.formats[0]}</span>
                )}
              </button>
            ))}
            <p className="px-1 pt-1 text-[11px] leading-relaxed text-neutral-600">
              Every entry opens a form first: how many, from which sources, and anything you want
              it to aim at.
            </p>
          </div>
          <div className="border-b border-neutral-800/60 px-3 py-2 text-[12px] tracking-wide text-neutral-500 uppercase">
            Study
          </div>
          <div className="max-h-44 shrink-0 overflow-y-auto border-b border-neutral-800/60 p-2">
            {view.sets.length === 0 && (
              <p className="px-1 py-2 text-[12px] leading-relaxed text-neutral-500">
                Ask for a quiz or a set of flashcards and the model makes one through a tool call.
                It lands here rather than in the answer, and this is where you take it.
              </p>
            )}
            {view.sets.map((set) => (
              <div
                key={set.id}
                className="mb-1 flex items-center gap-1 rounded-lg bg-neutral-900/60 px-2 py-1.5"
              >
                <button
                  onClick={() => setTaking(set)}
                  title={`Take "${set.title}"`}
                  className="min-w-0 flex-1 truncate text-left text-[12px] text-neutral-200"
                >
                  {set.title}
                  <span className="ml-1 text-[11px] text-neutral-500">
                    {quantities(set.items.length, set.kind)}
                  </span>
                </button>
                <IconButton
                  icon="close"
                  label={`Remove ${set.title}`}
                  onClick={() => void act(() => api.removeNotebookSet(set.id))}
                />
              </div>
            ))}
          </div>
          <div className="border-b border-neutral-800/60 px-3 py-2 text-[12px] tracking-wide text-neutral-500 uppercase">
            Outputs
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto p-2">
            {view.outputs.length === 0 && (
              <p className="px-1 py-2 text-[12px] leading-relaxed text-neutral-500">
                Nothing saved yet. <em>Save .md</em> under an answer writes it into this notebook's
                folder.
              </p>
            )}
            {view.outputs.map((output) => (
              <OutputRow
                key={output.id}
                output={output}
                onRemove={() => void act(() => api.removeNotebookOutput(output.id))}
              />
            ))}
          </div>
        </div>
      </div>

      {status && (
        <div className="shrink-0 border-t border-neutral-800/60 px-3 py-1.5 text-[12px] text-neutral-400">
          {status}
        </div>
      )}
      <Rules
        notebook={notebook}
        onSave={(instructions) => act(() => api.setNotebookInstructions(notebookId, instructions))}
      />
      {reading && <SourceRead source={reading} onClose={() => setReading(null)} />}
      {taking && (
        <StudySetDialog set={taking} sources={sourceNames} onClose={() => setTaking(null)} />
      )}
      {making && (
        <MakeDialog
          entry={making}
          // Only the sources that are switched on: one switched off cannot be read
          // however it is ticked, so offering it here would be a box that does
          // nothing.
          sources={view.sources.filter((source) => source.enabled)}
          busy={busy}
          onRun={(options) => void runEntry(options)}
          onClose={() => setMaking(null)}
        />
      )}
    </div>
  );
}

/**
 * One source read in full.
 *
 * The card holds a name, a state and three lines of description, which on a
 * 288 px rail is most of a digest and none of the facts behind it — so the one
 * thing that says what a document *contains* is the one thing that gets cut.
 * This is the rest of the entry: the whole description, where the source came
 * from, and the copy this notebook made of it.
 *
 * It reads the payload the rail already has and nothing else, which is why there
 * is no command behind it: a dialog that needed its own query would be a second
 * answer to "what does this notebook hold".
 *
 * Drawn whether or not the description overflows *here*. Whether it fits is a
 * layout question, and asking it (`scrollHeight > clientHeight`) would hide the
 * control exactly when a wider font or a narrower pane pushed the text past the
 * clamp — one rule, nothing measured, nothing to disagree with itself.
 */
function SourceRead({ source, onClose }: { source: SourceView; onClose: () => void }) {
  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-neutral-950/60 pt-16"
      onMouseDown={onClose}
      onKeyDown={(e) => {
        if (e.key === "Escape") onClose();
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={`Source: ${source.name}`}
        className="flex max-h-[calc(100vh-8rem)] w-[34rem] max-w-[calc(100vw-2rem)] flex-col overflow-hidden rounded-xl border border-neutral-800 bg-neutral-900 shadow-2xl"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2 border-b border-neutral-800 px-4 py-3">
          <span className="min-w-0 flex-1 truncate text-sm font-medium text-neutral-200">
            {source.name}
          </span>
          <State state={source.state} />
          <button
            autoFocus
            onClick={onClose}
            title="Close"
            className="shrink-0 text-neutral-500 transition hover:text-neutral-200"
          >
            <Icon name="close" size={15} />
          </button>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto p-4">
          <div className="text-[11px] tracking-wide text-neutral-500 uppercase">
            What it contains
          </div>
          <p className="mt-1 text-[13px] leading-relaxed whitespace-pre-wrap text-neutral-200">
            {source.digest || "Not described yet."}
          </p>

          <dl className="mt-4 grid grid-cols-[6.5rem_1fr] gap-x-3 gap-y-1.5 text-[12px]">
            <dt className="text-neutral-500">From</dt>
            <dd className="break-words text-neutral-400">{source.origin || source.kind}</dd>
            <dt className="text-neutral-500">Copy</dt>
            <dd className="break-words font-mono text-[11px] text-neutral-500">
              {source.path ?? "none — nothing was copied in"}
            </dd>
            <dt className="text-neutral-500">Read</dt>
            <dd className="text-neutral-400">
              {source.chars > 0
                ? `${source.chars.toLocaleString()} characters`
                : "nothing readable was found in it"}
            </dd>
            <dt className="text-neutral-500">Used</dt>
            <dd className="text-neutral-400">
              {source.enabled ? "yes — answers may read it" : "no — switched off"}
            </dd>
            <dt className="text-neutral-500">Described</dt>
            <dd className="text-neutral-400">
              {source.digestAt === null ? "not yet" : when(source.digestAt)}
              {source.state === "stale" ? " — the file has changed since" : ""}
            </dd>
            <dt className="text-neutral-500">Added</dt>
            <dd className="text-neutral-400">{when(source.createdAt)}</dd>
          </dl>
        </div>

        <div className="flex items-center gap-2 border-t border-neutral-800 px-4 py-3">
          {source.path && (
            <button
              onClick={() => source.path && void api.revealPath(source.path)}
              className="rounded-lg border border-neutral-700 px-2.5 py-1 text-xs text-neutral-300 transition hover:border-neutral-600 hover:text-neutral-100"
            >
              Show the file
            </button>
          )}
          <span className="flex-1" />
          <button
            onClick={onClose}
            className="rounded-lg px-2.5 py-1 text-xs text-neutral-400 transition hover:text-neutral-200"
          >
            Done
          </button>
        </div>
      </div>
    </div>
  );
}

/**
 * What to say after a press.
 *
 * A set is named by what it holds, because the reader has to go and find it in a
 * list rather than in a file; a document by where it went, because that is the
 * thing they would otherwise have to hunt for. One place, so the two never come to
 * describe the same result differently.
 */
function whatWasMade(entry: Generator, made: Made): string {
  const set = made.sets[0];
  if (set) {
    return `Made "${set.title}" — ${quantities(set.items.length, set.kind)}, under Study. Take it whenever you like.`;
  }
  // More than one file is worth naming: the exam's second document is the answers,
  // and a reader who is not told it is there will look for them on the paper.
  if (made.outputs.length > 1) {
    return `${made.outputs.map((one) => `"${one.title}"`).join(" and ")} — both in the notebook's folder as .${entry.formats[0]}.`;
  }
  return entry.action === "table"
    ? `${entry.title} — every row cites a source, exported as a .${entry.formats[0]}.`
    : `${entry.title} — written into the notebook's folder as a .${entry.formats[0]}.`;
}

/** A source's sync state, in one word and one colour. */
function State({ state }: { state: string }) {
  const label = state === "ok" ? "current" : state;
  const tone =
    state === "ok"
      ? "text-neutral-500"
      : state === "stale"
        ? "text-amber-500"
        : "text-red-400";
  return (
    <span
      title={
        state === "ok"
          ? "The copy is the file this digest describes"
          : state === "stale"
            ? "The file changed — Re-sync to read it again"
            : "The file is gone"
      }
      className={`shrink-0 text-[11px] ${tone}`}
    >
      {label}
    </span>
  );
}

function OutputRow({ output, onRemove }: { output: NotebookOutputRow; onRemove: () => void }) {
  return (
    <div className="mb-1 flex items-center gap-1 rounded-lg bg-neutral-900/60 px-2 py-1.5">
      <button
        onClick={() => output.target && void api.revealPath(output.target)}
        title={output.target}
        className="min-w-0 flex-1 truncate text-left text-[12px] text-neutral-200"
      >
        {output.title}
        <span className="ml-1 text-[11px] text-neutral-500">.{output.kind}</span>
      </button>
      <IconButton icon="close" label={`Remove ${output.title}`} onClick={onRemove} />
    </div>
  );
}

/**
 * The notebook's own instruction blocks, in the same shape a project's rules
 * take and through the same rule about which of them apply.
 *
 * Collapsed by default: most notebooks have none, and the one thing a reader
 * wants from this strip is the source list.
 */
function Rules({
  notebook,
  onSave,
}: {
  notebook: NotebookRow;
  onSave: (instructions: Instruction[]) => void;
}) {
  const [open, setOpen] = useState(false);
  const [body, setBody] = useState("");

  if (!open) {
    return (
      <div className="shrink-0 border-t border-neutral-800/60 px-3 py-1.5">
        <button onClick={() => setOpen(true)} className="text-[12px] text-neutral-500 hover:text-neutral-300">
          {notebook.instructions.length > 0
            ? `${notebook.instructions.length} rule${notebook.instructions.length === 1 ? "" : "s"} — edit`
            : "Add a rule for every answer"}
        </button>
      </div>
    );
  }
  return (
    <div className="shrink-0 border-t border-neutral-800/60 p-3">
      <div className="mb-1 text-[11px] tracking-wide text-neutral-500 uppercase">
        Rules for every answer
      </div>
      {notebook.instructions.map((rule) => (
        <div key={rule.id} className="mb-1 rounded-lg bg-neutral-900/60 px-2 py-1.5 text-[12px]">
          <div className="flex items-center gap-2">
            <span className="flex-1 text-neutral-200">{rule.name}</span>
            <button
              onClick={() => onSave(notebook.instructions.filter((r) => r.id !== rule.id))}
              className={quiet}
            >
              Remove
            </button>
          </div>
          <p className="mt-0.5 text-neutral-500">{rule.body}</p>
        </div>
      ))}
      <textarea
        value={body}
        onChange={(e) => setBody(e.target.value)}
        rows={2}
        placeholder="e.g. Answer in bullet points and give the page for every citation."
        aria-label="A new rule for this notebook"
        className="mt-1 w-full resize-none rounded-lg bg-neutral-900 px-2 py-1.5 text-[12px] text-neutral-200 outline-hidden"
      />
      <div className="mt-1 flex items-center gap-2">
        <button
          disabled={!body.trim()}
          onClick={() => {
            const text = body.trim();
            if (!text) return;
            setBody("");
            onSave([
              ...notebook.instructions,
              {
                id: `rule-${Date.now()}`,
                name: text.split(/\s+/).slice(0, 4).join(" "),
                scope: "chat",
                body: text,
                enabled: true,
              },
            ]);
          }}
          className={quiet}
        >
          Add
        </button>
        <button onClick={() => setOpen(false)} className={quiet}>
          Done
        </button>
      </div>
    </div>
  );
}
