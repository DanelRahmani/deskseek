/**
 * Checks for the motion rule: every animation either explains a state change or
 * is removed.
 *
 *   npm run check
 *
 * A Node script has no layout, so nothing here can watch a transition run. What
 * it can check is the *inventory*: every distinct thing that moves in this app
 * is named in `PURPOSEFUL` beside the state it is explaining, nothing else moves
 * at all, and no keyframe is left defined but unplayed. A decorative animation
 * therefore fails the gate until its author writes down what it is for, which is
 * the rule made mechanical instead of a style guide nobody opens.
 *
 * The second half is the trap the CSS comment already documents: under reduced
 * motion the blanket rule has to keep applying, and the caret has to survive it.
 * Lose the blanket rule and the app keeps moving for someone who asked it not
 * to; lose the caret's exception and it parks on the invisible half of its own
 * blink — which is how this app shipped once already.
 *
 * Like `theme.check.ts` this reads the source rather than importing it, and for
 * the same reason: the thing under test is a stylesheet and a set of class
 * names. `src/` has no dependencies of its own, so a recursive read of a few
 * dozen files is enough and there is no globber here on purpose.
 */
// @ts-ignore — see theme.check.ts: the app's tsconfig carries no Node types, and
// adding @types/node for two imports would be a devDependency for nothing.
import { readFileSync, readdirSync } from "node:fs";

/** Every animation allowed to exist, keyed by its name, and the state it explains. */
const PURPOSEFUL: Record<string, string> = {
  "caret-blink": "the answer is still arriving",
  "panel-in-left": "a panel opened, from the edge it lives on",
  "panel-in-right": "a panel opened, from the edge it lives on",
  reveal: "a collapsed group opened",
  pulse: "a chat, a step or a task is running",
};

/**
 * Names that are Tailwind's own animations rather than one of ours. They are
 * matched against `animate-<name>` in the components; the `@keyframes` they
 * generate lives in Tailwind's layer, never in `index.css`, which is why the
 * "defined but never played" check below does not look for them.
 */
const TAILWIND_ANIMATIONS = ["pulse"];

const failures: string[] = [];

function fail(message: string): void {
  failures.push(message);
}

// --- reading the sources -----------------------------------------------------

const css = readFileSync("src/index.css", "utf8");

function keyframesIn(source: string): string[] {
  return [...source.matchAll(/@keyframes\s+([A-Za-z0-9_-]+)/g)].map((match) => match[1]);
}

/**
 * The names played by an `animation:` shorthand in a stylesheet.
 *
 * `animation-duration:` and friends cannot match — the character after
 * `animation` would be `-` rather than `:` — and the lookbehind keeps the
 * vendor-prefixed form out.
 */
function animationsIn(source: string): string[] {
  return [...source.matchAll(/(?<![-\w])animation:\s*([A-Za-z][A-Za-z0-9_-]*)/g)]
    .map((match) => match[1])
    .filter((name) => name !== "none");
}

/** Every `.tsx` under `src`, so a new animation cannot hide in an untried file. */
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
 * The body of the block opened by the brace at `open`, matched by depth.
 *
 * Depth rather than "up to the next `}`" because the reduced-motion rules nest:
 * the OS-preference block contains a rule for everything and a second one for
 * the caret, and stopping at the first brace would hide the exception this file
 * exists to protect.
 */
function matchBody(source: string, open: number): string | null {
  let depth = 0;
  for (let i = open; i < source.length; i += 1) {
    if (source[i] === "{") depth += 1;
    else if (source[i] === "}") {
      depth -= 1;
      if (depth === 0) return source.slice(open + 1, i);
    }
  }
  return null;
}

/**
 * Every rule whose selector mentions `needle`, as text: the selector and its
 * body, so a rule can be recognised by *what it selects* as well as by what it
 * declares.
 *
 * Plural on purpose: the in-app setting is written as two sibling rules (one for
 * everything, one for the caret), while the OS preference is one nested block.
 * Asking for "the" body silently sees only half of the first case, and asking
 * for a bare body loses the selector — both of which this file's own first
 * drafts got wrong, which is why it checks the caret by rule and not by name.
 */
function rulesFor(source: string, needle: string): string[] {
  const rules: string[] = [];
  let from = 0;
  for (;;) {
    const at = source.indexOf(needle, from);
    if (at < 0) break;
    const open = source.indexOf("{", at);
    if (open < 0) break;
    const body = matchBody(source, open);
    if (body !== null) rules.push(`${source.slice(at, open)}{${body}}`);
    from = at + needle.length;
  }
  return rules;
}

const keyframes = keyframesIn(css);
const played = animationsIn(css);

/** `animate-<name>` as written in the components, with the first file to use it. */
const utilities = new Map<string, string>();
for (const path of tsxUnder("src")) {
  const text = readFileSync(path, "utf8");
  if (text.includes("animate-[")) {
    fail(
      `${path} uses an arbitrary animate-[...] value. Its timing and its name are ` +
        `outside every rule here — put it in index.css as a named animation instead.`,
    );
  }
  for (const match of text.matchAll(/\banimate-([a-z][a-z0-9-]*)/g)) {
    if (!utilities.has(match[1])) utilities.set(match[1], path);
  }
}

// --- everything that moves is explained --------------------------------------

for (const name of new Set([...keyframes, ...played, ...utilities.keys()])) {
  if (!(name in PURPOSEFUL)) {
    fail(
      `"${name}" moves and nothing says why. Either add it to PURPOSEFUL in ` +
        `motion.check.ts with the state it explains, or take the movement out.`,
    );
  }
}

for (const [name, path] of utilities) {
  if (!TAILWIND_ANIMATIONS.includes(name)) {
    fail(
      `${path} uses animate-${name}, which this app has not declared. Add it to ` +
        `TAILWIND_ANIMATIONS (and to PURPOSEFUL, with the state it explains) if Tailwind ` +
        `really has that animation — a misspelled utility does nothing at all.`,
    );
  }
}

// --- nothing is defined and then never played --------------------------------

for (const name of keyframes) {
  if (!played.includes(name)) {
    fail(`@keyframes ${name} is defined in index.css and no rule plays it. Remove it.`);
  }
}

for (const name of played) {
  if (!keyframes.includes(name)) {
    fail(
      `index.css says \`animation: ${name}\` but defines no @keyframes by that name, ` +
        `so the declaration is silently ignored.`,
    );
  }
}

// --- the reduced-motion contract ---------------------------------------------

const REDUCED: [string, string][] = [
  ["@media (prefers-reduced-motion: reduce)", "the OS preference"],
  ['[data-reduce-motion="true"]', "the in-app setting"],
];

for (const [selector, label] of REDUCED) {
  const rules = rulesFor(css, selector);
  if (rules.length === 0) {
    fail(`there is no reduced-motion rule for ${label} (${selector}).`);
    continue;
  }
  // Every rule that mentions the selector, joined: the setting is two rules and
  // the caret's exception is the second of them.
  const body = rules.join("\n");
  for (const declaration of [
    "animation-duration",
    "animation-iteration-count",
    "transition-duration",
  ]) {
    if (!body.includes(declaration)) {
      fail(
        `the reduced-motion rule for ${label} does not neutralise ${declaration}, ` +
          `so something keeps moving for a user who asked it not to.`,
      );
    }
  }
  // The caret is the documented exception: stopped, it still has to be visible.
  if (!/\.caret\s*\{[^}]*animation:\s*none/.test(body)) {
    fail(
      `the caret loses its reduced-motion exception for ${label} — it will park on the ` +
        `invisible half of its blink.`,
    );
  }
  if (!/\.caret\s*\{[^}]*opacity:\s*1/.test(body)) {
    fail(`the caret has to be held visible under reduced motion (${label}).`);
  }
}

// --- the gate ----------------------------------------------------------------

if (failures.length > 0) {
  throw new Error(`FAIL: ${failures.length} motion problem(s):\n  ${failures.join("\n  ")}`);
}

console.log(
  `motion.check.ts: all checks passed (${Object.keys(PURPOSEFUL).length} animations, ` +
    `each with the state it explains)`,
);
