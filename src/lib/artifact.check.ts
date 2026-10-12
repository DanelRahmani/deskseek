/**
 * Checks for the artifact frame in artifact.ts. Plain Node, no test framework:
 *
 *   npm run check
 *
 * The frame is the only place an artifact's own markup is trusted, so the
 * assertions here are about the policy, not about the rendering.
 */
import { frameDoc, htmlDoc, KIND_LABELS, SANDBOX } from "./artifact.ts";

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
console.log("artifact.check.ts: all checks passed");

