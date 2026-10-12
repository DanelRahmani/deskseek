/**
 * Checks for the transcript grouping in transcript.ts. Plain Node, no test
 * framework:
 *
 *   npm run check
 *
 * The pairing is easy to get wrong by inspection and invisible to the type
 * checker: a result attached to the wrong call draws a plausible card with the
 * wrong output, and a live run that is also stored draws the same step twice.
 * The window is checked the same way — an off-by-one there loses a turn rather
 * than drawing a wrong one, and it does it on exactly the long chats nobody
 * scrolls to the top of.
 */
import { buildTranscript, windowTranscript, TRANSCRIPT_WINDOW } from "./transcript.ts";
import type { MessageRow, ToolRun } from "../types";

function assert(ok: boolean, message: string): void {
  if (!ok) throw new Error(`FAIL: ${message}`);
}

let seq = 0;
const msg = (role: MessageRow["role"], over: Partial<MessageRow> = {}): MessageRow => ({
  id: `m${(seq += 1)}`,
  chat_id: "c1",
  seq,
  role,
  content: null,
  reasoning_content: null,
  tool_calls_json: null,
  tool_call_id: null,
  attachments_json: null,
  created_at: 0,
  ...over,
});

const calls = (...entries: [string, string, unknown][]) =>
  JSON.stringify(
    entries.map(([id, name, args]) => ({ id, function: { name, arguments: JSON.stringify(args) } })),
  );

const run = (id: string, tool: string, over: Partial<ToolRun> = {}): ToolRun => ({
  id,
  tool,
  args: {},
  status: "ok",
  ...over,
});

// A tool turn and its results fold into one group, pairing by call id.
const grouped = buildTranscript(
  [
    msg("user", { content: "read the files" }),
    msg("assistant", { tool_calls_json: calls(["a", "read_file", { path: "x" }], ["b", "read_file", { path: "y" }]) }),
    msg("tool", { tool_call_id: "a", content: "x contents" }),
    msg("tool", { tool_call_id: "b", content: "y contents" }),
    msg("assistant", { content: "Done." }),
  ],
  [],
);
assert(grouped.length === 3, `expected user, one group, answer — got ${grouped.length}`);
assert(grouped[0].kind === "message", "the user turn stays a message");
assert(grouped[1].kind === "steps", "the tool turn becomes a group");
assert(grouped[2].kind === "message", "the closing prose stays a message");
if (grouped[1].kind === "steps") {
  const steps = grouped[1].steps;
  assert(steps.length === 2, "both calls are in the group");
  assert(steps[0].tool === "read_file", "the tool name comes from the call row");
  assert(steps[0].output === "x contents", "the result is paired by call id, not by order");
  assert(steps[1].output === "y contents", "the second result pairs with the second call");
}

// A plain message between two tool turns splits them into two groups.
const split = buildTranscript(
  [
    msg("assistant", { tool_calls_json: calls(["a", "grep", {}]) }),
    msg("tool", { tool_call_id: "a", content: "hit" }),
    msg("assistant", { content: "Here is what I found." }),
    msg("assistant", { tool_calls_json: calls(["b", "read_file", {}]) }),
    msg("tool", { tool_call_id: "b", content: "body" }),
  ],
  [],
);
assert(
  split.filter((i) => i.kind === "steps").length === 2,
  "prose breaks a tool run into two groups",
);

// A stored failure is detected from the `Error: ` prefix the loop writes.
const failed = buildTranscript(
  [
    msg("assistant", { tool_calls_json: calls(["a", "run_command", {}]) }),
    msg("tool", { tool_call_id: "a", content: "Error: command not found" }),
  ],
  [],
);
if (failed[0].kind === "steps") {
  assert(failed[0].steps[0].status === "error", "a stored failure is not drawn as a success");
} else {
  throw new Error("FAIL: expected a group for the failed call");
}

// A result with no matching call row still shows, so grouping hides nothing.
const orphan = buildTranscript([msg("tool", { tool_call_id: "z", content: "loose" })], []);
assert(orphan[0].kind === "steps" && orphan[0].steps[0].output === "loose", "an orphan result is kept");

// A live run with no stored row yet is appended as its own group.
const live = buildTranscript(
  [msg("user", { content: "go" })],
  [run("a", "read_file", { status: "running" })],
);
assert(live.length === 2 && live[1].kind === "steps", "a live run is appended");
if (live[1].kind === "steps") {
  assert(live[1].steps[0].status === "running", "the live status is kept");
}

// A run that is both stored and live is updated in place, never drawn twice.
const both = buildTranscript(
  [
    msg("assistant", { tool_calls_json: calls(["a", "read_file", {}]) }),
    msg("tool", { tool_call_id: "a", content: "stored" }),
  ],
  [run("a", "read_file", { status: "ok", output: "fresh", shot: "data:image/png;base64,AA" })],
);
const groups = both.filter((i) => i.kind === "steps");
assert(groups.length === 1, "a stored + live call is one group, not two");
if (groups[0].kind === "steps") {
  assert(groups[0].steps.length === 1, "the call appears once");
  assert(groups[0].steps[0].output === "fresh", "the live run's richer data wins");
  assert(!!groups[0].steps[0].shot, "the screenshot survives the merge");
}

// No tools and no tool turns is just the messages, untouched.
const plain = buildTranscript([msg("user", { content: "hi" }), msg("assistant", { content: "hello" })], []);
assert(plain.length === 2 && plain.every((i) => i.kind === "message"), "a plain chat is unchanged");

// An empty transcript is empty, not a phantom group.
assert(buildTranscript([], []).length === 0, "nothing in, nothing out");

// A fabricated 500-message chat: 100 turns, each a question, two tool calls, a
// result each, and an answer. This is the fixture the window is sized against.
const long: MessageRow[] = [];
for (let turn = 0; turn < 100; turn += 1) {
  long.push(msg("user", { content: `question ${turn}` }));
  long.push(msg("assistant", { tool_calls_json: calls([`a${turn}`, "read_file", {}], [`b${turn}`, "grep", {}]) }));
  long.push(msg("tool", { tool_call_id: `a${turn}`, content: "contents" }));
  long.push(msg("tool", { tool_call_id: `b${turn}`, content: "hit" }));
  long.push(msg("assistant", { content: `answer ${turn}` }));
}
assert(long.length === 500, `the fixture is 500 messages, got ${long.length}`);
const fitted = buildTranscript(long, []);
assert(
  fitted.length === 300,
  `100 turns fold into 300 items (question, group, answer), got ${fitted.length}`,
);

// The window keeps the newest end — the part anyone is reading — and counts
// what it left out. `hidden + shown` is the whole thing, for every limit: an
// off-by-one here silently drops a turn with nothing on screen to say so.
const last = fitted[fitted.length - 1];
for (let limit = 1; limit <= fitted.length; limit += 1) {
  const { shown, hidden } = windowTranscript(fitted, limit);
  assert(hidden + shown.length === fitted.length, `limit ${limit} loses or duplicates items`);
  assert(shown[shown.length - 1] === last, `limit ${limit} drops the newest item`);
  assert(shown.length === Math.min(limit, fitted.length), `limit ${limit} shows the wrong count`);
}
const windowed = windowTranscript(fitted, TRANSCRIPT_WINDOW);
assert(windowed.shown.length === TRANSCRIPT_WINDOW, "the default window shows one screen of items");
assert(windowed.hidden === 300 - TRANSCRIPT_WINDOW, "the rest are counted as hidden");
assert(
  windowed.shown[0] === fitted[fitted.length - TRANSCRIPT_WINDOW],
  "the window starts exactly at the cut, with no gap",
);

// Revealing is monotonic: each step shows strictly more, and the newest item
// never moves. This is what keeps a scroll position sane across a reveal.
let limit = TRANSCRIPT_WINDOW;
let previous = windowed.shown.length;
while (limit < fitted.length) {
  const next = windowTranscript(fitted, (limit += TRANSCRIPT_WINDOW));
  assert(next.shown.length > previous, "a reveal must add items");
  assert(
    next.shown[next.shown.length - 1] === last && next.shown.includes(windowed.shown[0]),
    "a reveal adds above the reader, never re-slices the tail",
  );
  previous = next.shown.length;
}
assert(
  windowTranscript(fitted, fitted.length).hidden === 0,
  "once everything is revealed nothing is hidden",
);

console.log("transcript.check.ts: all checks passed");
