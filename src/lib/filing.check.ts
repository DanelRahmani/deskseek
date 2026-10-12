/**
 * Checks for the attachment filing rule in filing.ts. Plain Node, no framework:
 *
 *   npm run check
 *
 * The rule is the promise "a file you add to a chat is in that chat's folder".
 * So the assertions are about which route a file takes and whether that route
 * files it — not about how a `File` is read, which is browser work.
 *
 * Three real holes prompted this: an image was never filed at all, a small text
 * file *picked with the paperclip* was not filed while the same file *dropped*
 * was, and a file past the cap was added as a bare name with no copy and no
 * word said about it.
 */
import {
  landsInChatFolder,
  MAX_IMAGE_BYTES,
  MAX_SEND_BYTES,
  MAX_TEXT_BYTES,
  refusalMessage,
  routeForAttachment,
  type AttachmentRoute,
} from "./filing.ts";

function ok(condition: boolean, message: string): void {
  if (!condition) throw new Error(`FAIL: ${message}`);
}

const MB = 1024 * 1024;

// A picture is shown to the model, up to the image cap and no further.
ok(routeForAttachment("image/png", 100 * 1024) === "image", "a small picture is an image");
ok(routeForAttachment("image/jpeg", MAX_IMAGE_BYTES) === "image", "the image cap is inclusive");
ok(routeForAttachment("image/gif", MAX_IMAGE_BYTES + 1) === "refuse", "past the cap is refused");

// Small text is pasted into the conversation. It is still filed — that is the
// half the paperclip used to skip.
ok(routeForAttachment("text/plain", 10) === "text", "a small text file is text");
ok(routeForAttachment("application/json", 10) === "text", "json counts as text");
ok(routeForAttachment("text/csv", MAX_TEXT_BYTES) === "text", "the text cap is inclusive");
ok(routeForAttachment("text/plain", MAX_TEXT_BYTES + 1) === "file", "long text moves as a file");

// Everything else is a file up to the send cap, and refused past it — a refusal
// that says so, rather than the bare name the old early-return handed back.
ok(routeForAttachment("application/pdf", 4 * MB) === "file", "a 4 MB document is a file");
ok(routeForAttachment("application/pdf", MAX_SEND_BYTES) === "file", "the send cap is inclusive");
ok(
  routeForAttachment("application/pdf", MAX_SEND_BYTES + 1) === "refuse",
  "past the send cap is refused",
);
ok(
  routeForAttachment("application/octet-stream", MAX_SEND_BYTES + 1) === "refuse",
  "an unknown type is bounded as well",
);

// THE PROMISE: with a chat to file it in, every route that is not refused lands
// in that chat's folder. This is the invariant the three original holes broke.
for (const route of ["image", "text", "file"] as AttachmentRoute[]) {
  ok(landsInChatFolder(route, true), `${route} must land in the chat's folder`);
}
ok(!landsInChatFolder("refuse", true), "a refused file is not filed");

// The one documented exception, asserted rather than assumed.
ok(!landsInChatFolder("file", false), "without a chat there is no folder yet");

// A refusal names the file and the limit it broke, because a silent drop is the
// thing this change exists to remove.
const refusal = refusalMessage("holiday.mp4", "video/mp4");
ok(refusal.includes("holiday.mp4"), `the refusal must name the file: ${refusal}`);
ok(refusal.includes("32 MB"), `the refusal must name the limit: ${refusal}`);
ok(
  refusalMessage("shot.png", "image/png").includes("25 MB"),
  "an oversized picture names the image limit",
);

console.log("filing.check.ts: all checks passed");
