import type { Attachment } from "../types";

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

function loadImage(dataUrl: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error("could not decode the image"));
    image.src = dataUrl;
  });
}

export async function toAttachment(file: File): Promise<Attachment> {
  const original = await readAsDataUrl(file);
  const mime = file.type || "application/octet-stream";

  if (!mime.startsWith("image/")) {
    return { name: file.name, mime, data_url: original };
  }

  try {
    const image = await loadImage(original);
    const largest = Math.max(image.width, image.height);
    const scale = largest > MAX_DIMENSION ? MAX_DIMENSION / largest : 1;
    if (scale === 1 && original.length <= RECOMPRESS_ABOVE_BYTES) {
      return { name: file.name, mime, data_url: original };
    }

    const canvas = document.createElement("canvas");
    canvas.width = Math.max(1, Math.round(image.width * scale));
    canvas.height = Math.max(1, Math.round(image.height * scale));
    const context = canvas.getContext("2d");
    if (!context) {
      return { name: file.name, mime, data_url: original };
    }
    context.drawImage(image, 0, 0, canvas.width, canvas.height);
    return {
      name: file.name,
      mime: "image/jpeg",
      data_url: canvas.toDataURL("image/jpeg", 0.85),
    };
  } catch {
    // Anything we cannot process is still sent as-is rather than dropped.
    return { name: file.name, mime, data_url: original };
  }
}

export async function filesToAttachments(files: FileList | File[]): Promise<Attachment[]> {
  const list = Array.from(files);
  return Promise.all(list.map(toAttachment));
}
