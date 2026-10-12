/**
 * Checks for the file tree's shape and keyboard in tree.ts. Plain Node, no test
 * framework:
 *
 *   npm run check
 *
 * Two things are being pinned here. First, that hiding a folder hides what is
 * *under* it — the tree used to filter only the levels it had already fetched,
 * which looked like a search and was not one, and a flattened list is exactly
 * where that bug would come back silently. Second, the arrow keys, because a
 * tree that responds to the mouse and not the keyboard is the failure this item
 * exists to prevent, and a component cannot be driven by a Node script.
 *
 * The `null` returns are half the contract, so they are asserted directly:
 * ArrowRight has to hand a *closed* folder back to the caller to open it, and
 * ArrowLeft on a top-level row has to hand it back too. A move function that
 * answered "stay where you are" would leave both keys doing nothing at all.
 */
import { flatten, move } from "./tree.ts";
import type { TreeEntry } from "../types";

const failures: string[] = [];

function need(ok: boolean, message: string): void {
  if (!ok) failures.push(message);
}

function same(actual: unknown, expected: unknown, what: string): void {
  const a = JSON.stringify(actual);
  const b = JSON.stringify(expected);
  if (a !== b) failures.push(`${what}: got ${a}, wanted ${b}`);
}

const file = (name: string, path: string): TreeEntry => ({ name, path, isDir: false, size: 0 });
const folder = (name: string, path: string): TreeEntry => ({ name, path, isDir: true, size: 0 });

/**
 *   src/            (open)
 *     App.tsx
 *     util/         (open)
 *       a.ts
 *   README.md
 *   docs/           (closed)
 *     x.md          — hidden, and that is the point
 */
const children: Record<string, TreeEntry[]> = {
  "": [folder("src", "src"), file("README.md", "README.md"), folder("docs", "docs")],
  src: [file("App.tsx", "src/App.tsx"), folder("util", "src/util")],
  "src/util": [file("a.ts", "src/util/a.ts")],
  docs: [file("x.md", "docs/x.md")],
};

const rows = flatten(children, ["src", "src/util"]);
const paths = rows.map((row) => row.path);
const levels = rows.map((row) => row.level);

// --- what is drawn ----------------------------------------------------------

same(paths, ["src", "src/App.tsx", "src/util", "src/util/a.ts", "README.md", "docs"], "reading order");
same(levels, [1, 2, 2, 3, 1, 1], "aria-level per row");
same(
  rows.map((row) => row.expanded),
  [true, false, true, false, false, false],
  "only an open folder is expanded",
);
need(!paths.includes("docs/x.md"), "a closed folder hides what is under it");

// Collapsing `src` hides its whole subtree in one go — including `src/util/a.ts`,
// which is two levels down and the case a per-level filter gets wrong. The row
// for `src` itself stays: a closed folder is still a folder you can see.
const shut = flatten(children, ["src/util"]);
same(shut.map((row) => row.path), ["src", "README.md", "docs"], "collapsing a folder keeps the folder");
need(
  !shut.some((row) => row.path.startsWith("src/")),
  "collapsing a folder hides its subtree",
);

// A folder whose listing is still in flight is drawn as open and empty rather
// than dropped: the row must not flicker while the fetch lands.
const pending = flatten({ "": [folder("src", "src")] }, ["src"]);
same(pending.map((row) => row.path), ["src"], "a folder with no listing yet is still drawn");
need(pending.length === 1 && pending[0].expanded, "…and still reads as expanded");

// --- where a key moves you -------------------------------------------------

same(move(rows, -1, "ArrowDown"), 0, "ArrowDown with nothing focused enters at the top");
same(move(rows, 99, "ArrowDown"), 0, "an out-of-range row is treated as nothing focused");
same(move(rows, 0, "ArrowDown"), 1, "ArrowDown steps to the next row");
same(move(rows, 5, "ArrowDown"), 5, "ArrowDown stops at the last row");
same(move(rows, 0, "ArrowUp"), 0, "ArrowUp stops at the first row");
same(move(rows, 3, "ArrowUp"), 2, "ArrowUp steps back");
same(move(rows, 4, "Home"), 0, "Home goes to the top");
same(move(rows, 0, "End"), 5, "End goes to the last row");

same(move(rows, 0, "ArrowRight"), 1, "ArrowRight on an open folder enters it");
same(move(rows, 1, "ArrowRight"), null, "ArrowRight on a file is not a move");
same(move(rows, 2, "ArrowRight"), 3, "ArrowRight on an open folder enters it");
same(move(rows, 5, "ArrowRight"), null, "ArrowRight on a closed folder is the caller's to open");
same(
  move(shut, 0, "ArrowRight"),
  null,
  "…and the caller must open it: nothing is inside until then",
);

same(move(rows, 3, "ArrowLeft"), 2, "ArrowLeft goes to the parent folder");
same(
  move(rows, 2, "ArrowLeft"),
  null,
  "ArrowLeft on an open folder is the caller's to close, not a step out",
);
same(move(rows, 4, "ArrowLeft"), null, "ArrowLeft on a top-level row has nowhere to go");

// A *closed* folder has nothing inside to go back to, so there Left steps out to
// the parent — and that is why closing has to come first, or a folder two levels
// down could never be shut from the keyboard.
const nested: Record<string, TreeEntry[]> = {
  "": [folder("src", "src")],
  src: [file("App.tsx", "src/App.tsx"), folder("util", "src/util")],
};
const closed = flatten(nested, ["src"]);
same(closed.map((row) => row.path), ["src", "src/App.tsx", "src/util"], "a nested fixture");
same(move(closed, 2, "ArrowLeft"), 0, "ArrowLeft on a closed folder steps out to its parent");
same(move(closed, 1, "ArrowLeft"), 0, "ArrowLeft on a child steps out to its parent");

same(move(rows, 0, "Enter"), null, "Enter is not a move — the caller activates the row");
same(move(rows, 0, " "), null, "Space is not a move either");
same(move(rows, 0, "a"), null, "a letter is not a move");
same(move([], 0, "ArrowDown"), null, "an empty tree cannot be walked");

// --- the gate ---------------------------------------------------------------

if (failures.length > 0) {
  throw new Error(`FAIL: ${failures.length} tree problem(s):\n  ${failures.join("\n  ")}`);
}

console.log(
  `tree.check.ts: all checks passed (${rows.length} rows, both fixtures, ` +
    `every arrow including the four the caller has to handle)`,
);
