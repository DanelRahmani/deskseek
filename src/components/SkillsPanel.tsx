import { useEffect, useState } from "react";
import { api } from "../api";
import type { SkillView } from "../types";

interface Props {
  chatId: string | null;
  onClose: () => void;
}

export function SkillsPanel({ chatId, onClose }: Props) {
  const [skills, setSkills] = useState<SkillView[]>([]);
  const [dir, setDir] = useState("");
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const reload = async () => {
    const [list, path] = await Promise.all([
      api.listSkills(chatId ?? undefined),
      api.skillsDir(),
    ]);
    setSkills(list);
    setDir(path);
  };

  useEffect(() => {
    reload().catch((e) => setStatus(String(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [chatId]);

  const addSkill = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const picked = await open({
        multiple: false,
        title: "Pick a skill file",
        filters: [{ name: "Skill", extensions: ["md", "markdown", "txt"] }],
      });
      if (typeof picked !== "string") return;
      setBusy(true);
      const id = await api.installSkill(picked);
      await reload();
      setStatus(`Installed "${id}".`);
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  };

  const removeSkill = async (skill: SkillView) => {
    setBusy(true);
    try {
      await api.deleteSkill(skill.id);
      await reload();
      setStatus(`Removed "${skill.name}".`);
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  };

  const toggle = async (id: string) => {
    if (!chatId) {
      setStatus("Open a chat to choose skills for it.");
      return;
    }
    const next = skills.map((s) => (s.id === id ? { ...s, enabled: !s.enabled } : s));
    setSkills(next);
    try {
      await api.setChatSkills(
        chatId,
        next.filter((s) => s.enabled).map((s) => s.id),
      );
    } catch (e) {
      setStatus(String(e));
    }
  };

  return (
    <aside className="flex h-full w-96 shrink-0 flex-col border-l border-neutral-800 bg-neutral-950">
      <header className="flex items-center justify-between border-b border-neutral-800 px-4 py-3">
        <h2 className="text-sm font-semibold">Skills</h2>
        <button onClick={onClose} className="text-neutral-500 hover:text-neutral-200">
          ×
        </button>
      </header>

      <div className="flex-1 space-y-2 overflow-y-auto p-4">
        <p className="text-xs text-neutral-600">
          Skills add instructions to this chat's system prompt. Each one is a folder of its own
          under <span className="font-mono">skills/&lt;name&gt;/SKILL.md</span>.
        </p>
        <div className="flex flex-wrap items-center gap-2">
          <button
            onClick={addSkill}
            disabled={busy}
            className="rounded-lg border border-neutral-800 px-3 py-1.5 text-xs text-neutral-300 hover:border-neutral-700 disabled:opacity-50"
          >
            Add skill…
          </button>
          <button
            onClick={() => dir && api.revealPath(dir)}
            disabled={!dir}
            className="rounded-lg border border-neutral-800 px-3 py-1.5 text-xs text-neutral-300 hover:border-neutral-700 disabled:opacity-50"
          >
            Open folder
          </button>
        </div>
        {dir && <p className="break-all font-mono text-[10px] text-neutral-600">{dir}</p>}
        {skills.length === 0 && status === null && (
          <p className="text-xs text-neutral-600">No skills found.</p>
        )}
        {skills.map((skill) => (
          <div
            key={skill.id}
            className={`flex items-start gap-1 rounded-lg border p-2.5 text-xs transition ${
              skill.enabled
                ? "border-sky-800 bg-sky-950/20"
                : "border-neutral-800 bg-neutral-900/40 hover:border-neutral-700"
            }`}
          >
            <label className="flex flex-1 cursor-pointer items-start gap-3">
              <input
                type="checkbox"
                checked={skill.enabled}
                onChange={() => toggle(skill.id)}
                className="mt-0.5"
              />
              <span>
                <span className="block font-medium text-neutral-200">{skill.name}</span>
                {skill.description && (
                  <span className="mt-0.5 block text-neutral-500">{skill.description}</span>
                )}
              </span>
            </label>
            <button
              onClick={() => removeSkill(skill)}
              disabled={busy}
              title="Delete this skill's folder"
              className="shrink-0 text-neutral-500 hover:text-red-400 disabled:opacity-50"
            >
              ×
            </button>
          </div>
        ))}
      </div>

      <footer className="border-t border-neutral-800 p-3">
        <span className="text-[11px] text-neutral-500">{status}</span>
      </footer>
    </aside>
  );
}
