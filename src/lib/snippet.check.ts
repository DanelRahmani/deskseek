/**
 * Checks for the snippet highlighter in snippet.ts. Plain Node, no test
 * framework:
 *
 *   npm run check
 *
 * Worth checking because the input is not text we wrote: a message may contain
 * brackets of its own, and the split has to tell those apart from the ones the
 * search added. Getting it wrong is invisible to the type checker — it draws a
 * plausible row with the emphasis in the wrong place, or with a stray bracket
 * left in the middle of a sentence.
 *
 * So every case states two things: the parts as the row draws them (a mark
 * written back as `[its text]`, joined by `|`) and the text they put back
 * together, which is the same snippet with only the *search's* brackets gone.
 */
import { searchTerms, snippetParts } from "./snippet.ts";

function assert(ok: boolean, message: string): void {
  if (!ok) throw new Error(`FAIL: ${message}`);
}

const cases: {
  snippet: string;
  query: string;
  /** What the row draws: marks kept, parts joined by `|`. */
  shape: string;
  /** What the parts concatenate to: the search's brackets gone, the rest kept. */
  joined: string;
  why: string;
}[] = [
  {
    snippet: "where is the [pars]er bug",
    query: "pars",
    shape: "where is the |[pars]|er bug",
    joined: "where is the parser bug",
    why: "the ordinary case",
  },
  {
    snippet: "the [parser] lives",
    query: "pars",
    shape: "the |[parser]| lives",
    joined: "the parser lives",
    why: "a prefix search marks the whole word it found",
  },
  {
    snippet: "[invoice] for [acme]",
    query: "invoice acme",
    shape: "[invoice]| for |[acme]",
    joined: "invoice for acme",
    why: "two terms in one snippet",
  },
  {
    snippet: "the list[0] is [empty]",
    query: "empty",
    shape: "the list[0] is |[empty]",
    joined: "the list[0] is empty",
    why: "the message's own index is left alone, brackets and all",
  },
  {
    snippet: "items[3] and items[4]",
    query: "items",
    shape: "items[3] and items[4]",
    joined: "items[3] and items[4]",
    why: "code full of brackets must not light up",
  },
  {
    snippet: "[a] mixed [b] text",
    query: "a b",
    shape: "[a] mixed [b] text",
    joined: "[a] mixed [b] text",
    why: "a query with no searchable term marks nothing",
  },
  {
    snippet: "trailing [mark",
    query: "mark",
    shape: "trailing [mark",
    joined: "trailing [mark",
    why: "an unclosed bracket is text",
  },
  {
    snippet: "no marks here at all",
    query: "pars",
    shape: "no marks here at all",
    joined: "no marks here at all",
    why: "no marks at all still comes back as parts",
  },
];

for (const { snippet, query, shape, joined, why } of cases) {
  const parts = snippetParts(snippet, searchTerms(query));
  const got = parts.map((part) => (part.match ? `[${part.text}]` : part.text)).join("|");
  assert(
    got === shape,
    `${why}: ${JSON.stringify(snippet)} + ${query} drew ${JSON.stringify(got)}, expected ${JSON.stringify(shape)}`,
  );
  const putBack = parts.map((part) => part.text).join("");
  assert(
    putBack === joined,
    `${why}: parts rejoined to ${JSON.stringify(putBack)}, expected ${JSON.stringify(joined)}`,
  );
  // Nothing is lost whatever else happens: a mark only ever shortens the text by
  // the two brackets it was written with. Counted from the flags, not from the
  // drawing — a message whose plain text begins with `[` would fool the latter.
  const marks = parts.filter((part) => part.match).length;
  assert(
    putBack.length === snippet.length - marks * 2,
    `${why}: the parts account for every character`,
  );
}

// The flags, not just the text: a row needs to know which part to emphasise.
const flagged = snippetParts("the [parser] lives", searchTerms("pars"));
assert(flagged.length === 3, `three parts, got ${flagged.length}`);
assert(!flagged[0].match && flagged[0].text === "the ", "the lead is plain");
assert(flagged[1].match && flagged[1].text === "parser", "the middle is the match");
assert(!flagged[2].match && flagged[2].text === " lives", "the tail is plain");

// The term rule mirrors the backend's: two characters or more, quotes stripped.
assert(searchTerms("a").length === 0, "a one-letter term is not searched");
assert(searchTerms("").length === 0, "an empty query has no terms");
assert(
  searchTerms('  "quoted" word  ').join(",") === "quoted,word",
  `quoting and spacing: ${searchTerms('  "quoted" word  ').join(",")}`,
);

console.log("snippet.check.ts: all checks passed");
