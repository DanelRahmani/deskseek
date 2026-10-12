/**
 * The file tree's two pure halves: the rows it draws, and where a key moves you.
 *
 * Both live here rather than inside the panel because they are the same data —
 * what is visible is what the keyboard walks — and because a component cannot be
 * run without a browser. `tree.check.ts` drives both, so the arrow keys that make
 * the tree usable without a mouse are tested by the gate instead of by hand.
 *
 * The drawing is deliberately flat: one array of rows, each carrying its own
 * `level`, rather than nested lists. A flat list is what the keyboard needs
 * (Up/Down are index arithmetic), and `aria-level` is how a tree says the same
 * thing to a screen reader — so the shape the mouse sees and the shape the
 * keyboard walks cannot drift apart.
 */
import type { TreeEntry } from "../types";

/** One drawn row: an entry, plus where it sits in reading order. */
export interface TreeRow {
  path: string;
  name: string;
  isDir: boolean;
  /** 1 for a top-level entry, 2 for a child of one. This is `aria-level`. */
  level: number;
  /** Only ever true for a directory. A file is never "expanded". */
  expanded: boolean;
}

/**
 * Every row the tree is currently showing, top to bottom.
 *
 * A directory's children appear only once it is open, so collapsing a folder
 * hides everything under it — the same rule the old recursive drawing followed,
 * done once here so the keyboard and the screen cannot disagree about what is
 * on screen.
 *
 * A directory whose listing has not arrived yet is drawn as an open, empty
 * folder rather than left out: the fetch is in flight, and a row that vanishes
 * while you look at it is worse than one that shows nothing for a moment.
 */
export function flatten(children: Record<string, TreeEntry[]>, open: string[]): TreeRow[] {
  const rows: TreeRow[] = [];
  const walk = (dir: string, level: number) => {
    for (const entry of children[dir] ?? []) {
      const expanded = entry.isDir && open.includes(entry.path);
      rows.push({
        path: entry.path,
        name: entry.name,
        isDir: entry.isDir,
        level,
        expanded,
      });
      if (expanded) walk(entry.path, level + 1);
    }
  };
  walk("", 1);
  return rows;
}

/** The six keys the tree itself owns. Anything else is not a move. */
const MOVES = ["ArrowDown", "ArrowUp", "Home", "End", "ArrowRight", "ArrowLeft"];

/**
 * Where a key moves the focus, or `null` when the key is not a move.
 *
 * The `null` matters: ArrowRight on a closed folder has to *open* it, which is
 * the caller's job, and ArrowLeft on an open one has to *close* it. A function
 * that answered "stay put" could not tell the caller the difference between "I
 * handled this" and "you handle this", and the folder would never move. Closing
 * before stepping out is the standard tree pattern and the only way to shut a
 * folder that is two levels down: after the parent step there is nothing left to
 * press.
 *
 * ArrowRight on an *open* folder moves into its first child, which is the other
 * half of that pattern and the reason `flatten` keeps a level on every row.
 *
 * `current` is allowed to be out of range (`-1` before anything is focused);
 * then any move key lands on the first row.
 */
export function move(rows: TreeRow[], current: number, key: string): number | null {
  if (rows.length === 0 || !MOVES.includes(key)) return null;
  const at = current >= 0 && current < rows.length ? current : -1;
  if (at < 0) return 0;

  switch (key) {
    case "ArrowDown":
      return Math.min(at + 1, rows.length - 1);
    case "ArrowUp":
      return Math.max(at - 1, 0);
    case "Home":
      return 0;
    case "End":
      return rows.length - 1;
    case "ArrowRight": {
      // `isDir && expanded` rather than "the next row is deeper": a file has no
      // children, so the level arithmetic alone would be right by luck, and
      // ArrowRight on a file must stay the caller's to ignore.
      const row = rows[at];
      const into =
        row.isDir &&
        row.expanded &&
        at + 1 < rows.length &&
        rows[at + 1].level === row.level + 1;
      return into ? at + 1 : null;
    }
    default: {
      // ArrowLeft: close first, then step out. A top-level row has no parent.
      const row = rows[at];
      if (row.isDir && row.expanded) return null;
      if (row.level === 1) return null;
      for (let i = at - 1; i >= 0; i -= 1) {
        if (rows[i].level < row.level) return i;
      }
      return null;
    }
  }
}
