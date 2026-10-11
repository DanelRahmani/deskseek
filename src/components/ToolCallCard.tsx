import { useEffect, useState } from "react";
import type { ToolRun } from "../types";
import { tasksFrom } from "../lib/tasks";

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
  update_tasks: "☑",
  fetch_url: "🌐",
  browser_read: "🌐",
  browser_open: "🌐",
  browser_extract: "🌐",
  browser_eval: "🌐",
  browser_click: "🖱",
  browser_type: "⌨",
  browser_screenshot: "📷",
  ui_snapshot: "🖥",
  ui_click: "🖱",
  ui_type: "⌨",
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
    case "fetch_url":
    case "browser_read":
    case "browser_open":
      return pick("url");
    case "web_search":
      return pick("query");
    case "browser_eval":
      return pick("expression");
    case "browser_click":
    case "browser_type":
      return pick("selector");
    case "ui_snapshot":
      return pick("window") || "the focused window";
    case "ui_click":
      return pick("name") || pick("id");
    case "ui_type":
      return pick("text");
    case "git_commit":
      return pick("message");
    case "update_tasks": {
      // The plan itself is pinned above the composer; here a count is enough.
      const tasks = tasksFrom([run]);
      return tasks.length
        ? `${tasks.filter((t) => t.status === "completed").length}/${tasks.length} done`
        : "";
    }
    default:
      return "";
  }
}

export function ToolCallCard({ run }: { run: ToolRun }) {
  const [open, setOpen] = useState(false);

  // A failed call opens itself: the error is the reason the card is there. A
  // screenshot does too — a picture nobody can see is not worth taking.
  useEffect(() => {
    if (run.status === "error" || run.shot) setOpen(true);
  }, [run.status, run.shot]);

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
      {open && run.shot && (
        <img
          src={run.shot}
          alt="Screenshot of the page the agent was working on"
          className="mt-2 w-full rounded border border-white/10 bg-white"
        />
      )}
      {open && run.output && (
        <pre
          className={`mt-2 max-h-72 overflow-auto whitespace-pre-wrap border-t border-white/5 pt-2 text-[11px] leading-relaxed ${
            run.status === "error" ? "text-red-300/90" : "text-neutral-400"
          }`}
        >
          {run.output}
        </pre>
      )}
    </div>
  );
}
