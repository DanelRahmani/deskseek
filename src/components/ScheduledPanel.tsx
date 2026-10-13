import { useCallback, useEffect, useState } from "react";
import { api, onApproval, onQuestion } from "../api";
import type { ScheduledDraft } from "../api";
import { ApprovalPrompt } from "./ApprovalPrompt";
import { Icon, IconButton } from "./Icon";
import { QuestionPrompt, answerFrom } from "./QuestionPrompt";
import { money } from "./SpendPanel";
import { when } from "../lib/time";
import type { McpServer, ParkedRequest, ProjectRow, ScheduledTask, TaskRun } from "../types";

interface Props {
  onClose: () => void;
  /** A task ran, so the sidebar's chats may be stale. */
  onChanged: () => void;
  /** A parked run is answered by opening its chat, when that is what it takes. */
  onOpenChat: (id: string) => void;
}

const WEEKDAYS = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
const NTHS = ["first", "second", "third", "fourth"];

/**
 * What can make a task due. `cadence` is the calendar below; the other two wait
 * for something to change, and are the reason a task has a `target` at all.
 */
const KINDS = [
  { id: "cadence", label: "On a schedule" },
  { id: "file", label: "When a file changes" },
  { id: "git", label: "When a git ref moves" },
  { id: "url", label: "When a page changes" },
  { id: "mcp", label: "When a server sends something" },
];

/** What each kind watches, as the form's own label for it. */
const TARGET_LABEL: Record<string, string> = {
  file: "Folder to watch",
  git: "Repository folder",
  url: "Page URL",
  mcp: "Server name",
};

/** The noun the refusal sentence uses, so it says which field is missing. */
const TARGET_NOUN: Record<string, string> = {
  file: "folder",
  git: "repository",
  url: "URL",
  mcp: "server",
};

/** What counts as a change, and the example that makes the field legible. */
const FILTER_LABEL: Record<string, string> = {
  file: "Only files matching, e.g. *.csv — blank watches everything",
  git: "Ref, e.g. main — blank means HEAD",
  url: "Text to look for, e.g. In stock — blank fires on any change",
  mcp: "Method, e.g. notifications/changed — blank means anything it sends",
};

/** The check interval, in the minutes a person thinks in rather than seconds. */
const INTERVALS = [
  { secs: 60, label: "Every minute" },
  { secs: 300, label: "Every 5 minutes" },
  { secs: 900, label: "Every 15 minutes" },
  { secs: 3600, label: "Every hour" },
];

const quiet =
  "rounded-lg px-3 py-1.5 text-[13px] text-neutral-300 transition duration-150 hover:bg-neutral-800 hover:text-neutral-100 disabled:opacity-40";
const field =
  "w-full rounded-lg border border-neutral-800 bg-neutral-950 px-2.5 py-1.5 text-[13px] text-neutral-200 outline-hidden transition duration-150 focus:border-neutral-600";

/**
 * "Every Wednesday at 08:00", "The first Wednesday of the month at 08:00".
 *
 * `weekday` is the Rust side's `num_days_from_monday`, which is why `WEEKDAYS`
 * starts at Monday — the one place the two conventions meet.
 */
function describe(task: ScheduledTask): string {
  if (task.kind === "file") {
    const what = task.filter.trim() || "any file";
    return `When ${what} changes in ${task.target || "a folder that is no longer set"}`;
  }
  if (task.kind === "git") {
    return `When ${task.filter.trim() || "HEAD"} moves in ${
      task.target || "a folder that is no longer set"
    }`;
  }
  if (task.kind === "url") {
    const needle = task.filter.trim();
    const page = task.target || "a page that is no longer set";
    return needle ? `When ${page} comes to say "${needle}"` : `When ${page} changes`;
  }
  if (task.kind === "mcp") {
    const method = task.filter.trim();
    const server = task.target || "a server that is no longer set";
    return method && method !== "*"
      ? `When ${server} sends ${method}`
      : `When ${server} sends anything`;
  }
  if (task.freq === "hourly") return `Every hour at :${task.time.slice(-2)}`;
  if (task.freq === "weekdays") return `Weekdays at ${task.time}`;
  if (task.freq === "weekly") return `Every ${WEEKDAYS[task.weekday]} at ${task.time}`;
  if (task.freq === "monthly") {
    return `The ${NTHS[task.nth - 1]} ${WEEKDAYS[task.weekday]} of the month at ${task.time}`;
  }
  return `Every day at ${task.time}`;
}

/** A trigger looks, rather than runs, so its second line says so. */
function isTrigger(task: ScheduledTask): boolean {
  return task.kind !== "cadence";
}

/**
 * The second line of a task's row. A pushed trigger has no next look at all, and
 * its `next_run_at` is the never-due sentinel — which must never be rendered as a
 * date, or the panel would offer a year that has not happened.
 */
function nextLine(task: ScheduledTask): string {
  if (!task.enabled) return "Paused";
  if (task.kind === "mcp") return `Waiting on ${task.target}`;
  if (isTrigger(task)) return `Next check: ${when(task.next_run_at)}`;
  return `Next: ${when(task.next_run_at)}`;
}

const BLANK: ScheduledDraft = {
  title: "",
  prompt: "",
  chat_id: null,
  project_id: null,
  freq: "daily",
  time: "09:00",
  weekday: 0,
  nth: 1,
  kind: "cadence",
  target: "",
  filter: "",
  interval_secs: 300,
};

/**
 * Scheduled tasks: a prompt, a calendar, and anything a run is waiting on.
 *
 * Each run goes through the ordinary turn path — the same chat, the same tools
 * — so there is nothing here to keep in step with the composer. What this panel
 * owns is the calendar, and the parked runs that need an answer before they can
 * carry on: the prompt shown for one is the chat's own, re-rendered from the
 * event the run emitted, so the two can never ask for different things.
 */
export function ScheduledPanel({ onClose, onChanged, onOpenChat }: Props) {
  const [tasks, setTasks] = useState<ScheduledTask[]>([]);
  const [projects, setProjects] = useState<ProjectRow[]>([]);
  const [servers, setServers] = useState<McpServer[]>([]);
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [form, setForm] = useState<ScheduledDraft>(BLANK);
  /** Runs waiting on an approval or a question, from the database. */
  const [parked, setParked] = useState<ParkedRequest[]>([]);
  /** Which task's run history is open, if any, and what it fetched. */
  const [runsOpen, setRunsOpen] = useState<string | null>(null);
  const [runs, setRuns] = useState<Record<string, TaskRun[]>>({});

  const reload = useCallback(async () => {
    const [list, projectList, waiting, mcpServers] = await Promise.all([
      api.listScheduled(),
      api.listProjects(),
      api.listParked(),
      api.mcpServers(),
    ]);
    setTasks(list);
    setProjects(projectList);
    setParked(waiting);
    setServers(mcpServers);
  }, []);

  useEffect(() => {
    reload().catch((e) => setStatus(String(e)));
  }, [reload]);

  // A run can park while this panel is open, and the two events that announce a
  // park are already fired — so they are what keeps this list current rather
  // than a timer of its own.
  useEffect(() => {
    let stop: (() => void) | undefined;
    const refresh = () => void reload().catch(() => {});
    void Promise.all([onApproval(refresh), onQuestion(refresh)]).then((fns) => {
      stop = () => fns.forEach((fn) => fn());
    });
    return () => stop?.();
  }, [reload]);

  /**
   * Answering a park stops the run waiting on it, and the row goes with it — so
   * the list is re-read rather than patched, which is what makes "answered" and
   * "gone" the same state.
   */
  const decide = async (
    requestId: string,
    decision: "once" | "session" | "always" | "deny",
  ) => {
    try {
      await api.respondApproval(requestId, decision);
      await reload();
      onChanged();
    } catch (e) {
      setStatus(String(e));
    }
  };

  const answer = async (requestId: string, selected: string[][], free: string[]) => {
    try {
      await api.respondQuestion(requestId, { answers: answerFrom(selected, free), cancelled: false });
      await reload();
    } catch (e) {
      setStatus(String(e));
    }
  };

  const skip = async (requestId: string) => {
    try {
      await api.respondQuestion(requestId, { answers: [], cancelled: true });
      await reload();
    } catch (e) {
      setStatus(String(e));
    }
  };

  const dismiss = async (park: ParkedRequest) => {
    try {
      await api.dismissParked(park.id);
      await reload();
    } catch (e) {
      setStatus(String(e));
    }
  };

  const add = async () => {
    if (!form.prompt.trim()) {
      setStatus("A task needs something to do.");
      return;
    }
    if (form.kind !== "cadence" && !form.target.trim()) {
      setStatus(`A trigger needs something to watch: a ${TARGET_NOUN[form.kind]}.`);
      return;
    }
    setBusy(true);
    try {
      await api.createScheduled(form);
      setForm(BLANK);
      await reload();
      setStatus(
        form.kind === "cadence"
          ? "Scheduled. It runs on the calendar above."
          : "Watching. It looks on its interval and starts a run when that changes.",
      );
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  };

  const toggle = async (task: ScheduledTask) => {
    try {
      await api.updateScheduled(task.id, {
        title: task.title,
        prompt: task.prompt,
        chat_id: task.chat_id,
        project_id: task.project_id,
        freq: task.freq,
        time: task.time,
        weekday: task.weekday,
        nth: task.nth,
        kind: task.kind,
        target: task.target,
        filter: task.filter,
        interval_secs: task.interval_secs,
        enabled: !task.enabled,
      });
      await reload();
    } catch (e) {
      setStatus(String(e));
    }
  };

  const remove = async (task: ScheduledTask) => {
    try {
      await api.deleteScheduled(task.id);
      await reload();
      setStatus(`Deleted "${task.title}".`);
    } catch (e) {
      setStatus(String(e));
    }
  };

  const runNow = async (task: ScheduledTask) => {
    setBusy(true);
    setStatus(`Running "${task.title}"…`);
    try {
      await api.runScheduledNow(task.id);
      onChanged();
      // The run just made a row, so an open history is now stale.
      if (runsOpen === task.id) await loadRuns(task.id);
      setStatus(`"${task.title}" ran. Its next scheduled run is unchanged.`);
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  };

  /** Read a task's run history. One query when it is opened, not a poll. */
  const loadRuns = async (taskId: string) => {
    const fresh = await api.taskRuns(taskId);
    setRuns((r) => ({ ...r, [taskId]: fresh }));
  };

  const toggleRuns = async (task: ScheduledTask) => {
    if (runsOpen === task.id) {
      setRunsOpen(null);
      return;
    }
    setRunsOpen(task.id);
    try {
      await loadRuns(task.id);
    } catch (e) {
      setStatus(String(e));
    }
  };

  const patch = (part: Partial<ScheduledDraft>) => setForm((prev) => ({ ...prev, ...part }));
  const needsWeekday = form.freq === "weekly" || form.freq === "monthly";
  const needsNth = form.freq === "monthly";
  const watching = form.kind !== "cadence";

  return (
    <aside className="panel-in-right flex h-full w-96 shrink-0 flex-col border-l border-neutral-800 bg-neutral-950">
      <header className="flex items-center justify-between border-b border-neutral-800 px-4 py-3">
        <h2 className="text-sm font-semibold">Scheduled</h2>
        <span className="ml-auto flex items-center gap-1">
          <IconButton
            icon="retry"
            label="Reload"
            disabled={busy}
            onClick={() => void reload().catch((e) => setStatus(String(e)))}
          />
          <IconButton icon="close" label="Close" onClick={onClose} />
        </span>
      </header>

      <div className="min-h-0 flex-1 space-y-4 overflow-y-auto p-4">
        <p className="text-[13px] leading-relaxed text-neutral-500">
          A task runs its prompt on a calendar and then tells you. Runs go through the normal
          turn path, in their own chat, and start in auto — nobody is watching at 09:00. The few
          things that are never auto-approved wait here rather than in a chat you would have to
          go looking for.
        </p>

        {parked.length > 0 && (
          <div className="space-y-2 rounded-xl border border-amber-900/60 bg-amber-950/20 p-3">
            <p className="text-[11px] font-medium tracking-wide text-amber-300 uppercase">
              Needs you ({parked.length})
            </p>
            <p className="text-[11px] leading-relaxed text-neutral-400">
              A scheduled run stopped and is waiting on you. Answering below resumes it — the
              chat it belongs to keeps its own copy of the same prompt.
            </p>
            {parked.map((park) =>
              park.status === "lost" ? (
                <div
                  key={park.id}
                  className="space-y-2 rounded-lg border border-neutral-800 bg-neutral-950/60 p-2"
                >
                  <button
                    onClick={() => onOpenChat(park.chatId)}
                    className="max-w-full truncate text-left text-[11px] text-sky-400 hover:text-sky-300"
                    title="Open the conversation this run is in"
                  >
                    {park.chatTitle}
                  </button>
                  <p className="text-[11px] leading-relaxed text-neutral-400">
                    It was still waiting when the app closed, so it never finished. Running the
                    task again is the way to pick it up.
                  </p>
                  <button
                    onClick={() => void dismiss(park)}
                    className="rounded-md border border-neutral-700 px-2 py-1 text-[11px] text-neutral-300 transition hover:border-neutral-600"
                  >
                    Dismiss
                  </button>
                </div>
              ) : (
                <div key={park.id}>
                  <button
                    onClick={() => onOpenChat(park.chatId)}
                    className="max-w-full truncate text-left text-[11px] text-sky-400 hover:text-sky-300"
                    title="Open the conversation this run is in"
                  >
                    {park.chatTitle}
                  </button>
                  {park.kind === "approval" ? (
                    <ApprovalPrompt request={park.payload} onDecide={decide} />
                  ) : (
                    <QuestionPrompt request={park.payload} onSubmit={answer} onSkip={skip} />
                  )}
                </div>
              ),
            )}
          </div>
        )}

        {tasks.length === 0 && (
          <p className="text-xs text-neutral-600">
            Nothing scheduled yet. Fill in New task below to add one.
          </p>
        )}

        {tasks.map((task) => (
          <div
            key={task.id}
            className="rounded-xl border border-neutral-800/70 bg-neutral-900/40 p-3"
          >
            <div className="flex items-center gap-2">
              <span className="min-w-0 flex-1 truncate text-[13px] font-medium text-neutral-200">
                {task.title}
              </span>
              {!task.enabled && (
                <span className="shrink-0 rounded bg-neutral-800 px-1.5 py-0.5 text-[10px] text-neutral-400">
                  paused
                </span>
              )}
              <button
                onClick={() => void toggle(task)}
                className="shrink-0 rounded-md px-2 py-0.5 text-[11px] text-sky-300 transition hover:bg-sky-500/15"
                title={task.enabled ? "Pause this task" : "Resume this task"}
              >
                {task.enabled ? "Pause" : "Resume"}
              </button>
              <button
                onClick={() => void runNow(task)}
                disabled={busy}
                className="shrink-0 rounded-md px-2 py-0.5 text-[11px] text-neutral-300 transition hover:bg-neutral-800 disabled:opacity-40"
                title="Run it now; when it next runs on its own does not change"
              >
                Run now
              </button>
              <button
                onClick={() => void remove(task)}
                className="shrink-0 text-neutral-500 hover:text-red-400"
                title="Delete this task"
              >
                <Icon name="close" />
              </button>
            </div>

            <p className="mt-1 text-[11px] text-neutral-500">{describe(task)}</p>
            <p className="mt-0.5 text-[11px] text-neutral-500">
              {nextLine(task)}
              {isTrigger(task) && task.last_checked_at
                ? ` · last checked ${when(task.last_checked_at)}`
                : ""}
              {task.last_run_at ? ` · last ran ${when(task.last_run_at)}` : ""}
            </p>
            {isTrigger(task) && task.last_note && (
              <p className="mt-0.5 text-[11px] text-neutral-600">{task.last_note}</p>
            )}

            <div className="mt-1 flex items-center gap-2">
              <button
                onClick={() => void toggleRuns(task)}
                className="rounded px-1.5 py-0.5 text-[11px] text-neutral-400 transition hover:bg-neutral-800 hover:text-neutral-200"
                title="What this task has actually run, and what it cost"
              >
                {runsOpen === task.id ? "Hide runs" : "Runs"}
              </button>
              {runsOpen === task.id && (runs[task.id] ?? []).length > 0 && (
                <span className="tabular-nums text-[10px] text-neutral-600">
                  {runs[task.id].length} run{runs[task.id].length === 1 ? "" : "s"} ·{" "}
                  {money(runs[task.id].reduce((sum, run) => sum + run.cost_usd, 0))}
                </span>
              )}
            </div>

            {runsOpen === task.id && (
              <div className="mt-1 space-y-1 rounded-lg border border-neutral-800 bg-neutral-950/60 p-2">
                {(runs[task.id] ?? []).length === 0 ? (
                  <p className="text-[11px] text-neutral-600">
                    Nothing recorded yet. A run that spent nothing leaves no entry here — the same
                    rule the spend view follows.
                  </p>
                ) : (
                  (runs[task.id] ?? []).map((run) => (
                    <div key={run.run_id} className="flex items-baseline gap-2 text-[11px]">
                      <span className="shrink-0 text-neutral-400">{when(run.created_at)}</span>
                      <span
                        className="min-w-0 flex-1 truncate text-neutral-500"
                        title={run.chat_title}
                      >
                        {run.chat_title || "chat deleted"}
                      </span>
                      {run.chat_title && (
                        <button
                          onClick={() => onOpenChat(run.chat_id)}
                          className="shrink-0 text-sky-400 transition hover:text-sky-300"
                          title="Open the conversation this run happened in"
                        >
                          open
                        </button>
                      )}
                      <span className="shrink-0 tabular-nums text-neutral-400">
                        {money(run.cost_usd)}
                      </span>
                    </div>
                  ))
                )}
              </div>
            )}

            <p className="mt-1 line-clamp-3 text-[11px] text-neutral-600" title={task.prompt}>
              {task.prompt}
            </p>
          </div>
        ))}

        <div className="rounded-xl border border-neutral-800/70 bg-neutral-900/40 p-3">
          <h3 className="mb-2 text-[12px] font-medium text-neutral-300">New task</h3>
          <div className="space-y-2">
            <input
              value={form.title}
              onChange={(e) => patch({ title: e.target.value })}
              placeholder="Title, e.g. Morning brief"
              className={field}
            />
            <textarea
              value={form.prompt}
              onChange={(e) => patch({ prompt: e.target.value })}
              placeholder="What to do, as if you were typing it into a chat"
              rows={3}
              className={`${field} resize-none`}
            />
            <select
              value={form.kind}
              onChange={(e) => patch({ kind: e.target.value })}
              className={field}
              title="What makes this task start"
            >
              {KINDS.map((kind) => (
                <option key={kind.id} value={kind.id}>
                  {kind.label}
                </option>
              ))}
            </select>
            {watching && (
              <>
                {form.kind === "mcp" ? (
                  <select
                    value={form.target}
                    onChange={(e) => patch({ target: e.target.value })}
                    className={field}
                    title="Which server wakes this task"
                  >
                    <option value="">Pick a server…</option>
                    {servers.map((server) => (
                      <option key={server.name} value={server.name}>
                        {server.name}
                      </option>
                    ))}
                  </select>
                ) : (
                  <input
                    value={form.target}
                    onChange={(e) => patch({ target: e.target.value })}
                    placeholder={TARGET_LABEL[form.kind]}
                    className={field}
                    title={TARGET_LABEL[form.kind]}
                  />
                )}
                <input
                  value={form.filter}
                  onChange={(e) => patch({ filter: e.target.value })}
                  placeholder={FILTER_LABEL[form.kind]}
                  className={field}
                  title={FILTER_LABEL[form.kind]}
                />
                {form.kind === "mcp" ? (
                  <p className="text-[11px] leading-relaxed text-neutral-600">
                    {servers.length === 0
                      ? "No servers set up yet — add one under Customise → Connectors."
                      : "Nothing is polled: the task starts when the server pushes that notification, and only while the app is open. An event that arrives while it is closed is not replayed."}
                  </p>
                ) : (
                  <>
                    <select
                      value={form.interval_secs}
                      onChange={(e) => patch({ interval_secs: Number(e.target.value) })}
                      className={field}
                      title="How often it looks"
                    >
                      {INTERVALS.map((interval) => (
                        <option key={interval.secs} value={interval.secs}>
                          {interval.label}
                        </option>
                      ))}
                    </select>
                    <p className="text-[11px] leading-relaxed text-neutral-600">
                      It looks on that interval and starts a run only when what it sees has changed
                      — a folder that stays busy starts one run, not fifty. A change it never sees
                      while the app is closed does not start one later. Six runs an hour is the cap.
                    </p>
                  </>
                )}
              </>
            )}
            {!watching && (
              <>
                <div className="flex gap-2">
                  <select
                    value={form.freq}
                    onChange={(e) => patch({ freq: e.target.value })}
                    className={field}
                    title="How often"
                  >
                    <option value="hourly">Every hour</option>
                    <option value="daily">Every day</option>
                    <option value="weekdays">Weekdays</option>
                    <option value="weekly">Every week</option>
                    <option value="monthly">Every month</option>
                  </select>
                  <input
                    type="time"
                    value={form.time}
                    onChange={(e) => patch({ time: e.target.value || "09:00" })}
                    className={`${field} w-32`}
                    title={form.freq === "hourly" ? "Only the minute is used" : "Local time"}
                  />
                </div>
                {form.freq === "hourly" && (
                  <p className="text-[11px] text-neutral-600">
                    Hourly reads the minute only: it runs at :{form.time.slice(-2)} past every
                    hour.
                  </p>
                )}
                {(needsWeekday || needsNth) && (
                  <div className="flex gap-2">
                    {needsNth && (
                      <select
                        value={form.nth}
                        onChange={(e) => patch({ nth: Number(e.target.value) })}
                        className={field}
                        title="Which one of the month"
                      >
                        {NTHS.map((label, index) => (
                          <option key={label} value={index + 1}>
                            {label}
                          </option>
                        ))}
                      </select>
                    )}
                    <select
                      value={form.weekday}
                      onChange={(e) => patch({ weekday: Number(e.target.value) })}
                      className={field}
                      title="Which day"
                    >
                      {WEEKDAYS.map((label, index) => (
                        <option key={label} value={index}>
                          {label}
                        </option>
                      ))}
                    </select>
                  </div>
                )}
              </>
            )}
            <select
              value={form.project_id ?? ""}
              onChange={(e) => patch({ project_id: e.target.value || null })}
              className={field}
              title="Which project the run belongs to"
            >
              <option value="">No project</option>
              {projects.map((project) => (
                <option key={project.id} value={project.id}>
                  {project.name}
                </option>
              ))}
            </select>
            <p className="text-[11px] text-neutral-600">
              Each run starts a fresh chat, so a daily task does not become one conversation that
              grows forever. The chat shows up in the sidebar under its title.
            </p>
            <button onClick={() => void add()} disabled={busy} className={quiet}>
              + Schedule it
            </button>
          </div>
        </div>

        {status && <p className="text-[12px] text-neutral-400">{status}</p>}
      </div>
    </aside>
  );
}
