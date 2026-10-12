import { useCallback, useEffect, useState } from "react";
import { api } from "../api";
import type { ScheduledDraft } from "../api";
import { Icon, IconButton } from "./Icon";
import type { ProjectRow, ScheduledTask } from "../types";

interface Props {
  onClose: () => void;
  /** A task ran, so the sidebar's chats may be stale. */
  onChanged: () => void;
}

const WEEKDAYS = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
const NTHS = ["first", "second", "third", "fourth"];

const quiet =
  "rounded-lg px-3 py-1.5 text-[13px] text-neutral-300 transition duration-150 hover:bg-neutral-800 hover:text-neutral-100 disabled:opacity-40";
const field =
  "w-full rounded-lg border border-neutral-800 bg-neutral-950 px-2.5 py-1.5 text-[13px] text-neutral-200 outline-none transition duration-150 focus:border-neutral-600";

/**
 * "Every Wednesday at 08:00", "The first Wednesday of the month at 08:00".
 *
 * `weekday` is the Rust side's `num_days_from_monday`, which is why `WEEKDAYS`
 * starts at Monday — the one place the two conventions meet.
 */
function describe(task: ScheduledTask): string {
  if (task.freq === "weekly") return `Every ${WEEKDAYS[task.weekday]} at ${task.time}`;
  if (task.freq === "monthly") {
    return `The ${NTHS[task.nth - 1]} ${WEEKDAYS[task.weekday]} of the month at ${task.time}`;
  }
  return `Every day at ${task.time}`;
}

/** A stored timestamp as a local, readable moment. */
function when(ts: number): string {
  return new Date(ts * 1000).toLocaleString(undefined, {
    weekday: "short",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
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
};

/**
 * Scheduled tasks: a prompt, a calendar, and nothing else.
 *
 * Each run goes through the ordinary turn path — the same chat, the same tools
 * — so there is nothing here to keep in step with the composer. What this panel
 * owns is only the calendar.
 */
export function ScheduledPanel({ onClose, onChanged }: Props) {
  const [tasks, setTasks] = useState<ScheduledTask[]>([]);
  const [projects, setProjects] = useState<ProjectRow[]>([]);
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [form, setForm] = useState<ScheduledDraft>(BLANK);

  const reload = useCallback(async () => {
    const [list, projectList] = await Promise.all([api.listScheduled(), api.listProjects()]);
    setTasks(list);
    setProjects(projectList);
  }, []);

  useEffect(() => {
    reload().catch((e) => setStatus(String(e)));
  }, [reload]);

  const add = async () => {
    if (!form.prompt.trim()) {
      setStatus("A task needs something to do.");
      return;
    }
    setBusy(true);
    try {
      await api.createScheduled(form);
      setForm(BLANK);
      await reload();
      setStatus("Scheduled. It runs on the calendar above.");
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
      setStatus(`"${task.title}" ran. Its next scheduled run is unchanged.`);
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  };

  const patch = (part: Partial<ScheduledDraft>) => setForm((prev) => ({ ...prev, ...part }));

  const needsWeekday = form.freq === "weekly" || form.freq === "monthly";
  const needsNth = form.freq === "monthly";

  return (
    <aside className="flex h-full w-96 shrink-0 flex-col border-l border-neutral-800 bg-neutral-950">
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
          turn path, in their own chat, and never wait for an approval — nobody is watching at
          09:00.
        </p>

        {tasks.length === 0 && <p className="text-xs text-neutral-600">Nothing scheduled yet.</p>}

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
              {task.enabled ? `Next: ${when(task.next_run_at)}` : "Paused"}
              {task.last_run_at ? ` · last ran ${when(task.last_run_at)}` : ""}
            </p>
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
            <div className="flex gap-2">
              <select
                value={form.freq}
                onChange={(e) => patch({ freq: e.target.value })}
                className={field}
                title="How often"
              >
                <option value="daily">Every day</option>
                <option value="weekly">Every week</option>
                <option value="monthly">Every month</option>
              </select>
              <input
                type="time"
                value={form.time}
                onChange={(e) => patch({ time: e.target.value || "09:00" })}
                className={`${field} w-32`}
                title="Local time"
              />
            </div>
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
