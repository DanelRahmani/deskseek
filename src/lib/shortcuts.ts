/**
 * Window-level keyboard shortcuts, as a pure resolver so the map can be pinned
 * without a browser (see shortcuts.check.ts). App.tsx is the only caller: it
 * turns a resolved action into the same state change the button would make.
 *
 * Cmd on macOS is accepted anywhere Ctrl is, because every shortcut here is a
 * modifier chord rather than a text-entering key.
 */
export type Shortcut =
  | "new-chat"
  | "close-tab"
  | "rename-chat"
  | "toggle-files"
  | "open-memory"
  | "open-settings"
  | "command-palette"
  | "cheatsheet"
  | "toggle-split"
  | "focus-pane-0"
  | "focus-pane-1"
  | "jump-to-latest"
  | "dismiss";

/**
 * The chords written the way a person would say them, for the settings screen.
 *
 * Kept beside the resolver so the two cannot disagree about which chords exist,
 * and typed against the same union so adding one to the resolver and forgetting
 * it here is a compile error rather than a silent gap. shortcuts.check.ts pins
 * that this table covers every action.
 */
export const SHORTCUTS: ReadonlyArray<{ keys: string; action: Shortcut; label: string }> = [
  { keys: "Ctrl+N", action: "new-chat", label: "New chat" },
  { keys: "Ctrl+W", action: "close-tab", label: "Close the tab" },
  { keys: "F2", action: "rename-chat", label: "Rename the open chat" },
  { keys: "Ctrl+B", action: "toggle-files", label: "Show or hide the file tree" },
  { keys: "Ctrl+M", action: "open-memory", label: "Open the memory settings" },
  { keys: "Ctrl+,", action: "open-settings", label: "Open settings" },
  { keys: "Ctrl+K", action: "command-palette", label: "Open the command palette" },
  { keys: "?", action: "cheatsheet", label: "Show this list" },
  { keys: "Ctrl+\\", action: "toggle-split", label: "Split the view" },
  { keys: "Ctrl+1", action: "focus-pane-0", label: "Focus the left pane" },
  { keys: "Ctrl+2", action: "focus-pane-1", label: "Focus the right pane" },
  { keys: "Ctrl+G", action: "jump-to-latest", label: "Jump to the newest message" },
  { keys: "Esc", action: "dismiss", label: "Close the panel on top" },
];

/** The slice of KeyboardEvent the resolver needs — keeps it testable. */
export interface KeyPress {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
}

/**
 * What a keypress means, or null when it means nothing to us.
 *
 * `typing` is true while a text field has focus. Modifier chords still fire
 * there (Ctrl+N in the composer should start a chat), but bare Escape does not:
 * the field owns it — closing the slash menu, cancelling an edit — and a global
 * handler must not steal it.
 */
export function resolveShortcut(e: KeyPress, typing = false): Shortcut | null {
  const mod = e.ctrlKey || e.metaKey;
  if (!mod) {
    if (typing || e.altKey) return null;
    if (e.key === "Escape") return e.shiftKey ? null : "dismiss";
    // `?` is Shift+/ on most layouts, so it arrives as the character itself
    // rather than as a shifted slash — the shift flag is not the thing to test.
    if (e.key === "?") return "cheatsheet";
    // F2 is the conventional rename key and means nothing inside a text field,
    // so it is safe as a bare key — and the `typing` guard above keeps it out of
    // the rename box itself, which is exactly where it would be most annoying.
    if (e.key === "F2") return "rename-chat";
    return null;
  }
  // AltGr arrives as Ctrl+Alt on some layouts; never treat it as our chord.
  if (e.altKey) return null;
  switch (e.key.toLowerCase()) {
    case "n":
      return e.shiftKey ? null : "new-chat";
    case "w":
      return e.shiftKey ? null : "close-tab";
    case "b":
      return e.shiftKey ? null : "toggle-files";
    case "m":
      return e.shiftKey ? null : "open-memory";
    case ",":
      return e.shiftKey ? null : "open-settings";
    case "k":
      return e.shiftKey ? null : "command-palette";
    case "\\":
      return e.shiftKey ? null : "toggle-split";
    case "1":
      return e.shiftKey ? null : "focus-pane-0";
    case "2":
      return e.shiftKey ? null : "focus-pane-1";
    // Ctrl+G rather than Ctrl+End or Ctrl+Down, which are the conventional
    // "go to the end" chords: a modifier chord fires while the composer has
    // focus, and those two move its caret, so claiming them would take a
    // keystroke the draft owns. Ctrl+G means nothing inside a textarea.
    case "g":
      return e.shiftKey ? null : "jump-to-latest";
    default:
      return null;
  }
}
