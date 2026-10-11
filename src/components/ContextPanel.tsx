import { useCallback, useEffect, useState } from "react";
import { api } from "../api";
import { Icon } from "./Icon";
import { formatTokens, tokensFromChars } from "../lib/context";
import type { ContextBreakdown } from "../types";

interface Props {
  chatId: string;
  /** The context window this chat's model has, in tokens. */
  limit: number;
  onClose: () => void;
}

/**
 * The bar's slices, in a fixed order so one does not jump about between
 * refreshes. The two greys sit in the middle on purpose: the system prompt and
 * memory are the slices somebody can actually act on, so those should catch the
 * eye rather than the two nobody can change.
 */
const GROUPS: { id: string; label: string; colour: string; advice: string }[] = [
  {
    id: "system",
    label: "System prompt",
    colour: "var(--accent-solid)",
    advice:
      "Most of this is DeskSeek's own guidance rather than your text. The base prompt in Settings → Models & prompt is the part you control.",
  },
  {
    id: "conversation",
    label: "Conversation",
    colour: "var(--n-500)",
    advice: "/compact summarises the chat and carries on from the summary.",
  },
  {
    id: "instructions",
    label: "Instructions",
    colour: "var(--ok-solid)",
    advice: "Turn off the scoped instructions this chat does not need.",
  },
  {
    id: "tools",
    label: "Tool schemas",
    colour: "var(--n-700)",
    advice:
      "Every tool's description is sent on every turn. Disabling MCP servers you are not using, or turning browsing off, is what cuts this.",
  },
  {
    id: "skills",
    label: "Skills",
    colour: "var(--warn-solid)",
    advice: "A skill's whole body is injected when it is enabled. Enable fewer for this chat.",
  },
  {
    id: "memory",
    label: "Memory",
    colour: "var(--danger-solid)",
    advice:
      "The profile is sent every turn and recalled facts are added per turn. Keeping MEMORY.md short is the lever.",
  },
];

/** Below this share of the whole, naming a largest slice is noise. */
const ADVICE_SHARE = 0.25;

/**
 * What one request to this chat is made of.
 *
 * Every number comes from the Rust command, which measures the same strings the
 * run sends — so this explains the request that is really about to go out rather
 * than an approximation of it. Tokens are summed upward from the pieces: a total
 * that did not equal the sum of what is listed under it would be worse than a
 * total that is a few tokens away from the meter.
 */
export function ContextPanel({ chatId, limit, onClose }: Props) {
  const [breakdown, setBreakdown] = useState<ContextBreakdown | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setBreakdown(await api.contextBreakdown(chatId));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, [chatId]);

  useEffect(() => {
    refresh();
    // Live while it is open: the numbers move as a run streams tool results in.
    const timer = setInterval(refresh, 3000);
    return () => clearInterval(timer);
  }, [refresh]);

  const parts = breakdown?.parts ?? [];
  const slices = GROUPS.map((group) => {
    const own = parts.filter((p) => p.group === group.id);
    const chars = own.reduce((sum, p) => sum + p.chars, 0);
    return {
      ...group,
      chars,
      tokens: own.reduce((sum, p) => sum + tokensFromChars(p.chars), 0),
      parts: own.map((p) => ({ label: p.label, tokens: tokensFromChars(p.chars) })),
    };
  }).filter((slice) => slice.chars > 0);

  const totalChars = slices.reduce((sum, slice) => sum + slice.chars, 0);
  const total = slices.reduce((sum, slice) => sum + slice.tokens, 0);
  const share = limit > 0 ? total / limit : 0;

  // Only the biggest slice, and only when it is worth saying: four suggestions
  // at once is the same as none.
  const biggest = [...slices].sort((a, b) => b.chars - a.chars)[0];
  const advice = biggest && biggest.chars / Math.max(1, totalChars) > ADVICE_SHARE ? biggest : null;

  const percent = (chars: number) => ((chars / Math.max(1, totalChars)) * 100).toFixed(0);

  return (
    <aside className="flex h-full w-80 shrink-0 flex-col border-l border-neutral-800 bg-neutral-950">
      <header className="flex items-center justify-between border-b border-neutral-800 px-4 py-3">
        <span className="text-xs font-semibold text-neutral-200">Context</span>
        <button onClick={onClose} className="text-neutral-500 hover:text-neutral-200" title="Close">
          <Icon name="close" size={15} />
        </button>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
        {error && <p className="mb-3 text-[11px] text-red-300">{error}</p>}

        {slices.length === 0 && !error && (
          <p className="text-[11px] text-neutral-600">Nothing to measure yet.</p>
        )}

        {slices.length > 0 && (
          <>
            <div className="flex h-3 overflow-hidden rounded-full bg-neutral-800">
              {slices.map((slice) => (
                <div
                  key={slice.id}
                  style={{
                    width: `${(slice.chars / Math.max(1, totalChars)) * 100}%`,
                    background: slice.colour,
                  }}
                  title={`${slice.label} — ${formatTokens(slice.tokens)} tokens`}
                />
              ))}
            </div>

            <p className="mt-2 text-[11px] leading-relaxed text-neutral-500">
              ~<span className="tabular-nums text-neutral-300">{formatTokens(total)}</span> of{" "}
              <span className="tabular-nums">{formatTokens(limit)}</span> tokens (
              {(share * 100).toFixed(1)}%)
              {limit > total && (
                <>
                  {" "}
                  — about {formatTokens(limit - total)} still free for the reply and the turns after
                  it.
                </>
              )}
            </p>

            {share > 0.8 && (
              <p className="mt-1 text-[11px] text-amber-300">
                Nearly full. /compact summarises this chat and continues from the summary.
              </p>
            )}

            <ul className="mt-4 space-y-2.5">
              {slices.map((slice) => (
                <li key={slice.id}>
                  <div className="flex items-center gap-2">
                    <span
                      className="h-2 w-2 shrink-0 rounded-sm"
                      style={{ background: slice.colour }}
                    />
                    <span className="text-xs text-neutral-300">{slice.label}</span>
                    <span className="ml-auto text-[11px] tabular-nums text-neutral-500">
                      {formatTokens(slice.tokens)} · {percent(slice.chars)}%
                    </span>
                  </div>
                  {slice.parts.length > 1 && (
                    <ul className="mt-1 space-y-0.5 pl-4">
                      {slice.parts.map((part) => (
                        <li key={part.label} className="flex gap-2 text-[11px] text-neutral-600">
                          <span className="truncate">{part.label}</span>
                          <span className="ml-auto shrink-0 tabular-nums">
                            {formatTokens(part.tokens)}
                          </span>
                        </li>
                      ))}
                    </ul>
                  )}
                </li>
              ))}
            </ul>

            {advice && (
              <p className="mt-4 rounded-lg border border-neutral-800 bg-neutral-900/60 px-2 py-1.5 text-[11px] leading-relaxed text-neutral-400">
                <span className="text-neutral-300">{advice.label}</span> is the largest slice.{" "}
                {advice.advice}
              </p>
            )}
          </>
        )}

        {breakdown && breakdown.images > 0 && (
          <p className="mt-2 text-[11px] leading-relaxed text-neutral-600">
            Plus {breakdown.images} image{breakdown.images === 1 ? "" : "s"}. The API charges for
            pictures by picture rather than by the characters of their data URL, so they are not
            counted above — counting them by size would overstate every image several times over.
          </p>
        )}
      </div>
    </aside>
  );
}
