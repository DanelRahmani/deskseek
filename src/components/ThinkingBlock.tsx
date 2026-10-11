import { useState } from "react";
import { Icon } from "./Icon";

export function ThinkingBlock({ text, streaming }: { text: string; streaming: boolean }) {
  const [open, setOpen] = useState(false);
  if (!text.trim()) return null;
  return (
    <div className="mb-2 rounded-xl border border-neutral-800/80 bg-neutral-900/40">
      <button
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-neutral-400 transition duration-150 hover:text-neutral-200"
      >
        <Icon
          name="chevron"
          size={12}
          className={`shrink-0 transition-transform duration-150 ${open ? "rotate-90" : ""}`}
        />
        <span>Thinking{streaming ? "…" : ""}</span>
        {streaming && (
          <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-sky-400" title="Working" />
        )}
      </button>
      {open && (
        <pre className="max-h-72 overflow-auto whitespace-pre-wrap border-t border-neutral-800 px-3 py-2 text-xs leading-relaxed text-neutral-400">
          {text}
        </pre>
      )}
    </div>
  );
}
