/**
 * Checks for the task-list reader in tasks.ts. Plain Node, no test framework:
 *
 *   npm run check
 *
 * The arguments it reads are whatever the model produced, so the interesting
 * cases are all the malformed ones.
 */
import { tasksFrom } from "./tasks.ts";
import type { ToolRun } from "../types.ts";

function assert(ok: boolean, message: string): void {
  if (!ok) throw new Error(`FAIL: ${message}`);
}

const run = (tool: string, args: unknown, status: ToolRun["status"] = "ok"): ToolRun => ({
  id: tool,
  tool,
  args,
  status,
});

assert(tasksFrom([]).length === 0, "no tool calls means no plan");
assert(tasksFrom([run("read_file", { path: "a" })]).length === 0, "an unrelated tool means no plan");

const plan = tasksFrom([
  run("update_tasks", {
    tasks: [
      { text: "Read the tests", status: "completed" },
      { text: "  Fix the parser  ", status: "in_progress" },
      { text: "Ship it" },
    ],
  }),
]);
assert(plan.length === 3, "every well-formed entry survives");
assert(plan[1].text === "Fix the parser", "the text is trimmed");
assert(plan[1].status === "in_progress", "a known status is kept");
assert(plan[2].status === "pending", "a missing status defaults to pending");

// The newest call wins: a plan replaces the last one rather than appending.
const replaced = tasksFrom([
  run("update_tasks", { tasks: [{ text: "old", status: "pending" }] }),
  run("read_file", { path: "a" }),
  run("update_tasks", { tasks: [{ text: "new", status: "in_progress" }] }),
]);
assert(replaced.length === 1 && replaced[0].text === "new", "the latest call replaces the plan");

// A rejected call still renders what it can, which is what the model meant.
const rejected = tasksFrom([
  run("update_tasks", { tasks: [{ text: "keep going", status: "done" }] }, "error"),
]);
assert(rejected.length === 1, "a failed call is still shown");
assert(rejected[0].status === "pending", "an unknown status falls back to pending");

// Malformed shapes must not throw and must not invent entries.
const junk = [
  null,
  "tasks",
  {},
  { tasks: "fix the bug" },
  { tasks: [null, 3, {}, { text: "   " }, { text: 7 }] },
];
for (const args of junk) {
  const tasks = tasksFrom([run("update_tasks", args)]);
  assert(tasks.length === 0, `junk input renders as nothing: ${JSON.stringify(args)}`);
}

// A plan cannot grow past the backend's cap, however many the model sends.
const flood = tasksFrom([
  run("update_tasks", { tasks: Array.from({ length: 50 }, (_, i) => ({ text: `step ${i}` })) }),
]);
assert(flood.length === 20, `a flood is capped, got ${flood.length}`);

// One runaway line cannot swamp the panel.
const long = tasksFrom([run("update_tasks", { tasks: [{ text: "x".repeat(900) }] })]);
assert(long[0].text.length === 200, "a long line is truncated");

console.log("tasks.check.ts: all checks passed");
