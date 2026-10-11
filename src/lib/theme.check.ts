/**
 * Checks for the theme model in theme.ts. Plain Node, no test framework:
 *
 *   npm run check
 *
 * Two things here are load-bearing. Every theme is measured, because a theme is
 * data and data goes wrong quietly: a mistyped hex still renders, it just
 * renders unreadable. And the Slate-dark defaults in index.css are compared
 * against what resolveTheme computes, because those are the same thirty values
 * written down twice, and that kind of pair drifts in silence.
 */
// The only check script that touches the filesystem, and the app's tsconfig
// carries no Node types — every other check is pure for exactly this reason.
// Adding @types/node for one readFileSync would swap a two-line exception for a
// devDependency, so the import is tolerated rather than typed. Keep everything
// else in this file pure: it is the half the typechecker still verifies.
// @ts-ignore
import { readFileSync } from "node:fs";
import {
  COLOUR_TOKENS,
  DEFAULT_THEME_ID,
  THEMES,
  contrast,
  mix,
  resolveMode,
  resolveTheme,
  themeById,
} from "./theme.ts";

const failures: string[] = [];

function need(ok: boolean, message: string): void {
  if (!ok) throw new Error(`FAIL: ${message}`);
}

/**
 * Contrast is checked in a batch rather than one assertion per pair: a theme
 * has ~70 measured pairs, and failing on the first hides the other nine that
 * are also wrong.
 */
function atLeast(ratio: number, floor: number, label: string): void {
  if (ratio < floor - 0.005) {
    failures.push(`${label}: ${ratio.toFixed(2)}:1, needs ${floor}:1`);
  }
}

/** Tiers a reader is meant to read as sentences. */
const TEXT_TIERS = ["--n-100", "--n-200", "--n-300", "--n-400"];
/** The faint tier: 10-11px metadata. Held to a documented 3:1 floor, not 4.5. */
const FAINT_TIERS = ["--n-500", "--n-600"];
const TEXT_FLOOR = 4.5;
const FAINT_FLOOR = 3;
const STATUSES = ["accent", "danger", "warn", "ok"];

// --- the resolution rules ---------------------------------------------------

need(resolveMode("system", true) === "dark", "system follows a dark OS");
need(resolveMode("system", false) === "light", "system follows a light OS");
need(resolveMode("dark", false) === "dark", "an explicit dark overrides a light OS");
need(resolveMode("light", true) === "light", "an explicit light overrides a dark OS");
// The value comes from a JSON file, so a hand-edited or future mode must not
// produce a third state: anything unrecognised is `system`.
need(resolveMode("sepia", false) === "light", "an unknown mode falls back to the OS");
need(themeById("nope").id === THEMES[0].id, "an unknown theme falls back to the first");
need(THEMES[0].id === DEFAULT_THEME_ID, "the default theme is the first one");
need(new Set(THEMES.map((t) => t.id)).size === THEMES.length, "theme ids are unique");

need(mix("#000000", "#ffffff", 0) === "#000000", "mix at 0 is the first colour");
need(mix("#000000", "#ffffff", 1) === "#ffffff", "mix at 1 is the second colour");

// --- every variant, measured -----------------------------------------------

const keySets = new Set<string>();
let pairs = 0;

for (const theme of THEMES) {
  for (const variant of ["dark", "light"] as const) {
    const tokens = resolveTheme(theme.id, variant, true);
    const where = `${theme.id}/${variant}`;
    keySets.add(Object.keys(tokens).sort().join(","));

    for (const token of COLOUR_TOKENS) {
      if (!/^#[0-9a-f]{6}$/.test(tokens[token] ?? "")) {
        failures.push(`${where} ${token}: not a hex colour (${tokens[token]})`);
      }
    }

    // Both surfaces, because the faint tier is read on panels as often as on
    // the page, and a panel is one ramp step away from it.
    for (const surface of ["--n-950", "--n-900"]) {
      for (const tier of TEXT_TIERS) {
        atLeast(contrast(tokens[tier], tokens[surface]), TEXT_FLOOR, `${where} ${tier} on ${surface}`);
        pairs++;
      }
      for (const tier of FAINT_TIERS) {
        atLeast(contrast(tokens[tier], tokens[surface]), FAINT_FLOOR, `${where} ${tier} on ${surface}`);
        pairs++;
      }
      for (const status of STATUSES) {
        atLeast(
          contrast(tokens[`--${status}-text`], tokens[surface]),
          TEXT_FLOOR,
          `${where} --${status}-text on ${surface}`,
        );
        pairs++;
      }
    }

    // A status label usually sits on its own tint, so that pair matters most.
    for (const status of STATUSES) {
      atLeast(
        contrast(tokens[`--${status}-text`], tokens[`--${status}-bg`]),
        TEXT_FLOOR,
        `${where} --${status}-text on --${status}-bg`,
      );
      pairs++;
    }
  }
}

// Two variants disagreeing on which tokens exist means one theme has a hole the
// others do not, and the stylesheet would fall back to whatever `:root` says.
need(keySets.size === 1, `every variant must emit the same tokens, saw ${keySets.size} different sets`);

// --- index.css paints the first frame, so it must agree ---------------------

const css = readFileSync(new URL("../index.css", import.meta.url), "utf8");
const root = /:root\s*\{([\s\S]*?)\}/.exec(css)?.[1] ?? "";

if (root.length === 0) {
  // One finding, not thirty: without the block every token is missing at once.
  failures.push("index.css has no :root block, so the first frame has no theme");
} else {
  const declared = new Map<string, string>();
  for (const [, name, value] of root.matchAll(/(--[a-z0-9-]+)\s*:\s*([^;]+);/g)) {
    declared.set(name, value.trim());
  }

  const fallback = resolveTheme(DEFAULT_THEME_ID, "dark", true);
  for (const [name, value] of Object.entries(fallback)) {
    const found = declared.get(name);
    if (found === undefined) failures.push(`index.css :root is missing ${name}`);
    else if (found !== value)
      failures.push(`index.css :root ${name} is ${found}, theme.ts computes ${value}`);
  }
}

// --- the gate ---------------------------------------------------------------

if (failures.length > 0) {
  throw new Error(
    `FAIL: ${failures.length} theme problem(s):\n  ${failures.join("\n  ")}`,
  );
}

console.log(`theme.check.ts: all checks passed (${pairs} contrast pairs, ${THEMES.length} themes)`);
