import { useState } from "react";
import type { Task } from "../lib/tasks";
import { Icon } from "./Icon";

const MARKS: Record<Task["status"], string> = {
  pending: "○",
  in_progress: "◐",
  completed: "✓",
};

const TONES: Record<Task["status"], string> = {
  pending: "text-neutral-500",
  in_progress: "text-sky-400",
  completed: "text-emerald-500",
};

/**
 * The model's plan for the current job, pinned above the composer so it stays
 * visible while the run scrolls past. Collapsible, because a long plan should
 * not eat the composer.
 */
export function TaskList({ tasks }: { tasks: Task[] }) {
  const [open, setOpen] = useState(true);
  const done = tasks.filter((task) => task.status === "completed").length;
  const working = tasks.some((task) => task.status === "in_progress");

  return (
    <div className="mb-2 rounded-xl border border-neutral-800 bg-neutral-900/70 text-xs">
      <button
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        className="flex w-full items-center gap-2 px-3 py-2 text-left"
      >
        <span className="font-medium text-neutral-300">Tasks</span>
        <span className="tabular-nums text-neutral-500">
          {done}/{tasks.length}
        </span>
        {working && (
          <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-sky-400" title="Working" />
        )}
        <Icon
          name="chevron"
          size={12}
          className={`ml-auto text-neutral-600 transition-transform duration-150 ${open ? "rotate-90" : ""}`}
        />
      </button>
      {open && (
        <ul className="space-y-1 border-t border-neutral-800 px-3 py-2">
          {tasks.map((task, index) => (
            <li key={`${task.text}-${index}`} className="flex items-start gap-2">
              <span className={`w-3 shrink-0 text-center ${TONES[task.status]}`}>
                {MARKS[task.status]}
              </span>
              <span
                className={
                  task.status === "completed"
                    ? "text-neutral-600 line-through"
                    : task.status === "in_progress"
                      ? "text-neutral-100"
                      : "text-neutral-400"
                }
              >
                {task.text}
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
