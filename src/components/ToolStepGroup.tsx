import { memo, useEffect, useState } from "react";
import type { ToolRun } from "../types";
import { ToolCallCard } from "./ToolCallCard";
import { Icon } from "./Icon";

/**
 * One collapsible line for a run of tool calls, so a turn that reads twelve
 * files is a single row rather than twelve cards. Opening it shows the
 * individual `ToolCallCard`s exactly as they were.
 *
 * A lone step needs no group and renders as the card itself. A failure — or a
 * screenshot, which is for the user's eyes rather than the model's — opens the
 * group, because those are the cases worth seeing without a click.
 */
/**
 * Memoised on the array's identity, which the transcript rebuild gives a group
 * only when the stored messages or the live runs actually changed — not when a
 * delta merely arrived. A group already on screen therefore costs nothing while
 * the answer below it streams.
 */
export const ToolStepGroup = memo(function ToolStepGroup({ steps }: { steps: ToolRun[] }) {
  const failed = steps.filter((s) => s.status === "error").length;
  const running = steps.filter((s) => s.status === "running");
  const last = running[running.length - 1];
  const [open, setOpen] = useState(failed > 0 || steps.some((s) => s.shot));

  // A step can fail after the group is already drawn, so the auto-open has to
  // react rather than only initialise.
  useEffect(() => {
    if (failed > 0 || steps.some((s) => s.shot)) setOpen(true);
  }, [failed, steps]);

  if (steps.length === 1) return <ToolCallCard run={steps[0]} />;

  const names = steps.map((s) => s.tool);
  const summary =
    names.slice(0, 3).join(", ") + (names.length > 3 ? `, +${names.length - 3} more` : "");

  return (
    <div className="mb-2 rounded-lg border border-neutral-800/80 bg-neutral-900/40 text-xs">
      <button
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        className="flex w-full items-center gap-2 px-3 py-2 text-left transition hover:text-neutral-200"
      >
        <Icon
          name="chevron"
          size={12}
          className={`shrink-0 transition-transform duration-150 ${open ? "rotate-90" : ""}`}
        />
        <span className="shrink-0 font-medium text-neutral-300">
          {steps.length} step{steps.length === 1 ? "" : "s"}
        </span>
        {last ? (
          <span className="flex min-w-0 items-center gap-1.5 text-sky-400">
            <span className="h-1.5 w-1.5 shrink-0 animate-pulse rounded-full bg-sky-400" />
            <span className="truncate">running {last.tool}</span>
          </span>
        ) : failed > 0 ? (
          <span className="truncate text-red-400">{failed} failed</span>
        ) : (
          <span className="truncate text-neutral-600">{summary}</span>
        )}
      </button>
      {open && (
        <div className="reveal border-t border-neutral-800/60 px-2 pt-2 pb-1">
          {steps.map((step) => (
            <ToolCallCard key={step.id} run={step} />
          ))}
        </div>
      )}
    </div>
  );
});
