import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import { api, onFsChanged } from "../api";
import type { FilePreview, TreeEntry } from "../types";

interface Props {
  chatId: string | null;
  onClose: () => void;
}

/// The project explorer. It watches the chat's folder, so files the agent
/// creates or edits appear here without a refresh, and an open file re-reads
/// itself when it is written to.
export function FilesPanel({ chatId, onClose }: Props) {
  const [root, setRoot] = useState("");
  const [children, setChildren] = useState<Record<string, TreeEntry[]>>({});
  const [open, setOpen] = useState<string[]>([]);
  const [file, setFile] = useState<FilePreview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const needle = filter.trim().toLowerCase();

  // Which loaded paths survive the filter. A folder is kept when it is itself a
  // hit or when a loaded descendant is, so a match never floats without the
  // folders above it. Only levels that have been opened are searched: the tree is
  // lazy, and pretending otherwise would mean reading the whole disk.
  const keep = useMemo(() => {
    if (!needle) return null;
    const kept = new Set<string>();
    for (const list of Object.values(children)) {
      for (const entry of list) {
        if (!entry.name.toLowerCase().includes(needle)) continue;
        kept.add(entry.path);
        // A folder that matches shows what is in it: filtering it down to nothing
        // would read as broken.
        if (entry.isDir) for (const child of children[entry.path] ?? []) kept.add(child.path);
        let parent = entry.path;
        while (parent.includes("/")) {
          parent = parent.slice(0, parent.lastIndexOf("/"));
          kept.add(parent);
        }
      }
    }
    return kept;
  }, [children, needle]);

  // Read by the fs-changed handler, which must not re-subscribe on every toggle.
  const openRef = useRef<string[]>([]);
  openRef.current = open;
  const selectedRef = useRef<string | null>(null);
  selectedRef.current = file?.path ?? null;

  const refresh = useCallback(async (id: string) => {
    try {
      const listing = await api.listTree(id, null);
      setRoot(listing.root);
      // Re-read every expanded folder in one pass; a stale branch is worse
      // than a slightly larger response.
      const expanded = await Promise.all(
        openRef.current.map((dir) => api.listTree(id, dir).catch(() => null)),
      );
      const next: Record<string, TreeEntry[]> = { "": listing.entries };
      for (const item of expanded) {
        if (item) next[item.path] = item.entries;
      }
      setChildren(next);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    if (!chatId) return;
    api.watchWorkspace(chatId).catch(() => {});
    refresh(chatId);
    return () => {
      api.unwatchWorkspace(chatId).catch(() => {});
    };
  }, [chatId, refresh]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    onFsChanged((e) => {
      if (!chatId) return;
      refresh(chatId);
      const current = selectedRef.current;
      if (current && e.paths.some((path) => samePath(path, current))) {
        api.readPreview(chatId, current).then(setFile).catch(() => {});
      }
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [chatId, refresh]);

  const toggle = async (dir: string) => {
    if (open.includes(dir)) {
      setOpen(open.filter((path) => path !== dir));
      return;
    }
    setOpen([...open, dir]);
    if (!children[dir] && chatId) {
      try {
        const listing = await api.listTree(chatId, dir);
        setChildren((prev) => ({ ...prev, [listing.path]: listing.entries }));
      } catch (e) {
        setError(String(e));
      }
    }
  };

  const select = async (path: string) => {
    if (!chatId) return;
    try {
      setFile(await api.readPreview(chatId, path));
    } catch (e) {
      setError(String(e));
    }
  };

  const level = (dir: string, depth: number): ReactNode =>
    (children[dir] ?? [])
      .filter((entry) => !keep || keep.has(entry.path))
      .map((entry) => {
        // While filtering, a kept folder is shown open: the hit may be inside it.
        const expanded = open.includes(entry.path) || (keep !== null && entry.isDir);
      return (
        <div key={entry.path}>
          <button
            onClick={() => (entry.isDir ? toggle(entry.path) : select(entry.path))}
            style={{ paddingLeft: `${depth * 12 + 6}px` }}
            className={`flex w-full items-center gap-1.5 py-[3px] pr-2 text-left text-xs transition hover:bg-neutral-900 ${
              file?.path === entry.path ? "text-sky-300" : "text-neutral-400"
            }`}
            title={entry.path}
          >
            <span className="w-2 shrink-0 text-[9px] text-neutral-600">
              {entry.isDir ? (expanded ? "▾" : "▸") : ""}
            </span>
            <span className={`truncate ${entry.isDir ? "" : "text-neutral-500"}`}>
              {entry.name}
            </span>
          </button>
          {entry.isDir && expanded && level(entry.path, depth + 1)}
        </div>
      );
    });

  return (
    <aside className="flex h-full w-72 shrink-0 flex-col border-r border-neutral-800 bg-neutral-950">
      <header className="flex shrink-0 flex-col gap-2 border-b border-neutral-800 px-3 py-2">
        <div className="flex items-center gap-2">
          <span className="text-xs font-medium text-neutral-300">Files</span>
          <span className="truncate text-[10px] text-neutral-600" title={root}>
            {root.split(/[\\/]/).filter(Boolean).pop() ?? ""}
          </span>
          <button
            onClick={() => chatId && refresh(chatId)}
            className="ml-auto text-xs text-neutral-500 transition hover:text-neutral-200"
            title="Re-read the tree"
          >
            ⟳
          </button>
          <button
            onClick={onClose}
            className="text-sm text-neutral-500 transition hover:text-neutral-200"
            title="Hide the file tree"
          >
            ×
          </button>
        </div>
        <input
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder="Filter what is open…"
          className="w-full rounded-md border border-neutral-800 bg-neutral-900 px-2 py-1 text-[11px] text-neutral-200 outline-none focus:border-neutral-700"
          title="Filters the folders you have opened. Nothing is read from disk to search."
        />
      </header>

      <div className="min-h-0 flex-1 overflow-auto py-1">
        {chatId ? (
          keep && keep.size === 0 ? (
            <p className="px-3 py-2 text-[11px] text-neutral-600">
              Nothing open matches “{filter.trim()}”. Open the folder you want searched.
            </p>
          ) : (children[""] ?? []).length > 0 ? (
            level("", 0)
          ) : (
            <p className="px-3 py-2 text-[11px] text-neutral-600">This folder is empty.</p>
          )
        ) : (
          <p className="px-3 py-2 text-[11px] text-neutral-600">Open a chat first.</p>
        )}
      </div>

      {error && (
        <p className="shrink-0 border-t border-neutral-800 px-3 py-2 text-[11px] text-red-400">
          {error}
        </p>
      )}

      {file && (
        <div className="flex min-h-0 flex-[1.4] flex-col border-t border-neutral-800">
          <div className="flex shrink-0 items-center gap-2 px-3 py-1.5">
            <span className="truncate text-[11px] text-neutral-300" title={file.path}>
              {file.path}
            </span>
            <button
              onClick={() => setFile(null)}
              className="ml-auto text-sm text-neutral-500 transition hover:text-neutral-200"
              title="Close the file"
            >
              ×
            </button>
          </div>
          {file.text != null ? (
            <pre className="min-h-0 flex-1 overflow-auto px-3 pb-3 text-[11px] leading-relaxed text-neutral-300">
              {file.text}
            </pre>
          ) : (
            <p className="px-3 pb-3 text-[11px] text-neutral-500">{file.note}</p>
          )}
        </div>
      )}
    </aside>
  );
}

/// The watcher reports absolute paths with the platform's separators; the tree
/// tracks workspace-relative ones with `/`. Compare the tails.
function samePath(changed: string, open: string): boolean {
  const normalise = (value: string) => value.replace(/\\/g, "/").toLowerCase();
  return normalise(changed).endsWith(normalise(open));
}
