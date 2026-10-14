/**
 * Checks for the audio overview's pure parts. Plain Node, no test framework:
 *
 *   npm run check
 *
 * Both of these are the kind of formatting that fails quietly: "0 minutes"
 * renders as a number, "$0.00" renders as a price, and neither throws. So each
 * edge case is asserted here instead.
 */
import { dictatedText, dictationName, MAX_DICTATION_SECONDS, minutes, price } from "./audio.ts";

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

// A dictation must not glue words together, and must not lead with a space.
assert(dictatedText("", "hello there") === "hello there", "an empty box takes the words alone");
assert(dictatedText("write me a", "poem") === "write me a poem", "a space joins them");
assert(dictatedText("write me a ", "poem") === "write me a poem", "and is not doubled");
assert(dictatedText("write me a\n", "poem") === "write me a\npoem", "a newline is already a break");
assert(dictatedText("done", "  spaced  ") === "done spaced", "the words are trimmed");
// Nothing heard leaves the draft exactly as it was, rather than adding a space.
assert(dictatedText("done", "   ") === "done", "silence changes nothing");

// The extension is what the endpoint sniffs, so the mapping is pinned here.
assert(dictationName("audio/webm;codecs=opus") === "dictation.webm", "the browser default");
assert(dictationName("audio/webm") === "dictation.webm", "without parameters");
assert(dictationName("audio/mp4") === "dictation.m4a", "Safari's container keeps its extension");
assert(dictationName("audio/ogg; codecs=opus") === "dictation.ogg", "with a space before the parameter");
assert(dictationName("audio/mpeg") === "dictation.mp3", "an mp3 is an mp3");
assert(dictationName("audio/wav") === "dictation.wav", "and a wav is a wav");
// Anything unrecognised still gets a name an endpoint will accept rather than an
// extensionless part.
assert(dictationName("application/octet-stream") === "dictation.webm", "an unknown type has a fallback");
assert(dictationName("") === "dictation.webm", "and so does no type at all");
assert(MAX_DICTATION_SECONDS === 300, "five minutes is the cap");

console.log("audio.check.ts: all checks passed (minutes never read zero, an unknown rate never reads free, and a dictation lands in the box to edit)");
