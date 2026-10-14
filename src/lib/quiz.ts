/**
 * Taking a study set: which choice is right, and what the score is so far.
 *
 * Deliberately small, because almost nothing about a set is the window's to
 * decide. What a set *is* — a quiz item offers two to six distinct options and its
 * answer is spelled exactly as one of them is — is checked in Rust
 * (`notebook::study_set`) before a set is ever stored, and that is the only place
 * it can be checked: the refusal happens while the model can still repair its own
 * call, and a set refused there never reaches a reader.
 *
 * What is left here is the half only the window can answer, because only the
 * window has the choices the reader made.
 */
import type { StudyItem } from "../types";

/**
 * Whether a choice is the right one.
 *
 * Exact, and on purpose. The choice *is* one of the strings this app drew, and the
 * answer was normalised to that same spelling before it was stored, so a looser
 * comparison — trim, case-fold, `startsWith` — could only ever turn a wrong answer
 * into a right one.
 */
export function right(item: StudyItem, choice: string): boolean {
  return choice === item.answer;
}

/** What a set has scored so far. */
export interface Score {
  /** How many items have been answered. */
  asked: number;
  /** How many of those were answered right. */
  right: number;
  /** The items answered wrongly, by index, in the order they were asked. */
  wrong: number[];
}

/**
 * The score over a set the reader is part way through.
 *
 * An item they have not reached is not a wrong one: counting it would make the
 * number move as they went, which is the one thing a score must not do — and it
 * would make "one wrong" indistinguishable from "seven unasked".
 */
export function score(items: StudyItem[], answers: Record<number, string>): Score {
  let asked = 0;
  let correct = 0;
  const wrong: number[] = [];
  items.forEach((item, index) => {
    const choice = answers[index];
    if (choice === undefined) return;
    asked += 1;
    if (right(item, choice)) correct += 1;
    else wrong.push(index);
  });
  return { asked, right: correct, wrong };
}

/**
 * "3 questions", "1 card" — spelled rather than made by adding an `s`, which is
 * the plural rule this app has already been caught by once.
 *
 * Here rather than in either caller because both the rail and the dialog count the
 * same set, and two copies of a wording is how the two come to disagree about what
 * a set is called.
 */
export function quantities(items: number, kind: string): string {
  const noun = kind === "quiz" ? "question" : "card";
  return items === 1 ? `1 ${noun}` : `${items} ${noun}s`;
}

/**
 * A copy in a new order: Fisher–Yates, from the random source it is given.
 *
 * The source is a parameter rather than a call to `Math.random` inside, because an
 * order nobody can predict is also an order nobody can test. The check script hands
 * it a seeded generator and holds it to the properties a shuffle has to have — the
 * same members, really moved — and the dialog hands it `Math.random`.
 */
export function shuffled<T>(values: T[], random: () => number): T[] {
  const out = [...values];
  for (let i = out.length - 1; i > 0; i -= 1) {
    const j = Math.floor(random() * (i + 1));
    [out[i], out[j]] = [out[j], out[i]];
  }
  return out;
}

/**
 * A set as one *sitting*: the items in a fresh order, and each item's options in a
 * fresh order of their own.
 *
 * This is the one thing a model cannot be trusted to do — it writes options in the
 * order it thought of them, which is how "the right answer is usually the first
 * one" becomes a strategy rather than a guess. The answer travels on its option's
 * **text**, which is exactly why 14.2 stored it spelled as that option is: a
 * shuffle cannot move an answer away from the choice that is right, so marking is
 * untouched by it.
 *
 * Dealt when the reader opens the set rather than when the set was stored, so the
 * same set is a different quiz each time — which is what makes it worth taking
 * twice.
 */
export function sitting(items: StudyItem[], random: () => number): StudyItem[] {
  return shuffled(items, random).map((item) => ({
    ...item,
    options: shuffled(item.options, random),
  }));
}
