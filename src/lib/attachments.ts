import type { Attachment } from "../types";
import { api } from "../api";
import { refusalMessage, routeForAttachment } from "./filing";

/**
 * Vision models tile images at roughly this size, so anything larger is wasted
 * bytes. Downscaling also keeps a message row to a few hundred kilobytes
 * instead of many megabytes.
 */
const MAX_DIMENSION = 1568;
const RECOMPRESS_ABOVE_BYTES = 400_000;

function readAsDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(new Error(`could not read ${file.name}`));
    reader.readAsDataURL(file);
  });
}

/** The text a `data:` URL carries, so a picked file matches a dropped one. */
function decodeDataUrlText(dataUrl: string): string {
  const comma = dataUrl.indexOf(",");
  if (comma < 0) return "";
  try {
    const binary = atob(dataUrl.slice(comma + 1));
    const bytes = Uint8Array.from(binary, (ch) => ch.charCodeAt(0));
    return new TextDecoder().decode(bytes);
  } catch {
    return "";
  }
}

function loadImage(dataUrl: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error("could not decode the image"));
    image.src = dataUrl;
  });
}

/**
 * Turn a `File` the webview holds into an attachment.
 *
 * One rule, from `filing.ts`: a file the user adds is filed in the chat's own
 * folder, and an image is also handed over inline. So a picture and a small text
 * file now reach the Rust side exactly as a `.docx` always did — the two used to
 * short-circuit here, which is how a picture could sit in a conversation without
 * ever reaching the folder that conversation owns.
 */
export async function toAttachment(
  file: File,
  chatId: string | null,
): Promise<Attachment> {
  const mime = file.type || "application/octet-stream";
  const route = routeForAttachment(mime, file.size);
  // Too large to move: said out loud rather than added as a name with nothing
  // behind it, which is what the old early-return did.
  if (route === "refuse") throw new Error(refusalMessage(file.name, mime));

  const source = await readAsDataUrl(file);
  if (!chatId) {
    // Nowhere to file it yet, because `send` is what creates the chat from the
    // draft. A picture is still shown and text still travels as its own text;
    // any other file is a name only, because handing a `.docx`'s bytes to a text
    // decoder would be mojibake.
    if (route === "image" || route === "text") return normalise(file.name, mime, source);
    return { name: file.name, mime, data_url: "", text: "" };
  }
  // The Rust side writes it into the chat's own folder — a `.docx` is a ZIP of
  // XML and a `.pdf` is neither, so nothing here could open them, but the
  // backend can — and hands back whatever can be read of it.
  const filed = await api.attachBytes(chatId, file.name, source);
  // An image is filed as the original it is and shown to the model as a
  // downscale, so the folder keeps the picture the user chose.
  return route === "image" ? normalise(filed.name, mime, source) : filed;
}

/**
 * Downscale and re-encode a file that has already been read, whatever its
 * source — a picked File, a pasted one, or a path the Rust side read for us.
 * One implementation, so a dropped image is treated exactly like a picked one.
 * A non-image keeps no bytes at all: an image is the only thing sent inline,
 * and a text file is carried as its own text.
 */
export async function normalise(
  name: string,
  mime: string,
  dataUrl: string,
  text?: string,
): Promise<Attachment> {
  if (!mime.startsWith("image/")) {
    return { name, mime, data_url: "", text: text ?? decodeDataUrlText(dataUrl) };
  }

  try {
    const image = await loadImage(dataUrl);
    const largest = Math.max(image.width, image.height);
    const scale = largest > MAX_DIMENSION ? MAX_DIMENSION / largest : 1;
    if (scale === 1 && dataUrl.length <= RECOMPRESS_ABOVE_BYTES) {
      return { name, mime, data_url: dataUrl };
    }

    const canvas = document.createElement("canvas");
    canvas.width = Math.max(1, Math.round(image.width * scale));
    canvas.height = Math.max(1, Math.round(image.height * scale));
    const context = canvas.getContext("2d");
    if (!context) {
      return { name, mime, data_url: dataUrl };
    }
    context.drawImage(image, 0, 0, canvas.width, canvas.height);
    return {
      name,
      mime: "image/jpeg",
      data_url: canvas.toDataURL("image/jpeg", 0.85),
    };
  } catch {
    // Anything we cannot process is still sent as-is rather than dropped.
    return { name, mime, data_url: dataUrl };
  }
}

/** A file the Rust side read for us — a native drop gives a path, not a File. */
export function pathToAttachment(raw: Attachment): Promise<Attachment> {
  return normalise(raw.name, raw.mime, raw.data_url, raw.text);
}
