import { useState } from "react";
import type { QuestionAnswer, QuestionEvent } from "../types";

/**
 * The wire shape an answer travels in, built from the two arrays this prompt
 * collects. One definition, because the composer and the Scheduled panel both
 * submit an answer and both must build it the same way.
 */
export function answerFrom(selected: string[][], free: string[]): QuestionAnswer[] {
  return selected.map((picked, index) => ({
    index,
    selected: picked,
    freeform: free[index] ?? "",
  }));
}

/**
 * The assistant asked a question mid-run, and the run is parked until this is
 * answered.
 *
 * Every question offers a typed answer next to its options, because the options
 * are the model's guesses and the user is the one who actually knows. The
 * answer is sent as one batch — one round trip for the whole call, so several
 * questions do not each cost a step.
 */
export function QuestionPrompt({
  request,
  onSubmit,
  onSkip,
}: {
  request: QuestionEvent;
  onSubmit: (requestId: string, selected: string[][], free: string[]) => void;
  onSkip: (requestId: string) => void;
}) {
  const [selected, setSelected] = useState<string[][]>(() => request.questions.map(() => []));
  const [free, setFree] = useState<string[]>(() => request.questions.map(() => ""));

  const toggle = (q: number, label: string, multi: boolean) => {
    setSelected((prev) => {
      const next = prev.map((s) => [...s]);
      const at = next[q].indexOf(label);
      if (multi) {
        if (at >= 0) next[q].splice(at, 1);
        else next[q].push(label);
      } else {
        next[q] = at >= 0 ? [] : [label];
      }
      return next;
    });
  };

  return (
    <div className="mb-2 rounded-xl border border-sky-900/60 bg-sky-950/30 px-3 py-2.5">
      <p className="mb-2 text-[11px] font-medium tracking-wide text-sky-300 uppercase">
        The assistant is asking
      </p>
      <div className="flex flex-col gap-3">
        {request.questions.map((q, qi) => (
          <div key={qi}>
            <p className="text-[13px] text-neutral-200">
              {q.header && (
                <span className="mr-1.5 text-[11px] text-neutral-500">[{q.header}]</span>
              )}
              {q.question}
            </p>
            {q.options.length > 0 && (
              <div className="mt-1.5 flex flex-wrap gap-1.5">
                {q.options.map((o) => {
                  const on = selected[qi]?.includes(o.label);
                  return (
                    <button
                      key={o.label}
                      onClick={() => toggle(qi, o.label, q.multiSelect)}
                      title={o.description || undefined}
                      aria-pressed={on}
                      className={`rounded-lg border px-2 py-1 text-[12px] transition ${
                        on
                          ? "border-sky-600 bg-sky-500/15 text-sky-200"
                          : "border-neutral-700 text-neutral-400 hover:border-neutral-600 hover:text-neutral-200"
                      }`}
                    >
                      {o.label}
                    </button>
                  );
                })}
              </div>
            )}
            <input
              value={free[qi] ?? ""}
              onChange={(e) =>
                setFree((prev) => prev.map((v, i) => (i === qi ? e.target.value : v)))
              }
              placeholder={q.options.length > 0 ? "Or type your own answer…" : "Your answer…"}
              className="mt-1.5 w-full rounded-lg border border-neutral-800 bg-neutral-950 px-2 py-1 text-[12px] text-neutral-200 outline-hidden transition focus:border-neutral-600"
            />
          </div>
        ))}
      </div>
      <div className="mt-2.5 flex items-center gap-2">
        <button
          onClick={() => onSubmit(request.requestId, selected, free)}
          className="rounded-lg bg-sky-600 px-3 py-1 text-[13px] font-medium text-accent-fg transition hover:bg-sky-500"
        >
          Send answer
        </button>
        <button
          onClick={() => onSkip(request.requestId)}
          className="rounded-lg px-2 py-1 text-[12px] text-neutral-500 transition hover:text-neutral-300"
          title="Let the assistant carry on without an answer"
        >
          Skip
        </button>
      </div>
    </div>
  );
}
