import { useEffect, useRef } from "react";
import { SHORTCUTS } from "../lib/shortcuts";
import { Icon } from "./Icon";

/**
 * Every chord in one place, on top of everything.
 *
 * A keyboard feature nobody can find is not a feature, and `?` is the one chord
 * the menus cannot advertise — so it has to advertise itself. Same conventions as
 * the palette, because it is the same kind of surface: a modal dialog, the label
 * announced, and focus put back where it was when it closes.
 */
export function ShortcutSheet({ onClose }: { onClose: () => void }) {
  // Captured during the first render: by the time an effect runs, autoFocus has
  // already moved focus to the close button.
  const restore = useRef<Element | null>(null);
  if (restore.current === null) restore.current = document.activeElement;
  useEffect(() => {
    const back = restore.current;
    return () => {
      if (back instanceof HTMLElement && document.contains(back)) back.focus();
    };
  }, []);

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-neutral-950/60 pt-24"
      onMouseDown={onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Keyboard shortcuts"
        className="w-[24rem] max-w-[calc(100vw-2rem)] overflow-hidden rounded-xl border border-neutral-800 bg-neutral-900 shadow-2xl"
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          // One focusable control, so Tab has nowhere to go but out of the
          // dialog. Trap it rather than let it wander into the page behind.
          if (e.key === "Tab") e.preventDefault();
        }}
      >
        <div className="flex items-center gap-2 border-b border-neutral-800 px-4 py-3">
          <span className="text-sm font-medium text-neutral-200">Keyboard</span>
          <button
            autoFocus
            onClick={onClose}
            className="ml-auto text-sm text-neutral-500 transition hover:text-neutral-200"
            title="Close"
          >
            <Icon name="close" size={15} />
          </button>
        </div>
        <ul className="max-h-80 overflow-y-auto p-3">
          {SHORTCUTS.map((shortcut) => (
            <li key={shortcut.action} className="flex gap-3 px-1 py-1 text-xs">
              <span className="w-20 shrink-0 font-mono text-neutral-300">{shortcut.keys}</span>
              <span className="text-neutral-500">{shortcut.label}</span>
            </li>
          ))}
        </ul>
        <p className="border-t border-neutral-800 px-4 py-2 text-[11px] text-neutral-600">
          Cmd works wherever Ctrl does. This list is generated from the same map the keys use.
        </p>
      </div>
    </div>
  );
}
