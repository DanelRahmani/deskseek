/**
 * The window shell: which chats are open as tabs, and which of them the one or
 * two panes show. Kept as pure functions so the pane-refill rules can be
 * checked without a browser (see shell.check.ts).
 */
export interface Shell {
  tabs: string[];
  /** One or two chat ids — pane 0 is the left one; null means "not picked yet". */
  panes: (string | null)[];
  /** The pane the sidebar, the tab strip and the file tree act on. */
  focus: number;
  /** Pane 0's width as a percentage of the split area. */
  splitAt: number;
}

export const MAX_PANES = 2;
export const SHELL_KEY = "deskseek.shell";
export const DEFAULT_SHELL: Shell = { tabs: [], panes: [null], focus: 0, splitAt: 50 };

const clampSplit = (n: number) => Math.min(80, Math.max(20, n));

/** Tolerant read: anything unexpected in the stored blob falls back to the
 *  defaults rather than taking the window down at startup. */
export function parseShell(raw: string | null): Shell {
  if (!raw) return DEFAULT_SHELL;
  let saved: Partial<Shell>;
  try {
    saved = JSON.parse(raw) as Partial<Shell>;
  } catch {
    return DEFAULT_SHELL;
  }
  const panes =
    Array.isArray(saved.panes) && saved.panes.length > 0
      ? saved.panes.slice(0, MAX_PANES).map((id) => (typeof id === "string" ? id : null))
      : [null];
  const focus =
    typeof saved.focus === "number" && saved.focus >= 0 && saved.focus < panes.length
      ? saved.focus
      : 0;
  return {
    tabs: Array.isArray(saved.tabs) ? saved.tabs.filter((t) => typeof t === "string") : [],
    panes,
    focus,
    splitAt: typeof saved.splitAt === "number" ? clampSplit(saved.splitAt) : DEFAULT_SHELL.splitAt,
  };
}

export function loadShell(): Shell {
  try {
    return parseShell(localStorage.getItem(SHELL_KEY));
  } catch {
    return DEFAULT_SHELL;
  }
}

export function saveShell(shell: Shell): void {
  try {
    localStorage.setItem(SHELL_KEY, JSON.stringify(shell));
  } catch {
    // A full or unavailable localStorage must not break the window.
  }
}

/**
 * Apply a removal to the shell: drop the matching tabs, blank the panes that
 * showed them, refill each blank pane with a tab that is not already on screen,
 * and keep the focus on a pane that still has something in it.
 *
 * Returns the same object when nothing changed, so React can skip the render.
 */
export function pruneShell(shell: Shell, gone: (id: string) => boolean): Shell {
  const tabs = shell.tabs.filter((t) => !gone(t));
  const panes = shell.panes.map((p) => (p && !gone(p) ? p : null));
  for (let i = 0; i < panes.length; i++) {
    if (panes[i]) continue;
    // Assigning as we go is what keeps the two panes from both refilling with
    // the same tab.
    panes[i] = tabs.find((t) => !panes.includes(t)) ?? null;
  }
  const focus = panes[shell.focus] ? shell.focus : Math.max(0, panes.findIndex((p) => p));
  const unchanged =
    tabs.length === shell.tabs.length &&
    focus === shell.focus &&
    panes.every((p, i) => p === shell.panes[i]);
  return unchanged ? shell : { tabs, panes, focus, splitAt: shell.splitAt };
}

/** Show `id` in the focused pane, or move the focus when it is already open. */
export function openChatIn(shell: Shell, id: string): Shell {
  const at = shell.panes.indexOf(id);
  if (at >= 0) return shell.focus === at ? shell : { ...shell, focus: at };
  const panes = [...shell.panes];
  panes[shell.focus] = id;
  return { ...shell, panes, tabs: shell.tabs.includes(id) ? shell.tabs : [...shell.tabs, id] };
}

/** Put `id` (or nothing) in one specific pane, and focus it. */
export function setPaneChat(shell: Shell, pane: number, id: string | null): Shell {
  const panes = [...shell.panes];
  panes[pane] = id;
  const tabs = id && !shell.tabs.includes(id) ? [...shell.tabs, id] : shell.tabs;
  return { ...shell, panes, tabs, focus: pane };
}

/** One pane, or two — the second taking the next tab that is not already shown. */
export function toggleSplitPanes(shell: Shell): Shell {
  if (shell.panes.length > 1) return { ...shell, panes: [shell.panes[0]], focus: 0 };
  const other = shell.tabs.find((t) => t !== shell.panes[0]) ?? null;
  return { ...shell, panes: [shell.panes[0], other], focus: 0 };
}
