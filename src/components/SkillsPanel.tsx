import { useEffect, useState } from "react";
import { api } from "../api";
import type { SkillView } from "../types";

interface Props {
  chatId: string | null;
  onClose: () => void;
}

export function SkillsPanel({ chatId, onClose }: Props) {
  const [skills, setSkills] = useState<SkillView[]>([]);
  const [status, setStatus] = useState<string | null>(null);

  useEffect(() => {
    api
      .listSkills(chatId ?? undefined)
      .then(setSkills)
      .catch((e) => setStatus(String(e)));
  }, [chatId]);

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
          Skills add instructions to this chat's system prompt. Files live in the app data
          folder under <span className="font-mono">skills/&lt;name&gt;/SKILL.md</span> — add your
          own by creating one.
        </p>
        {skills.length === 0 && status === null && (
          <p className="text-xs text-neutral-600">No skills found.</p>
        )}
        {skills.map((skill) => (
          <label
            key={skill.id}
            className={`flex cursor-pointer items-start gap-3 rounded-lg border p-2.5 text-xs transition ${
              skill.enabled
                ? "border-sky-800 bg-sky-950/20"
                : "border-neutral-800 bg-neutral-900/40 hover:border-neutral-700"
            }`}
          >
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
        ))}
      </div>

      <footer className="border-t border-neutral-800 p-3">
        <span className="text-[11px] text-neutral-500">{status}</span>
      </footer>
    </aside>
  );
}
