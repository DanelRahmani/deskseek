/**
 * Reading the file path out of a document tool's result. The Rust side writes
 * the path alone on the first line — `Saved to: <path>` — precisely so this does
 * not have to guess where the path ends and the description begins.
 *
 * Pure, so `docs.check.ts` can pin it: this decides what the panel will hand to
 * the "Open" button, and it must never hand it something that is not a document.
 */
export const DOC_TOOLS = ["doc_create", "sheet_create", "slides_create"];

/** The extensions DeskSeek itself writes, and the only ones it will open. */
const OPENABLE = ["md", "docx", "xlsx", "pptx", "pdf"];

const SAVED = /^Saved to: (.+)$/m;

export function isDocTool(tool: string): boolean {
  return DOC_TOOLS.includes(tool);
}

/** The path a document tool reported, or null when it wrote nothing. */
export function savedPath(tool: string, output: string | undefined): string | null {
  if (!output || !isDocTool(tool)) return null;
  const line = SAVED.exec(output);
  if (!line) return null;
  const path = line[1].trim();
  const dot = path.lastIndexOf(".");
  const ext = dot < 0 ? "" : path.slice(dot + 1).toLowerCase();
  // A path we did not generate is not one we offer to open.
  return OPENABLE.includes(ext) ? path : null;
}
