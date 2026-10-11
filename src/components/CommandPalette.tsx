import { useEffect, useId, useMemo, useRef, useState } from "react";
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
 *
 * It is announced as a modal dialog holding a combobox, because a sighted user
 * sees a list appear under what they type and a screen reader has to be told the
 * same thing: which list is attached to which field (`aria-controls`), which
 * entry the cursor is on (`aria-activedescendant`) and that the rest of the page
 * is inert (`aria-modal`). The highlight never moves DOM focus, which is what
 * makes that combination necessary rather than decorative.
 */
export function CommandPalette({ items, onRun, onClose }: Props) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const matches = useMemo(() => matchPalette(items, query), [items, query]);
  const listId = useId();
  const optionId = (index: number) => `${listId}-option-${index}`;

  // Where focus was before the palette opened, so closing it puts focus back
  // where it was instead of dropping it on the document body. Captured during
  // the first render because `autoFocus` has already moved focus by the time an
  // effect could run.
  const restore = useRef<Element | null>(null);
  if (restore.current === null) restore.current = document.activeElement;
  useEffect(() => {
    const back = restore.current;
    return () => {
      if (back instanceof HTMLElement && document.contains(back)) back.focus();
    };
  }, []);

  // A shorter list can leave the cursor past the end of it.
  useEffect(() => {
    setActive((current) => Math.min(current, Math.max(0, matches.length - 1)));
  }, [matches.length]);

  const move = (step: number) =>
    setActive((i) => (matches.length ? (i + step + matches.length) % matches.length : 0));

  const onKeyDown = (e: ReactKeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      move(1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      move(-1);
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
        role="dialog"
        aria-modal="true"
        aria-label="Command palette"
        className="w-[32rem] max-w-[calc(100vw-2rem)] overflow-hidden rounded-xl border border-neutral-800 bg-neutral-900 shadow-2xl"
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          // The options are not tab stops, so without this Tab walks out of the
          // dialog into the page behind it — a focus leak in a modal. Tab keeps
          // moving down the list instead.
          if (e.key !== "Tab") return;
          e.preventDefault();
          move(e.shiftKey ? -1 : 1);
        }}
      >
        <input
          autoFocus
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={onKeyDown}
          role="combobox"
          aria-expanded="true"
          aria-controls={listId}
          aria-autocomplete="list"
          aria-activedescendant={matches[active] ? optionId(active) : undefined}
          placeholder="Type a command or a chat…"
          className="w-full border-b border-neutral-800 bg-neutral-900 px-4 py-3 text-sm text-neutral-200 outline-none"
        />
        <div
          id={listId}
          role="listbox"
          aria-label="Results"
          className="max-h-80 overflow-y-auto p-1"
        >
          {matches.length === 0 && (
            <p className="px-3 py-4 text-xs text-neutral-600">Nothing matches.</p>
          )}
          {matches.map((item, index) => (
            <button
              key={item.id}
              id={optionId(index)}
              role="option"
              aria-selected={index === active}
              tabIndex={-1}
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
