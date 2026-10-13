import { useEffect, useState } from "react";
import { api } from "../api";
import type { ToolRun } from "../types";
import { savedPath } from "../lib/docs";
import { tasksFrom } from "../lib/tasks";
import { chartSvg } from "../lib/chart";
import { ChartImage } from "./ChartImage";

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
  artifact_create: "📊",
  doc_create: "📄",
  sheet_create: "🧮",
  slides_create: "📽",
  fetch_url: "🌐",
  browser_read: "🌐",
  browser_open: "🌐",
  browser_extract: "🌐",
  browser_eval: "🌐",
  browser_click: "🖱",
  browser_type: "⌨",
  browser_screenshot: "📷",
  browser_console: "🐞",
  browser_network: "🔌",
  browser_tabs: "🗂",
  browser_tab: "🗂",
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
    case "browser_tab": {
      // The index is a number, which `pick` deliberately ignores.
      const index = typeof args.index === "number" ? ` ${args.index}` : "";
      return `${pick("action") || "switch"}${index}`;
    }
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
    case "artifact_create":
      // The panel is already showing it, so the card only names it.
      return pick("title");
    case "doc_create":
      return `${pick("name")} (${pick("format") || "md"})`;
    case "sheet_create":
    case "slides_create":
      return pick("name");
    default:
      return "";
  }
}

/**
 * The chart the model just published, drawn from the tool's own arguments.
 *
 * The arguments carry the spec, so the picture belongs in the conversation as
 * well as in the panel — with no round trip and no extra state to keep in step.
 * The markup is ours and every value in it went through `esc`; `chart.check.ts`
 * asserts that a hostile label cannot become markup.
 */
function inlineChart(run: ToolRun) {
  if (run.tool !== "artifact_create") return null;
  const args = (run.args ?? {}) as Record<string, unknown>;
  if (args.kind !== "chart" || typeof args.content !== "string") return null;
  return chartSvg(args.content);
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

  // A document has been written to disk; the path is the only useful thing to do
  // with it, so the card offers it rather than making the user hunt for it.
  const document = run.status === "ok" ? savedPath(run.tool, run.output) : null;
  // A chart is the one result that is a picture rather than a path or a log, so
  // it is drawn here — not behind the chevron, which is for detail.
  const chart = inlineChart(run);

  return (
    <div className={`mb-2 rounded-lg border px-3 py-2 text-xs ${tone}`}>
      <button
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center gap-2 text-left"
      >
        <span>{ICONS[run.tool] ?? "🔧"}</span>
        <span className="font-medium">{run.tool}</span>
        <span className="truncate text-neutral-500">{describe(run)}</span>
        {/* The status in lower case, not the tracked-caps label it used to be:
            one quiet word after the description reads as state, where uppercase
            tracking reads as a badge. */}
        <span className="ml-auto shrink-0 text-[10px] opacity-70">
          {run.status === "running" ? "running…" : run.status === "ok" ? "done" : "failed"}
        </span>
      </button>
      {chart &&
        ("error" in chart ? (
          // The panel shows the reason in full; a line here is enough to say so.
          <p className="mt-1.5 border-l-2 border-amber-800/60 pl-2 text-[10px] leading-relaxed text-amber-200/70">
            That chart could not be drawn — the panel has the reason.
          </p>
        ) : (
          // White behind the drawing is `ChartImage`'s business, not this card's.
          <ChartImage svg={chart.svg} className="mt-2" />
        ))}
      {run.note && (
        // Not a function of `open`: the caveat is the point of the card, and a
        // warning behind a click is a warning nobody reads.
        <p className="mt-1.5 border-l-2 border-amber-800/60 pl-2 text-[10px] leading-relaxed text-amber-200/70">
          {run.note}
        </p>
      )}
      {open && run.shot && (
        <img
          src={run.shot}
          alt="Screenshot of the page the agent was working on"
          className="mt-2 w-full rounded border border-white/10 bg-white"
        />
      )}
      {document && (
        <div className="mt-2 flex items-center gap-2 border-t border-white/5 pt-2">
          <button
            onClick={() => api.openDocument(document).catch(() => {})}
            className="rounded px-2 py-0.5 text-[11px] text-neutral-300 transition duration-150 hover:bg-neutral-800 hover:text-neutral-100"
          >
            Open
          </button>
          <span className="truncate text-[10px] text-neutral-600" title={document}>
            {document}
          </span>
        </div>
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
