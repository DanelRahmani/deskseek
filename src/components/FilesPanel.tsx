import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import { api, onFsChanged } from "../api";
import { flatten, move } from "../lib/tree";
import { Icon } from "./Icon";
import type { FilePreview, TreeEntry } from "../types";

interface Props {
  chatId: string | null;
  onClose: () => void;
}

/// The project explorer. It watches the chat's folder, so files the agent
/// creates or edits appear here without a refresh, and an open file re-reads
/// itself when it is written to.
///
/// The box at the top searches the folder on disk by **name**, so a file three
/// levels down is reachable without opening the folders above it. It used to
/// filter only the levels that had been expanded, which looked like a search and
/// was not one.
export function FilesPanel({ chatId, onClose }: Props) {
  const [root, setRoot] = useState("");
  const [children, setChildren] = useState<Record<string, TreeEntry[]>>({});
  const [open, setOpen] = useState<string[]>([]);
  const [file, setFile] = useState<FilePreview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  // `null` means "not searching": the tree is drawn. An array means results are.
  const [hits, setHits] = useState<TreeEntry[] | null>(null);
  const [searching, setSearching] = useState(false);

  // The search reads the folder from disk, so it is debounced and skipped for a
  // single letter — a keystroke must not walk a large tree.
  useEffect(() => {
    const needle = filter.trim();
    if (!chatId || needle.length < 2) {
      setHits(null);
      setSearching(false);
      return;
    }
    setSearching(true);
    let alive = true;
    const timer = window.setTimeout(() => {
      api
        .findInTree(chatId, needle)
        .then((found) => {
          if (!alive) return;
          setHits(found);
          setSearching(false);
        })
        .catch((e) => {
          if (!alive) return;
          setError(String(e));
          // Back to the tree rather than an empty result list: a failed search
          // must not look like a search that found nothing.
          setHits(null);
          setSearching(false);
        });
    }, 200);
    return () => {
      alive = false;
      window.clearTimeout(timer);
    };
  }, [chatId, filter]);

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

  const rows = useMemo(() => flatten(children, open), [children, open]);
  // The row the keyboard is on. Clamped rather than corrected on the way in: a
  // folder closing takes its children out from under the cursor.
  const [cursor, setCursor] = useState(0);
  const at = Math.min(cursor, Math.max(0, rows.length - 1));

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

  /**
   * The tree's own keys. `move` owns where focus goes; what is left for this
   * handler is the two keys it hands back on purpose — ArrowRight on a closed
   * folder opens it, ArrowLeft on an open one shuts it.
   *
   * Enter and Space are deliberately *not* here: a row is a button, so the
   * browser already turns them into the click that opens or selects it. Doing it
   * by hand as well would fire it twice.
   */
  const onTreeKey = (e: KeyboardEvent<HTMLDivElement>) => {
    const row = rows[at];
    if (!row) return;
    const stepped = move(rows, at, e.key);
    if (stepped !== null) {
      e.preventDefault();
      setCursor(stepped);
      return;
    }
    // What is left are the two keys the folders own.
    if (!row.isDir) return;
    if (e.key === "ArrowRight" && !row.expanded) {
      e.preventDefault();
      void toggle(row.path);
    } else if (e.key === "ArrowLeft" && row.expanded) {
      e.preventDefault();
      void toggle(row.path);
    }
  };

  return (
    <aside className="panel-in-left flex h-full w-72 shrink-0 flex-col border-r border-neutral-800 bg-neutral-950">
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
            aria-label="Re-read the tree"
          >
            ⟳
          </button>
          <button
            onClick={onClose}
            className="text-sm text-neutral-500 transition hover:text-neutral-200"
            title="Hide the file tree"
            aria-label="Hide the file tree"
          >
            <Icon name="close" size={15} />
          </button>
        </div>
        <input
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder="Find a file by name…"
          aria-label="Find a file by name"
          className="w-full rounded-md border border-neutral-800 bg-neutral-900 px-2 py-1 text-[11px] text-neutral-200 outline-hidden focus:border-neutral-700"
          title="Searches every folder under this one by file name. The same noise folders are skipped, so it never hangs. To search inside files, ask the model to grep."
        />
      </header>

      <div className="min-h-0 flex-1 overflow-auto py-1">
        {hits ? (
          hits.length === 0 ? (
            <p role="status" className="px-3 py-2 text-[11px] text-neutral-600">
              {searching
                ? "Searching…"
                : `No file name contains “${filter.trim()}”. To search inside files, ask the model to grep.`}
            </p>
          ) : (
            hits.map((entry) => (
              <button
                key={entry.path}
                onClick={() => select(entry.path)}
                className={`flex w-full flex-col items-start gap-0.5 px-3 py-[3px] text-left transition hover:bg-neutral-900 ${
                  file?.path === entry.path ? "text-sky-300" : "text-neutral-400"
                }`}
                title={entry.path}
              >
                <span className="w-full truncate text-xs">{entry.name}</span>
                <span className="w-full truncate text-[10px] text-neutral-600">
                  {entry.path.includes("/")
                    ? entry.path.slice(0, entry.path.lastIndexOf("/"))
                    : "."}
                </span>
              </button>
            ))
          )
        ) : chatId ? (
          rows.length > 0 ? (
            <div role="tree" aria-label="Project files" onKeyDown={onTreeKey}>
              {rows.map((row, index) => (
                <button
                  key={row.path}
                  role="treeitem"
                  aria-level={row.level}
                  aria-expanded={row.isDir ? row.expanded : undefined}
                  aria-selected={row.isDir ? undefined : file?.path === row.path}
                  tabIndex={index === at ? 0 : -1}
                  onFocus={() => setCursor(index)}
                  onClick={() => (row.isDir ? toggle(row.path) : select(row.path))}
                  style={{ paddingLeft: `${(row.level - 1) * 12 + 6}px` }}
                  className={`flex w-full items-center gap-1.5 py-[3px] pr-2 text-left text-xs transition hover:bg-neutral-900 ${
                    file?.path === row.path ? "text-sky-300" : "text-neutral-400"
                  }`}
                  title={row.path}
                >
                  <span className="grid w-3 shrink-0 place-items-center text-neutral-600">
                    {row.isDir && (
                      <Icon
                        name="chevron"
                        size={11}
                        className={`transition-transform duration-150 ${row.expanded ? "rotate-90" : ""}`}
                      />
                    )}
                  </span>
                  <span className={`truncate ${row.isDir ? "" : "text-neutral-500"}`}>
                    {row.name}
                  </span>
                </button>
              ))}
            </div>
          ) : (
            <p className="px-3 py-2 text-[11px] text-neutral-600">
              This folder is empty. Ask for a file and it lands here.
            </p>
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
              <Icon name="close" size={15} />
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
