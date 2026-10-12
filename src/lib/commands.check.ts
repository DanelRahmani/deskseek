/**
 * Checks for the command palette's pure parts. Plain Node, no test framework:
 *
 *   npm run check
 *
 * A palette that hides a command the user typed — or ranks a worse match first —
 * fails silently: the list simply looks wrong, and nothing says why.
 */
import { buildPalette, matchPalette } from "./commands.ts";

function assert(ok: boolean, message: string): void {
  if (!ok) throw new Error(`FAIL: ${message}`);
}

const chats = [
  { id: "c1", title: "Quarterly report", kind: "chat" },
  { id: "c2", title: "Fix the parser", kind: "coding" },
];
const items = buildPalette(chats, true);

// Export needs a chat; the statics never do.
assert(items.some((i) => i.id === "export"), "export is offered while a chat is open");

// A leading marker scopes the list to one half of it: `>` the actions, `@` the
// conversations. Either marker on its own still lists its whole scope.
const actionsOnly = matchPalette(items, ">");
assert(actionsOnly.length > 0, "> lists the actions");
assert(!actionsOnly.some((i) => i.id.startsWith("chat:")), "> hides the conversations");
const chatsOnly = matchPalette(items, "@");
assert(chatsOnly.length === chats.length, "@ lists exactly the conversations");
assert(chatsOnly.every((i) => i.id.startsWith("chat:")), "@ hides the actions");
assert(matchPalette(items, "> new").some((i) => i.id === "new-chat"), "> still searches inside the scope");
assert(matchPalette(items, "> Quarterly").length === 0, "an action scope excludes a chat title");
assert(matchPalette(items, "@ Quarterly").length === 1, "@ finds the chat that matches");
// A marker that is not one of ours is part of the query, not a scope.
assert(matchPalette(items, "?x").length === 0, "an unknown marker is just text");
assert(!buildPalette(chats, false).some((i) => i.id === "export"), "export is hidden with no chat");

// Archiving is offered either way round, but never both at once, and never with
// no chat open — there would be nothing to archive.
assert(items.some((i) => i.id === "archive"), "an open chat can be archived");
assert(
  !items.some((i) => i.id === "unarchive"),
  "a chat that is not archived is not offered an unarchive",
);
const inArchive = buildPalette(chats, true, true);
assert(inArchive.some((i) => i.id === "unarchive"), "an archived chat can be taken back out");
assert(!inArchive.some((i) => i.id === "archive"), "and is not offered archiving again");
assert(
  !buildPalette(chats, false, true).some((i) => i.id === "unarchive"),
  "no chat open means no archive action",
);

// One entry per chat, in the order given, tagged so the caller can route it.
const chatItems = items.filter((i) => i.id.startsWith("chat:"));
assert(chatItems.length === 2, "every chat is listed");
assert(chatItems[0].id === "chat:c1", "chats keep their order");
assert(chatItems[1].hint === "Coding", "a coding chat says so");

// Finding.
assert(matchPalette(items, "").length === items.length, "an empty query shows everything");
assert(matchPalette(items, "   ").length === items.length, "whitespace is not a query");
assert(matchPalette(items, "SETTINGS")[0].id === "settings", "matching ignores case");
assert(matchPalette(items, "quarterly")[0].id === "chat:c1", "a chat is found by its title");
assert(matchPalette(items, "parser")[0].id === "chat:c2", "and by another word in it");

// Every term has to match, so a second word narrows.
assert(matchPalette(items, "export markdown")[0].id === "export", "all terms may match");
assert(matchPalette(items, "export zzz").length === 0, "a missing term excludes the item");

// Ranking: a label that begins with the query beats one that merely contains it.
const ranked = matchPalette(
  [
    { id: "a", label: "Settings", hint: "" },
    { id: "b", label: "Open settings", hint: "" },
  ],
  "settings",
);
assert(ranked[0].id === "a" && ranked[1].id === "b", "a prefix match outranks a buried one");

// A hint is searchable...
const byHint = matchPalette(items, "action").map((i) => i.id);
assert(byHint.includes("settings"), "a hint is searchable");
assert(!byHint.includes("chat:c1"), "an Action hint does not drag chats in");
// ...but a label match still outranks a hint-only match.
const mixed = matchPalette(
  [
    { id: "hint", label: "Zzz", hint: "settings" },
    { id: "label", label: "Settings", hint: "zzz" },
  ],
  "settings",
);
assert(mixed[0].id === "label", "a label match outranks a hint match");

// No match is an empty list, not everything.
assert(matchPalette(items, "zzzznope").length === 0, "no match means nothing");

console.log("commands.check.ts: all checks passed");
