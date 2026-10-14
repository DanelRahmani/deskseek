/**
 * Checks for taking a study set, in quiz.ts. Plain Node, no test framework:
 *
 *   npm run check
 *
 * Two failures are worth a check here, and both are silent when they happen: a
 * score that counts a question the reader has not reached as a wrong one (a number
 * that moves while they are still going), and a comparison loose enough to mark a
 * near miss right (a quiz that lies to the person taking it).
 *
 * The rules it does *not* restate are the ones Rust owns — what a set may be is
 * refused in `notebook::study_set` before it is stored — so there is no second
 * copy of them here. The last check is the boundary between the two: the window
 * marks a choice by its text, so two options that read the same are
 * indistinguishable to it, and that is exactly why the validator refuses them.
 */
import { quantities, right, score, shuffled, sitting } from "./quiz.ts";
import type { StudyItem } from "../types.ts";

function ok(condition: boolean, message: string): void {
  if (!condition) throw new Error(`FAIL: ${message}`);
}

const item = (prompt: string, answer: string, options: string[] = []): StudyItem => ({
  prompt,
  answer,
  options,
  why: "",
  source: "",
});

const quiz = [item("Rise?", "9%", ["9%", "12%"]), item("Margin?", "41%", ["20%", "41%"])];

// The right choice is the one spelled as the answer — the option as it was drawn,
// which is what the answer was normalised to before it was stored.
ok(right(quiz[0], "9%"), "the answer as written is right");
ok(!right(quiz[0], "12%"), "the other option is not");
ok(!right(quiz[0], "9"), "a prefix of the answer is not the answer");
ok(!right(quiz[0], " 9%"), "and neither is it with a space in front");

// Nothing answered is nothing scored: not a score of zero, and not two wrong.
const nothing = score(quiz, {});
ok(nothing.asked === 0 && nothing.right === 0, "an untouched set has asked nothing");
ok(nothing.wrong.length === 0, "and has nothing wrong in it");

// Half way through: the one answered counts, the one not reached is neither right
// nor wrong — so the number cannot move while the reader is still going.
const half = score(quiz, { 0: "12%" });
ok(half.asked === 1, "one question asked");
ok(half.right === 0, "and it was answered wrongly");
ok(half.wrong.join(",") === "0", `the wrong one is named by index: got ${half.wrong.join(",")}`);

// A finished set: two asked, one right, and the wrong one named in the order it
// was asked rather than in the order it was written.
const done = score(quiz, { 0: "9%", 1: "20%" });
ok(done.asked === 2 && done.right === 1, "one of two right");
ok(done.wrong.join(",") === "1", "the second question is the wrong one");
ok(
  done.right + done.wrong.length === done.asked,
  "every answered item is either right or wrong, never both and never neither",
);

// The boundary with Rust: marking is by the text of the choice and never by where
// it sits, so the window cannot tell two identical options apart — the refusal has
// to happen upstream, and it does.
const twins = [item("Rise?", "9%", ["9%", "9%"])];
ok(
  score(twins, { 0: "9%" }).right === 1 && right(twins[0], twins[0].options[1]),
  "the text is what is marked, so a duplicate option is invisible here",
);

// A count is spelled rather than pluralised by adding an `s`: one card is a card,
// and the rail and the dialog both read this wording.
ok(quantities(1, "flashcards") === "1 card", `one card: got ${quantities(1, "flashcards")}`);
ok(quantities(2, "flashcards") === "2 cards", "and two are cards");
ok(quantities(1, "quiz") === "1 question", "one question, not one questions");
ok(quantities(9, "quiz") === "9 questions", "and nine are questions");

// --- dealing a sitting -------------------------------------------------------

// A seeded generator: the order has to be unpredictable in the app and repeatable
// in here, or the properties below could only be asserted by luck.
function seeded(seed: number): () => number {
  let state = seed % 2147483647;
  if (state <= 0) state += 2147483646;
  return () => {
    state = (state * 16807) % 2147483647;
    return state / 2147483647;
  };
}

const long = [
  item("One", "a", ["a", "b", "c"]),
  item("Two", "c", ["a", "c"]),
  item("Three", "b", ["b", "a"]),
];

const order = (set: StudyItem[]) => set.map((one) => one.prompt).join("|");
/** The same prompts, sorted — an order-free fingerprint of which items are here. */
const prompts = (set: StudyItem[]) => [...set].map((one) => one.prompt).sort().join("|");

// The same members, and really moved. A "shuffle" that hands back what it was given
// is the bug this rule exists to prevent, so it has to be able to fail here.
let moved = false;
for (let seed = 1; seed <= 20; seed += 1) {
  const dealt = sitting(long, seeded(seed));
  ok(dealt.length === long.length, `seed ${seed}: a sitting is the same number of items`);
  ok(
    prompts(dealt) === prompts(long),
    `seed ${seed}: the items changed rather than just their order`,
  );
  if (order(dealt) !== order(long)) moved = true;
  for (const one of dealt) {
    const from = long.find((original) => original.prompt === one.prompt);
    ok(from !== undefined, `seed ${seed}: "${one.prompt}" is not one of the set's items`);
    ok(
      [...one.options].sort().join("|") === [...(from?.options ?? [])].sort().join("|"),
      `seed ${seed}: the options of "${one.prompt}" are the same options, reordered`,
    );
    // The answer rides on its option's *text*, not on where it sits — which is what
    // lets the order be dealt again without marking changing.
    ok(
      one.options.includes(one.answer),
      `seed ${seed}: the answer to "${one.prompt}" is still one of its options`,
    );
  }
}
ok(moved, "at least one seed must deal a different order, or this is not a shuffle");

// Marking is untouched by the deal: answering a sitting with its own answers is all
// right, and a different option is still wrong.
const dealt = sitting(long, seeded(7));
const answers: Record<number, string> = {};
dealt.forEach((one, at) => {
  answers[at] = one.answer;
});
ok(score(dealt, answers).right === dealt.length, "answering a sitting with its answers is right");
const other = dealt[0].options.find((one) => one !== dealt[0].answer);
ok(other !== undefined && !right(dealt[0], other), "and a different option is still wrong");

// With a source that always answers the same, the order it deals is the same order
// every time — so "shuffled" cannot pass by being a coin toss either.
const fixed = () => 0;
ok(
  order(sitting(long, fixed)) === order(sitting(long, fixed)),
  "the same source deals the same order twice",
);
ok(
  [...shuffled([1, 2, 3], fixed)].sort().join("|") === "1|2|3",
  "and what it deals is a permutation of what it was given",
);
// Nothing to deal, and one thing to deal, are both fine: no crash, no loop.
ok(shuffled([], fixed).length === 0, "nothing deals as nothing");
ok(shuffled([7], fixed).join() === "7", "one item deals as itself");

console.log("quiz.check.ts: all checks passed");