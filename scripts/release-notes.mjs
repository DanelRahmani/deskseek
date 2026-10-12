/**
 * Prints this tag's release notes out of `CHANGELOG.md`:
 *
 *   node scripts/release-notes.mjs v0.4.0
 *
 * The release workflow runs it and the notes become the draft's body. When the
 * runner has set `GITHUB_OUTPUT` they go to the step's output instead of stdout,
 * which is the whole of the coupling to Actions — and it is deliberately the
 * script that writes that file rather than a shell heredoc, because a
 * `{ …; node …; }` block reports the exit status of its *last* command, so a
 * missing section would have been written as an empty body and the step would
 * still have gone green.
 *
 * Run by hand, or with no section for the tag, it says so and exits non-zero: a
 * release that quietly ships with no notes is the failure this replaces, and a tag
 * is the publish button.
 *
 * The parsing itself is in `src/lib/changelog.ts`, where `npm run check` holds it
 * to a fixture. This file is only the command line around it.
 */
import { appendFileSync, readFileSync } from "node:fs";
import { releaseNotes } from "../src/lib/changelog.ts";

const tag = process.argv[2];
if (!tag) {
  console.error("usage: node scripts/release-notes.mjs <tag>");
  process.exit(2);
}

const notes = releaseNotes(readFileSync("CHANGELOG.md", "utf8"), tag);
if (notes === null) {
  console.error(`CHANGELOG.md has no section for ${tag} — add one before tagging.`);
  process.exit(1);
}

const output = process.env.GITHUB_OUTPUT;
if (output) appendFileSync(output, `body<<__CHANGELOG__\n${notes}\n__CHANGELOG__\n`);
else console.log(notes);
