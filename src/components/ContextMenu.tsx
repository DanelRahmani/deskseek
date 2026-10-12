import { useEffect, useRef } from "react";

export interface MenuItem {
  label: string;
  onSelect: () => void;
  /** Drawn in red: the one item in the menu that destroys something. */
  danger?: boolean;
}

/**
 * A right-click menu, placed at the cursor and rendered on its own layer.
 *
 * `fixed` at the app root rather than an absolutely-positioned child of the
 * sidebar: the conversation list scrolls, and any ancestor with `overflow-y-auto`
 * would clip a menu drawn inside it. The viewport clamp keeps a row near the
 * bottom edge from opening off-screen.
 */
export function ContextMenu({
  x,
  y,
  items,
  onClose,
}: {
  x: number;
  y: number;
  items: MenuItem[];
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onDown = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [onClose]);

  const width = 200;
  const height = items.length * 30 + 8;
  const left = Math.max(4, Math.min(x, window.innerWidth - width - 4));
  const top = Math.max(4, Math.min(y, window.innerHeight - height - 4));

  return (
    <div
      ref={ref}
      role="menu"
      style={{ left, top, width }}
      className="fixed z-50 rounded-lg border border-neutral-700 bg-neutral-900 py-1 shadow-xl shadow-black/40"
    >
      {items.map((item) => (
        <button
          key={item.label}
          role="menuitem"
          onClick={() => {
            onClose();
            item.onSelect();
          }}
          className={`block w-full px-3 py-1.5 text-left text-[13px] transition ${
            item.danger
              ? "text-red-300 hover:bg-red-950/40"
              : "text-neutral-300 hover:bg-neutral-800"
          }`}
        >
          {item.label}
        </button>
      ))}
    </div>
  );
}
