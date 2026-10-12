/**
 * Checks for `CHANGELOG.md` and the reader the release workflow turns a tag with.
 * Plain Node, no test framework:
 *
 *   npm run check
 *
 * Two failures are worth a check here, and both are silent without one. A heading
 * that drifts out of the shape — a hyphen instead of the em dash, a version out of
 * order, a second section marked unreleased — still reads fine to a person and stops
 * the release step finding the section at all. And a reader that returns too much
 * would paste the *next* version's notes into this release's body, which is the
 * one place nobody proof-reads.
 *
 * The file is the source of truth for the reader's contract, so it is checked
 * directly rather than through a fixture that agrees with the reader by
 * construction. The fixture below exists for the shapes the real file does not
 * have (a section with nothing in it, a version that was never released).
 */
// @ts-ignore — see theme.check.ts: the app's tsconfig carries no Node types, and
// adding @types/node for one readFileSync would be a devDependency for nothing.
import { readFileSync } from "node:fs";
import { releaseNotes } from "./changelog.ts";

const failures: string[] = [];

function need(ok: boolean, message: string): void {
  if (!ok) failures.push(message);
}

function same(actual: unknown, expected: unknown, what: string): void {
  const a = JSON.stringify(actual);
  const b = JSON.stringify(expected);
  if (a !== b) failures.push(`${what}: got ${a}, wanted ${b}`);
}

// --- the reader, over a fixture with the shapes the real file does not have ---

const FIXTURE = [
  "# Changelog",
  "",
  "Prose a reader wants and the reader must not return.",
  "",
  "## 2.0.0 — unreleased",
  "",
  "- the next thing",
  "",
  "## 1.1.1 — 2026-01-03",
  "",
  "- a line",
  "- and another",
  "",
  "## 1.0.0 — 2026-01-02",
  "",
  "## 0.9.0 — 2026-01-01",
  "",
  "- the oldest, with no heading after it",
].join("\n");

same(releaseNotes(FIXTURE, "v2.0.0"), "- the next thing", "the newest section, reached by a v-tag");
same(releaseNotes(FIXTURE, "2.0.0"), "- the next thing", "the same section, reached by a bare version");
same(
  releaseNotes(FIXTURE, "1.1.1"),
  "- a line\n- and another",
  "a section stops at the next heading and drops the blank line around it",
);
same(releaseNotes(FIXTURE, "0.9.0"), "- the oldest, with no heading after it", "the last section in the file");
same(releaseNotes(FIXTURE, "0.9"), null, "a version the file does not have");
same(releaseNotes(FIXTURE, "0.10.0"), null, "a version newer than everything in the file");
same(releaseNotes(FIXTURE, "1.0.0"), null, "a heading with nothing under it");

// --- the real file, held to what the reader assumes ---------------------------

const file: string = readFileSync("CHANGELOG.md", "utf8");
const lines = file.split(/\r?\n/);

const indented = lines.filter((line) => /^\s+## /.test(line));
need(
  indented.length === 0,
  `a heading is indented, so the reader can never find it: ${JSON.stringify(indented[0])}`,
);

const headings = lines
  .filter((line) => line.startsWith("## "))
  .map((line) => {
    const match = /^## (\S+) — (.+)$/.exec(line);
    need(match !== null, `not a version heading: ${JSON.stringify(line)}`);
    return { version: match?.[1] ?? "", when: match?.[2] ?? "" };
  });

need(headings.length >= 2, "fewer than two version sections");

for (const heading of headings) {
  need(/^\d+\.\d+\.\d+$/.test(heading.version), `${heading.version} is not major.minor.patch`);
  need(releaseNotes(file, heading.version) !== null, `the ${heading.version} section has no notes in it`);
}

const unreleased = headings.filter((h) => h.when === "unreleased");
// At *most* one, and not exactly one: dating the section is the release, so a file
// that has just been released legitimately has none until the next change lands.
need(unreleased.length <= 1, `${unreleased.length} sections marked unreleased, wanted at most one`);
if (unreleased.length === 1) {
  need(headings[0]?.when === "unreleased", "the unreleased section is not the first one");
}

for (const heading of headings) {
  if (heading.when === "unreleased") continue;
  need(/^\d{4}-\d{2}-\d{2}$/.test(heading.when), `${heading.version} is dated ${JSON.stringify(heading.when)}, wanted YYYY-MM-DD`);
}

/** Negative when `a` is older than `b`, like a comparator. */
function compare(a: string, b: string): number {
  const left = a.split(".").map(Number);
  const right = b.split(".").map(Number);
  return left[0] - right[0] || left[1] - right[1] || left[2] - right[2];
}

for (let i = 1; i < headings.length; i += 1) {
  need(
    compare(headings[i - 1].version, headings[i].version) > 0,
    `${headings[i - 1].version} sits above ${headings[i].version} but is not newer`,
  );
}

if (failures.length > 0) {
  throw new Error(`FAIL: ${failures.length} changelog problem(s):\n  ${failures.join("\n  ")}`);
}

const released = headings.filter((h) => h.when !== "unreleased").map((h) => h.version);
const standing =
  unreleased.length === 1 ? `${unreleased[0].version} unreleased` : "nothing unreleased";
console.log(
  `changelog.check.ts: all checks passed (${headings.length} sections, ${standing}, ` +
    `released: ${released.join(", ")}, and no tag's notes take the next version's with them)`,
);
