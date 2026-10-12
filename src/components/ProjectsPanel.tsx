import { useCallback, useEffect, useState } from "react";
import { api } from "../api";
import { Icon, IconButton } from "./Icon";
import { InstructionsSection } from "./InstructionsSection";
import { money } from "./SpendPanel";
import type { ChatRow, Instruction, MemoryEntryView, ProjectRow, ProjectSpend } from "../types";

interface Props {
  /** The current section's chats. Grouping happens here rather than in App. */
  chats: ChatRow[];
  activeId: string | null;
  onSelectChat: (id: string) => void;
  onNewChatInProject: (project: ProjectRow) => void;
  /** Membership changed, so the sidebar's copy of the chats is stale. */
  onChanged: () => void;
  onClose: () => void;
}

const quiet =
  "rounded-lg px-3 py-1.5 text-[13px] text-neutral-300 transition duration-150 hover:bg-neutral-800 hover:text-neutral-100 disabled:opacity-40";

/**
 * Projects: a named container with a folder, the rules every chat in it
 * inherits, the memory learned inside it, and the chats that belong to it.
 *
 * Membership is still the loose part. A project does not own a chat's memory or
 * files — the chat's own folder does that, which is why it can be taken out of a
 * project without anything else changing. What a project adds is the shared
 * things: its rules, its folder's memory and what the whole work has cost.
 */
export function ProjectsPanel({
  chats,
  activeId,
  onSelectChat,
  onNewChatInProject,
  onChanged,
  onClose,
}: Props) {
  const [projects, setProjects] = useState<ProjectRow[]>([]);
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  /** All-time spend per project, keyed by project id. */
  const [spend, setSpend] = useState<ProjectSpend[]>([]);
  /** Edits in flight, before Save. Keyed by project id. */
  const [rules, setRules] = useState<Record<string, Instruction[]>>({});
  /** Which project's rules are open, if any. */
  const [rulesOpen, setRulesOpen] = useState<string | null>(null);
  /** Which project's memory is open, if any. */
  const [memoryOpen, setMemoryOpen] = useState<string | null>(null);
  const [memory, setMemory] = useState<Record<string, MemoryEntryView[]>>({});

  const reload = useCallback(async () => {
    const [list, spent] = await Promise.all([api.listProjects(), api.projectSpend()]);
    setProjects(list);
    setSpend(spent);
  }, []);

  useEffect(() => {
    reload().catch((e) => setStatus(String(e)));
  }, [reload]);

  const add = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const folder = await open({
        directory: true,
        multiple: false,
        title: "Choose this project's folder",
      });
      if (typeof folder !== "string") return;
      setBusy(true);
      const name = folder.split(/[\\/]/).filter(Boolean).pop() ?? "New project";
      await api.createProject(name, folder);
      await reload();
      setStatus(`Created "${name}". A chat started in it works in that folder.`);
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  };

  const commitRename = async (project: ProjectRow) => {
    const name = draft.trim();
    setRenaming(null);
    if (!name || name === project.name) return;
    try {
      await api.renameProject(project.id, name);
      await reload();
    } catch (e) {
      setStatus(String(e));
    }
  };

  const remove = async (project: ProjectRow) => {
    const kept = chats.filter((c) => c.project_id === project.id).length;
    setBusy(true);
    try {
      await api.deleteProject(project.id);
      await reload();
      onChanged();
      setStatus(
        `Deleted "${project.name}". Its ${kept} chat${kept === 1 ? "" : "s"} were kept and unassigned.`,
      );
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  };

  const assign = async (chatId: string, projectId: string) => {
    try {
      await api.setChatProject(chatId, projectId || null);
      onChanged();
    } catch (e) {
      setStatus(String(e));
    }
  };

  const members = (projectId: string) => chats.filter((c) => c.project_id === projectId);
  const unassigned = chats.filter((c) => !c.project_id);

  /** `$1.20 · 4 turns`, or an honest "no spend" when nothing has run yet. */
  const spendLabel = (id: string) => {
    const s = spend.find((x) => x.projectId === id);
    if (!s) return "no spend yet";
    return `${money(s.costUsd)} · ${s.turns} turn${s.turns === 1 ? "" : "s"}`;
  };

  const rulesFor = (project: ProjectRow) => rules[project.id] ?? project.instructions;
  const rulesDirty = (project: ProjectRow) =>
    JSON.stringify(rulesFor(project)) !== JSON.stringify(project.instructions);

  const saveRules = async (project: ProjectRow) => {
    const next = rulesFor(project);
    setBusy(true);
    try {
      await api.setProjectInstructions(project.id, next);
      // Keep the saved copy as the draft, so "saved" and "not dirty" agree.
      setRules((r) => ({ ...r, [project.id]: next }));
      await reload();
      setStatus(`Saved ${next.length} rule${next.length === 1 ? "" : "s"} for "${project.name}".`);
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  };

  const toggleMemory = async (project: ProjectRow) => {
    if (memoryOpen === project.id) {
      setMemoryOpen(null);
      return;
    }
    setMemoryOpen(project.id);
    try {
      const entries = await api.projectMemory(project.id);
      setMemory((m) => ({ ...m, [project.id]: entries }));
    } catch (e) {
      setStatus(String(e));
    }
  };

  return (
    <aside className="panel-in-right flex h-full w-80 shrink-0 flex-col border-l border-neutral-800 bg-neutral-950">
      <header className="flex items-center justify-between border-b border-neutral-800 px-4 py-3">
        <h2 className="text-sm font-semibold">Projects</h2>
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

      <div className="min-h-0 flex-1 space-y-3 overflow-y-auto p-4">
        <p className="text-[13px] leading-relaxed text-neutral-500">
          A project is a folder and the conversations that belong to it. Deleting one keeps its
          chats — they are only unassigned.
        </p>

        <button onClick={() => void add()} disabled={busy} className={quiet}>
          + New project…
        </button>

        {projects.length === 0 && (
          <p className="text-xs text-neutral-600">
            No projects yet. Start one above, then add chats to it with + Chat.
          </p>
        )}

        {projects.map((project) => (
          <div key={project.id} className="rounded-xl border border-neutral-800/70 bg-neutral-900/40 p-3">
            <div className="flex items-center gap-2">
              {renaming === project.id ? (
                <input
                  autoFocus
                  value={draft}
                  onChange={(e) => setDraft(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") void commitRename(project);
                    if (e.key === "Escape") setRenaming(null);
                  }}
                  onBlur={() => void commitRename(project)}
                  className="min-w-0 flex-1 rounded-md border border-neutral-700 bg-neutral-900 px-2 py-1 text-xs text-neutral-100 outline-hidden focus:border-neutral-500"
                />
              ) : (
                <button
                  onClick={() => {
                    setRenaming(project.id);
                    setDraft(project.name);
                  }}
                  className="min-w-0 flex-1 truncate text-left text-[13px] font-medium text-neutral-200 hover:text-neutral-100"
                  title="Rename"
                >
                  {project.name}
                </button>
              )}
              <button
                onClick={() => onNewChatInProject(project)}
                className="shrink-0 rounded-md px-2 py-0.5 text-[11px] text-sky-300 transition hover:bg-sky-500/15"
                title="Start a chat inside this project"
              >
                + Chat
              </button>
              <button
                onClick={() => void remove(project)}
                disabled={busy}
                title="Delete the project, keeping its chats"
                className="shrink-0 text-neutral-500 hover:text-red-400 disabled:opacity-50"
              >
                <Icon name="close" />
              </button>
            </div>

            {project.folder && (
              <button
                onClick={() => void api.revealPath(project.folder as string)}
                className="mt-1 block max-w-full truncate text-left font-mono text-[10px] text-neutral-600 hover:text-neutral-400"
                title={project.folder}
              >
                {project.folder}
              </button>
            )}

            <div className="mt-1 flex items-center gap-1 text-[11px] text-neutral-500">
              <span className="tabular-nums">{spendLabel(project.id)}</span>
              <button
                onClick={() => setRulesOpen(rulesOpen === project.id ? null : project.id)}
                className="rounded px-1.5 py-0.5 text-neutral-400 transition hover:bg-neutral-800 hover:text-neutral-200"
                title="Rules every chat in this project follows"
              >
                Rules
              </button>
              <button
                onClick={() => void toggleMemory(project)}
                className="rounded px-1.5 py-0.5 text-neutral-400 transition hover:bg-neutral-800 hover:text-neutral-200"
                title="What the assistant has remembered about this project"
              >
                Memory
              </button>
            </div>

            {rulesOpen === project.id && (
              <div className="mt-2 space-y-2 rounded-lg border border-neutral-800 bg-neutral-950/60 p-2">
                <p className="text-[11px] text-neutral-500">
                  Applied to every chat in this project, without touching the chats.
                </p>
                <InstructionsSection
                  instructions={rulesFor(project)}
                  onChange={(next) => setRules((r) => ({ ...r, [project.id]: next }))}
                />
                <button
                  onClick={() => void saveRules(project)}
                  disabled={busy || !rulesDirty(project)}
                  className="rounded-lg bg-sky-600 px-3 py-1 text-[12px] font-medium text-white transition hover:bg-sky-500 disabled:opacity-40"
                >
                  Save rules
                </button>
              </div>
            )}

            {memoryOpen === project.id && (
              <div className="mt-2 space-y-1 rounded-lg border border-neutral-800 bg-neutral-950/60 p-2">
                {!project.folder ? (
                  <p className="text-[11px] text-neutral-600">
                    No folder, so this project's chats each keep their own memory.
                  </p>
                ) : (memory[project.id] ?? []).length === 0 ? (
                  <p className="text-[11px] text-neutral-600">
                    Nothing remembered about this project yet.
                  </p>
                ) : (
                  (memory[project.id] ?? []).map((entry) => (
                    <div key={entry.text} className="flex items-baseline gap-2">
                      <span className="min-w-0 flex-1 text-[11px] text-neutral-300">
                        {entry.text}
                      </span>
                      <span className="shrink-0 text-[10px] text-neutral-600">{entry.date}</span>
                    </div>
                  ))
                )}
              </div>
            )}

            <div className="mt-2 space-y-0.5">
              {members(project.id).length === 0 && (
                <p className="text-[11px] text-neutral-600">
                  No chats in it yet. + Chat up top starts one inside this project.
                </p>
              )}
              {members(project.id).map((chat) => (
                <div key={chat.id} className="group flex items-center gap-1">
                  <button
                    onClick={() => onSelectChat(chat.id)}
                    className={`flex-1 truncate rounded px-1.5 py-1 text-left text-xs transition ${
                      activeId === chat.id
                        ? "bg-neutral-800 text-neutral-100"
                        : "text-neutral-400 hover:bg-neutral-800/50 hover:text-neutral-200"
                    }`}
                  >
                    {chat.title}
                  </button>
                  <button
                    onClick={() => void assign(chat.id, "")}
                    title="Take it out of the project"
                    className="hidden shrink-0 text-neutral-500 hover:text-neutral-300 group-hover:block"
                  >
                    <Icon name="close" />
                  </button>
                </div>
              ))}
            </div>
          </div>
        ))}

        {projects.length > 0 && unassigned.length > 0 && (
          <div className="border-t border-neutral-800 pt-3">
            <p className="mb-2 text-[11px] font-medium tracking-wide text-neutral-600 uppercase">
              Not in a project
            </p>
            <div className="space-y-1">
              {unassigned.slice(0, 20).map((chat) => (
                <div key={chat.id} className="flex items-center gap-2">
                  <span className="min-w-0 flex-1 truncate text-xs text-neutral-400" title={chat.title}>
                    {chat.title}
                  </span>
                  <select
                    value=""
                    onChange={(e) => void assign(chat.id, e.target.value)}
                    className="w-32 shrink-0 rounded-md border border-neutral-800 bg-neutral-900 px-1.5 py-0.5 text-[11px] text-neutral-300 outline-hidden focus:border-neutral-700"
                  >
                    <option value="">Move to…</option>
                    {projects.map((project) => (
                      <option key={project.id} value={project.id}>
                        {project.name}
                      </option>
                    ))}
                  </select>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>

      <footer className="border-t border-neutral-800 p-3">
        <span className="text-[11px] text-neutral-500">{status}</span>
      </footer>
    </aside>
  );
}
