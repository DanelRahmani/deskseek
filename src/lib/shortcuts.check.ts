/**
 * Checks for the shortcut map in shortcuts.ts. Plain Node, no test framework:
 *
 *   npm run check
 *
 * The resolver is the whole feature's safety net: a chord that fires for the
 * wrong key silently steals a keystroke from the user, and one that stops firing
 * for the right key is invisible in review.
 */
import { resolveShortcut } from "./shortcuts.ts";

function assert(ok: boolean, message: string): void {
  if (!ok) throw new Error(`FAIL: ${message}`);
}

/** A keypress with everything unset except what the case cares about. */
const press = (key: string, mods: Partial<Record<"ctrlKey" | "metaKey" | "shiftKey" | "altKey", true>> = {}) => ({
  key,
  ctrlKey: false,
  metaKey: false,
  shiftKey: false,
  altKey: false,
  ...mods,
});

// The chords the app advertises.
assert(resolveShortcut(press("n", { ctrlKey: true })) === "new-chat", "Ctrl+N starts a chat");
assert(resolveShortcut(press("w", { ctrlKey: true })) === "close-tab", "Ctrl+W closes the tab");
assert(resolveShortcut(press("b", { ctrlKey: true })) === "toggle-files", "Ctrl+B toggles files");
assert(resolveShortcut(press("m", { ctrlKey: true })) === "open-memory", "Ctrl+M opens memory");
assert(resolveShortcut(press(",", { ctrlKey: true })) === "open-settings", "Ctrl+, opens settings");
assert(resolveShortcut(press("\\", { ctrlKey: true })) === "toggle-split", "Ctrl+\\ toggles the split");
assert(resolveShortcut(press("1", { ctrlKey: true })) === "focus-pane-0", "Ctrl+1 focuses pane 0");
assert(resolveShortcut(press("2", { ctrlKey: true })) === "focus-pane-1", "Ctrl+2 focuses pane 1");

// Cmd is Ctrl on the other side of the world.
assert(resolveShortcut(press("n", { metaKey: true })) === "new-chat", "Cmd+N is the same chord");

// A capital letter must not need Shift held, nor may Shift silently change the
// action: the webview reports "N" when Shift is down.
assert(resolveShortcut(press("N", { ctrlKey: true })) === "new-chat", "an uppercase key still matches");
assert(resolveShortcut(press("n", { ctrlKey: true, shiftKey: true })) === null, "Ctrl+Shift+N is not ours");
assert(resolveShortcut(press("1", { ctrlKey: true, shiftKey: true })) === null, "Ctrl+Shift+1 is not ours");

// Bare keys are text, except Escape outside a field.
assert(resolveShortcut(press("n")) === null, "N alone is just typing");
assert(resolveShortcut(press("Escape")) === "dismiss", "Escape dismisses the top panel");
assert(resolveShortcut(press("Escape", { shiftKey: true })) === null, "Shift+Escape is not ours");

// Escape belongs to whatever field has focus; it must not also close a panel.
assert(resolveShortcut(press("Escape"), true) === null, "Escape while typing is the field's");
assert(resolveShortcut(press("n", { ctrlKey: true }), true) === "new-chat", "a chord still works while typing");

// AltGr (Ctrl+Alt, reported by some layouts) is never one of our chords.
assert(resolveShortcut(press("n", { ctrlKey: true, altKey: true })) === null, "Ctrl+Alt+N is not ours");
assert(resolveShortcut(press("Escape", { altKey: true })) === null, "Alt+Escape is not ours");

// A modifier we do not map is a no-op, not a guess.
assert(resolveShortcut(press("q", { ctrlKey: true })) === null, "an unmapped chord does nothing");

console.log("shortcuts.check.ts: all checks passed");
