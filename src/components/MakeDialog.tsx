import { useState } from "react";
import { Icon } from "./Icon";
import type { Generator, MakeOptions, SourceView } from "../types";

interface Props {
  /** The entry being asked for — which is also the form: the fields come from
   *  what the entry declares, so a new entry draws its own form and no code here
   *  learns about it. */
  entry: Generator;
  /** The sources that could be read: the ones switched on. */
  sources: SourceView[];
  /** A run is already going; nothing may be started on top of it. */
  busy: boolean;
  onRun: (options: MakeOptions) => void;
  onClose: () => void;
}

const field = "rounded-lg bg-neutral-950 px-2 py-1.5 text-[13px] text-neutral-100 outline-hidden";
const legend = "text-[11px] tracking-wide text-neutral-500 uppercase";

/**
 * What the reader wants before the model is asked for it: how many, from which
 * documents, and a line to steer with — for every entry, including the ones that
 * take no number.
 *
 * The form is drawn from the entry rather than written for it: an entry that
 * declares a count gets a number field labelled with its own noun, an entry with
 * more than one format gets a choice, and an entry with neither gets the two
 * things every entry takes. That is what makes the *next* entry — a set, an exam —
 * a line in Rust rather than a form in here.
 *
 * Nothing is validated here that Rust does not validate again: this refuses to
 * send an empty source list because "no sources" and "every source" are different
 * asks that would otherwise look the same on the wire, and everything else is
 * checked where the model is paid for.
 */
export function MakeDialog({ entry, sources, busy, onRun, onClose }: Props) {
  const countField = entry.count;
  const openField = entry.open;
  const [count, setCount] = useState<number>(countField ? countField.default : 0);
  const [open, setOpen] = useState<number>(openField ? openField.default : 0);
  const [format, setFormat] = useState<string>(entry.formats[0] ?? "");
  // Everything switched on is ticked to begin with: the ordinary request is "all of
  // them", and a form that started empty would make the reader work for it.
  const [ticked, setTicked] = useState<string[]>(sources.map((source) => source.name));
  const [steer, setSteer] = useState("");

  const toggle = (name: string) =>
    setTicked((now) => (now.includes(name) ? now.filter((one) => one !== name) : [...now, name]));

  const nothingToRead = sources.length > 0 && ticked.length === 0;

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
        aria-label={`${entry.title} options`}
        className="flex max-h-[calc(100vh-8rem)] w-[30rem] max-w-[calc(100vw-2rem)] flex-col overflow-hidden rounded-xl border border-neutral-800 bg-neutral-900 shadow-2xl"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2 border-b border-neutral-800 px-4 py-3">
          <span className="min-w-0 flex-1 truncate text-sm font-medium text-neutral-200">
            {entry.title}
          </span>
          {entry.formats.length === 1 && (
            <span className="shrink-0 text-[11px] text-neutral-500">.{entry.formats[0]}</span>
          )}
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
          <div className="flex flex-wrap items-end gap-4">
            {countField && (
              <label className="flex flex-col gap-1">
                <span className={legend}>{countField.noun}</span>
                <input
                  type="number"
                  min={countField.min}
                  max={countField.max}
                  value={count}
                  onChange={(e) => setCount(Number(e.target.value))}
                  className={`${field} w-24`}
                />
              </label>
            )}
            {openField && (
              <label className="flex flex-col gap-1">
                <span className={legend}>{openField.noun}</span>
                <input
                  type="number"
                  min={openField.min}
                  max={openField.max}
                  value={open}
                  onChange={(e) => setOpen(Number(e.target.value))}
                  className={`${field} w-24`}
                />
                <span className="text-[11px] text-neutral-500">0 leaves them out</span>
              </label>
            )}
            {entry.formats.length > 1 && (
              <label className="flex flex-col gap-1">
                <span className={legend}>Written as</span>
                <select
                  value={format}
                  onChange={(e) => setFormat(e.target.value)}
                  className={field}
                >
                  {entry.formats.map((one) => (
                    <option key={one} value={one}>
                      .{one}
                    </option>
                  ))}
                </select>
              </label>
            )}
          </div>

          <div className="mt-4">
            <div className={legend}>Sources it may read</div>
            {sources.length === 0 ? (
              <p className="mt-1 text-[12px] text-neutral-500">
                This notebook has nothing to read. Add a document first.
              </p>
            ) : (
              <div className="mt-1 flex flex-col gap-0.5">
                {sources.map((source) => (
                  <label
                    key={source.id}
                    className="flex items-center gap-2 rounded-lg px-1 py-1 text-[12px] text-neutral-300 transition hover:bg-neutral-800/60"
                  >
                    <input
                      type="checkbox"
                      checked={ticked.includes(source.name)}
                      onChange={() => toggle(source.name)}
                      className="shrink-0"
                    />
                    <span className="min-w-0 flex-1 truncate">{source.name}</span>
                  </label>
                ))}
              </div>
            )}
          </div>

          <label className="mt-4 flex flex-col gap-1">
            <span className={legend}>Anything to steer it with</span>
            <textarea
              value={steer}
              onChange={(e) => setSteer(e.target.value)}
              rows={2}
              placeholder="e.g. only the national-accounts material, and nothing about the 1970s"
              className={`${field} resize-none leading-relaxed`}
            />
          </label>
        </div>

        <div className="flex items-center gap-2 border-t border-neutral-800 px-4 py-3">
          <span className="flex-1 text-[12px] text-neutral-500">
            {nothingToRead ? "Tick at least one source, or none will be read." : ""}
          </span>
          <button
            onClick={onClose}
            className="rounded-lg px-2.5 py-1 text-xs text-neutral-400 transition hover:text-neutral-200"
          >
            Cancel
          </button>
          <button
            disabled={busy || nothingToRead || sources.length === 0}
            onClick={() =>
              onRun({
                count: countField ? count : null,
                open: openField ? open : null,
                format: entry.formats.length > 0 ? format : null,
                // Every one of them means the same as naming none: the wire says
                // "empty is everything switched on", and sending all of them would
                // be a longer way of saying it.
                sources: ticked.length === sources.length ? [] : ticked,
                steer,
              })
            }
            className="rounded-lg bg-sky-600 px-2.5 py-1 text-xs text-white transition hover:bg-sky-500 disabled:opacity-40"
          >
            Make it
          </button>
        </div>
      </div>
    </div>
  );
}
