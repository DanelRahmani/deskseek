import { useCallback, useEffect, useState } from "react";
import { api } from "../api";
import { chartSvg, recapSpec } from "../lib/chart";
import { ChartImage } from "./ChartImage";
import { IconButton } from "./Icon";
import type { Recap, SectionTokens, SpendReport } from "../types";

interface Props {
  onClose: () => void;
  /** A chat from the list: the panel closes and that conversation comes up. */
  onOpenChat: (id: string) => void;
}

/** The windows worth asking for. Days, not dates — the panel is a glance. */
const RANGES = [7, 30, 90];

/**
 * Who spent a row, in words. A kind the app does not know yet is shown as its
 * own id rather than dropped: a spender that cannot be named must still be
 * visible, or the list stops adding up to the figure above it.
 */
const KIND_LABELS: Record<string, string> = {
  turn: "Turns",
  summary: "Summaries and folds",
  revision: "In-place revisions",
};

/**
 * The window the panel opens on, and so the one anything outside it should
 * cover when it says "the same window the spend shows".
 */
export const DEFAULT_SPEND_DAYS = 30;

/** `$0.0040` is noise; `$0.004` is the number. Cents once it is worth cents. */
export function money(usd: number): string {
  if (usd === 0) return "$0";
  if (usd < 0.01) return `$${usd.toFixed(4)}`;
  return `$${usd.toFixed(2)}`;
}

function tokens(n: number): string {
  if (n < 1_000) return String(n);
  if (n < 1_000_000) return `${(n / 1_000).toFixed(1)}k`;
  return `${(n / 1_000_000).toFixed(2)}M`;
}

/**
 * `2026-10-12` as `Mon 12 Oct`.
 *
 * Built from the parts rather than from `new Date("2026-10-12")`, which parses as
 * *UTC* midnight and would show the previous day west of Greenwich. The string
 * from the database is already a local day; this only has to spell it.
 */
function dayLabel(day: string): string {
  const [year, month, date] = day.split("-").map(Number);
  if (!year || !month || !date) return day;
  return new Date(year, month - 1, date).toLocaleDateString(undefined, {
    weekday: "short",
    day: "numeric",
    month: "short",
  });
}

function Row({
  label,
  value,
  detail,
  onClick,
}: {
  label: string;
  value: string;
  detail?: string;
  onClick?: () => void;
}) {
  const body = (
    <>
      <span className="min-w-0 flex-1 truncate text-left">{label}</span>
      {detail && <span className="shrink-0 text-[10px] text-neutral-600">{detail}</span>}
      <span className="shrink-0 tabular-nums text-neutral-300">{value}</span>
    </>
  );
  if (!onClick) {
    return <div className="flex items-center gap-2 px-2 py-1.5 text-[12px]">{body}</div>;
  }
  return (
    <button
      onClick={onClick}
      className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-[12px] text-neutral-400 transition duration-150 hover:bg-neutral-800/60 hover:text-neutral-200"
    >
      {body}
    </button>
  );
}

/**
 * One section's tokens as a pie of input against output.
 *
 * Drawn by `chart.ts` rather than with a `conic-gradient`: the slices have to add
 * up to the totals printed above them, and a gradient has no way to be wrong out
 * loud. Nothing is computed here — these are the report's own numbers.
 */
function SectionPie({ section }: { section: SectionTokens }) {
  const name = section.kind === "coding" ? "Coding" : "Chatting";
  const drawn = chartSvg(
    JSON.stringify({
      type: "pie",
      labels: ["Input", "Output"],
      series: [{ name: "tokens", values: [section.promptTokens, section.completionTokens] }],
    }),
  );
  return (
    <div className="px-2 py-1">
      <div className="flex items-center gap-2 text-[12px]">
        <span className="min-w-0 flex-1 truncate text-neutral-400">{name}</span>
        <span className="shrink-0 text-[10px] text-neutral-600">
          {section.turns} turn{section.turns === 1 ? "" : "s"}
        </span>
        <span className="shrink-0 tabular-nums text-neutral-300">
          {tokens(section.promptTokens + section.completionTokens)}
        </span>
      </div>
      {"svg" in drawn && <ChartImage svg={drawn.svg} className="mt-1" />}
    </div>
  );
}

/**
 * The recap as one picture, or a sentence when there is nothing to draw.
 *
 * `recapSpec` returning `null` for an empty window is what keeps this from
 * drawing a row of zero-height bars, which reads as "you did nothing" when it
 * means "this is the wrong window".
 */
function RecapChart({ recap }: { recap: Recap }) {
  const spec = recapSpec(recap);
  if (spec === null) {
    return (
      <p className="px-2 text-[12px] text-neutral-600">
        Nothing to draw for this window — the figures below are all zero.
      </p>
    );
  }
  const drawn = chartSvg(JSON.stringify(spec));
  return "svg" in drawn ? <ChartImage svg={drawn.svg} className="mt-1" /> : null;
}

/**
 * What this has cost, over a window of turns.
 *
 * Read from `turn_usage`, which records one row per turn as the run goes, so a
 * turn that was stopped still appears for what it really spent. The chat list is
 * deliberately capped: the answer to "what is costing me" is the handful of
 * conversations at the top, not every chat ever opened.
 */
export function SpendPanel({ onClose, onOpenChat }: Props) {
  const [days, setDays] = useState(DEFAULT_SPEND_DAYS);
  const [report, setReport] = useState<SpendReport | null>(null);
  const [recap, setRecap] = useState<Recap | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(true);

  const load = useCallback(async (window: number) => {
    setBusy(true);
    try {
      // Both views of one window, in one round trip each and from the same rows:
      // the recap takes its totals from `spend_report`, so asking for them
      // together cannot show two different windows.
      const [spent, shape] = await Promise.all([api.spendReport(window), api.recap(window)]);
      setReport(spent);
      setRecap(shape);
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    void load(days);
  }, [days, load]);

  // The longest bar is the whole width; every other one is its share of it.
  const busiestDay = Math.max(1, ...(report?.days.map((d) => d.costUsd) ?? []));

  // Busiest means most turns, which is not the newest day. The peak hour comes
  // first from the query, which orders hours by turns.
  const busiest = [...(recap?.days ?? [])].sort((a, b) => b.turns - a.turns)[0];
  const peak = recap?.hours[0];

  return (
    <aside className="panel-in-right flex h-full w-80 shrink-0 flex-col border-l border-neutral-800 bg-neutral-950">
      <header className="flex items-center gap-2 border-b border-neutral-800 px-4 py-3">
        <h2 className="text-sm font-semibold">Spend</h2>
        <span className="ml-auto flex items-center gap-1">
          <IconButton
            icon="retry"
            label="Re-read the figures"
            disabled={busy}
            onClick={() => void load(days)}
          />
          <IconButton icon="close" label="Close" onClick={onClose} />
        </span>
      </header>

      <div className="flex gap-1 border-b border-neutral-800 px-4 py-2">
        {RANGES.map((window) => (
          <button
            key={window}
            onClick={() => setDays(window)}
            className={`rounded-md px-2 py-1 text-[11px] transition duration-150 ${
              window === days
                ? "bg-neutral-800 text-neutral-100"
                : "text-neutral-500 hover:text-neutral-300"
            }`}
          >
            {window} days
          </button>
        ))}
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto p-4">
        {error && <p className="text-[12px] text-red-300">{error}</p>}
        {!error && !report && <p className="text-[12px] text-neutral-600">Reading…</p>}

        {report && report.turns === 0 && (
          <p className="text-[13px] leading-relaxed text-neutral-500">
            Nothing in the last {days} days. A turn is recorded here as it finishes, with what it
            actually spent — so this fills in as you use the app.
          </p>
        )}

        {report && report.turns > 0 && (
          <>
            <div className="rounded-xl border border-neutral-800/70 bg-neutral-900/40 p-3">
              <p className="text-2xl font-semibold tabular-nums text-neutral-100">
                {money(report.totalUsd)}
              </p>
              <p className="mt-0.5 text-[11px] text-neutral-500">
                {report.turns} call{report.turns === 1 ? "" : "s"} ·{" "}
                {tokens(report.promptTokens)} in · {tokens(report.completionTokens)} out
              </p>
            </div>

            <h3 className="mt-4 px-2 text-[11px] font-medium uppercase tracking-wide text-neutral-600">
              Where the tokens went
            </h3>
            <div className="mt-1">
              {report.sections.map((section) => (
                <SectionPie key={section.kind} section={section} />
              ))}
            </div>
            <p className="mt-2 px-2 text-[11px] leading-relaxed text-neutral-600">
              Input and output are billed at different rates, and cached input at a fraction of
              fresh — so this is where the tokens went, not what each part cost. The figure at the
              top is the bill.
            </p>

            {recap && (
              <>
                <h3 className="mt-4 px-2 text-[11px] font-medium uppercase tracking-wide text-neutral-600">
                  The shape of it
                </h3>
                <RecapChart recap={recap} />
                <div className="mt-1 space-y-1 px-2 text-[12px]">
                  <div className="flex items-center gap-2">
                    <span className="min-w-0 flex-1 text-neutral-400">Busiest day</span>
                    <span className="shrink-0 text-neutral-300">
                      {busiest ? `${dayLabel(busiest.day)} · ${busiest.turns}` : "—"}
                    </span>
                  </div>
                  <div className="flex items-center gap-2">
                    <span className="min-w-0 flex-1 text-neutral-400">Peak hour</span>
                    <span className="shrink-0 tabular-nums text-neutral-300">
                      {peak ? `${String(peak.hour).padStart(2, "0")}:00 · ${peak.turns}` : "—"}
                    </span>
                  </div>
                </div>
              </>
            )}

            <h3 className="mt-4 px-2 text-[11px] font-medium uppercase tracking-wide text-neutral-600">
              By day
            </h3>
            <div className="mt-1 space-y-1">
              {report.days.map((day) => (
                <div key={day.day} className="px-2 py-1">
                  <div className="flex items-center gap-2 text-[12px]">
                    <span className="min-w-0 flex-1 truncate text-neutral-400">
                      {dayLabel(day.day)}
                    </span>
                    <span className="shrink-0 text-[10px] text-neutral-600">
                      {day.turns} turn{day.turns === 1 ? "" : "s"}
                    </span>
                    <span className="shrink-0 tabular-nums text-neutral-300">
                      {money(day.costUsd)}
                    </span>
                  </div>
                  {/* The bar is the whole point of a per-day list: it makes the
                      heavy day visible without a chart library. */}
                  <div className="mt-1 h-1 w-full overflow-hidden rounded-full bg-neutral-800">
                    <div
                      className="h-full rounded-full bg-sky-500/70"
                      style={{ width: `${(day.costUsd / busiestDay) * 100}%` }}
                    />
                  </div>
                </div>
              ))}
            </div>

            <h3 className="mt-4 px-2 text-[11px] font-medium uppercase tracking-wide text-neutral-600">
              By model
            </h3>
            <div className="mt-1">
              {report.models.map((model) => (
                <Row
                  key={model.model}
                  label={model.model}
                  detail={`${model.turns} call${model.turns === 1 ? "" : "s"}`}
                  value={model.priced ? money(model.costUsd) : "unknown"}
                />
              ))}
            </div>

            <h3 className="mt-4 px-2 text-[11px] font-medium uppercase tracking-wide text-neutral-600">
              By what spent it
            </h3>
            <div className="mt-1">
              {report.kinds.map((kind) => (
                <Row
                  key={kind.kind}
                  label={KIND_LABELS[kind.kind] ?? kind.kind}
                  detail={`${kind.calls} call${kind.calls === 1 ? "" : "s"}`}
                  value={money(kind.costUsd)}
                />
              ))}
            </div>
            {/* Said plainly rather than hidden: a call the user did not type is
                still a call they paid for, and one with no published price is
                counted in tokens and unknown in money — not free. */}
            {report.kinds.some((kind) => !kind.priced) && (
              <p className="mt-2 px-2 text-[11px] leading-relaxed text-amber-300/80">
                Some of these calls have no published price for their model. Their tokens are
                counted above; their money is unknown rather than guessed, so the total is a
                floor rather than the whole bill.
              </p>
            )}

            <h3 className="mt-4 px-2 text-[11px] font-medium uppercase tracking-wide text-neutral-600">
              By conversation
            </h3>
            <div className="mt-1">
              {report.chats.map((chat) => (
                <Row
                  key={chat.chatId}
                  label={chat.title}
                  detail={`${chat.turns} call${chat.turns === 1 ? "" : "s"}`}
                  value={money(chat.costUsd)}
                  onClick={() => onOpenChat(chat.chatId)}
                />
              ))}
            </div>

            <p className="mt-4 px-2 text-[11px] leading-relaxed text-neutral-600">
              Prices are the list rates for the model and the hour, so a figure here is an estimate
              of your bill rather than the bill itself.
            </p>
          </>
        )}
      </div>
    </aside>
  );
}
