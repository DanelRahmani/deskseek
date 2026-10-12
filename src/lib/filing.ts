/**
 * Where an attachment is allowed to go, and the one rule that decides it.
 *
 * Every file the user adds is filed in the chat's own folder — the chat owns
 * what was put in it — and an image is *additionally* handed to the model
 * inline, because a picture is the one thing it can be shown rather than told.
 *
 * The rule lives apart from `attachments.ts` for one reason: it is pure, so a
 * check script can hold it to account without a window, a `File` or an API.
 */

/**
 * Pictures are downscaled on the way to the model, so this only bounds what is
 * filed. It matches the Rust side's `MAX_IMAGE_BYTES`.
 */
export const MAX_IMAGE_BYTES = 25 * 1024 * 1024;
/** Text beyond this is filed but not pasted into the conversation. */
export const MAX_TEXT_BYTES = 200 * 1024;
/**
 * The largest file worth moving through the webview at all. A dropped file has
 * another route — the Rust side is handed its path — but a picked one has to
 * travel as bytes, and a 2 GB video must never become a message row.
 */
export const MAX_SEND_BYTES = 32 * 1024 * 1024;

/** The same rule the Rust side uses — text the text-only model can read. */
export function isTextMime(mime: string): boolean {
  return mime.startsWith("text/") || mime === "application/json" || mime === "application/xml";
}

/**
 * `image` is filed and shown; `text` is filed and pasted in; `file` is filed and
 * named for the model to read on demand; `refuse` is too large to move, so it is
 * reported instead of being added.
 */
export type AttachmentRoute = "image" | "text" | "file" | "refuse";

export function routeForAttachment(mime: string, size: number): AttachmentRoute {
  if (mime.startsWith("image/")) return size > MAX_IMAGE_BYTES ? "refuse" : "image";
  if (isTextMime(mime) && size <= MAX_TEXT_BYTES) return "text";
  return size > MAX_SEND_BYTES ? "refuse" : "file";
}

/** Why a file was refused, in a sentence the composer can show as-is. */
export function refusalMessage(name: string, mime: string): string {
  const mb = (bytes: number) => Math.round(bytes / (1024 * 1024));
  const limit = mime.startsWith("image/") ? mb(MAX_IMAGE_BYTES) : mb(MAX_SEND_BYTES);
  return `${name} is larger than ${limit} MB, so it was not added.`;
}

/**
 * Whether an attachment that took this route ends up in the chat's folder.
 *
 * The `hasChat` half is the honest caveat, not an oversight: a file added before
 * the chat exists has no folder to join yet, and `send` is what creates the chat
 * from the draft. The check script asserts both halves, so the exception stays a
 * decision rather than becoming a hole nobody noticed.
 */
export function landsInChatFolder(route: AttachmentRoute, hasChat: boolean): boolean {
  return hasChat && route !== "refuse";
}
