/**
 * Checks for the pane/tab rules in shell.ts. Plain Node, no test framework:
 *
 *   npm run check
 *
 * The panes must never show the same chat twice, and closing a tab must never
 * leave a pane blank while another tab is still open — both are easy to get
 * wrong by inspection and invisible in the type checker.
 */
import { DEFAULT_SHELL, openChatIn, openInOtherPane, parseShell, setPaneChat, pruneShell, toggleSplitPanes } from "./shell.ts";
import type { Shell } from "./shell.ts";

function assert(ok: boolean, message: string): void {
  if (!ok) throw new Error(`FAIL: ${message}`);
}

const eq = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
const shell = (s: Partial<Shell>): Shell => ({ ...DEFAULT_SHELL, ...s });

// A corrupt or foreign blob must not take the window down at startup.
assert(eq(parseShell("{not json"), DEFAULT_SHELL), "corrupt JSON falls back to the default shell");
assert(eq(parseShell(null), DEFAULT_SHELL), "no stored shell falls back to the default");
assert(parseShell('{"splitAt":900}').splitAt === 80, "splitAt is clamped to the maximum");
assert(parseShell('{"splitAt":1}').splitAt === 20, "splitAt is clamped to the minimum");
assert(parseShell('{"panes":[]}').panes.length === 1, "an empty pane list is repaired");
assert(parseShell('{"focus":9,"panes":["a"]}').focus === 0, "an out-of-range focus falls back to 0");
assert(parseShell('{"panes":["a","b","c"]}').panes.length === 2, "extra panes are dropped");

// Closing the tab a pane was showing leaves that pane with a different chat.
const closed = pruneShell(shell({ tabs: ["a", "b", "c"], panes: ["a", "b"], focus: 0 }), (t) => t === "a");assert(eq(closed.tabs, ["b", "c"]), "the closed tab is gone");
assert(closed.panes[0] === "c", `the emptied pane refills with the next free tab (got ${closed.panes[0]})`);
assert(closed.panes[1] === "b", "the other pane is left alone");

// With nothing left to refill with, both panes go blank and focus stays valid.
const emptied = pruneShell(shell({ tabs: ["a"], panes: ["a", null], focus: 1 }), (t) => t === "a");
assert(eq(emptied.tabs, []), "the last tab is gone");
assert(eq(emptied.panes, [null, null]), "both panes go blank");
assert(emptied.focus === 0, "focus lands on a valid pane index");

// A removal that matches nothing must be identity, so React skips the render.
const untouched = shell({ tabs: ["a"], panes: ["a"] });
assert(pruneShell(untouched, () => false) === untouched, "a no-op prune returns the same object");

// A notebook is a tab too, and the only rule that brings is the one the caller's
// `known` set has to answer: an id held by a chat *or* a notebook survives a
// restore, and one held by neither is dropped. A notebook deleted while the app
// was closed must not come back as a tab, exactly as a deleted chat must not.
const held = new Set(["chat-1", "book-1"]);
const restored = pruneShell(
  shell({ tabs: ["chat-1", "book-1", "gone"], panes: ["book-1", "chat-1"], focus: 0 }),
  (id) => !held.has(id),
);
assert(eq(restored.tabs, ["chat-1", "book-1"]), "a notebook tab survives and a vanished one does not");
assert(
  restored.panes[0] === "book-1" && restored.focus === 0,
  "the pane keeps showing its notebook",
);
assert(eq(restored.panes[1], "chat-1"), "and the other pane keeps its chat");

// Clicking an already-open chat moves the focus instead of duplicating a tab.
const refocused = openChatIn(shell({ tabs: ["a", "b"], panes: ["a", "b"], focus: 0 }), "b");
assert(refocused.focus === 1, "clicking an open chat focuses its pane");
assert(refocused.tabs.length === 2, "clicking an open chat adds no tab");
const opened = openChatIn(shell({ tabs: ["a"], panes: ["a"], focus: 0 }), "z");
assert(eq(opened.tabs, ["a", "z"]) && opened.panes[0] === "z", "a new chat takes the focused pane and gains a tab");

// Splitting picks a tab that is not already on screen; unsplitting keeps pane 0.
const split = toggleSplitPanes(shell({ tabs: ["a", "b"], panes: ["a"] }));
assert(eq(split.panes, ["a", "b"]), "splitting fills pane 1 with the next tab");
assert(eq(toggleSplitPanes(split).panes, ["a"]), "unsplitting keeps the left pane");
const lonely = toggleSplitPanes(shell({ tabs: ["a"], panes: ["a"] }));
assert(lonely.panes.length === 2 && lonely.panes[1] === null, "splitting with one tab leaves pane 1 blank");

// Choosing a chat for a pane opens a tab for it and focuses that pane.
const placed = setPaneChat(shell({ tabs: [], panes: [null, null], focus: 0 }), 1, "n");
assert(eq(placed.tabs, ["n"]) && placed.panes[1] === "n", "setPaneChat opens a tab when needed");
assert(placed.focus === 1, "setPaneChat focuses the pane it filled");

// "Open in split view" puts the chat in the pane that is not focused, adding a
// second pane when there is only one — and never shows it on both sides.
const splitOne = openInOtherPane(shell({ tabs: ["a"], panes: ["a"], focus: 0 }), "z");
assert(splitOne.panes.length === 2, "opening beside a single pane splits it");
assert(splitOne.panes[0] === "a" && splitOne.panes[1] === "z", "the chat lands in the other pane");
assert(splitOne.focus === 1, "the new pane is focused");
assert(eq(splitOne.tabs, ["a", "z"]), "a tab is opened for it");
const fromRight = openInOtherPane(shell({ tabs: ["a", "b"], panes: ["a", "b"], focus: 1 }), "z");
assert(fromRight.panes[0] === "z" && fromRight.panes[1] === "b", "the other pane is the one not focused");
const again = openInOtherPane(shell({ tabs: ["a", "b"], panes: ["a", "b"], focus: 1 }), "a");
assert(again.focus === 0 && eq(again.panes, ["a", "b"]), "a chat already open is focused, not duplicated");

console.log("shell.check.ts: all checks passed");
