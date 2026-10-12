import { useEffect, useState } from "react";
import { api } from "../api";
import { Icon, IconButton } from "./Icon";
import type { SkillView } from "../types";

interface Props {
  chatId: string | null;
  /** Hand the user to a fresh chat with this as their first message. */
  onAsk: (text: string) => void;
  onClose: () => void;
  /** Rendered inside the Customise hub: fills it, and draws no panel chrome. */
  embedded?: boolean;
}

/**
 * What a skill wants described is a way of working, which is easier to talk
 * about than to type into a form — so this offers a conversation instead of an
 * empty text box. The assistant has `create_skill`, so the same screen can also
 * be where you edit one by saying what to change.
 */
const BUILD_PROMPT =
  "I want to turn something I keep repeating into a skill. Ask me what it should cover — a couple of questions at a time — then draft it and save it with create_skill.";

export function SkillsPanel({ chatId, onAsk, onClose, embedded = false }: Props) {
  const [skills, setSkills] = useState<SkillView[]>([]);
  const [dir, setDir] = useState("");
  const [pasted, setPasted] = useState("");
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

  /** Install what is in the box. The common case by far: a skill arrives as text. */
  const installPasted = async () => {
    setBusy(true);
    try {
      const id = await api.installSkillText(pasted);
      setPasted("");
      await reload();
      setStatus(`Installed "${id}". Switch it on for this chat with the toggle.`);
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  };

  const addExamples = async () => {
    setBusy(true);
    try {
      const added = await api.installExamples();
      await reload();
      setStatus(
        added.length === 0
          ? "Every bundled example is already installed."
          : `Added ${added.join(", ")}.`,
      );
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

  /**
   * Flip the *other* switch: whether every new chat starts with this skill.
   *
   * It is written into the skill's own file rather than into this chat, so the
   * list is reloaded afterwards — what is on disk is the answer, not a guess
   * made here.
   */
  const setDefault = async (skill: SkillView) => {
    setBusy(true);
    try {
      await api.setSkillDefault(skill.id, !skill.default_on);
      await reload();
      setStatus(
        skill.default_on
          ? `"${skill.name}" is no longer always on. This chat still has it.`
          : `"${skill.name}" will start switched on in every new chat.`,
      );
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <aside
      className={`flex h-full flex-col bg-neutral-950 ${
        embedded ? "min-w-0 flex-1" : "w-96 shrink-0 border-l border-neutral-800"
      }`}
    >
      <header className="flex items-center justify-between border-b border-neutral-800 px-4 py-3">
        <h2 className="text-sm font-semibold">Skills</h2>
        <span className="ml-auto flex items-center gap-1">
          <IconButton
            icon="retry"
            label="Reload the list"
            disabled={busy}
            onClick={() => void reload().catch((e) => setStatus(String(e)))}
          />
          {!embedded && <IconButton icon="close" label="Close" onClick={onClose} />}
        </span>
      </header>

      <div className="flex-1 space-y-2 overflow-y-auto p-4">
        <p className="text-[13px] leading-relaxed text-neutral-500">
          A skill is an instruction file that joins this chat's system prompt while it is switched
          on. Each one is a folder under <span className="font-mono">skills/&lt;name&gt;/SKILL.md</span>, so
          they are yours to edit or copy between machines.
        </p>
        <p className="text-xs leading-relaxed text-neutral-500">
          The assistant can also reach these by itself: the whole list is always in its prompt, so
          it switches one on when a job clearly matches, and anything it writes with
          `create_skill` lands in this list switched on. None of their text is sent until that
          happens — a skill that is off costs one line of list.
        </p>
        <p className="text-xs leading-relaxed text-neutral-500">
          <span className="text-neutral-400">Always on</span> is the other switch: it writes
          `default: on` into the skill's own file, so <em>every new chat</em> starts with it. Use it
          for standing preferences; its text then goes with every request in those chats.
        </p>
        <button
          onClick={() => onAsk(BUILD_PROMPT)}
          disabled={busy}
          className="flex w-full items-center gap-2 rounded-xl bg-sky-500/10 px-3 py-2.5 text-left transition duration-150 hover:bg-sky-500/15 disabled:opacity-50"
        >
          <span className="text-[13px] font-medium text-sky-300">Build one by talking it through</span>
          <span className="ml-auto text-[11px] text-sky-300/70">opens a chat</span>
        </button>

        <div className="space-y-2 rounded-xl border border-neutral-800/70 bg-neutral-900/40 p-3">
          <label className="block text-xs text-neutral-500">Or paste one</label>
          <textarea
            value={pasted}
            onChange={(e) => setPasted(e.target.value)}
            rows={4}
            spellCheck={false}
            placeholder={
              "---\nname: Concise\ndescription: Answer in as few words as possible.\n---\nBe terse. Skip pleasantries."
            }
            className="w-full resize-y rounded-lg border border-neutral-800 bg-neutral-950 p-2 font-mono text-[11px] leading-relaxed text-neutral-300 outline-hidden transition duration-150 focus:border-neutral-700"
          />
          <div className="flex items-center gap-2">
            <span className="text-[11px] text-neutral-600">
              The name comes from the frontmatter, or the first heading.
            </span>
            <button
              onClick={() => void installPasted()}
              disabled={busy || !pasted.trim()}
              className="ml-auto rounded-lg bg-sky-600 px-3 py-1.5 text-xs font-medium text-accent-fg transition duration-150 hover:bg-sky-500 disabled:bg-neutral-800 disabled:text-neutral-600"
            >
              Install
            </button>
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <button
            onClick={() => void addSkill()}
            disabled={busy}
            className="rounded-lg px-3 py-1.5 text-[13px] text-neutral-300 transition duration-150 hover:bg-neutral-800 hover:text-neutral-100 disabled:opacity-40"
          >
            Add from file…
          </button>
          <button
            onClick={() => void addExamples()}
            disabled={busy}
            className="rounded-lg px-3 py-1.5 text-[13px] text-neutral-300 transition duration-150 hover:bg-neutral-800 hover:text-neutral-100 disabled:opacity-40"
          >
            Restore examples
          </button>
          <button
            onClick={() => dir && api.revealPath(dir)}
            disabled={!dir}
            className="rounded-lg px-3 py-1.5 text-[13px] text-neutral-300 transition duration-150 hover:bg-neutral-800 hover:text-neutral-100 disabled:opacity-40"
          >
            Open folder
          </button>
        </div>
        {dir && <p className="break-all font-mono text-[10px] text-neutral-600">{dir}</p>}
        {skills.length === 0 && status === null && (
          <p className="text-xs text-neutral-600">
            No skills found. Restore the examples above, or ask the assistant to save one from a
            conversation.
          </p>
        )}
        {skills.map((skill) => (
          <div
            key={skill.id}
            className={`rounded-xl border p-3 text-[13px] transition duration-150 ${
              skill.enabled
                ? "border-sky-800 bg-sky-950/20"
                : "border-neutral-800/70 bg-neutral-900/40 hover:border-neutral-700"
            }`}
          >
            <div className="flex items-start gap-1">
              <label className="flex flex-1 cursor-pointer items-start gap-3">
                <input
                  type="checkbox"
                  checked={skill.enabled}
                  onChange={() => toggle(skill.id)}
                  className="mt-0.5"
                  title={`Use "${skill.name}" in this chat`}
                />
                <span className="min-w-0">
                  <span className="block font-medium text-neutral-200">{skill.name}</span>
                  {skill.description && (
                    <span className="mt-1 block leading-relaxed text-neutral-500">
                      {skill.description}
                    </span>
                  )}
                </span>
              </label>
              <button
                onClick={() => removeSkill(skill)}
                disabled={busy}
                title="Delete this skill's folder"
                className="shrink-0 text-neutral-500 hover:text-red-400 disabled:opacity-50"
              >
                <Icon name="close" />
              </button>
            </div>
            {/* Outside the label on purpose: inside it, a click would also work
                the chat's checkbox, which is a different question. */}
            <button
              type="button"
              onClick={() => void setDefault(skill)}
              disabled={busy}
              title={
                skill.default_on
                  ? "Every new chat starts with this skill on. Click to stop that."
                  : "Start every new chat with this skill switched on."
              }
              className={`mt-2 rounded py-0.5 pr-2 pl-1 text-[11px] transition duration-150 ${
                skill.default_on
                  ? "text-emerald-300 hover:bg-emerald-500/10"
                  : "text-neutral-500 hover:bg-neutral-800 hover:text-neutral-300"
              }`}
            >
              {skill.default_on ? "✓ always on for new chats" : "always on for new chats?"}
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
