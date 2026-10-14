/**
 * How a citation reads, and whether it still points at anything.
 *
 * The rule that decides whether the *model* may cite a label lives in Rust, in
 * `retrieval.rs`: the answer is parsed there, and the labels that resolved are
 * stored with it. This module is the other end of that — a stored label turned
 * into something a reader can look at, which is a different question and the only
 * one the window can answer: an answer written yesterday can cite a source that
 * was removed today, and a chip that still looks clickable would be a link to
 * nothing.
 *
 * Deliberately not a second copy of the citation grammar: it never parses the
 * document, it reads a list the app already decided.
 */
export interface Citation {
  /** The label as it was stored: `report.pdf#Method`. */
  label: string;
  /** The source it names: `report.pdf`. */
  name: string;
  /** The part of it that was cited, or null for the whole source. */
  heading: string | null;
  /** Whether the notebook still holds a source by that name. */
  known: boolean;
}

/**
 * Turn a stored citation list into chips, newest rule first: a label that names
 * a source the notebook no longer has is marked rather than hidden, because the
 * answer really did cite it and the reader is owed the difference between "the
 * file moved" and "the model made it up" — the second is caught before it is
 * stored.
 */
export function citations(labels: string[], sources: string[]): Citation[] {
  const out: Citation[] = [];
  for (const label of labels) {
    if (out.some((seen) => seen.label === label)) continue;
    const [rawName, rawHeading] = label.split("#");
    const name = (rawName ?? "").trim();
    if (!name) continue;
    const heading = (rawHeading ?? "").trim();
    out.push({
      label,
      name,
      heading: heading === "" ? null : heading,
      known: sources.some((source) => source.trim().toLowerCase() === name.toLowerCase()),
    });
  }
  return out;
}
