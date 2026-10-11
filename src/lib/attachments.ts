import type { Attachment } from "../types";

/**
 * Vision models tile images at roughly this size, so anything larger is wasted
 * bytes. Downscaling also keeps a message row to a few hundred kilobytes
 * instead of many megabytes.
 */
const MAX_DIMENSION = 1568;
const RECOMPRESS_ABOVE_BYTES = 400_000;
/** A picture is worth a row; a whole photo library is not. */
const MAX_IMAGE_BYTES = 25 * 1024 * 1024;
/** Text beyond this is dropped at the source rather than stored in full. */
const MAX_TEXT_BYTES = 200 * 1024;

/** The same rule the Rust side uses — text the text-only model can read. */
function isTextMime(mime: string): boolean {
  return mime.startsWith("text/") || mime === "application/json" || mime === "application/xml";
}

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

export async function toAttachment(file: File): Promise<Attachment> {
  const mime = file.type || "application/octet-stream";
  if (mime.startsWith("image/")) {
    if (file.size > MAX_IMAGE_BYTES) throw new Error(`${file.name} is larger than 25 MB.`);
    return normalise(file.name, mime, await readAsDataUrl(file));
  }
  if (isTextMime(mime) && file.size <= MAX_TEXT_BYTES) {
    return normalise(file.name, mime, await readAsDataUrl(file));
  }
  // Not an image and not readable text: the model would never receive it, so
  // the bytes are never loaded — a 2 GB video must not become a message row.
  return { name: file.name, mime, data_url: "", text: "" };
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

export async function filesToAttachments(files: FileList | File[]): Promise<Attachment[]> {
  const list = Array.from(files);
  return Promise.all(list.map(toAttachment));
}
