import { useEffect, useMemo, useState } from "react";
import type { KeyboardEvent as ReactKeyboardEvent } from "react";
import { matchPalette } from "../lib/commands";
import type { PaletteItem } from "../lib/commands";

interface Props {
  items: PaletteItem[];
  onRun: (id: string) => void;
  onClose: () => void;
}

/**
 * A keyboard-first way to reach anything the sidebar and the shortcuts already
 * reach. It owns its own cursor so typing never moves focus out of the field.
 */
export function CommandPalette({ items, onRun, onClose }: Props) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const matches = useMemo(() => matchPalette(items, query), [items, query]);

  // A shorter list can leave the cursor past the end of it.
  useEffect(() => {
    setActive((current) => Math.min(current, Math.max(0, matches.length - 1)));
  }, [matches.length]);

  const onKeyDown = (e: ReactKeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((i) => (matches.length ? (i + 1) % matches.length : 0));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((i) => (matches.length ? (i - 1 + matches.length) % matches.length : 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const chosen = matches[active];
      if (chosen) onRun(chosen.id);
    } else if (e.key === "Escape") {
      // The field owns Escape while it has focus, so the window handler never
      // sees this one — the palette closes itself instead.
      e.preventDefault();
      e.stopPropagation();
      onClose();
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-neutral-950/60 pt-24"
      onMouseDown={onClose}
    >
      <div
        className="w-[32rem] max-w-[calc(100vw-2rem)] overflow-hidden rounded-xl border border-neutral-800 bg-neutral-900 shadow-2xl"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <input
          autoFocus
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={onKeyDown}
          placeholder="Type a command or a chat…"
          className="w-full border-b border-neutral-800 bg-neutral-900 px-4 py-3 text-sm text-neutral-200 outline-none"
        />
        <div className="max-h-80 overflow-y-auto p-1">
          {matches.length === 0 && (
            <p className="px-3 py-4 text-xs text-neutral-600">Nothing matches.</p>
          )}
          {matches.map((item, index) => (
            <button
              key={item.id}
              onMouseEnter={() => setActive(index)}
              onClick={() => onRun(item.id)}
              className={`flex w-full items-center justify-between gap-3 rounded-md px-3 py-2 text-left text-sm ${
                index === active ? "bg-neutral-800 text-neutral-100" : "text-neutral-400"
              }`}
            >
              <span className="truncate">{item.label}</span>
              <span className="shrink-0 text-[11px] text-neutral-600">{item.hint}</span>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
