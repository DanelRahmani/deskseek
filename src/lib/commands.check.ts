/**
 * Checks for the command palette's pure parts. Plain Node, no test framework:
 *
 *   npm run check
 *
 * A palette that hides a command the user typed — or ranks a worse match first —
 * fails silently: the list simply looks wrong, and nothing says why.
 */
import { buildPalette, composerOffer, matchMentions, matchPalette } from "./commands.ts";

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

// The composer's own menus. A `/` command owns the whole box; `@` and `$` are
// mentions inside a sentence, so only the word being typed has to be one.
const offer = (draft: string) => {
  const found = composerOffer(draft);
  if (!found) throw new Error(`FAIL: ${JSON.stringify(draft)} should offer a menu`);
  return found;
};
assert(offer("/").scope === "command", "a lone slash opens the commands");
assert(offer("/p:stand").query === "/p:stand", "and is filtered on what follows it");
assert(composerOffer("/export now") === null, "a command with a space in it is text");
assert(offer("read @App.tsx").scope === "file", "a mention works mid-sentence");
assert(offer("read @App.tsx").query === "App.tsx", "and is filtered on the name");
assert(offer("@").scope === "file", "a bare marker lists its whole scope");
assert(offer("@").query === "", "which filters on nothing, so everything is in it");
assert(offer("$deploy").scope === "skill", "a dollar is a skill");
// The marker nearest the cursor is the one being typed.
assert(offer("@a $b").scope === "skill", "the last marker wins");
assert(offer("@a $b").query === "b", "and it is filtered on itself");
// A marker that is not one of ours is ordinary text, in the composer as in the
// palette: a menu that opens on a guess is worse than no menu.
assert(composerOffer("!files") === null, "an unknown marker opens nothing");
assert(composerOffer("read #note") === null, "nor does one mid-sentence");
assert(composerOffer("what is 2 + 2") === null, "and plain text is plain text");

// The two mention menus are the palette's ranking over a list the caller built.
const fileItems = [
  { id: "src/App.tsx", label: "src/App.tsx", hint: "File" },
  { id: "src/lib/app.test.ts", label: "src/lib/app.test.ts", hint: "File" },
  { id: "src/lib", label: "src/lib", hint: "Folder" },
];
assert(matchMentions(fileItems, "").length === 3, "a bare @ lists the whole scope");
assert(matchMentions(fileItems, "app")[0].id === "src/App.tsx", "a file is found by name");
assert(matchMentions(fileItems, "lib").some((i) => i.id === "src/lib"), "a folder too");
assert(matchMentions(fileItems, "zzz").length === 0, "a name that is not there is not offered");
// The path is what is searched, so a file is reachable from a word in its folder.
assert(matchMentions(fileItems, "src/app")[0].id === "src/App.tsx", "part of a path finds it");
// A path that begins with the query outranks one that merely contains it.
const rankedFiles = matchMentions(
  [
    { id: "docs/readme-old.md", label: "docs/readme-old.md", hint: "File" },
    { id: "README.md", label: "README.md", hint: "File" },
  ],
  "readme",
);
assert(rankedFiles[0].id === "README.md", "the path that starts with the query comes first");
// A skill is searched by its id and by the name it goes by.
const skillItems = [{ id: "deploy-notes", label: "deploy-notes", hint: "Deploy notes — the release checklist" }];
assert(matchMentions(skillItems, "deploy-no")[0].id === "deploy-notes", "a skill is found by its id");
assert(matchMentions(skillItems, "checklist")[0].id === "deploy-notes", "and by what it is for");

// A mention fills the box rather than sending it. The caller can tell the two
// apart because the scope is not `command` — the rule the prompt library set for
// `/p:`, which fills the box for the same reason.
const fileOffer = offer("@App");
assert(fileOffer.scope !== "command", "a mention is not a command, so nothing is sent");
assert(matchMentions(fileItems, fileOffer.query)[0].id === "src/App.tsx", "and resolves to a path");

console.log("commands.check.ts: all checks passed");
