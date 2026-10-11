import { useState } from "react";
import type { ToolRun } from "../types";

const ICONS: Record<string, string> = {
  read_file: "📄",
  list_dir: "📁",
  glob: "🔍",
  grep: "🔍",
  write_file: "✏️",
  edit_file: "✏️",
  run_command: "▶",
  git_status: "🌿",
  git_diff: "🌿",
  git_log: "🌿",
  git_commit: "🌿",
  git_push: "🌿",
};

function describe(run: ToolRun): string {
  const args = (run.args ?? {}) as Record<string, unknown>;
  const pick = (key: string) => (typeof args[key] === "string" ? String(args[key]) : "");
  switch (run.tool) {
    case "run_command":
      return pick("command");
    case "read_file":
    case "write_file":
      return pick("path");
    case "edit_file":
      return pick("path");
    case "list_dir":
      return pick("path") || ".";
    case "glob":
    case "grep":
      return pick("pattern");
    case "git_commit":
      return pick("message");
    default:
      return "";
  }
}

export function ToolCallCard({ run }: { run: ToolRun }) {
  const [open, setOpen] = useState(false);
  const tone =
    run.status === "running"
      ? "border-sky-900/60 bg-sky-950/30 text-sky-300"
      : run.status === "ok"
        ? "border-neutral-800 bg-neutral-900/60 text-neutral-400"
        : "border-red-900/60 bg-red-950/30 text-red-300";

  return (
    <div className={`mb-2 rounded-lg border px-3 py-2 text-xs ${tone}`}>
      <button
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center gap-2 text-left"
      >
        <span>{ICONS[run.tool] ?? "🔧"}</span>
        <span className="font-medium">{run.tool}</span>
        <span className="truncate text-neutral-500">{describe(run)}</span>
        <span className="ml-auto shrink-0 text-[10px] uppercase tracking-wide opacity-70">
          {run.status === "running" ? "running…" : run.status}
        </span>
      </button>
      {open && run.output && (
        <pre className="mt-2 max-h-72 overflow-auto whitespace-pre-wrap border-t border-white/5 pt-2 text-[11px] leading-relaxed text-neutral-400">
          {run.output}
        </pre>
      )}
    </div>
  );
}
