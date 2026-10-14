import { Icon } from "./Icon";
import type { Generator } from "../types";

interface Props {
  /** The entries, read from Rust — this file knows no entry by name. */
  entries: Generator[];
  /** Chosen: the caller closes this and opens that entry's own form. */
  onPick: (entry: Generator) => void;
  onClose: () => void;
}

/**
 * What a notebook can make, as a panel of tiles instead of eight lines in the rail.
 *
 * The rail is 224 px wide and had eight entries under a heading, each one a line of
 * text with the same weight as the two lists around it — which left nowhere to say
 * what a briefing *is*, and made the thing a reader came to do look like a menu of
 * settings. This is the same list with room: a bento of tiles, each carrying the
 * entry's own line and the format it writes.
 *
 * It is a *chooser*, not a second form. Picking a tile closes this and opens the
 * form that entry has always opened, which is drawn from what the entry declares —
 * so a new entry in Rust still needs no code in the window, in either place.
 */
export function MakePanel({ entries, onPick, onClose }: Props) {
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
        aria-label="Make something from these sources"
        className="flex max-h-[calc(100vh-8rem)] w-[40rem] max-w-[calc(100vw-2rem)] flex-col overflow-hidden rounded-2xl border border-neutral-800 bg-neutral-900 shadow-2xl"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2 border-b border-neutral-800 px-4 py-3">
          <Icon name="sparkle" size={15} className="shrink-0 text-sky-400" />
          <span className="min-w-0 flex-1 text-sm font-medium text-neutral-200">
            Make something
          </span>
          <button
            autoFocus
            onClick={onClose}
            aria-label="Close"
            className="rounded-lg p-1 text-neutral-500 transition duration-150 hover:bg-neutral-800 hover:text-neutral-200"
          >
            <Icon name="close" />
          </button>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto p-4">
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            {entries.map((entry) => (
              <button
                key={entry.id}
                onClick={() => onPick(entry)}
                className={`flex flex-col items-start gap-1.5 rounded-2xl border p-3 text-left transition duration-150 ${
                  WIDE[entry.id] ?? ""
                } ${TINTS[entry.action] ?? PLAIN}`}
              >
                <span className="flex w-full items-baseline gap-2">
                  <span className="min-w-0 flex-1 text-[13px] font-medium text-neutral-100">
                    {entry.title}
                  </span>
                  {entry.formats.length > 0 && (
                    // The format it really writes, beside the name — an entry that
                    // writes no file has none to name, which is why this is read off
                    // the entry rather than assumed.
                    <span className="shrink-0 text-[11px] text-neutral-500">
                      .{entry.formats[0]}
                    </span>
                  )}
                </span>
                <span className="text-[11px] leading-relaxed text-neutral-400">{entry.note}</span>
              </button>
            ))}
          </div>
          <p className="pt-3 text-[11px] leading-relaxed text-neutral-500">
            Every entry opens a form first: how many, from which sources, and anything you want it
            to aim at.
          </p>
        </div>
      </div>
    </div>
  );
}

/**
 * The tiles that get the whole width, so the grid is a bento rather than a table.
 *
 * A lookup **with a fallback** rather than a case per entry, and deliberately: an
 * entry added in Rust that nobody has thought about yet gets an ordinary tile — which
 * is a correct tile — instead of a hole or a missing row. A table TypeScript had to
 * keep in step with `GENERATORS` would be the drift `tray.check.ts` exists to catch;
 * here the fallback is the right answer, so there is nothing to catch.
 */
const WIDE: Record<string, string> = {
  exam: "sm:col-span-2",
  "fact-table": "sm:col-span-2",
};

/** What each kind of thing a notebook makes is tinted with. */
const TINTS: Record<string, string> = {
  set: "border-sky-900/60 bg-sky-500/5 hover:border-sky-700 hover:bg-sky-500/10",
  document: "border-amber-900/60 bg-amber-500/5 hover:border-amber-700 hover:bg-amber-500/10",
  table: "border-emerald-900/60 bg-emerald-500/5 hover:border-emerald-700 hover:bg-emerald-500/10",
  exam: "border-rose-900/60 bg-rose-500/5 hover:border-rose-700 hover:bg-rose-500/10",
};

const PLAIN = "border-neutral-800 bg-neutral-900/60 hover:border-neutral-600";
