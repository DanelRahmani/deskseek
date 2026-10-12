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
 * Filter and rank. Every whitespace-separated term must appear somewhere in the
 * item, so a second word narrows rather than widens. A label that *starts with*
 * the query outranks one that merely contains it, and a label match outranks one
 * that only came from the hint. Ties keep the input order, which is why the sort
 * has to be stable.
 */
export function matchPalette(items: readonly PaletteItem[], query: string): PaletteItem[] {
  const terms = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  if (terms.length === 0) return items.slice();

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
