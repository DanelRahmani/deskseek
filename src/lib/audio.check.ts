/**
 * Checks for the audio overview's pure parts. Plain Node, no test framework:
 *
 *   npm run check
 *
 * Both of these are the kind of formatting that fails quietly: "0 minutes"
 * renders as a number, "$0.00" renders as a price, and neither throws. So each
 * edge case is asserted here instead.
 */
import { minutes, price } from "./audio.ts";

function assert(ok: boolean, message: string): void {
  if (!ok) throw new Error(`FAIL: ${message}`);
}

// A length never rounds away to nothing.
assert(minutes(0) === "about 1 minute", "an empty clip is still a minute");
assert(minutes(7) === "about 1 minute", "seven seconds is one minute");
// 3,500 characters at 900 a minute: 234 seconds, and seconds are never shown.
assert(minutes(60) === "about 1 minute", "a minute is one minute");
assert(minutes(90) === "about 2 minutes", "ninety seconds rounds up");
assert(minutes(234) === "about 4 minutes", "the cap is about four minutes");

// An unknown rate is not free.
assert(price(0, false) === "price unknown", "no published rate is unknown money");
assert(price(1.5, false) === "price unknown", "and a number beside an unknown rate is still unknown");
assert(price(0, true) === "$0.00", "a published rate of zero really is free");
// A fraction of a cent is a price, not nothing.
assert(price(0.004, true) === "under $0.01", "a fraction of a cent is said, not rounded away");
assert(price(0.01, true) === "$0.01", "a cent is a cent");
assert(price(0.055, true) === "$0.06", "and it is rounded to cents");
assert(price(12.5, true) === "$12.50", "dollars keep their cents");

console.log("audio.check.ts: all checks passed (minutes never read zero, and an unknown rate never reads free)");
