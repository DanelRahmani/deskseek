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
  | "toggle-files"
  | "open-memory"
  | "open-settings"
  | "toggle-split"
  | "focus-pane-0"
  | "focus-pane-1"
  | "dismiss";

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
    return e.key === "Escape" && !typing && !e.altKey && !e.shiftKey ? "dismiss" : null;
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
    case "\\":
      return e.shiftKey ? null : "toggle-split";
    case "1":
      return e.shiftKey ? null : "focus-pane-0";
    case "2":
      return e.shiftKey ? null : "focus-pane-1";
    default:
      return null;
  }
}
