import { useEffect, useState } from "react";
import { api } from "../api";
import { Icon, IconButton } from "./Icon";
import type { Artifact, ChatRow } from "../types";

const KIND_LABEL: Record<string, string> = {
  html: "Page",
  svg: "SVG",
  mermaid: "Diagram",
  chart: "Chart",
  markdown: "Note",
  code: "Code",
};

function when(seconds: number): string {
  const delta = Date.now() / 1000 - seconds;
  if (delta < 60) return "just now";
  if (delta < 3600) return `${Math.floor(delta / 60)}m ago`;
  if (delta < 86400) return `${Math.floor(delta / 3600)}h ago`;
  return `${Math.floor(delta / 86400)}d ago`;
}

interface Props {
  /** Every chat we know about, both sections, so a title can be resolved. */
  chatsById: Record<string, ChatRow>;
  onOpen: (artifactId: string) => void;
  onClose: () => void;
}

/**
 * Every artifact, from every chat, in one list.
 *
 * Artifacts were only reachable from the conversation that made them, which is
 * the wrong shape: they are the things worth keeping, and a document is usually
 * wanted long after the chat that produced it is forgotten.
 */
export function ArtifactsGallery({ chatsById, onOpen, onClose }: Props) {
  const [items, setItems] = useState<Artifact[]>([]);
  const [status, setStatus] = useState<string | null>(null);

  const reload = async () => setItems(await api.listAllArtifacts());

  useEffect(() => {
    reload().catch((e) => setStatus(String(e)));
  }, []);

  return (
    <aside className="flex h-full w-96 shrink-0 flex-col border-l border-neutral-800 bg-neutral-950">
      <header className="flex items-center justify-between border-b border-neutral-800 px-4 py-3">
        <h2 className="text-sm font-semibold">Artifacts</h2>
        <span className="ml-auto flex items-center gap-1">
          <IconButton
            icon="retry"
            label="Reload the list"
            onClick={() => void reload().catch((e) => setStatus(String(e)))}
          />
          <IconButton icon="close" label="Close" onClick={onClose} />
        </span>
      </header>

      <div className="min-h-0 flex-1 space-y-2 overflow-y-auto p-4">
        {items.length === 0 && (
          <p className="text-xs text-neutral-600">
            Nothing yet. An artifact — a page, a diagram, a note — lands here when the assistant
            publishes one.
          </p>
        )}
        {items.map((artifact) => {
          const chat = chatsById[artifact.chat_id];
          return (
            <button
              key={artifact.id}
              onClick={() => onOpen(artifact.id)}
              className="block w-full rounded-xl border border-neutral-800/70 bg-neutral-900/40 px-3 py-2 text-left transition duration-150 hover:border-neutral-700"
            >
              <span className="flex items-baseline gap-2">
                <span className="truncate text-[13px] font-medium text-neutral-200">
                  {artifact.title}
                </span>
                <span className="ml-auto shrink-0 rounded bg-neutral-800 px-1.5 py-0.5 text-[10px] text-neutral-400">
                  {KIND_LABEL[artifact.kind] ?? artifact.kind}
                </span>
              </span>
              <span className="mt-1 flex items-center gap-2 text-[11px] text-neutral-600">
                <span className="truncate">{chat?.title ?? "a deleted chat"}</span>
                <span>·</span>
                <span className="shrink-0">{when(artifact.updated_at)}</span>
                {artifact.current_version > 1 && (
                  <>
                    <span>·</span>
                    <span className="shrink-0">v{artifact.current_version}</span>
                  </>
                )}
              </span>
            </button>
          );
        })}
        {status && <p className="text-[11px] text-red-400">{status}</p>}
      </div>

      <footer className="border-t border-neutral-800 p-3">
        <span className="flex items-center gap-1.5 text-[11px] text-neutral-500">
          <Icon name="split" size={11} />
          {items.length} artifact{items.length === 1 ? "" : "s"}
        </span>
      </footer>
    </aside>
  );
}
