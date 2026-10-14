import { useState } from "react";
import { citations } from "../lib/citation";
import { quantities, right, score, sitting } from "../lib/quiz";
import { Icon } from "./Icon";
import type { StudySetRow } from "../types";

interface Props {
  /** The set being taken. */
  set: StudySetRow;
  /** The notebook's source names, for resolving what an item came from. */
  sources: string[];
  onClose: () => void;
}

/**
 * Taking a study set: a quiz a question at a time, or a stack of cards to flip.
 *
 * The window's half of `make_study_set`. The set is drawn from what was *stored* —
 * items are objects rather than prose to be parsed — so this component decides
 * nothing about what a set is. It decides what the reader sees, which choice they
 * made, and what that scored; "which choice is right" and "what has it scored so
 * far" live in `lib/quiz.ts`, once, so the number on this screen is the same number
 * the check script asserts.
 *
 * An item's source goes through `citations`, the same comparison the answer chips
 * make: an item from a source that has since been removed says so rather than
 * naming something the reader cannot find.
 */
export function StudySetDialog({ set, sources, onClose }: Props) {
  const [index, setIndex] = useState(0);
  const [answers, setAnswers] = useState<Record<number, string>>({});
  const [flipped, setFlipped] = useState(false);
  // The order is dealt once when the set opens and again on *Start again* — per
  // attempt rather than per render, or the question would reshuffle while the reader
  // was reading it. It is the same items either way, so the counts below are the
  // set's own.
  const [dealt, setDealt] = useState(() => sitting(set.items, Math.random));

  const quiz = set.kind === "quiz";
  const finished = index >= set.items.length;
  const item = dealt[Math.min(index, dealt.length - 1)];
  const chosen = answers[index];
  const result = score(dealt, answers);

  const restart = () => {
    setIndex(0);
    setAnswers({});
    setFlipped(false);
    setDealt(sitting(set.items, Math.random));
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-neutral-950/60 pt-16"
      onMouseDown={onClose}
      onKeyDown={(e) => {
        if (e.key === "Escape") onClose();
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={`Study set: ${set.title}`}
        className="flex max-h-[calc(100vh-8rem)] w-[34rem] max-w-[calc(100vw-2rem)] flex-col overflow-hidden rounded-xl border border-neutral-800 bg-neutral-900 shadow-2xl"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2 border-b border-neutral-800 px-4 py-3">
          <span className="min-w-0 flex-1 truncate text-sm font-medium text-neutral-200">
            {set.title}
          </span>
          <span className="shrink-0 text-[11px] text-neutral-500">
            {quantities(set.items.length, set.kind)} · {quiz ? "a quiz" : "flashcards"}
          </span>
          <button
            autoFocus
            onClick={onClose}
            title="Close"
            className="shrink-0 text-neutral-500 transition hover:text-neutral-200"
          >
            <Icon name="close" size={15} />
          </button>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto p-4">
          {set.items.length === 0 ? (
            // Only reachable from a row that was damaged between writes: the
            // validator refuses an empty set, so this is the store's read failing
            // safe rather than a shape the model can produce.
            <p className="text-[13px] text-neutral-400">
              This set arrived with nothing in it. Ask for it again.
            </p>
          ) : finished ? (
            <>
              <div className="text-[11px] tracking-wide text-neutral-500 uppercase">
                {quiz ? "Score" : "That is the set"}
              </div>
              {quiz && (
                <p className="mt-1 text-[15px] text-neutral-100">
                  {result.right} of {set.items.length} right
                </p>
              )}
              {quiz && result.wrong.length > 0 && (
                <>
                  <div className="mt-4 text-[11px] tracking-wide text-neutral-500 uppercase">
                    What to look at again
                  </div>
                  <ul className="mt-1 flex flex-col gap-1.5">
                    {result.wrong.map((missed) => (
                      <li key={missed} className="text-[12px] leading-relaxed text-neutral-400">
                        {dealt[missed].prompt}{" "}
                        <span className="text-neutral-200">{dealt[missed].answer}</span>
                      </li>
                    ))}
                  </ul>
                </>
              )}
            </>
          ) : quiz ? (
            <>
              <div className="text-[11px] tracking-wide text-neutral-500 uppercase">
                Question {index + 1} of {set.items.length}
              </div>
              <p className="mt-1 text-[14px] leading-relaxed text-neutral-100">{item.prompt}</p>
              <div className="mt-3 flex flex-col gap-1.5">
                {item.options.map((option) => {
                  // Before a choice is made every option looks the same; after, the
                  // right one is marked and so is the one that was picked, which is
                  // the only useful thing to show a reader who got it wrong.
                  const isRight = right(item, option);
                  const picked = chosen === option;
                  const tone =
                    chosen === undefined
                      ? "border-neutral-700 text-neutral-300 hover:border-neutral-600 hover:text-neutral-100"
                      : isRight
                        ? "border-emerald-800/70 bg-emerald-950/30 text-emerald-200"
                        : picked
                          ? "border-red-900/70 bg-red-950/30 text-red-200"
                          : "border-neutral-800/70 text-neutral-500";
                  return (
                    <button
                      key={option}
                      disabled={chosen !== undefined}
                      onClick={() => setAnswers({ ...answers, [index]: option })}
                      className={`rounded-lg border px-3 py-2 text-left text-[13px] transition duration-150 ${tone}`}
                    >
                      {option}
                      {chosen !== undefined && isRight && (
                        <span className="ml-2 text-[11px] opacity-80">right</span>
                      )}
                      {picked && !isRight && (
                        <span className="ml-2 text-[11px] opacity-80">what you chose</span>
                      )}
                    </button>
                  );
                })}
              </div>
              {chosen !== undefined && (
                <div className="mt-3">
                  {item.why !== "" && (
                    <p className="text-[12px] leading-relaxed text-neutral-400">{item.why}</p>
                  )}
                  <Cite source={item.source} sources={sources} />
                </div>
              )}
            </>
          ) : (
            <>
              <div className="text-[11px] tracking-wide text-neutral-500 uppercase">
                Card {index + 1} of {set.items.length}
              </div>
              <button
                onClick={() => setFlipped(true)}
                title="Show the other side"
                className="mt-2 w-full rounded-xl border border-neutral-800 bg-neutral-950/60 px-4 py-6 text-center text-[14px] leading-relaxed text-neutral-100 transition duration-150 hover:border-neutral-700"
              >
                {item.prompt}
              </button>
              {flipped ? (
                <div className="mt-3 rounded-xl bg-neutral-950/60 p-3">
                  <p className="text-[14px] leading-relaxed text-neutral-100">{item.answer}</p>
                  {item.why !== "" && (
                    <p className="mt-2 text-[12px] leading-relaxed text-neutral-400">{item.why}</p>
                  )}
                  <Cite source={item.source} sources={sources} />
                </div>
              ) : (
                <p className="mt-2 text-[12px] text-neutral-500">
                  Answer it to yourself, then turn the card over.
                </p>
              )}
            </>
          )}
        </div>

        <div className="flex items-center gap-2 border-t border-neutral-800 px-4 py-3">
          <span className="flex-1 text-[12px] text-neutral-500">
            {quiz && !finished && result.asked > 0
              ? `${result.right} of ${result.asked} right so far`
              : ""}
          </span>
          {set.items.length > 0 && finished && (
            <button
              onClick={restart}
              className="rounded-lg border border-neutral-700 px-2.5 py-1 text-xs text-neutral-300 transition hover:border-neutral-600 hover:text-neutral-100"
            >
              {quiz ? "Start again" : "Go through them again"}
            </button>
          )}
          {set.items.length > 0 && !finished && quiz && chosen !== undefined && (
            <button
              onClick={() => {
                setFlipped(false);
                setIndex(index + 1);
              }}
              className="rounded-lg bg-sky-600 px-2.5 py-1 text-xs text-white transition hover:bg-sky-500"
            >
              {index + 1 === set.items.length ? "See the score" : "Next question"}
            </button>
          )}
          {set.items.length > 0 && !finished && !quiz && (
            <>
              <button
                disabled={flipped}
                onClick={() => setFlipped(true)}
                className="rounded-lg border border-neutral-700 px-2.5 py-1 text-xs text-neutral-300 transition hover:border-neutral-600 hover:text-neutral-100 disabled:opacity-40"
              >
                Show the answer
              </button>
              <button
                disabled={!flipped}
                onClick={() => {
                  setFlipped(false);
                  setIndex(index + 1);
                }}
                className="rounded-lg bg-sky-600 px-2.5 py-1 text-xs text-white transition hover:bg-sky-500 disabled:opacity-40"
              >
                {index + 1 === set.items.length ? "Finish" : "Next card"}
              </button>
            </>
          )}
          <button
            onClick={onClose}
            className="rounded-lg px-2.5 py-1 text-xs text-neutral-400 transition hover:text-neutral-200"
          >
            Done
          </button>
        </div>
      </div>
    </div>
  );
}

/** The source an item came from, marked when the notebook no longer holds it — the
 *  same comparison, and the same words, as a citation chip under an answer. */
function Cite({ source, sources }: { source: string; sources: string[] }) {
  if (source === "") return null;
  const [cite] = citations([source], sources);
  if (!cite) return null;
  return (
    <p className="mt-2 text-[11px]">
      <span className={cite.known ? "text-sky-300" : "text-amber-400"}>
        {cite.name}
        {cite.heading ? ` → ${cite.heading}` : ""}
        {cite.known ? "" : " (gone)"}
      </span>
    </p>
  );
}
