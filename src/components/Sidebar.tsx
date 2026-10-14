import { useRef, useState } from "react";
import type { ChatRow, MessageHit, NotebookRow, SidebarView } from "../types";
import { searchTerms, snippetParts } from "../lib/snippet";
import { Icon } from "./Icon";

interface Props {
  /** What the sidebar is listing: a section, or the notebooks view. */
  view: SidebarView;
  onView: (view: SidebarView) => void;
  chats: ChatRow[];
  activeId: string | null;
  onSelect: (id: string) => void;
  onNew: () => void;
  onDelete: (id: string) => void;
  /** Archive a chat, or take it back out of the archive. */
  onArchive: (id: string, archived: boolean) => void;
  /** Right-clicked a row: open its menu at the cursor. */
  onContextMenu: (id: string, x: number, y: number) => void;
  /** The row whose title is being typed, if any — driven by the context menu. */
  renamingId: string | null;
  /** Save a hand-typed title, or pass `null` when the edit was abandoned. */
  onRename: (id: string, title: string | null) => void;
  onSettings: () => void;
  onMemory: () => void;
  /** Opens the Customise hub: skills, connectors, instructions, design systems. */
  onCustomise: () => void;
  onProjects: () => void;
  /** The notebooks this app holds, as the list the notebooks view draws. */
  notebooks: NotebookRow[];
  /** Open one of them in the focused pane. */
  onOpenNotebook: (id: string) => void;
  /** Make a new one and open it — what the sidebar's own button could never do. */
  onNewNotebook: () => void;
  onArtifacts: () => void;
  onScheduled: () => void;
  /** What the last turns cost: the spend panel. */
  onSpend: () => void;
  keyMask: string | null;
  /** Chats with a run in flight, so a background turn is visible at a glance. */
  runningIds: string[];
  /** Chats that finished while they were not the one being read. */
  unreadIds: string[];
  /** Messages matching the search box, from every conversation. */
  hits: MessageHit[];
  /** Whether the backend has another page of them. */
  moreHits: boolean;
  /** The raw box text; the owner debounces and runs the full-text query. */
  onSearch: (query: string) => void;
  /** Ask for the next page of the search already on screen. */
  onMoreHits: (query: string) => void;
}

const SECTIONS: { id: SidebarView; label: string }[] = [
  { id: "chat", label: "Chat" },
  { id: "coding", label: "Coding" },
  { id: "notebooks", label: "Notebooks" },
];

export function Sidebar({
  view,
  onView,
  chats,
  activeId,
  onSelect,
  onNew,
  onDelete,
  onArchive,
  onContextMenu,
  renamingId,
  onRename,
  onSettings,
  onMemory,
  onCustomise,
  onProjects,
  notebooks,
  onOpenNotebook,
  onNewNotebook,
  onArtifacts,
  onScheduled,
  onSpend,
  keyMask,
  runningIds,
  unreadIds,
  hits,
  moreHits,
  onSearch,
  onMoreHits,
}: Props) {
  const [query, setQuery] = useState("");
  // Derived from the box rather than kept alongside it: the box is already the
  // single source for the list below, and a second copy could disagree with it.
  const terms = searchTerms(query);
  // Collapsed by default: the archive exists to be out of the way, and the count
  // on the summary row is enough to say it is not empty.
  const [showArchived, setShowArchived] = useState(false);
  // A plain substring match on the title or the folder. A search box over a
  // short list does not need more, and anything fancier would need a reason.
  const needle = query.trim().toLowerCase();
  const active = chats.filter((c) => !c.archived);
  const archived = chats.filter((c) => c.archived);
  const shown = needle
    ? active.filter(
        (c) =>
          c.title.toLowerCase().includes(needle) ||
          (c.workspace ?? "").toLowerCase().includes(needle),
      )
    : active;

  const rowCls = (id: string) =>
    `group flex items-center gap-1 rounded-lg px-2.5 py-1.5 text-sm transition duration-150 ${
      activeId === id
        ? "bg-neutral-800 text-neutral-100"
        : "text-neutral-400 hover:bg-neutral-800/50 hover:text-neutral-200"
    }`;

  // Escape and Enter both blur the box, so blur is the single commit point; the
  // flag is what tells the abandon case apart from the save case.
  const cancelled = useRef(false);
  const commit = (chat: ChatRow, raw: string) => {
    const next = raw.trim();
    const abandoned = cancelled.current;
    cancelled.current = false;
    // An unchanged or emptied box is the same as pressing Escape — there is
    // nothing to save, and a blank title is not a title.
    if (abandoned || !next || next === chat.title) onRename(chat.id, null);
    else onRename(chat.id, next);
  };

  /** The row's title, replaced by a one-line editor while it is being renamed. */
  const titleField = (c: ChatRow, editorCls: string) =>
    renamingId === c.id ? (
      <input
        autoFocus
        defaultValue={c.title}
        onFocus={(e) => e.currentTarget.select()}
        onKeyDown={(e) => {
          if (e.key === "Enter") e.currentTarget.blur();
          if (e.key === "Escape") {
            cancelled.current = true;
            e.currentTarget.blur();
          }
        }}
        onBlur={(e) => commit(c, e.currentTarget.value)}
        className={editorCls}
      />
    ) : null;

  return (
    <aside className="flex h-full w-64 shrink-0 flex-col border-r border-neutral-800 bg-neutral-900">
      <div className="px-3 pt-3">
        <div className="mb-3 flex items-center gap-2 px-1">
          <div className="grid h-6 w-6 place-items-center rounded-md bg-sky-500/15 text-xs font-bold text-sky-400">
            D
          </div>
          <span className="text-sm font-semibold tracking-tight">DeskSeek</span>
        </div>

        <div className="mb-3 grid grid-cols-3 gap-1 rounded-lg bg-neutral-950/70 p-1">
          {SECTIONS.map((s) => (
            <button
              key={s.id}
              onClick={() => onView(s.id)}
              className={`rounded-lg px-2 py-1.5 text-[13px] font-medium transition duration-150 ${
                view === s.id
                  ? "bg-neutral-800 text-neutral-100"
                  : "text-neutral-400 hover:text-neutral-200"
              }`}
            >
              {s.label}
            </button>
          ))}
        </div>

        <button
          onClick={view === "notebooks" ? onNewNotebook : onNew}
          className="mb-2 w-full rounded-xl bg-neutral-800/70 px-3 py-2 text-left text-[13px] text-neutral-200 transition duration-150 hover:bg-neutral-800"
        >
          + New {view === "coding" ? "coding task" : view === "notebooks" ? "notebook" : "chat"}
        </button>
      </div>

      {/* Searching messages has nothing to say about a list of notebooks, so the box
          belongs to the chat list rather than to the sidebar as a whole. */}
      {view !== "notebooks" && (
        <div className="px-3 pb-2">
          <input
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              onSearch(e.target.value);
            }}
            placeholder="Search…"
            className="w-full rounded-lg border border-neutral-800/80 bg-neutral-950 px-2.5 py-1.5 text-[13px] text-neutral-200 outline-hidden transition duration-150 focus:border-neutral-600"
          />
        </div>
      )}

      {/* One slot, two lists. The notebooks view is a peer of Chat and Coding rather
          than a sixth item in the list of settings, because it is where a notebook is
          *chosen* — and the one already open in the focused pane is marked, so the list
          and the pane cannot disagree about which one that is. */}
      {view === "notebooks" && (
        <nav className="flex-1 overflow-y-auto px-3 pb-2">
          {notebooks.length === 0 && (
            <p className="px-2 py-2 text-[12px] leading-relaxed text-neutral-500">
              No notebooks yet. New notebook makes one: a place holding its own sources and the
              questions asked of them.
            </p>
          )}
          {notebooks.map((book) => (
            <button
              key={book.id}
              onClick={() => onOpenNotebook(book.id)}
              title={`Open "${book.title}"`}
              className={`mb-1 flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-[13px] transition duration-150 ${
                book.id === activeId
                  ? "bg-neutral-800/70 text-neutral-100"
                  : "text-neutral-300 hover:bg-neutral-800/50"
              }`}
            >
              <Icon name="book" size={13} className="shrink-0 text-sky-400" />
              <span className="min-w-0 flex-1 truncate">{book.title}</span>
            </button>
          ))}
        </nav>
      )}

      {view !== "notebooks" && (
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
                className="mb-1 block w-full rounded-lg px-2 py-1.5 text-left transition duration-150 hover:bg-neutral-800/50"
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
                  {/* The backend marks the matched words with brackets; drawn
                      as-is the row would read `[pars]er`. They become emphasis
                      here, and a bracket that is the message's own stays put. */}
                  {snippetParts(hit.snippet, terms).map((part, at) =>
                    part.match ? (
                      <mark key={at} className="rounded-sm bg-sky-500/20 text-sky-200">
                        {part.text}
                      </mark>
                    ) : (
                      <span key={at}>{part.text}</span>
                    ),
                  )}
                </span>
              </button>
            ))}
            {moreHits && (
              <button
                onClick={() => onMoreHits(query)}
                className="mb-1 w-full rounded-lg px-2 py-1.5 text-left text-[11px] text-neutral-500 transition duration-150 hover:bg-neutral-800/50 hover:text-neutral-300"
              >
                Show more matches
              </button>
            )}
          </div>
        )}
        {chats.length === 0 && (
          <p className="px-1 py-4 text-[13px] text-neutral-500">
            No conversations yet. Ctrl+N starts one.
          </p>
        )}
        {shown.length === 0 && hits.length === 0 && query.trim() && (
          <p className="px-1 py-4 text-[13px] text-neutral-500">
            Nothing matches "{query.trim()}".
          </p>
        )}
        {shown.map((c) => (
          <div
            key={c.id}
            className={rowCls(c.id)}
            onContextMenu={(e) => {
              e.preventDefault();
              onContextMenu(c.id, e.clientX, e.clientY);
            }}
          >
            {titleField(
              c,
              "min-w-0 flex-1 rounded-md border border-neutral-600 bg-neutral-950 px-1.5 py-0.5 text-[13px] text-neutral-100 outline-hidden",
            ) ?? (
              <button
                onClick={() => onSelect(c.id)}
                className="flex flex-1 items-center gap-1.5 truncate text-left"
                title={`${c.workspace ?? c.title} — right-click to rename`}
              >
                {runningIds.includes(c.id) && (
                  <span
                    className="h-1.5 w-1.5 shrink-0 animate-pulse rounded-full bg-sky-400"
                    title="Working"
                  />
                )}
                <span className="truncate">{c.title}</span>
              </button>
            )}
            {unreadIds.includes(c.id) && (
              <span
                className="h-1.5 w-1.5 shrink-0 rounded-full bg-emerald-400"
                title="Finished since you last looked"
              />
            )}
            {c.cost_usd > 0 && (
              <span
                className="shrink-0 text-[10px] text-neutral-600"
                title="Total spent on this chat"
              >
                {c.cost_usd < 0.01 ? "<$0.01" : `$${c.cost_usd.toFixed(2)}`}
              </span>
            )}
            <button
              onClick={() => onArchive(c.id, true)}
              className="hidden shrink-0 rounded px-1 text-neutral-500 hover:text-sky-400 group-hover:block"
              title="Archive — keeps it, out of the way"
            >
              <Icon name="archive" />
            </button>
            <button
              onClick={() => onDelete(c.id)}
              className="hidden shrink-0 rounded px-1 text-neutral-500 hover:text-red-400 group-hover:block"
              title="Delete"
            >
              <Icon name="close" />
            </button>
          </div>
        ))}

        {archived.length > 0 && (
          <div className="mt-3 border-t border-neutral-800 pt-2">
            <button
              onClick={() => setShowArchived((open) => !open)}
              className="flex w-full items-center gap-1.5 rounded-md px-2 py-1 text-[11px] text-neutral-500 transition hover:text-neutral-300"
              title="Archived chats are only hidden, never deleted"
            >
              <Icon
                name="chevron"
                size={11}
                className={`transition-transform duration-150 ${showArchived ? "rotate-90" : ""}`}
              />
              Archived ({archived.length})
            </button>
            {showArchived &&
              archived.map((c) => (
                <div
                  key={c.id}
                  className={rowCls(c.id)}
                  onContextMenu={(e) => {
                    e.preventDefault();
                    onContextMenu(c.id, e.clientX, e.clientY);
                  }}
                >
                  {titleField(
                    c,
                    "min-w-0 flex-1 rounded-md border border-neutral-700 bg-neutral-900 px-1.5 py-0.5 text-[13px] text-neutral-400 outline-hidden",
                  ) ?? (
                    <button
                      onClick={() => onSelect(c.id)}
                      className="flex-1 truncate text-left text-neutral-500"
                      title={`${c.workspace ?? c.title} — right-click to rename`}
                    >
                      {c.title}
                    </button>
                  )}
                  <button
                    onClick={() => onArchive(c.id, false)}
                    className="hidden shrink-0 rounded px-1 text-neutral-500 hover:text-sky-400 group-hover:block"
                    title="Take out of the archive"
                  >
                    <Icon name="restore" />
                  </button>
                  <button
                    onClick={() => onDelete(c.id)}
                    className="hidden shrink-0 rounded px-1 text-neutral-500 hover:text-red-400 group-hover:block"
                    title="Delete"
                  >
                    <Icon name="close" />
                  </button>
                </div>
              ))}
          </div>
        )}
      </nav>
      )}

      <div className="space-y-1 border-t border-neutral-800 p-3">
        <button
          onClick={onMemory}
          className="w-full rounded-md px-2 py-1.5 text-left text-sm text-neutral-400 transition duration-150 hover:bg-neutral-800/60 hover:text-neutral-200"
        >
          Memory
        </button>
        <button
          onClick={onProjects}
          className="w-full rounded-md px-2 py-1.5 text-left text-sm text-neutral-400 transition duration-150 hover:bg-neutral-800/60 hover:text-neutral-200"
        >
          Projects
        </button>
        <button
          onClick={onArtifacts}
          className="w-full rounded-md px-2 py-1.5 text-left text-sm text-neutral-400 transition duration-150 hover:bg-neutral-800/60 hover:text-neutral-200"
        >
          Artifacts
        </button>
        <button
          onClick={onCustomise}
          className="w-full rounded-md px-2 py-1.5 text-left text-sm text-neutral-400 transition duration-150 hover:bg-neutral-800/60 hover:text-neutral-200"
        >
          Customise
        </button>
        <button
          onClick={onScheduled}
          className="w-full rounded-md px-2 py-1.5 text-left text-sm text-neutral-400 transition duration-150 hover:bg-neutral-800/60 hover:text-neutral-200"
        >
          Scheduled
        </button>
        <button
          onClick={onSpend}
          className="w-full rounded-md px-2 py-1.5 text-left text-sm text-neutral-400 transition duration-150 hover:bg-neutral-800/60 hover:text-neutral-200"
        >
          Spend
        </button>
        <button
          onClick={onSettings}
          className="flex w-full items-center justify-between rounded-md px-2 py-1.5 text-sm text-neutral-400 transition duration-150 hover:bg-neutral-800/60 hover:text-neutral-200"
        >
          <span>Settings</span>
          <span className="text-[11px] text-neutral-600">{keyMask ?? "no key"}</span>
        </button>
      </div>
    </aside>
  );
}
