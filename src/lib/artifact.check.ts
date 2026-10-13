/**
 * Checks for the artifact frame in artifact.ts. Plain Node, no test framework:
 *
 *   npm run check
 *
 * The frame is the only place an artifact's own markup is trusted, so the
 * assertions here are about the policy, not about the rendering.
 */
import { frameDoc, htmlDoc, KIND_LABELS, patchRange, SANDBOX, windowAround } from "./artifact.ts";

function ok(condition: boolean, message: string): void {
  if (!condition) throw new Error(`FAIL: ${message}`);
}

// The security-relevant half: a frame that could reach the app would be a hole,
// so the missing flag is asserted rather than assumed.
ok(!SANDBOX.includes("allow-same-origin"), "the frame must not share the app's origin");
ok(SANDBOX.includes("allow-scripts"), "an artifact page needs its own scripts");

// A page keeps its own markup, and the policy lands ahead of it.
const page = "<!doctype html><html><body><h1>Hi</h1></body></html>";
const framed = htmlDoc(page);
ok(framed.includes("Content-Security-Policy"), "the page needs a policy");
ok(framed.includes("<h1>Hi</h1>"), "the page's own markup must survive");
ok(framed.includes("script-src 'unsafe-inline'"), "a page may run its own scripts");
ok(framed.includes("default-src 'none'"), "nothing may reach the network");

// Quirks mode makes a real page render wrong, so the doctype stays first.
ok(
  framed.startsWith("<!doctype html>"),
  `meta must not precede the doctype: ${framed.slice(0, 40)}`,
);
// A fragment has no doctype to protect; the meta simply leads.
ok(htmlDoc("<p>frag</p>").startsWith("<meta "), "a fragment gets the meta up front");

// Diagrams get a document of ours, with scripts switched off entirely.
for (const kind of ["svg", "mermaid"] as const) {
  const doc = frameDoc(kind, "<svg><circle r='4'/></svg>");
  ok(doc.includes("<svg><circle r='4'/></svg>"), `${kind} markup must be embedded verbatim`);
  ok(doc.includes("script-src 'none'"), `${kind} must not script`);
  ok(doc.startsWith("<!doctype html>"), `${kind} needs a standards-mode document`);
  ok(doc.includes("background:#fff"), `${kind} needs a light background to be readable`);
}

// Every kind the model may send has a label, so the header never shows a raw id.
for (const kind of ["html", "svg", "mermaid", "chart", "markdown", "code"]) {
  ok(!!KIND_LABELS[kind], `${kind} needs a label`);
}

// --- editing one sentence in place ------------------------------------------

// The patch takes the selection's *offsets*. The same words appearing twice is
// the case a search by text gets wrong, so it is the case asserted first.
const doc = "Alpha Beta gamma. Beta again.";
const first = doc.indexOf("Beta");
const second = doc.indexOf("Beta", first + 1);
ok(first !== second && second >= 0, "the fixture needs the word twice");
ok(
  patchRange(doc, first, first + 4, "Delta") === "Alpha Delta gamma. Beta again.",
  "patching the first of two identical words must touch only that one",
);
ok(
  patchRange(doc, second, second + 4, "Delta") === "Alpha Beta gamma. Delta again.",
  "patching the second must touch only that one",
);

// A selection at the start, in the middle and running to the end.
ok(patchRange(doc, 0, 5, "Omega") === "Omega Beta gamma. Beta again.", "a selection at the start");
ok(
  patchRange(doc, first, first + 4, "delta") === "Alpha delta gamma. Beta again.",
  "a selection in the middle",
);
ok(
  patchRange(doc, second, doc.length, "and done.") === "Alpha Beta gamma. and done.",
  "a selection running to the end",
);

// An empty range inserts, an empty replacement deletes.
ok(patchRange("ab", 1, 1, "X") === "aXb", "an empty selection inserts");
ok(patchRange("abc", 1, 2, "") === "ac", "an empty replacement deletes");

// Offsets come from the DOM, so a stale or nonsense pair is clamped rather than
// thrown — and it must land somewhere defined, not somewhere random.
ok(patchRange("abc", 2, 0, "X") === "Xc", "a reversed range is put the right way round");
ok(patchRange("abc", -5, 99, "X") === "X", "out of range clamps to the whole string");
ok(patchRange("abc", Number.NaN, Number.NaN, "X") === "Xabc", "NaN clamps to the start");
ok(patchRange("abc", 1.7, 2.2, "X") === "aXc", "a fractional offset is truncated");

// What travels to the model: the selection, and only the radius of each side —
// never the whole document.
const long = "x".repeat(500) + "SELECTED" + "y".repeat(500);
const win = windowAround(long, 500, 508, 10);
ok(win.selection === "SELECTED", "the selection itself travels");
ok(win.before === "x".repeat(10), `only the window before it: ${win.before.length}`);
ok(win.after === "y".repeat(10), `only the window after it: ${win.after.length}`);
// The window is clamped at the ends of the document, so the request is always
// three strings rather than one that runs off the edge.
const atStart = windowAround("hi", 0, 2, 10);
ok(atStart.before === "" && atStart.selection === "hi" && atStart.after === "", "a whole-document selection");
ok(
  windowAround(long, -3, 9999, 10).selection.length === long.length,
  "a stale pair still describes something real",
);

console.log("artifact.check.ts: all checks passed");

