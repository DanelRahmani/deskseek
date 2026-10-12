/**
 * Turning a search snippet into the pieces a result row draws.
 *
 * The backend hands back one string with the matched words bracketed —
 * `snippet()` in SQLite can mark a hit, but it cannot say *where* the marks were
 * except by putting characters in the text. Drawn as-is the user reads
 * `[pars]er`, so the marks have to come back out again somewhere. That is here:
 * a pure function, so the split can be checked without a browser, and one file,
 * so the rule about what counts as a mark exists once.
 *
 * The ambiguity that decides the whole design: a message is allowed to contain
 * `[` and `]` — code does, all the time. So a bracket pair is only a mark when
 * what is inside it is a word the user actually searched for. Anything else is
 * the message's own punctuation and stays visible.
 */

export interface SnippetPart {
  text: string;
  /** True for a stretch the search matched, so the row can emphasise it. */
  match: boolean;
}

/**
 * The words a query searches for, as the snippet would mark them.
 *
 * Mirrors the backend's `fts_query`: whitespace-separated, at least two
 * characters, and quoted words are unquoted because that is what the user
 * typed rather than what was searched. It is deliberately the *shape* of the
 * rule and not the whole of it — if the two ever drift, a mark is left
 * unhighlighted, which is a blemish, where highlighting the wrong brackets
 * would be a lie.
 */
export function searchTerms(query: string): string[] {
  return query
    .split(/\s+/)
    .map((term) => term.replace(/"/g, "").toLowerCase())
    .filter((term) => term.length >= 2);
}

/**
 * Split a snippet on its marks. Text with no marks comes back as one part, so a
 * caller can always map over the result.
 */
export function snippetParts(snippet: string, terms: string[]): SnippetPart[] {
  const wanted = new Set(terms.filter((term) => term.length >= 2));
  const parts: SnippetPart[] = [];
  let plain = "";
  let i = 0;

  while (i < snippet.length) {
    const inner = markedAt(snippet, i, wanted);
    if (inner === null) {
      plain += snippet[i];
      i += 1;
      continue;
    }
    if (plain) parts.push({ text: plain, match: false });
    parts.push({ text: inner, match: true });
    plain = "";
    i += inner.length + 2;
  }
  if (plain) parts.push({ text: plain, match: false });
  return parts;
}

/**
 * The marked word starting at `at`, or null when the bracket there is the
 * message's own. A prefix match counts — the backend searches `term*`, so the
 * snippet marks the whole word that was found, not the part that was typed.
 */
function markedAt(snippet: string, at: number, wanted: Set<string>): string | null {
  if (snippet[at] !== "[") return null;
  const close = snippet.indexOf("]", at + 1);
  if (close === -1) return null;
  const inner = snippet.slice(at + 1, close);
  if (!inner || inner.includes("[")) return null;
  const lower = inner.toLowerCase();
  for (const term of wanted) {
    if (lower.startsWith(term)) return inner;
  }
  return null;
}
