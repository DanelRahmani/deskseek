/**
 * The app's one token estimate, and how it is shown.
 *
 * Both the header meter and the context panel read from here: a second estimate
 * anywhere else would disagree with the number printed right next to it, and the
 * two numbers sit side by side in the same panel.
 */

/** Roughly this many characters per token. Deliberately crude — it is an estimate. */
export const CHARS_PER_TOKEN = 3.6;

/**
 * Characters in, tokens out.
 *
 * Sum the characters first and convert once: converting each part and adding
 * them up rounds differently from rounding the total, and the meter and the
 * panel would then show different numbers for the same conversation.
 */
export function tokensFromChars(chars: number): number {
  return Math.ceil(chars / CHARS_PER_TOKEN);
}

/** Short form for a meter: 948, 12.4k, 1.00M. */
export function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return String(n);
}
