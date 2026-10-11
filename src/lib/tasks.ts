import type { ToolRun } from "../types";

export type TaskStatus = "pending" | "in_progress" | "completed";

export interface Task {
  text: string;
  status: TaskStatus;
}

/** The tool the model calls to publish its plan. */
export const TASK_TOOL = "update_tasks";

/** Kept in step with MAX_TASKS in src-tauri/src/tools/mod.rs. */
const MAX_TASKS = 20;
const MAX_TEXT = 200;
const STATUSES: TaskStatus[] = ["pending", "in_progress", "completed"];

/**
 * The latest plan the model published, read out of the raw tool arguments.
 *
 * Tolerant by design: those arguments are whatever the model produced, and this
 * only feeds a display widget. A malformed entry is skipped rather than thrown,
 * and the backend has already told the model off for it.
 */
export function tasksFrom(tools: ToolRun[]): Task[] {
  for (let i = tools.length - 1; i >= 0; i--) {
    const run = tools[i];
    if (run.tool !== TASK_TOOL) continue;
    const raw = (run.args as { tasks?: unknown } | null | undefined)?.tasks;
    if (!Array.isArray(raw)) return [];
    const tasks: Task[] = [];
    for (const item of raw.slice(0, MAX_TASKS)) {
      if (!item || typeof item !== "object") continue;
      const text = (item as { text?: unknown }).text;
      if (typeof text !== "string" || !text.trim()) continue;
      const status = (item as { status?: unknown }).status;
      tasks.push({
        text: text.trim().slice(0, MAX_TEXT),
        status: STATUSES.includes(status as TaskStatus) ? (status as TaskStatus) : "pending",
      });
    }
    return tasks;
  }
  return [];
}
