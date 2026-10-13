import { useEffect, useState } from "react";
import { api } from "../api";
import { Icon } from "./Icon";
import type { PromptView } from "../types";

const inputCls =
  "w-full rounded-lg border border-neutral-800 bg-neutral-900 px-3 py-2 text-sm text-neutral-200 outline-hidden focus:border-neutral-700";

const quiet =
  "rounded-lg px-3 py-1.5 text-[13px] text-neutral-300 transition duration-150 hover:bg-neutral-800 hover:text-neutral-100 disabled:opacity-40";

/**
 * Saved prompts: the requests that get typed again and again.
 *
 * A prompt is a *one-shot text*, where a skill is a set of *rules* — the model
 * never sees this list, and nothing here costs catalogue space in a request. Its
 * whole job is the composer: `/p:<id>` in the slash menu, or the palette, puts
 * the text in the box to be edited and sent.
 *
 * One markdown file each, in its own folder, hand-editable — the app is a
 * convenience over the folder rather than the only way in.
 */
export function PromptsSection() {
  const [prompts, setPrompts] = useState<PromptView[]>([]);
  const [name, setName] = useState("");
  const [body, setBody] = useState("");
  /** The id being edited, so Save says "revise" rather than "add". */
  const [editing, setEditing] = useState<string | null>(null);
  const [dir, setDir] = useState("");
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const reload = async () => setPrompts(await api.listPrompts());

  useEffect(() => {
    reload().catch((e) => setStatus(String(e)));
    api
      .promptsDir()
      .then(setDir)
      .catch(() => {});
  }, []);

  const clear = () => {
    setName("");
    setBody("");
    setEditing(null);
  };

  const save = async () => {
    setBusy(true);
    try {
      const id = await api.savePrompt(name, body);
      clear();
      await reload();
      const prompt = name.trim();
      setStatus(
        `Saved "${prompt}". Type \`/p:${id}\` in the composer, or pick it from Ctrl+K.`,
      );
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  };

  const edit = (prompt: PromptView) => {
    setName(prompt.name);
    setBody(prompt.body);
    setEditing(prompt.id);
    setStatus(null);
  };

  const remove = async (prompt: PromptView) => {
    setBusy(true);
    try {
      await api.deletePrompt(prompt.id);
      if (editing === prompt.id) clear();
      await reload();
      setStatus(`Removed "${prompt.name}".`);
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  };

  /** Import another assistant's prompt folder: each `.md` becomes a saved prompt. */
  const importFolder = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const folder = await open({
        directory: true,
        multiple: false,
        title: "Pick a folder of prompt files",
      });
      if (typeof folder !== "string") return;
      setBusy(true);
      const report = await api.importPrompts(folder);
      await reload();
      const parts: string[] = [];
      if (report.imported.length) parts.push(`Imported ${report.imported.join(", ")}.`);
      if (report.skipped.length) {
        parts.push(`Left ${report.skipped.join(", ")} alone — already saved.`);
      }
      setStatus(parts.join(" ") || "No prompt files found in that folder.");
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="space-y-6">
      <p className="text-[13px] leading-relaxed text-neutral-500">
        A prompt is a request you retype — a review checklist, a standing question, the shape of
        a brief. Picking one puts its text in the composer to edit before sending; unlike a skill,
        it is never part of what the model is told.
      </p>

      <section className="space-y-3">
        <h3 className="text-[11px] font-medium uppercase tracking-wide text-neutral-600">
          {editing ? `Editing "${name}"` : "New prompt"}
        </h3>
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Name, e.g. Review a diff"
          spellCheck={false}
          className={inputCls}
        />
        <textarea
          value={body}
          onChange={(e) => setBody(e.target.value)}
          placeholder="The text that lands in the composer."
          spellCheck={false}
          rows={10}
          className={`${inputCls} resize-y font-mono text-[12px] leading-relaxed`}
        />
        <div className="flex items-center gap-2">
          <button onClick={() => void save()} disabled={busy} className={quiet}>
            {editing ? "Save changes" : "+ Add prompt"}
          </button>
          {editing && (
            <button onClick={clear} className={quiet}>
              Cancel
            </button>
          )}
          <span className="text-[11px] text-neutral-600">
            Saving with the same name revises that prompt.
          </span>
        </div>
      </section>

      <section className="space-y-2">
        <div className="flex items-center justify-between">
          <h3 className="text-[11px] font-medium uppercase tracking-wide text-neutral-600">
            Saved ({prompts.length})
          </h3>
          <button
            onClick={() => void importFolder()}
            disabled={busy}
            className={quiet}
            title="Read a folder of markdown prompt files — another assistant's, or your own"
          >
            Import folder…
          </button>
        </div>
        {prompts.length === 0 && (
          <p className="text-xs text-neutral-600">
            Nothing saved yet. Fill in the form above to keep one.
          </p>
        )}
        {prompts.map((prompt) => (
          <div
            key={prompt.id}
            className="rounded-xl border border-neutral-800/70 bg-neutral-900/40 p-3"
          >
            <div className="flex items-center gap-2">
              <span className="min-w-0 flex-1 truncate text-[13px] font-medium text-neutral-200">
                {prompt.name}
              </span>
              <button
                onClick={() => edit(prompt)}
                className="shrink-0 rounded-md px-2 py-0.5 text-[11px] text-sky-300 transition hover:bg-sky-500/15"
                title="Load it into the form above"
              >
                Edit
              </button>
              <button
                onClick={() => void remove(prompt)}
                disabled={busy}
                className="shrink-0 rounded px-1 text-neutral-500 transition hover:text-red-400"
                title="Delete this prompt"
              >
                <Icon name="close" size={13} />
              </button>
            </div>
            {/* The command is the useful thing to know, so it is shown rather
                than left to be discovered in the composer. */}
            <p className="mt-1 font-mono text-[11px] text-neutral-500">/p:{prompt.id}</p>
            <p className="mt-1 line-clamp-2 whitespace-pre-wrap text-[11px] text-neutral-600">
              {prompt.body}
            </p>
          </div>
        ))}
      </section>

      {dir && (
        <p className="text-[11px] text-neutral-600">
          One Markdown file each, in <span className="font-mono">{dir}</span> — edit them by hand
          if you prefer.
        </p>
      )}
      {status && <p className="text-[11px] text-neutral-500">{status}</p>}
    </div>
  );
}
