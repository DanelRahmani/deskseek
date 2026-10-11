import { useState } from "react";

export function ThinkingBlock({ text, streaming }: { text: string; streaming: boolean }) {
  const [open, setOpen] = useState(false);
  if (!text.trim()) return null;
  return (
    <div className="mb-2 rounded-lg border border-neutral-800 bg-neutral-900/60">
      <button
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-neutral-400 hover:text-neutral-200"
      >
        <span className="text-[10px]">{open ? "▾" : "▸"}</span>
        <span>Thinking{streaming ? "…" : ""}</span>
      </button>
      {open && (
        <pre className="max-h-72 overflow-auto whitespace-pre-wrap border-t border-neutral-800 px-3 py-2 text-xs leading-relaxed text-neutral-400">
          {text}
        </pre>
      )}
    </div>
  );
}
