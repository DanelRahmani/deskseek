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
