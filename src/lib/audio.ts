/**
 * How an audio overview is described before and after it is made. Pure string
 * work, so the two things that must never be wrong about a price — that an
 * unknown rate does not read as free, and that a clip is never quoted as zero
 * minutes — are pinned by audio.check.ts rather than trusted to the JSX.
 */

/**
 * "about 4 minutes" for a length, rounded to the nearest minute and never down
 * to none: an estimate that says "0 minutes" reads as a broken number rather
 * than as a short clip.
 */
export function minutes(seconds: number): string {
  const whole = Math.max(1, Math.round(seconds / 60));
  return `about ${whole} minute${whole === 1 ? "" : "s"}`;
}

/**
 * The money in a sentence, or the honest admission that there is not one.
 *
 * The distinction is the whole point: a clip whose endpoint publishes no
 * per-minute rate costs money that this app cannot name, and "$0.00" would be a
 * claim rather than a silence. Same rule the spend view keeps.
 */
export function price(usd: number, priced: boolean): string {
  if (!priced) return "price unknown";
  // Under a cent is a real answer — a minute of speech is often a fraction of
  // one — so it is said rather than rounded away.
  if (usd > 0 && usd < 0.01) return "under $0.01";
  return `$${usd.toFixed(2)}`;
}

/**
 * The longest recording the dictation button will send.
 *
 * Speech to text is billed by the minute and every byte of the recording travels
 * through the window before it is transcribed, so an accidental twenty-minute
 * hold is both a bill and a frozen frame. The recorder stops *itself* here
 * rather than refusing afterwards: what was said is kept, and the button says
 * why it stopped.
 */
export const MAX_DICTATION_SECONDS = 300;

/**
 * What a recording is called on the wire.
 *
 * The endpoint sniffs the **extension**, not the MIME type a browser reports —
 * so this is the one place that decides it, and a wrong one is a 400 that reads
 * as a bad key. Browsers record WebM/Opus by default and MP4/AAC on some
 * platforms; everything else the endpoints accept is named after its own
 * container.
 */
export function dictationName(mime: string): string {
  const base = mime.split(";")[0].trim().toLowerCase();
  if (base === "audio/mp4" || base === "audio/m4a" || base === "audio/x-m4a") return "dictation.m4a";
  if (base === "audio/ogg" || base === "audio/opus") return "dictation.ogg";
  if (base === "audio/mpeg" || base === "audio/mp3") return "dictation.mp3";
  if (base === "audio/wav" || base === "audio/x-wav" || base === "audio/wave") return "dictation.wav";
  // WebM is the browser default and the fallback on purpose: it is what a
  // Chromium webview records when it says nothing else, and it is the container
  // both endpoints accept when they are handed a name they did not expect.
  return "dictation.webm";
}

/**
 * Where a transcription lands: appended to the draft, never replacing it.
 *
 * The rule the prompt library already keeps is that text arrives in the box to
 * be edited, and a dictation is the most literal case of it. One space between
 * it and what was there — but not at the start of an empty box, and not doubled
 * when the draft already ends in whitespace, because a leading space in an empty
 * composer reads as a mistake.
 */
export function dictatedText(draft: string, heard: string): string {
  const words = heard.trim();
  if (words.length === 0) return draft;
  if (draft.length === 0 || /\s$/.test(draft)) return draft + words;
  return `${draft} ${words}`;
}

