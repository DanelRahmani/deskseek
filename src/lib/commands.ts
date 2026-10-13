/**
 * The command palette's contents and its matching rule, kept pure so both can be
 * pinned without a browser (see commands.check.ts). App.tsx builds the list and
 * turns the chosen id back into the same state change its button would make.
 */

export interface PaletteItem {
  id: string;
  label: string;
  hint: string;
}

/**
 * Statics first, then one entry per chat. Chat entries carry `chat:<id>`, so the
 * caller can tell a navigation apart from an action without a second lookup.
 */
export function buildPalette(
  chats: ReadonlyArray<{ id: string; title: string; kind: string }>,
  hasChat: boolean,
  activeArchived = false,
): PaletteItem[] {
  const items: PaletteItem[] = [
    { id: "new-chat", label: "New chat", hint: "Action" },
    { id: "settings", label: "Settings", hint: "Action" },
    { id: "memory", label: "Memory", hint: "Action" },
    {
      id: "customise",
      label: "Customise",
      hint: "Skills, connectors, instructions, design systems",
    },
    { id: "projects", label: "Projects", hint: "Grouped chats with one folder" },
    { id: "artifacts", label: "Artifacts", hint: "Every artifact, from every chat" },
    { id: "scheduled", label: "Scheduled tasks", hint: "Recurring prompts on a calendar" },
    { id: "spend", label: "Spend", hint: "What the last turns cost, by day and model" },
    { id: "files", label: "Show or hide the file tree", hint: "Action" },
    { id: "split", label: "Split into two panes", hint: "Action" },
    { id: "backup", label: "Back up the whole profile", hint: "Action" },
    { id: "restore", label: "Restore a backup over this profile…", hint: "Action" },
    { id: "maintain", label: "Maintain the database", hint: "Action" },
    { id: "import-chat", label: "Import a conversation from JSON…", hint: "Action" },
  ];
  // Exporting needs something to export; undoing needs something to undo, which
  // is also only true once a chat is open.
  if (hasChat) {
    items.push({ id: "export", label: "Export this chat to Markdown", hint: "Action" });
    items.push({ id: "export-pdf", label: "Export this chat as a PDF", hint: "Action" });
    items.push({ id: "export-json", label: "Export this chat as JSON", hint: "Action" });
    items.push({ id: "duplicate", label: "Duplicate this chat", hint: "Action" });
    items.push({ id: "rewind", label: "Undo the agent's last file changes", hint: "Action" });
    // One entry either way, never both: from inside an archived chat the only
    // useful move is back out of the archive.
    items.push(
      activeArchived
        ? { id: "unarchive", label: "Take this chat out of the archive", hint: "Action" }
        : { id: "archive", label: "Archive this chat", hint: "Action" },
    );
  }
  for (const chat of chats) {
    items.push({
      id: `chat:${chat.id}`,
      label: chat.title,
      hint: chat.kind === "coding" ? "Coding" : "Chat",
    });
  }
  return items;
}

/**
 * A leading `>` or `@` scopes the search to one half of the list, and the halves
 * are the ones the ids already name: anything that is not `chat:<id>` is an
 * action, and every chat entry is one. Typing the marker on its own lists the
 * whole scope, which is how a user finds out what is in there.
 */
function inScope(marker: string, item: PaletteItem): boolean {
  return marker === ">" ? !item.id.startsWith("chat:") : item.id.startsWith("chat:");
}

/**
 * Filter and rank. Every whitespace-separated term must appear somewhere in the
 * item, so a second word narrows rather than widens. A label that *starts with*
 * the query outranks one that merely contains it, and a label match outranks one
 * that only came from the hint. Ties keep the input order, which is why the sort
 * has to be stable.
 */
function rank(items: readonly PaletteItem[], terms: readonly string[]): PaletteItem[] {
  const scored: { item: PaletteItem; score: number }[] = [];
  for (const item of items) {
    const label = item.label.toLowerCase();
    const haystack = `${label} ${item.hint.toLowerCase()}`;
    if (!terms.every((term) => haystack.includes(term))) continue;
    const first = terms[0];
    const score = label.startsWith(first) ? 0 : label.includes(first) ? 1 : 2;
    scored.push({ item, score });
  }
  scored.sort((a, b) => a.score - b.score);
  return scored.map((entry) => entry.item);
}

export function matchPalette(items: readonly PaletteItem[], query: string): PaletteItem[] {
  const trimmed = query.trim();
  const marker = trimmed.slice(0, 1);
  const scoped = marker === ">" || marker === "@";
  const rest = scoped ? trimmed.slice(1) : trimmed;
  const terms = rest.toLowerCase().split(/\s+/).filter(Boolean);
  // A bare marker lists its whole scope; an empty query lists everything.
  if (terms.length === 0) {
    return scoped ? items.filter((item) => inScope(marker, item)) : items.slice();
  }
  return rank(
    scoped ? items.filter((item) => inScope(marker, item)) : items,
    terms,
  );
}

/** What the composer is offering: which menu, filtered on what. */
export type ComposerOffer = { scope: "command" | "file" | "skill"; query: string };

/**
 * The menu the composer should show, or null for none.
 *
 * A `/` is a command and owns the whole box: it starts the text and no space has
 * been typed yet, which is the rule the prompt library already set. `@` and `$`
 * are mentions *inside* a sentence, so only the word being typed has to be one —
 * that is what keeps the list up while `read @App.tsx and …` is written.
 *
 * A marker nobody knows (`!`, `#`) is ordinary text and opens nothing, the same
 * rule `matchPalette` keeps for a marker that is not `>` or `@`: a menu that
 * appears on a guess is worse than no menu.
 */
export function composerOffer(draft: string): ComposerOffer | null {
  if (draft.startsWith("/") && !draft.includes(" ")) {
    return { scope: "command", query: draft.toLowerCase() };
  }
  const word = draft.slice(draft.lastIndexOf(" ") + 1);
  if (word.startsWith("@")) return { scope: "file", query: word.slice(1) };
  if (word.startsWith("$")) return { scope: "skill", query: word.slice(1) };
  return null;
}

/**
 * The `@` and `$` menus, over the list the caller built for that scope: a file's
 * label is its path, so any part of it finds the file, and a skill's label is its
 * id, so the name it goes by finds it too. The id is what gets inserted, as it is
 * for a chat entry in the palette. A bare marker lists the whole scope, which is
 * how a user finds out what is in there.
 */
export function matchMentions(items: readonly PaletteItem[], query: string): PaletteItem[] {
  const terms = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  return terms.length === 0 ? items.slice() : rank(items, terms);
}
