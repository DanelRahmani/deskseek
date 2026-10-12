import { useEffect, useState } from "react";
import { api } from "../api";
import { Icon } from "./Icon";
import type { DesignView } from "../types";

const inputCls =
  "w-full rounded-lg border border-neutral-800 bg-neutral-900 px-3 py-2 text-sm text-neutral-200 outline-hidden focus:border-neutral-700";

const TEMPLATE = `## Typography

- Headings: 
- Body: 

## Palette

| Token | Value |
| --- | --- |
| \`--accent\` |  |

## Rules

- Spacing follows one scale.
`;

/**
 * A design system is a stored style reference the assistant applies when it
 * builds or writes UI.
 *
 * Its own store, its own switch. It used to be a skill with a `design-` id,
 * which meant this list and the Skills list were one list with a filter on it —
 * and the assistant's copy of that list had no filter at all. Two stores, two
 * commands, two lists that cannot disagree.
 */
export function DesignSystemsSection({ chatId }: { chatId: string | null }) {
  const [systems, setSystems] = useState<DesignView[]>([]);
  const [name, setName] = useState("");
  const [body, setBody] = useState("");
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const reload = async () => {
    setSystems(await api.listDesignSystems(chatId ?? undefined));
  };

  useEffect(() => {
    reload().catch((e) => setStatus(String(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [chatId]);

  const save = async () => {
    setBusy(true);
    try {
      const id = await api.saveDesignSystem(name, body);
      setName("");
      setBody("");
      await reload();
      setStatus(`Saved \`${id}\`.`);
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  };

  /** The starting point that saves the blank page: read a folder that already
   *  has a look, and draft the reference from what is really in it. */
  const fromProject = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const folder = await open({
        directory: true,
        multiple: false,
        title: "Draft a design system from a folder",
      });
      if (typeof folder !== "string") return;
      setBusy(true);
      const id = await api.designSystemFromProject(folder);
      await reload();
      setStatus(`Drafted \`${id}\` — it is in the list above, ready to edit.`);
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  };

  const remove = async (system: DesignView) => {
    setBusy(true);
    try {
      await api.deleteDesignSystem(system.id);
      await reload();
      setStatus(`Removed "${system.name}".`);
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  };

  /**
   * The design systems' own switch. It sends only its own list, because
   * `set_chat_designs` carries the skills through on the Rust side — the two
   * stores no longer have to be stitched together here.
   */
  const toggle = async (id: string) => {
    if (!chatId) {
      setStatus("Open a chat to switch a design system on for it.");
      return;
    }
    setBusy(true);
    try {
      const next = systems
        .filter((s) => (s.id === id ? !s.enabled : s.enabled))
        .map((s) => s.id);
      await api.setChatDesigns(chatId, next);
      await reload();
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="space-y-3">
      <p className="text-xs leading-relaxed text-neutral-500">
        A design system is a reference the assistant applies when it builds or writes UI — type
        scale, palette, spacing, and the rules that hold them together. It joins this chat's prompt
        while it is switched on, so a page or a document comes out looking like the rest.
      </p>

      <div className="flex flex-wrap items-center gap-2">
        <button onClick={() => void fromProject()} disabled={busy} className={quiet}>
          Draft from a folder…
        </button>
      </div>

      {systems.length === 0 && status === null && (
        <p className="text-xs text-neutral-600">
          None yet. Draft one from a folder, or write one in the box below.
        </p>
      )}
      {systems.map((system) => (
        <div
          key={system.id}
          className={`flex items-start gap-1 rounded-xl border p-3 text-[13px] transition duration-150 ${
            system.enabled
              ? "border-sky-800 bg-sky-950/20"
              : "border-neutral-800/70 bg-neutral-900/40 hover:border-neutral-700"
          }`}
        >
          <label
            className={`flex flex-1 items-start gap-3 ${chatId ? "cursor-pointer" : "cursor-not-allowed"}`}
          >
            <input
              type="checkbox"
              checked={system.enabled}
              disabled={!chatId || busy}
              onChange={() => void toggle(system.id)}
              className="mt-0.5"
            />
            <span>
              <span className="block font-medium text-neutral-200">{system.name}</span>
              <span className="mt-1 block font-mono text-[10px] text-neutral-600">{system.id}</span>
            </span>
          </label>
          <button
            onClick={() => void remove(system)}
            disabled={busy}
            title="Delete this design system"
            className="shrink-0 text-neutral-500 hover:text-red-400 disabled:opacity-50"
          >
            <Icon name="close" />
          </button>
        </div>
      ))}

      <div className="space-y-2 rounded-xl border border-neutral-800/70 bg-neutral-900/40 p-3">
        <label className="block text-xs text-neutral-500">New design system</label>
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Name, e.g. Axiom house style"
          className={inputCls}
        />
        <textarea
          value={body}
          onChange={(e) => setBody(e.target.value)}
          rows={10}
          spellCheck={false}
          placeholder={TEMPLATE}
          className={`${inputCls} resize-y font-mono text-[11px] leading-relaxed`}
        />
        <div className="flex items-center gap-2">
          <span className="text-[11px] text-neutral-600">
            Markdown. The placeholder shows the shape; save with the same name to revise.
          </span>
          <button
            onClick={() => void save()}
            disabled={busy || !name.trim() || !body.trim()}
            className="ml-auto rounded-lg bg-sky-600 px-3 py-1.5 text-xs font-medium text-accent-fg transition duration-150 hover:bg-sky-500 disabled:bg-neutral-800 disabled:text-neutral-600"
          >
            Save
          </button>
        </div>
      </div>

      {status && <p className="text-[11px] text-neutral-500">{status}</p>}
    </div>
  );
}

const quiet =
  "rounded-lg px-3 py-1.5 text-[13px] text-neutral-300 transition duration-150 hover:bg-neutral-800 hover:text-neutral-100 disabled:opacity-40";
