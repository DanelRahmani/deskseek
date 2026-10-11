import type { ChatRow } from "../types";
import { Icon } from "./Icon";

interface Props {
  tabs: ChatRow[];
  panes: (string | null)[];
  focus: number;
  split: boolean;
  /** Chats with a run in flight, so a background turn is visible at a glance. */
  runningIds: string[];
  onSelect: (id: string) => void;
  onClose: (id: string) => void;
  onToggleSplit: () => void;
}

export function TabStrip({
  tabs,
  panes,
  focus,
  split,
  runningIds,
  onSelect,
  onClose,
  onToggleSplit,
}: Props) {
  const active = panes[focus];
  return (
    <div className="flex h-9 shrink-0 items-stretch border-b border-neutral-800 bg-neutral-950">
      <div className="flex min-w-0 flex-1 items-stretch overflow-x-auto">
        {tabs.length === 0 && (
          <span className="self-center px-3 text-xs text-neutral-600">No open chats</span>
        )}
        {tabs.map((chat) => {
          const pane = panes.indexOf(chat.id);
          return (
            <div
              key={chat.id}
              className={`group flex max-w-56 min-w-0 shrink-0 items-center gap-1.5 border-r border-neutral-800 px-2.5 text-xs transition ${
                chat.id === active
                  ? "bg-neutral-900 text-neutral-100"
                  : "text-neutral-400 hover:bg-neutral-900/60 hover:text-neutral-200"
              }`}
              title={chat.title}
            >
              {pane >= 0 && (
                <span
                  className="grid h-4 w-4 shrink-0 place-items-center rounded bg-sky-500/15 text-[10px] font-semibold text-sky-400"
                  title={`Shown in pane ${pane + 1}`}
                >
                  {pane + 1}
                </span>
              )}
              {runningIds.includes(chat.id) && (
                <span
                  className="h-1.5 w-1.5 shrink-0 animate-pulse rounded-full bg-sky-400"
                  title="Working"
                />
              )}
              <button onClick={() => onSelect(chat.id)} className="truncate py-2 text-left">
                {chat.title}
              </button>
              <button
                onClick={() => onClose(chat.id)}
                className="shrink-0 rounded px-1 text-neutral-600 opacity-0 transition group-hover:opacity-100 hover:text-red-400"
                title="Close tab"
              >
                <Icon name="close" size={13} />
              </button>
            </div>
          );
        })}
      </div>
      <button
        onClick={onToggleSplit}
        className={`shrink-0 border-l border-neutral-800 px-3 text-xs transition ${
          split ? "bg-sky-500/10 text-sky-300" : "text-neutral-500 hover:text-neutral-200"
        }`}
        title={split ? "Close the second pane" : "Split into two panes"}
      >
        <span className="flex items-center gap-1.5">
          <Icon name="split" size={15} />
          {split ? "Unsplit" : "Split"}
        </span>
      </button>
    </div>
  );
}
