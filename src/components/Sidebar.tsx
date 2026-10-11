import { useState } from "react";
import type { ChatRow, MessageHit, SectionKind } from "../types";

interface Props {
  kind: SectionKind;
  onKind: (kind: SectionKind) => void;
  chats: ChatRow[];
  activeId: string | null;
  onSelect: (id: string) => void;
  onNew: () => void;
  onDelete: (id: string) => void;
  onSettings: () => void;
  onMemory: () => void;
  onSkills: () => void;
  onMcp: () => void;
  keyMask: string | null;
  /** Chats with a run in flight, so a background turn is visible at a glance. */
  runningIds: string[];
  /** Messages matching the search box, from every conversation. */
  hits: MessageHit[];
  /** The raw box text; the owner debounces and runs the full-text query. */
  onSearch: (query: string) => void;
}

const SECTIONS: { id: SectionKind; label: string }[] = [
  { id: "chat", label: "Chat" },
  { id: "coding", label: "Coding" },
];

export function Sidebar({
  kind,
  onKind,
  chats,
  activeId,
  onSelect,
  onNew,
  onDelete,
  onSettings,
  onMemory,
  onSkills,
  onMcp,
  keyMask,
  runningIds,
  hits,
  onSearch,
}: Props) {
  const [query, setQuery] = useState("");
  // A plain substring match on the title or the folder. A search box over a
  // short list does not need more, and anything fancier would need a reason.
  const needle = query.trim().toLowerCase();
  const shown = needle
    ? chats.filter(
        (c) =>
          c.title.toLowerCase().includes(needle) ||
          (c.workspace ?? "").toLowerCase().includes(needle),
      )
    : chats;

  return (
    <aside className="flex h-full w-64 shrink-0 flex-col border-r border-neutral-800 bg-neutral-950">
      <div className="px-3 pt-3">
        <div className="mb-3 flex items-center gap-2 px-1">
          <div className="grid h-6 w-6 place-items-center rounded-md bg-sky-500/15 text-xs font-bold text-sky-400">
            D
          </div>
          <span className="text-sm font-semibold tracking-tight">DeskSeek</span>
        </div>

        <div className="mb-3 grid grid-cols-2 gap-1 rounded-lg bg-neutral-900 p-1">
          {SECTIONS.map((s) => (
            <button
              key={s.id}
              onClick={() => onKind(s.id)}
              className={`rounded-md px-2 py-1 text-xs font-medium transition ${
                kind === s.id
                  ? "bg-neutral-800 text-neutral-100"
                  : "text-neutral-400 hover:text-neutral-200"
              }`}
            >
              {s.label}
            </button>
          ))}
        </div>

        <button
          onClick={onNew}
          className="mb-2 w-full rounded-lg border border-neutral-800 px-3 py-2 text-left text-sm text-neutral-300 transition hover:border-neutral-700 hover:bg-neutral-900"
        >
          + New {kind === "coding" ? "coding task" : "chat"}
        </button>
      </div>

      <div className="px-3 pb-2">
        <input
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            onSearch(e.target.value);
          }}
          placeholder="Search…"
          className="w-full rounded-md border border-neutral-800 bg-neutral-900 px-2 py-1.5 text-xs text-neutral-200 outline-none focus:border-neutral-700"
        />
      </div>

      <nav className="flex-1 overflow-y-auto px-3 pb-2">
        {query.trim() && hits.length > 0 && (
          <div className="mb-2">
            <p className="px-1 pb-1 text-[10px] font-medium tracking-wide text-neutral-600 uppercase">
              In messages
            </p>
            {hits.map((hit) => (
              <button
                key={hit.messageId}
                onClick={() => onSelect(hit.chatId)}
                className="mb-1 block w-full rounded-md px-2 py-1.5 text-left transition hover:bg-neutral-900"
                title={hit.chatTitle}
              >
                <span className="block truncate text-[11px] text-neutral-400">
                  {hit.chatTitle}
                  <span className="text-neutral-600">
                    {" · "}
                    {hit.chatKind === "coding" ? "Coding" : "Chat"}
                  </span>
                </span>
                <span className="mt-0.5 line-clamp-2 block text-[11px] text-neutral-500">
                  {hit.snippet}
                </span>
              </button>
            ))}
          </div>
        )}
        {chats.length === 0 && (
          <p className="px-1 py-4 text-xs text-neutral-600">No conversations yet.</p>
        )}
        {chats.length > 0 && shown.length === 0 && hits.length === 0 && (
          <p className="px-1 py-4 text-xs text-neutral-600">
            Nothing matches "{query.trim()}".
          </p>
        )}
        {shown.map((c) => (
          <div
            key={c.id}
            className={`group flex items-center gap-1 rounded-md px-2 py-1.5 text-sm transition ${
              activeId === c.id
                ? "bg-neutral-800 text-neutral-100"
                : "text-neutral-400 hover:bg-neutral-900 hover:text-neutral-200"
            }`}
          >
            <button
              onClick={() => onSelect(c.id)}
              className="flex flex-1 items-center gap-1.5 truncate text-left"
              title={c.workspace ?? c.title}
            >
              {runningIds.includes(c.id) && (
                <span
                  className="h-1.5 w-1.5 shrink-0 animate-pulse rounded-full bg-sky-400"
                  title="Working"
                />
              )}
              <span className="truncate">{c.title}</span>
            </button>
            {c.cost_usd > 0 && (
              <span
                className="shrink-0 text-[10px] text-neutral-600"
                title="Total spent on this chat"
              >
                {c.cost_usd < 0.01 ? "<$0.01" : `$${c.cost_usd.toFixed(2)}`}
              </span>
            )}
            <button
              onClick={() => onDelete(c.id)}
              className="hidden shrink-0 rounded px-1 text-neutral-500 hover:text-red-400 group-hover:block"
              title="Delete"
            >
              ×
            </button>
          </div>
        ))}
      </nav>

      <div className="space-y-1 border-t border-neutral-800 p-3">
        <button
          onClick={onMemory}
          className="w-full rounded-md px-2 py-1.5 text-left text-sm text-neutral-400 transition hover:bg-neutral-900 hover:text-neutral-200"
        >
          Memory
        </button>
        <button
          onClick={onSkills}
          className="w-full rounded-md px-2 py-1.5 text-left text-sm text-neutral-400 transition hover:bg-neutral-900 hover:text-neutral-200"
        >
          Skills
        </button>
        <button
          onClick={onMcp}
          className="w-full rounded-md px-2 py-1.5 text-left text-sm text-neutral-400 transition hover:bg-neutral-900 hover:text-neutral-200"
        >
          MCP servers
        </button>
        <button
          onClick={onSettings}
          className="flex w-full items-center justify-between rounded-md px-2 py-1.5 text-sm text-neutral-400 transition hover:bg-neutral-900 hover:text-neutral-200"
        >
          <span>Settings</span>
          <span className="text-[11px] text-neutral-600">{keyMask ?? "no key"}</span>
        </button>
      </div>
    </aside>
  );
}
