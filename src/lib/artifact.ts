/**
 * How an artifact becomes a frame on screen. Pure string work, so the two
 * halves of the sandbox — the attribute and the policy — are pinned by
 * artifact.check.ts instead of being trusted to the component.
 */

/**
 * No `allow-same-origin`. That is the load-bearing omission: without it the
 * frame gets a unique opaque origin, so an artifact cannot reach the app's DOM,
 * its storage, or the Tauri APIs behind it. Scripts are allowed only inside the
 * frame, which is what a self-contained page needs.
 */
export const SANDBOX = "allow-scripts allow-forms";

/** A self-contained page: inline script and style, but no network at all. */
const HTML_CSP =
  "default-src 'none'; img-src data:; style-src 'unsafe-inline'; font-src data:; script-src 'unsafe-inline'";

/** A picture: styles and data images, but no scripts and no network. */
const STATIC_CSP =
  "default-src 'none'; img-src data:; style-src 'unsafe-inline'; font-src data:; script-src 'none'";

const SHELL_HEAD =
  '<meta charset="utf-8"><style>html,body{margin:0;padding:16px;background:#fff;color:#111;' +
  "font:14px system-ui,sans-serif}svg{max-width:100%;height:auto}</style>";

const meta = (policy: string) =>
  `<meta http-equiv="Content-Security-Policy" content="${policy}">`;

const DOCTYPE = /^\s*<!doctype[^>]*>/i;

/**
 * Put the policy on an HTML artifact. The meta is inserted after the doctype
 * when there is one — a meta before it would drop the page into quirks mode —
 * and prepended otherwise, which the browser hoists into the head. An artifact
 * is not obliged to have a `<head>` at all.
 */
export function htmlDoc(html: string): string {
  const found = DOCTYPE.exec(html);
  if (!found) return meta(HTML_CSP) + html;
  return html.slice(0, found[0].length) + meta(HTML_CSP) + html.slice(found[0].length);
}

/**
 * The `srcdoc` for an artifact. HTML passes through — its own styles and
 * scripts are the point — while SVG and Mermaid are wrapped in a minimal page
 * so the diagram is centred on white rather than on the app's dark background.
 */
export function frameDoc(kind: "html" | "svg" | "mermaid", content: string): string {
  if (kind === "html") return htmlDoc(content);
  return `<!doctype html><html><head>${meta(STATIC_CSP)}${SHELL_HEAD}</head><body>${content}</body></html>`;
}

/** What the panel says it is showing. */
export const KIND_LABELS: Record<string, string> = {
  html: "Page",
  svg: "SVG",
  mermaid: "Diagram",
  chart: "Chart",
  markdown: "Note",
  code: "Code",
};

/**
 * Characters of context sent either side of a selection when revising it, so the
 * model can see what the sentence is part of without being handed the document.
 */
export const REVISE_WINDOW = 240;

/** The three pieces of a revision request. */
export interface SelectionWindow {
  before: string;
  selection: string;
  after: string;
}

/**
 * A selection start/end that is inside `content` and the right way round.
 *
 * Offsets come from a DOM selection, which means they arrive from outside this
 * module and can be stale: the document was replaced, the selection was never
 * updated, or a number arrived that is not a number. Clamping here is what stops
 * that from throwing or from patching the wrong place, and it is the reason both
 * functions below go through this rather than trusting their arguments.
 */
function clampRange(content: string, start: number, end: number): [number, number] {
  const limit = content.length;
  const inside = (at: number, fallback: number) =>
    Number.isFinite(at) ? Math.min(Math.max(Math.trunc(at), 0), limit) : fallback;
  const from = inside(start, 0);
  const to = inside(end, from);
  return from <= to ? [from, to] : [to, from];
}

/**
 * The selection, plus a little of what surrounds it — never the whole document.
 *
 * Only a window travels: the model is rewriting one sentence, and sending the
 * rest of the document would invite it to rewrite the rest of the document.
 */
export function windowAround(
  content: string,
  start: number,
  end: number,
  radius: number = REVISE_WINDOW,
): SelectionWindow {
  const [from, to] = clampRange(content, start, end);
  return {
    before: content.slice(Math.max(0, from - radius), from),
    selection: content.slice(from, to),
    after: content.slice(to, Math.min(content.length, to + radius)),
  };
}

/**
 * Replaces one range of an artifact's text with new text.
 *
 * By offsets rather than by searching for the text: a sentence that appears twice
 * is exactly the case a search gets wrong, and the selection already knows which
 * one was meant. The range is clamped the same way `windowAround` clamps it.
 */
export function patchRange(
  content: string,
  start: number,
  end: number,
  replacement: string,
): string {
  const [from, to] = clampRange(content, start, end);
  return content.slice(0, from) + replacement + content.slice(to);
}
