/**
 * Checks for the document-result reader in docs.ts. Plain Node, no test
 * framework:  npm run check
 */
import { isDocTool, savedPath } from "./docs.ts";

function ok(condition: boolean, message: string): void {
  if (!condition) throw new Error(`FAIL: ${message}`);
}

const written =
  "Saved to: C:\\Users\\danel\\AppData\\documents\\q3-report.docx\n" +
  "Word document, 7 block(s).";

ok(savedPath("doc_create", written) === "C:\\Users\\danel\\AppData\\documents\\q3-report.docx",
  "the path must be read whole, spaces and all");

const spaced = "Saved to: C:\\a b\\my report.pdf\nPDF, 3 block(s).";
ok(savedPath("doc_create", spaced) === "C:\\a b\\my report.pdf", "a space in the path is fine");

// Every tool that writes a document is recognised, and nothing else is.
for (const tool of ["doc_create", "sheet_create", "slides_create"]) {
  ok(isDocTool(tool), `${tool} should be a document tool`);
}
for (const tool of ["write_file", "artifact_create", "run_command", "read_file"]) {
  ok(!isDocTool(tool), `${tool} should not be a document tool`);
}

// A failure has no path, and a tool that does not write documents has none even
// when its output happens to mention one.
const failed = "the browser could not render the PDF. Failed to load";
ok(savedPath("doc_create", failed) === null, "a failure must not offer an Open button");
ok(savedPath("read_file", written) === null, "a read must never offer to open anything");
ok(savedPath("doc_create", undefined) === null, "a running call has no path yet");

// The extension is the guard: the panel must not be talked into opening a file
// DeskSeek did not write.
ok(savedPath("doc_create", "Saved to: C:\\Windows\\System32\\cmd.exe") === null,
  "only the extensions we generate may be opened");
ok(savedPath("doc_create", "Saved to: /etc/passwd") === null, "an extensionless path is refused");
ok(savedPath("doc_create", "Saved to: C:\\x\\notes.MD") === "C:\\x\\notes.MD",
  "the extension check is case-insensitive");

console.log("docs.check.ts: all checks passed");
