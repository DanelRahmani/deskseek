/**
 * Reads one version's notes out of `CHANGELOG.md`.
 *
 * The release workflow calls this with the tag it is building, and a wrong answer
 * here is silent in the worst way: the GitHub release would simply arrive with no
 * notes, and nobody notices until someone asks what changed in a version. So the
 * parsing is a pure function with every edge the file can present pinned by
 * `changelog.check.ts` — the tag's leading `v`, a body that has to stop at the
 * next heading, the last section in the file, and a version the file does not
 * have.
 *
 * The heading shape is the contract (`## <version> — <date|unreleased>`); the
 * check also holds the real file to it, so a heading that drifts is caught in the
 * gate rather than by a release with no notes.
 */

/** `## 0.4.0 — unreleased`, `## 0.3.0 — 2026-10-12`. */
const VERSION_HEADING = /^##\s+(\S+)\s+—\s+(.+?)\s*$/;

/**
 * The body of the section for `tag`, or `null` when the file has no section for
 * it — including when it has an empty one, because a heading with nothing under
 * it is the same problem to whoever reads the release.
 *
 * A leading `v` is accepted and ignored, because that is what a git tag is.
 */
export function releaseNotes(markdown: string, tag: string): string | null {
  const version = tag.replace(/^v/, "");
  // Split rather than regex over the whole file: a section runs until the next
  // heading, which is a line-level question. `\r?` so a Windows checkout parses
  // the same as a Linux one.
  const lines = markdown.split(/\r?\n/);
  const start = lines.findIndex((line) => {
    const match = VERSION_HEADING.exec(line);
    return match !== null && match[1] === version;
  });
  if (start === -1) return null;

  const rest = lines.slice(start + 1);
  const end = rest.findIndex((line) => VERSION_HEADING.test(line));
  const body = (end === -1 ? rest : rest.slice(0, end)).join("\n").trim();
  return body === "" ? null : body;
}
