/**
 * Checks for the accessibility rules the app claims to follow. Plain Node, no
 * test framework:
 *
 *   npm run check
 *
 * A Node script cannot drive a screen reader or press Tab, so what is checked
 * here is the *structure* those things depend on, in the four places where this
 * app had drifted and where a regression would be silent:
 *
 *  - a full-screen overlay is a dialog with a name, or it is nothing to a screen
 *    reader — the app used to open four of them and announce a plain div;
 *  - the file tree and the transcript keep the roles that say what they are;
 *  - `outline-none` is banned outright, because `outline-hidden` is the Tailwind
 *    v4 spelling that survives Windows High Contrast Mode: both remove the ring,
 *    but only one puts it back as a system-coloured outline for the people who
 *    have asked the OS for high contrast. Every control in this app used to
 *    vanish the moment it was focused there;
 *  - a button whose whole content is an icon or an emoji has a name of its own.
 *
 * What this cannot check is announced in the file rather than pretended away:
 * there is no focus *trap* on the big dialogs, so Tab can still leave one for
 * the page behind it — `aria-modal` says otherwise, and that is this app's
 * standing known gap. The upgrade is one attribute: `inert` on the app's own
 * content while a dialog is open (React 19 and the webviews this ships in both
 * support it), which needs the content in one wrapper element first.
 *
 * The tree's *behaviour* is not here either — a component needs a browser. It is
 * in `tree.check.ts`, which drives the pure module the keys go through. This
 * file only insists the tree still says it is a tree.
 *
 * Third script to walk `src/` (see theme.check.ts, motion.check.ts). The walker
 * is four lines; copying it beats a shared module two working scripts would have
 * to adopt for one function.
 */
// @ts-ignore — see theme.check.ts: the app's tsconfig carries no Node types, and
// adding @types/node for one readFileSync would be a devDependency for nothing.
import { readFileSync, readdirSync } from "node:fs";

const failures: string[] = [];

function fail(message: string): void {
  failures.push(message);
}

function tsxUnder(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = `${dir}/${entry.name}`;
    if (entry.isDirectory()) out.push(...tsxUnder(path));
    else if (entry.name.endsWith(".tsx")) out.push(path);
  }
  return out;
}

/**
 * The index of the `>` that closes the tag opening at `open`.
 *
 * Brace- and quote-aware because JSX attributes are full of both: `onClick={() =>
 * …}` carries a `>` that ends nothing, and a `title="a > b"` carries one inside a
 * string. Stopping at the first `>` would hand back half a tag, and half a tag is
 * exactly where a missing `aria-label` would look present.
 */
function tagEnd(source: string, open: number): number {
  let braces = 0;
  for (let i = open + 1; i < source.length; i += 1) {
    const c = source[i];
    if (c === "{") braces += 1;
    else if (c === "}") braces -= 1;
    else if (braces === 0) {
      if (c === '"' || c === "'" || c === "`") {
        i += 1;
        while (i < source.length && source[i] !== c) i += 1;
      } else if (c === ">") return i;
    }
  }
  return -1;
}

/** Every opening tag whose text contains `needle`, as text. */
function tagsWith(source: string, needle: string): string[] {
  const out: string[] = [];
  let from = 0;
  for (;;) {
    const at = source.indexOf(needle, from);
    if (at < 0) break;
    from = at + needle.length;
    const open = source.lastIndexOf("<", at);
    if (open < 0) continue;
    const end = tagEnd(source, open);
    if (end > open) out.push(source.slice(open, end + 1));
  }
  return out;
}

/** Every `<button>` as its opening tag plus its content. Buttons never nest here. */
function buttons(source: string): { tag: string; body: string }[] {
  const out: { tag: string; body: string }[] = [];
  let from = 0;
  for (;;) {
    const at = source.indexOf("<button", from);
    if (at < 0) break;
    const end = tagEnd(source, at);
    const close = source.indexOf("</button>", end);
    if (end < 0 || close < 0) break;
    out.push({ tag: source.slice(at, end + 1), body: source.slice(end + 1, close) });
    from = close + 9;
  }
  return out;
}

const sources = new Map(tsxUnder("src").map((path) => [path, readFileSync(path, "utf8")]));

// --- a full-screen overlay is a dialog ---------------------------------------

for (const [path, source] of sources) {
  // `pointer-events-none` is the app's way of saying "this is a notice, not a
  // dialogue" — the drag-and-drop hint is the one of those.
  const overlays = tagsWith(source, "fixed inset-0").filter(
    (tag) => !/\bpointer-events-none\b/.test(tag),
  );
  if (overlays.length === 0) continue;
  if (!/"dialog"/.test(source)) {
    fail(
      `${path} draws a full-screen overlay and never calls it a dialog. A screen ` +
        `reader reads the page behind it as if nothing had opened.`,
    );
  }
  if (!/aria-modal/.test(source)) {
    fail(`${path} draws a full-screen overlay with no \`aria-modal\` to say the page behind is inert.`);
  }
}

for (const [path, source] of sources) {
  for (const tag of tagsWith(source, 'role="dialog"')) {
    if (!/aria-label=|aria-labelledby=/.test(tag)) {
      fail(
        `${path}: a dialog with no \`aria-label\` or \`aria-labelledby\` is announced ` +
          `as "dialog" and nothing else.`,
      );
    }
  }
  // --- the tree says it is a tree --------------------------------------------
  for (const tag of tagsWith(source, 'role="tree"')) {
    if (!/aria-label=|aria-labelledby=/.test(tag)) {
      fail(`${path}: a tree with no name is "tree" — say which tree.`);
    }
  }
  // --- the transcript says what it is ----------------------------------------
  for (const tag of tagsWith(source, 'role="log"')) {
    if (!/aria-label=|aria-labelledby=/.test(tag)) {
      fail(`${path}: a live log with no name is announced as "log".`);
    }
    if (!/aria-busy/.test(tag)) {
      fail(
        `${path}: a live log without \`aria-busy\` reads out every frame of a ` +
          `streaming answer. Hold the announcements back until the turn settles.`,
      );
    }
  }
}

// --- focus that survives High Contrast Mode ----------------------------------

for (const [path, source] of sources) {
  for (const tag of tagsWith(source, "outline-none")) {
    fail(
      `${path}: \`outline-none\` removes focus in Windows High Contrast Mode and ` +
        `puts nothing back. Use \`outline-hidden\`, and a \`focus:\` style where the ` +
        `control is reached by Tab. Tag: ${tag.replace(/\s+/g, " ").slice(0, 70)}`,
    );
  }
}

// --- a button made of one glyph has a name -----------------------------------

for (const [path, source] of sources) {
  for (const { tag, body } of buttons(source)) {
    // `title` counts: it is a real accessible-name source, and the app's own
    // glyph buttons rely on it. What is checked is the button with *no* name —
    // an icon and nothing else, which keyboard and touch both announce as
    // "button".
    if (/aria-label=|aria-labelledby=|title=/.test(tag)) continue;
    // Only the unambiguous shapes: content that is *entirely* an element, or
    // entirely glyphs. A body with a `{…}` in it might be a name at runtime, and
    // guessing wrong would mean chasing a warning that is not a bug.
    const glyphs = /^[^A-Za-z0-9{]*$/u.test(body.trim());
    if (!glyphs && !/^<[A-Za-z][^>]*\/>$/.test(body.trim())) continue;
    fail(
      `${path}: a button whose whole content is ${body.trim() || "nothing"} has no ` +
        `name at all. Add \`aria-label\`.`,
    );
  }
}

// --- the gate ----------------------------------------------------------------

if (failures.length > 0) {
  throw new Error(`FAIL: ${failures.length} accessibility problem(s):\n  ${failures.join("\n  ")}`);
}

console.log(
  `a11y.check.ts: all checks passed (${sources.size} components: dialogs named, ` +
    `tree and log declared, no bare outline-none, no unnamed glyph button)`,
);
