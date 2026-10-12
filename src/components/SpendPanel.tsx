import { useCallback, useEffect, useState } from "react";
import { api } from "../api";
import { IconButton } from "./Icon";
import type { SpendReport } from "../types";

interface Props {
  onClose: () => void;
  /** A chat from the list: the panel closes and that conversation comes up. */
  onOpenChat: (id: string) => void;
}

/** The windows worth asking for. Days, not dates — the panel is a glance. */
const RANGES = [7, 30, 90];

/** `$0.0040` is noise; `$0.004` is the number. Cents once it is worth cents. */
function money(usd: number): string {
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
 * What this has cost, over a window of turns.
 *
 * Read from `turn_usage`, which records one row per turn as the run goes, so a
 * turn that was stopped still appears for what it really spent. The chat list is
 * deliberately capped: the answer to "what is costing me" is the handful of
 * conversations at the top, not every chat ever opened.
 */
export function SpendPanel({ onClose, onOpenChat }: Props) {
  const [days, setDays] = useState(30);
  const [report, setReport] = useState<SpendReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(true);

  const load = useCallback(async (window: number) => {
    setBusy(true);
    try {
      setReport(await api.spendReport(window));
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
  const busiestDay = Math.max(1, ...(report?.days.map((d) => d.cost_usd) ?? []));

  return (
    <aside className="flex h-full w-80 shrink-0 flex-col border-l border-neutral-800 bg-neutral-950">
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
                {money(report.total_usd)}
              </p>
              <p className="mt-0.5 text-[11px] text-neutral-500">
                {report.turns} turn{report.turns === 1 ? "" : "s"} ·{" "}
                {tokens(report.prompt_tokens)} in · {tokens(report.completion_tokens)} out
              </p>
            </div>

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
                      {money(day.cost_usd)}
                    </span>
                  </div>
                  {/* The bar is the whole point of a per-day list: it makes the
                      heavy day visible without a chart library. */}
                  <div className="mt-1 h-1 w-full overflow-hidden rounded-full bg-neutral-800">
                    <div
                      className="h-full rounded-full bg-sky-500/70"
                      style={{ width: `${(day.cost_usd / busiestDay) * 100}%` }}
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
                  detail={`${model.turns} turn${model.turns === 1 ? "" : "s"}`}
                  value={money(model.cost_usd)}
                />
              ))}
            </div>

            <h3 className="mt-4 px-2 text-[11px] font-medium uppercase tracking-wide text-neutral-600">
              By conversation
            </h3>
            <div className="mt-1">
              {report.chats.map((chat) => (
                <Row
                  key={chat.chat_id}
                  label={chat.title}
                  detail={`${chat.turns} turn${chat.turns === 1 ? "" : "s"}`}
                  value={money(chat.cost_usd)}
                  onClick={() => onOpenChat(chat.chat_id)}
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
