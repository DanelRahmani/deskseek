import type { ChatRow, SectionKind } from "../types";

interface Props {
  kind: SectionKind;
  onKind: (kind: SectionKind) => void;
  chats: ChatRow[];
  activeId: string | null;
  onSelect: (id: string) => void;
  onNew: () => void;
  onDelete: (id: string) => void;
  onSettings: () => void;
  keyMask: string | null;
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
  keyMask,
}: Props) {
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

      <nav className="flex-1 overflow-y-auto px-3 pb-2">
        {chats.length === 0 && (
          <p className="px-1 py-4 text-xs text-neutral-600">No conversations yet.</p>
        )}
        {chats.map((c) => (
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
              className="flex-1 truncate text-left"
              title={c.title}
            >
              {c.title}
            </button>
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

      <div className="border-t border-neutral-800 p-3">
        <button
          onClick={onSettings}
          className="flex w-full items-center justify-between rounded-md px-2 py-2 text-sm text-neutral-400 transition hover:bg-neutral-900 hover:text-neutral-200"
        >
          <span>Settings</span>
          <span className="text-[11px] text-neutral-600">{keyMask ?? "no key"}</span>
        </button>
      </div>
    </aside>
  );
}
