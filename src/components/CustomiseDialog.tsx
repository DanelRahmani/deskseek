import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { Icon } from "./Icon";
import { SkillsPanel } from "./SkillsPanel";
import { McpPanel } from "./McpPanel";
import { InstructionsSection } from "./InstructionsSection";
import { DesignSystemsSection } from "./DesignSystemsSection";
import { PromptsSection } from "./PromptsSection";
import type { Instruction, Settings } from "../types";

export type CustomiseSection = "skills" | "connectors" | "instructions" | "design" | "prompts";

/**
 * What can be added to DeskSeek, in one place: skills, connectors (the MCP
 * servers), scoped instructions, design systems, and saved prompts.
 *
 * One hub rather than five panels because they are the same kind of thing — the
 * standing configuration a chat draws on, or in a prompt's case the standing
 * text the user does — and because the sidebar had grown a footer of separate
 * entries for each.
 */
const SECTIONS: { id: CustomiseSection; label: string; keywords: string }[] = [
  { id: "skills", label: "Skills", keywords: "instruction reusable toggle create skill" },
  {
    id: "connectors",
    label: "Connectors",
    keywords: "mcp servers tools stdio http remote token",
  },
  { id: "instructions", label: "Instructions", keywords: "scoped rules context prompt" },
  {
    id: "design",
    label: "Design systems",
    keywords: "design tokens palette typography style house",
  },
  {
    id: "prompts",
    label: "Prompts",
    keywords: "saved prompt snippet template retype slash insert",
  },
];

interface Props {
  chatId: string | null;
  /** The entry to open on: `/skills` lands on Skills, `/mcp` on Connectors. */
  section: CustomiseSection;
  onAsk: (text: string) => void;
  onSaved: (settings: Settings) => void;
  onClose: () => void;
}

export function CustomiseDialog({ chatId, section, onAsk, onSaved, onClose }: Props) {
  const [tab, setTab] = useState<CustomiseSection>(section);
  const [query, setQuery] = useState("");
  const [settings, setSettings] = useState<Settings | null>(null);
  const [status, setStatus] = useState<string | null>(null);

  // Where focus goes when this closes. Captured during the first render, before
  // anything inside has had a chance to take it, so Escape or Close hands the
  // caret back to whatever opened the hub instead of dropping it on the body.
  const restore = useRef<Element | null>(null);
  if (restore.current === null) restore.current = document.activeElement;
  useEffect(() => {
    const back = restore.current;
    return () => {
      if (back instanceof HTMLElement && document.contains(back)) back.focus();
    };
  }, []);

  useEffect(() => setTab(section), [section]);

  useEffect(() => {
    api
      .getSettings()
      .then(setSettings)
      .catch((e) => setStatus(String(e)));
  }, []);

  const needle = query.trim().toLowerCase();
  const visible = needle
    ? SECTIONS.filter((s) => `${s.label} ${s.keywords}`.toLowerCase().includes(needle))
    : SECTIONS;
  useEffect(() => {
    if (visible.length > 0 && !visible.some((s) => s.id === tab)) setTab(visible[0].id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [needle]);

  /**
   * Instructions save as they change, like the theme picker: a scoped rule you
   * cannot see take effect is not worth a Save button, and this section has no
   * form of its own to submit.
   */
  const saveInstructions = async (next: Instruction[]) => {
    if (!settings) return;
    const previous = settings;
    setSettings({ ...settings, instructions: next });
    try {
      const saved = await api.saveSettings({ ...settings, instructions: next });
      setSettings(saved);
      onSaved(saved);
      setStatus("Instructions saved.");
    } catch (e) {
      setSettings(previous);
      setStatus(String(e));
    }
  };

  // These two fill the body rather than scrolling inside it: the skills list and
  // the server editor each own their own scrolling.
  const fills = tab === "skills" || tab === "connectors";

  return (
    <div
      className="fixed inset-0 z-50 grid place-items-center bg-black/60 p-4"
      onKeyDown={(e) => {
        // Nothing in here owns Escape — the search box has no state to lose —
        // so it closes the hub, the way it closes every other dialog.
        if (e.key === "Escape") onClose();
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Customise DeskSeek"
        className="flex h-[85vh] w-full max-w-4xl flex-col overflow-hidden rounded-2xl border border-neutral-800 bg-neutral-950"
      >
        <div className="flex shrink-0 items-center gap-3 border-b border-neutral-800 px-5 py-3">
          <h2 className="text-sm font-semibold">Customise</h2>
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search…"
            spellCheck={false}
            className="ml-auto w-52 rounded-lg border border-neutral-800 bg-neutral-900 px-2 py-1 text-xs text-neutral-200 outline-hidden focus:border-neutral-700"
          />
          <button
            onClick={onClose}
            className="text-neutral-500 hover:text-neutral-200"
            title="Close"
          >
            <Icon name="close" size={15} />
          </button>
        </div>

        <div className="flex min-h-0 flex-1">
          <nav className="flex w-48 shrink-0 flex-col gap-0.5 overflow-y-auto border-r border-neutral-800 p-2">
            {visible.map((entry) => (
              <button
                key={entry.id}
                onClick={() => setTab(entry.id)}
                className={`rounded-md px-3 py-2 text-left text-xs transition ${
                  tab === entry.id
                    ? "bg-neutral-800 font-medium text-neutral-100"
                    : "text-neutral-500 hover:bg-neutral-900 hover:text-neutral-300"
                }`}
              >
                {entry.label}
              </button>
            ))}
            {visible.length === 0 && (
              <p className="px-3 py-2 text-[11px] text-neutral-600">
                Nothing matches — clear the box to see every section.
              </p>
            )}
          </nav>

          {fills ? (
            <div className="flex min-h-0 min-w-0 flex-1">
              {tab === "skills" ? (
                <SkillsPanel chatId={chatId} onAsk={onAsk} onClose={onClose} embedded />
              ) : (
                <McpPanel onClose={onClose} embedded />
              )}
            </div>
          ) : (
            <div className="min-w-0 flex-1 space-y-5 overflow-y-auto px-5 py-4">
              {tab === "instructions" &&
                (settings ? (
                  <InstructionsSection
                    instructions={settings.instructions}
                    onChange={(next) => void saveInstructions(next)}
                  />
                ) : (
                  <p className="text-xs text-neutral-500">{status ?? "Loading…"}</p>
                ))}
              {tab === "design" && <DesignSystemsSection chatId={chatId} />}
              {tab === "prompts" && <PromptsSection />}
              {status && <p className="text-[11px] text-neutral-600">{status}</p>}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
