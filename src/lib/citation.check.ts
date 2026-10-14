/**
 * Checks for the citation chips in citation.ts. Plain Node, no test framework:
 *
 *   npm run check
 *
 * What is worth checking here is the one thing a chip must not do — claim a
 * source the notebook no longer holds looks like any other, or turn a label into
 * a link that goes nowhere. The *grammar* is not checked here because it is not
 * implemented here: `retrieval.rs` owns what a citation is, and a second copy of
 * that in TypeScript is exactly the drift this file exists to prevent.
 */
import { citations } from "./citation.ts";

function ok(condition: boolean, message: string): void {
  if (!condition) throw new Error(`FAIL: ${message}`);
}

const sources = ["report.pdf", "Notes.md"];

// A label with a part names the file and the part; the label itself is kept, so
// the chip can show what the model actually wrote.
const split = citations(["report.pdf#Method"], sources);
ok(split.length === 1, "one label is one chip");
ok(split[0].name === "report.pdf", "the file name comes before the hash");
ok(split[0].heading === "Method", "the heading comes after it");
ok(split[0].label === "report.pdf#Method", "the label itself is kept whole");
ok(split[0].known, "a source that is still here is known");

// A whole-source citation has no heading rather than an empty string: the chip
// has to tell "the whole document" from "a part of it".
const whole = citations(["report.pdf"], sources);
ok(whole[0].heading === null, "a whole-source citation has no heading");
ok(whole[0].known, "it is known as well");

// Case is not the point, the same way it is not on the Rust side.
ok(citations(["REPORT.PDF"], sources)[0].known, "a different case is the same source");

// A source that has been removed: the chip says so rather than looking like the
// others. This is the case the module exists for.
const gone = citations(["old.pdf"], sources);
ok(!gone[0].known, "a removed source must not read as known");
ok(gone[0].name === "old.pdf", "and it still names what was cited");

// Order is the answer's order, repeats collapse, and a label with no name in it
// is not a chip at all.
const many = citations(["report.pdf", "old.pdf", "report.pdf", "#Method", "  "], sources);
ok(
  many.map((c) => c.label).join("|") === "report.pdf|old.pdf",
  `repeats must collapse and empty names must drop: got ${many.map((c) => c.label).join("|")}`,
);

// Nothing stored is nothing shown — an answer with no citations gets no strip.
ok(citations([], sources).length === 0, "no labels means no chips");

console.log("citation.check.ts: all checks passed");
