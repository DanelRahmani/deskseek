import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { minutes, price } from "../lib/audio";
import type { AudioClip, AudioScript, SpeechEstimate } from "../types";
import { Icon } from "./Icon";

/**
 * An audio overview: a script read out loud over this conversation.
 *
 * Two steps on purpose, and the order is the point. The script is written first
 * — a cheap call on the summariser's model — and then **priced before it is
 * spoken**, because speech is billed by the minute and this is the only place in
 * the app where a single action can cost real money. So the dialog is a quote:
 * what will be said, who will say it, about how long it is, and what that costs.
 * Nothing is sent until the button is pressed.
 *
 * The script can be edited, and the price follows it. That is why the estimate
 * comes back from the backend rather than being computed here — the arithmetic
 * and the published rate both live on that side, and the number quoted has to be
 * the number recorded.
 */
export function AudioOverviewDialog({
  chatId,
  onNotice,
  onClose,
}: {
  chatId: string;
  onNotice: (text: string) => void;
  onClose: () => void;
}) {
  const [script, setScript] = useState<AudioScript | null>(null);
  const [text, setText] = useState("");
  const [estimate, setEstimate] = useState<SpeechEstimate | null>(null);
  const [clip, setClip] = useState<AudioClip | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [writing, setWriting] = useState(true);
  const [speaking, setSpeaking] = useState(false);

  /**
   * Writes a script, and forgets whatever was made from the last one. Also the
   * "Write another" button, so there is one path to a new script rather than a
   * mount path and a click path that can drift apart.
   */
  const writeScript = async () => {
    setWriting(true);
    setFailure(null);
    setClip(null);
    try {
      const written = await api.audioScript(chatId);
      setScript(written);
      setText(written.script);
    } catch (e) {
      setFailure(String(e));
    } finally {
      setWriting(false);
    }
  };

  // Focus goes back where it was, the same convention the palette and the
  // shortcut sheet keep: a dialog is a detour, not a destination.
  const restore = useRef<Element | null>(null);
  if (restore.current === null) restore.current = document.activeElement;
  useEffect(() => {
    const back = restore.current;
    return () => {
      if (back instanceof HTMLElement && document.contains(back)) back.focus();
    };
  }, []);

  useEffect(() => {
    void writeScript();
    // Once per chat: the dialog is opened for one conversation and closed with
    // it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [chatId]);

  // The price of what is in the box, not of what came back: an edit that made
  // the clip longer has to move the number. Long enough that typing a sentence
  // is one round trip rather than forty.
  useEffect(() => {
    if (!script) return;
    let alive = true;
    const timer = window.setTimeout(() => {
      api
        .audioEstimate(text)
        .then((next) => alive && setEstimate(next))
        .catch(() => alive && setEstimate(null));
    }, 250);
    return () => {
      alive = false;
      window.clearTimeout(timer);
    };
  }, [script, text]);

  const speak = async () => {
    setSpeaking(true);
    setFailure(null);
    try {
      const made = await api.audioOverview(chatId, text);
      setClip(made);
      onNotice(
        `Audio overview saved as ${made.name} in this chat's folder — ${price(made.usd, made.priced)}.`,
      );
    } catch (e) {
      setFailure(String(e));
    } finally {
      setSpeaking(false);
    }
  };


  const busy = writing || speaking;
  const over = estimate !== null && estimate.chars > estimate.limit;

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
        aria-label="Audio overview"
        className="flex w-[34rem] max-w-[calc(100vw-2rem)] flex-col overflow-hidden rounded-xl border border-neutral-800 bg-neutral-900 shadow-2xl"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2 border-b border-neutral-800 px-4 py-3">
          <span className="text-sm font-medium text-neutral-200">Audio overview</span>
          {script && (
            <span className="text-xs text-neutral-500">
              {script.endpoint} · {script.model} · {script.voice}
            </span>
          )}
          <button
            autoFocus
            onClick={onClose}
            className="ml-auto text-neutral-500 transition hover:text-neutral-200"
            title="Close"
          >
            <Icon name="close" size={15} />
          </button>
        </div>

        <div className="flex flex-col gap-3 p-4">
          {clip ? (
            <>
              {/* The clip is the answer, so the player replaces the quote. */}
              <audio controls src={clip.dataUrl} className="w-full" />
              <p className="text-xs text-neutral-500">
                {minutes(clip.seconds)} · {price(clip.usd, clip.priced)} · saved in this chat's
                folder as <span className="font-mono text-neutral-400">{clip.name}</span>
              </p>
              <div className="flex items-center gap-2">
                <button
                  onClick={() => void api.revealPath(clip.path)}
                  className="rounded-lg border border-neutral-700 px-2.5 py-1 text-xs text-neutral-300 transition hover:border-neutral-600 hover:text-neutral-100"
                >
                  Show in folder
                </button>
                <button
                  onClick={onClose}
                  className="rounded-lg px-2.5 py-1 text-xs text-neutral-400 transition hover:text-neutral-200"
                >
                  Done
                </button>
              </div>
            </>
          ) : (
            <>
              {writing ? (
                <p className="text-[13px] text-neutral-400">Writing the script…</p>
              ) : script ? (
                <>
                  <label className="text-[11px] font-medium tracking-wide text-neutral-500 uppercase">
                    What will be said
                  </label>
                  <textarea
                    value={text}
                    onChange={(e) => setText(e.target.value)}
                    rows={12}
                    spellCheck={false}
                    className="w-full resize-y rounded-lg border border-neutral-700 bg-neutral-950 px-3 py-2 text-[13px] leading-relaxed text-neutral-200"
                  />
                  <p className="text-xs text-neutral-500">
                    {estimate
                      ? `${estimate.chars.toLocaleString()} characters · ${minutes(estimate.seconds)} · ${price(estimate.usd, estimate.priced)}`
                      : "Pricing…"}
                  </p>
                </>
              ) : null}

              {failure && (
                <p className="rounded-lg border border-amber-900/60 bg-amber-950/30 px-3 py-2 text-xs text-amber-300">
                  {failure}
                </p>
              )}
              {over && (
                <p className="text-xs text-amber-400">
                  One request reads out at most 3,500 characters — this will be refused rather
                  than trimmed.
                </p>
              )}

              <div className="flex items-center gap-2">
                <button
                  disabled={busy || !script || over || text.trim().length === 0}
                  onClick={() => void speak()}
                  className="rounded-lg border border-sky-700 bg-sky-950/50 px-3 py-1.5 text-xs font-medium text-sky-200 transition hover:border-sky-600 disabled:opacity-40"
                >
                  {speaking ? "Speaking…" : "Create the clip"}
                </button>
                <button
                  disabled={busy}
                  onClick={() => void writeScript()}
                  className="rounded-lg px-2.5 py-1.5 text-xs text-neutral-400 transition hover:text-neutral-200 disabled:opacity-40"
                >
                  Write another
                </button>
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
