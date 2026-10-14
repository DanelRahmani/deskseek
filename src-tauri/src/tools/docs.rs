//! Document generation: `.md`, `.docx`, `.xlsx`, `.pptx` and `.pdf`.
//!
//! Everything the model writes arrives as Markdown, and one small line-based
//! reader turns it into blocks. That is deliberately *not* a Markdown
//! implementation: it reads the subset a model actually writes for a document —
//! headings, bullets, numbered lists, tables, fenced code, paragraphs, and the two
//! things a paper needs that prose never did, an indented line for an option and a
//! `\pagebreak` line so a cover is a page rather than a title — and treats
//! anything it does not recognise as a paragraph. Every writer consumes the same
//! blocks, so `.docx` and `.pdf` cannot disagree about what the document says.
//!
//! The PDF is rendered by the Chromium-family browser already on the machine
//! (`--print-to-pdf`), the same binary `browse.rs` already finds — no second
//! HTML engine, no bundled renderer, and the best fidelity of the five.

use std::path::{Path, PathBuf};

use serde_json::Value;

/// One element of a document. Small on purpose — see the module comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Heading { level: u8, text: String },
    /// `depth` is the nesting level, 0 for a top-level bullet.
    Bullet { depth: u8, text: String },
    Numbered { text: String },
    /// A line that arrived indented — an exam option under its question. The
    /// indent is a fact about the line, not four characters kept in its text.
    Indented(String),
    Paragraph(String),
    Table(Vec<Vec<String>>),
    Code(String),
    /// A `\pagebreak` line: the block after it starts a page. A cover is only a
    /// cover if what it covers opens on the next page.
    Break,
}

/// The one line that means "start a page here". Exported because the cover that
/// uses it is written in another module, and a second copy of the string is how
/// the two would drift.
pub const PAGE_BREAK: &str = "\\pagebreak";

/// Caps, so one call cannot ask for a 200 MB spreadsheet or a 5,000-slide deck.
const MAX_SLIDES: usize = 60;
const MAX_ROWS: usize = 5_000;
const MAX_COLUMNS: usize = 64;

/// The look of a document this app writes: one body font and size, and one scale of
/// headings, in points.
///
/// Times New Roman because these are documents a reader prints and marks, and
/// because the PDF's own stylesheet has been set in that family all along — this is
/// the `.docx` catching up with it rather than a second opinion about what a
/// document looks like. A code block is the one exception, and deliberately:
/// monospace is what makes it a code block rather than a paragraph somebody
/// indented.
const BODY_FONT: &str = "Times New Roman";
const BODY_SIZE: usize = 11;
const HEADING_SIZES: [usize; 3] = [16, 14, 12];
const CODE_FONT: &str = "Consolas";
const CODE_SIZE: usize = 10;

/// How far an indented line sits in from the margin, in twips — twentieths of a
/// point, which is what Word measures an indent in. Half an inch, Word's own step.
/// The printed page uses the same distance derived from this rather than typed a
/// second time.
const INDENT_TWIPS: i32 = 720;

/// A heading's size, by level.
///
/// Level 2 is 14 because that is what an exam's questions are written as — the
/// level the paper's own shape asks the model for, so a question comes out bold at
/// 14 with its options at 11 underneath it.
fn heading_size(level: u8) -> usize {
    match level {
        0 | 1 => HEADING_SIZES[0],
        2 => HEADING_SIZES[1],
        _ => HEADING_SIZES[2],
    }
}

/// The three tools this module serves.
pub fn is_document_tool(name: &str) -> bool {
    matches!(name, "doc_create" | "sheet_create" | "slides_create")
}

// ------------------------------------------------------------------ parsing

fn heading(line: &str) -> Option<(u8, String)> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    // A space is required: `#hashtag` is not a heading.
    let rest = line[hashes..].strip_prefix(' ')?;
    Some((hashes as u8, rest.trim().to_string()))
}

/// The one markdown heading reader, for callers outside this module that need to
/// tell a heading from body text — importing another assistant's memory file, for
/// one. `pub(crate)` because a second copy of "is this a heading" is how the two
/// drift on `#hashtag`.
pub(crate) fn heading_text(line: &str) -> Option<String> {
    heading(line).map(|(_, text)| text)
}

fn bullet(line: &str) -> Option<(u8, String)> {
    let indent = line.len() - line.trim_start().len();
    let rest = line.trim_start();
    let body = rest
        .strip_prefix("- ")
        .or_else(|| rest.strip_prefix("* "))
        .or_else(|| rest.strip_prefix("+ "))?;
    Some(((indent / 2).min(6) as u8, body.trim().to_string()))
}

fn numbered(line: &str) -> Option<String> {
    let rest = line.trim_start();
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let body = rest[digits..].strip_prefix(". ")?;
    Some(body.trim().to_string())
}

/// A line that arrives indented and is not a list item: how an exam option is
/// written. The indent is read as a fact so the writers can set a real one — four
/// leading spaces in a Word document are four characters wide, not four spaces of
/// margin, which is what a paper indented by spaces looks like.
fn indented(line: &str) -> Option<String> {
    let rest = line.trim_start();
    let lead = line.len() - rest.len();
    // Four spaces, or a tab: a model that indents with a tab means the same thing,
    // and a tab left in the text is a character Word will not print.
    if lead < 4 && !line.starts_with('\t') {
        return None;
    }
    Some(rest.to_string())
}

fn is_table_row(line: &str) -> bool {
    let t = line.trim();
    t.len() > 2 && t.starts_with('|') && t.ends_with('|')
}

fn cells(line: &str) -> Vec<String> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(|c| c.trim().to_string())
        .collect()
}

/// The `|---|---|` row under a table's header. It is structure, not content.
fn is_separator_row(line: &str) -> bool {
    let cells = cells(line);
    !cells.is_empty()
        && cells
            .iter()
            .all(|c| !c.is_empty() && c.chars().all(|ch| ch == '-' || ch == ':'))
}

/// Markdown in, blocks out.
pub fn parse_blocks(md: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut lines = md.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim_end();
        if trimmed.trim().is_empty() {
            continue;
        }

        // A fence swallows everything up to its close, verbatim — including
        // lines that would otherwise read as headings or bullets.
        if trimmed.trim_start().starts_with("```") {
            let mut body = String::new();
            for inner in lines.by_ref() {
                if inner.trim_start().starts_with("```") {
                    break;
                }
                body.push_str(inner);
                body.push('\n');
            }
            let body = body.trim_end_matches('\n').to_string();
            if !body.is_empty() {
                out.push(Block::Code(body));
            }
            continue;
        }

        if is_table_row(trimmed) {
            let mut rows = Vec::new();
            if !is_separator_row(trimmed) {
                rows.push(cells(trimmed));
            }
            while let Some(next) = lines.peek() {
                if !is_table_row(next) {
                    break;
                }
                let row = lines.next().unwrap_or_default();
                if !is_separator_row(row) {
                    rows.push(cells(row));
                }
            }
            if !rows.is_empty() {
                out.push(Block::Table(rows));
            }
            continue;
        }

        if trimmed.trim() == PAGE_BREAK {
            out.push(Block::Break);
            continue;
        }

        if let Some((level, text)) = heading(trimmed) {
            out.push(Block::Heading { level, text });
        } else if let Some((depth, text)) = bullet(trimmed) {
            out.push(Block::Bullet { depth, text });
        } else if let Some(text) = numbered(trimmed) {
            out.push(Block::Numbered { text });
        } else if let Some(text) = indented(trimmed) {
            out.push(Block::Indented(text));
        } else {
            out.push(Block::Paragraph(trimmed.trim().to_string()));
        }
    }
    out
}

// ---------------------------------------------------------- inline emphasis

/// One piece of a line: what it says, and how it is set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
}

/// The emphasis in one line of Markdown, as runs.
///
/// Deliberately the two markers a model actually writes — `**bold**` and `*italic*`
/// — and deliberately not `_`, which turns up inside file names and would make half
/// of `q3_file.docx` italic. Anything unmatched is left as the text it is: a stray
/// asterisk in a document about tax rates is not emphasis, and eating it would drop
/// a character the model meant to write.
///
/// This is what keeps a `.docx` from showing the markup that asked for the
/// emphasis. The `.md` writer is the one place it does *not* apply, because there
/// the asterisks are the format.
pub fn runs_of(text: &str) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find('*') {
        if at > 0 {
            push_run(&mut out, &rest[..at], false, false);
        }
        let after = &rest[at..];
        let (marker, bold) = match after.strip_prefix("**") {
            Some(inner) => (inner, true),
            None => (&after[1..], false),
        };
        let close = if bold { "**" } else { "*" };
        match marker.find(close) {
            // A marker with a close after it is emphasis, and what is between them
            // is one run of it. An *empty* pair is not: `****` is four asterisks.
            Some(end) if end > 0 => {
                push_run(&mut out, &marker[..end], bold, !bold);
                rest = &marker[end + close.len()..];
            }
            // No close: the asterisk is text, and the rest carries on after it.
            _ => {
                push_run(&mut out, "*", false, false);
                rest = &after[1..];
            }
        }
    }
    push_run(&mut out, rest, false, false);
    out
}

/// Adds a run, merging it into the one before when they are set the same way — so
/// `a **b**` is two runs rather than three pieces of one sentence.
fn push_run(out: &mut Vec<Run>, text: &str, bold: bool, italic: bool) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = out.last_mut() {
        if last.bold == bold && last.italic == italic {
            last.text.push_str(text);
            return;
        }
    }
    out.push(Run {
        text: text.to_string(),
        bold,
        italic,
    });
}

// ----------------------------------------------------------------- uploading

/// Plain, filesystem-safe stem from a document name. Kept to ASCII letters,
/// digits and single dashes so the file survives every shell and archive.
pub fn slug(name: &str) -> String {
    slug_with(name, "document")
}

/// As `slug`, with the fallback named by the caller — a skill is not a
/// "document", and neither is anything else that needs a safe directory name.
pub fn slug_with(name: &str, fallback: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in name.chars().take(80) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed
    }
}

/// A path in `dir` that is not taken yet, so a second document with the same
/// name is `name-2` rather than overwriting the first.
pub(crate) fn free_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.{ext}"));
    if !first.exists() {
        return first;
    }
    for n in 2..1000 {
        let candidate = dir.join(format!("{stem}-{n}.{ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    // A thousand collisions is not a real situation; the timestamp is a floor.
    dir.join(format!("{stem}-{}.{ext}", crate::store::db::now_ts()))
}

/// What the model is told, and what the panel's Open button parses. The path is
/// alone on the first line so neither has to guess where it ends.
fn saved(path: &Path, note: &str) -> String {
    format!("Saved to: {}\n{note}", path.display())
}

// --------------------------------------------------------------------- docx

fn docx_bytes(blocks: &[Block]) -> Result<Vec<u8>, String> {
    use docx_rs::{BreakType, Docx, Paragraph, Run as TextRun, RunFonts, Table, TableCell, TableRow};

    // Every run says its own font, size and emphasis rather than naming a Word
    // style: this file defines no styles, so `.style("Heading1")` would point at
    // something the reader's Word has never seen — which is how a heading ends up
    // as plain text with the markup still in it.
    let set = |runs: &[Run], font: &str, size: usize, all_bold: bool| -> Vec<TextRun> {
        runs.iter()
            .map(|run| {
                let mut out = TextRun::new()
                    .add_text(run.text.as_str())
                    .fonts(
                        RunFonts::new()
                            .ascii(font)
                            .hi_ansi(font)
                            .east_asia(font)
                            .cs(font),
                    )
                    // Half-points, which is why this doubles: 11pt is written 22.
                    .size(size * 2);
                if all_bold || run.bold {
                    out = out.bold();
                }
                if run.italic {
                    out = out.italic();
                }
                out
            })
            .collect()
    };
    let paragraph = |text: &str, font: &str, size: usize, bold: bool| {
        let mut out = Paragraph::new();
        for run in set(&runs_of(text), font, size, bold) {
            out = out.add_run(run);
        }
        out
    };
    let body = |text: &str| paragraph(text, BODY_FONT, BODY_SIZE, false);

    let mut doc = Docx::new();
    // Word has no "list" here, so bullets and numbers are written into the
    // text. `number` resets whenever the run of numbered lines is broken.
    let mut number = 0usize;
    // A break is a property of the block that follows it rather than a paragraph of
    // its own: a paragraph holding a break would leave an empty line at the top of
    // the page it had just started, which is the one thing a cover is for.
    let mut fresh_page = false;
    for block in blocks {
        if !matches!(block, Block::Numbered { .. }) {
            number = 0;
        }
        if matches!(block, Block::Break) {
            fresh_page = true;
            continue;
        }
        let start = |paragraph: Paragraph| {
            if fresh_page {
                paragraph.page_break_before(true)
            } else {
                paragraph
            }
        };
        doc = match block {
            Block::Heading { level, text } => {
                doc.add_paragraph(start(paragraph(text, BODY_FONT, heading_size(*level), true)))
            }
            Block::Bullet { depth, text } => doc.add_paragraph(start(body(&format!(
                "{}{text}",
                "    ".repeat(*depth as usize + 1) + "• "
            )))),
            Block::Numbered { text } => {
                number += 1;
                doc.add_paragraph(start(body(&format!("    {number}. {text}"))))
            }
            // An option is set in from the margin rather than written with the four
            // spaces it arrived with: those are four characters wide in Word, which
            // is what a paper indented by spaces looks like.
            Block::Indented(text) => doc.add_paragraph(start(
                paragraph(text, BODY_FONT, BODY_SIZE, false)
                    .indent(Some(INDENT_TWIPS), None, None, None),
            )),
            Block::Code(text) => {
                let mut out = Paragraph::new();
                for (i, line) in text.lines().enumerate() {
                    if i > 0 {
                        out = out.add_run(TextRun::new().add_break(BreakType::TextWrapping));
                    }
                    for run in set(&runs_of(line), CODE_FONT, CODE_SIZE, false) {
                        out = out.add_run(run);
                    }
                }
                doc.add_paragraph(out)
            }
            Block::Paragraph(text) => doc.add_paragraph(start(body(text))),
            Block::Table(rows) => {
                // A table cannot carry the property itself, so a break before one is
                // spent on an empty paragraph — otherwise it would be dropped and the
                // table would quietly stay on the page before it.
                if fresh_page {
                    doc = doc.add_paragraph(Paragraph::new().page_break_before(true));
                }
                doc.add_table(Table::new(
                    rows.iter()
                        .map(|row| {
                            TableRow::new(
                                row.iter()
                                    .map(|cell| TableCell::new().add_paragraph(body(cell)))
                                    .collect(),
                            )
                        })
                        .collect(),
                ))
            }
            // Handled above, before the match: it says where the *next* block starts.
            Block::Break => unreachable!(),
        };
        fresh_page = false;
    }

    let mut buf = Vec::new();
    doc.build()
        .pack(std::io::Cursor::new(&mut buf))
        .map_err(|e| format!("could not build the .docx: {e}"))?;
    Ok(buf)
}

// --------------------------------------------------------------------- html

fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

/// One line of Markdown as HTML: the emphasis becomes tags and everything else is
/// escaped. Deliberately the same reader the `.docx` uses, so the two writers
/// cannot disagree about what was emphasised — only about how to say it. The
/// text is escaped *after* the runs are found, because the escaping has to happen
/// to the text rather than to the tags this function adds.
fn inline_html(text: &str) -> String {
    runs_of(text)
        .iter()
        .map(|run| {
            let escaped = escape_html(&run.text);
            match (run.bold, run.italic) {
                (true, _) => format!("<strong>{escaped}</strong>"),
                (false, true) => format!("<em>{escaped}</em>"),
                _ => escaped,
            }
        })
        .collect()
}

/// The page a `.pdf` is printed from. The font and the heading sizes are the ones
/// `docx_bytes` sets, so the same document does not change its look with its
/// extension — 11pt body, headings at 16/14/12, and a code block in monospace.
const PRINT_CSS: &str = "body{margin:0;font:11pt/1.5 'Times New Roman',Times,serif;color:#111}\
h1{font-size:16pt;margin:0 0 10pt}h2{font-size:14pt;margin:16pt 0 6pt}\
h3{font-size:12pt;margin:14pt 0 5pt}h4,h5,h6{font-size:11pt;margin:12pt 0 4pt}\
p{margin:0 0 8pt}ul,ol{margin:0 0 8pt;padding-left:20pt}li{margin:0 0 3pt}\
table{border-collapse:collapse;width:100%;margin:0 0 10pt;font-size:11pt}\
th,td{border:1px solid #999;padding:4pt 6pt;text-align:left}\
th{background:#eee}pre{background:#f4f4f4;border:1px solid #ddd;padding:6pt;\
font:10pt/1.4 Consolas,monospace;white-space:pre-wrap}@page{margin:20mm}";

/// The blocks as a printable HTML page. Every piece of text is escaped: this
/// string is a document, but it is built from model output all the same.
pub fn html_doc(title: &str, blocks: &[Block]) -> String {
    let mut body = String::new();
    let mut open_list: Option<&str> = None;
    let close_list = |body: &mut String, open: &mut Option<&str>| {
        if let Some(tag) = open.take() {
            body.push_str(&format!("</{tag}>"));
        }
    };

    for block in blocks {
        match block {
            Block::Bullet { .. } | Block::Numbered { .. } => {
                let want = if matches!(block, Block::Bullet { .. }) { "ul" } else { "ol" };
                if open_list != Some(want) {
                    close_list(&mut body, &mut open_list);
                    body.push_str(&format!("<{want}>"));
                    open_list = Some(want);
                }
                match block {
                    Block::Bullet { depth, text } => body.push_str(&format!(
                        "<li style=\"margin-left:{}pt\">{}</li>",
                        *depth as usize * 14,
                        inline_html(text)
                    )),
                    Block::Numbered { text } => {
                        body.push_str(&format!("<li>{}</li>", inline_html(text)))
                    }
                    _ => unreachable!(),
                }
            }
            Block::Table(rows) => {
                close_list(&mut body, &mut open_list);
                body.push_str("<table>");
                for (index, row) in rows.iter().enumerate() {
                    let tag = if index == 0 { "th" } else { "td" };
                    body.push_str("<tr>");
                    for cell in row {
                        body.push_str(&format!("<{tag}>{}</{tag}>", inline_html(cell)));
                    }
                    body.push_str("</tr>");
                }
                body.push_str("</table>");
            }
            other => {
                close_list(&mut body, &mut open_list);
                match other {
                    Block::Heading { level, text } => body.push_str(&format!(
                        "<h{level}>{}</h{level}>",
                        inline_html(text)
                    )),
                    Block::Paragraph(text) => {
                        body.push_str(&format!("<p>{}</p>", inline_html(text)))
                    }
                    // The same distance the `.docx` indents an option by, in the
                    // unit a stylesheet wants it: twips are twentieths of a point.
                    Block::Indented(text) => body.push_str(&format!(
                        "<p style=\"margin-left:{}pt\">{}</p>",
                        INDENT_TWIPS / 20,
                        inline_html(text)
                    )),
                    // A zero-height div pushes what follows onto a fresh page without
                    // printing a line of its own.
                    Block::Break => body.push_str("<div style=\"break-before:page\"></div>"),
                    // A code block is the one thing whose asterisks are text.
                    Block::Code(text) => {
                        body.push_str(&format!("<pre>{}</pre>", escape_html(text)))
                    }
                    _ => unreachable!(),
                }
            }
        }
    }
    close_list(&mut body, &mut open_list);

    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{}</title>\
         <style>{PRINT_CSS}</style></head><body>{body}</body></html>",
        escape_html(title)
    )
}

/// `file:///C:/path/to/x.html` — the form Chromium accepts on Windows.
pub fn file_url(path: &Path) -> String {
    let slashed = path.display().to_string().replace('\\', "/");
    if slashed.starts_with('/') {
        format!("file://{slashed}")
    } else {
        format!("file:///{slashed}")
    }
}

/// True when the bytes really are a PDF. Chromium returns zero even when it
/// writes nothing useful, so the file itself is the only honest check.
pub fn looks_like_pdf(bytes: &[u8]) -> bool {
    bytes.starts_with(b"%PDF")
}

async fn render_pdf(browser: &str, html: &str) -> Result<Vec<u8>, String> {
    let exe = crate::tools::browse::find_browser(browser)
        .ok_or("No Chromium-family browser was found, so a PDF cannot be rendered. Set one in Settings → Browsing, or choose md or docx.")?;

    let stamp = uuid::Uuid::new_v4();
    let job = std::env::temp_dir().join(format!("deskseek-print-{stamp}"));
    std::fs::create_dir_all(&job).map_err(|e| e.to_string())?;
    let input = job.join("document.html");
    let output = job.join("document.pdf");
    std::fs::write(&input, html).map_err(|e| e.to_string())?;

    let result = tokio::process::Command::new(&exe)
        .arg("--headless=new")
        .arg("--disable-gpu")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--no-pdf-header-footer")
        .arg(format!("--user-data-dir={}", job.join("profile").display()))
        .arg(format!("--print-to-pdf={}", output.display()))
        .arg(file_url(&input))
        .output()
        .await;

    let bytes = match result {
        Ok(out) if out.status.success() => std::fs::read(&output).map_err(|e| {
            format!(
                "the browser did not write a PDF ({e}). {}",
                first_line(&String::from_utf8_lossy(&out.stderr))
            )
        })?,
        Ok(out) => {
            let _ = std::fs::remove_dir_all(&job);
            return Err(format!(
                "the browser could not render the PDF. {}",
                first_line(&String::from_utf8_lossy(&out.stderr))
            ));
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(&job);
            return Err(format!("could not start {}: {e}", exe.display()));
        }
    };
    let _ = std::fs::remove_dir_all(&job);

    if !looks_like_pdf(&bytes) {
        return Err("the browser reported success but wrote no PDF".to_string());
    }
    Ok(bytes)
}

fn first_line(text: &str) -> String {
    let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    line.trim().chars().take(300).collect()
}

// ---------------------------------------------------------------- the tools

fn text_arg(args: &Value, key: &str) -> String {    args.get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

/// The formats `doc_create` writes. One list, so anything that *names* a format —
/// a notebook's generator menu — can be held against the writer's own set rather
/// than against a copy of it that could drift.
pub const DOC_FORMATS: [&str; 3] = ["md", "docx", "pdf"];

fn require_name(args: &Value) -> Result<String, String> {
    let name = text_arg(args, "name");
    if name.is_empty() {
        return Err("a document needs a \"name\", e.g. \"Q3 report\"".to_string());
    }
    Ok(name)
}

/// Runs one of the three document tools. The caller supplies the output folder
/// so this module never has to know what an `AppState` is, and so a test can
/// point it at a temp directory.
///
/// The path comes back beside the text because the writer is the only thing that
/// knows it: a notebook's output row has to remember where its file is, and
/// reading the path back out of the sentence below would be a second answer to a
/// question this function has already answered.
pub async fn execute(
    name: &str,
    args: &Value,
    dir: &Path,
    browser: &str,
) -> Result<(String, Option<std::path::PathBuf>), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("could not use {}: {e}", dir.display()))?;

    match name {
        "doc_create" => doc_create(args, dir, browser).await,
        "sheet_create" => sheet_create(args, dir),
        "slides_create" => slides_create(args, dir),
        other => Err(format!("{other} is not a document tool")),
    }
}

async fn doc_create(
    args: &Value,
    dir: &Path,
    browser: &str,
) -> Result<(String, Option<std::path::PathBuf>), String> {
    let name = require_name(args)?;
    let content = args
        .get("content")
        .and_then(Value::as_str)
        .filter(|c| !c.trim().is_empty())
        .ok_or("doc_create needs non-empty \"content\"")?;
    let format = text_arg(args, "format").to_ascii_lowercase();
    if !DOC_FORMATS.contains(&format.as_str()) {
        return Err("doc_create format must be md, docx or pdf".to_string());
    }

    let path = free_path(dir, &slug(&name), format.as_str());
    let blocks = parse_blocks(content);
    let note = match format.as_str() {
        "md" => {
            std::fs::write(&path, content).map_err(|e| e.to_string())?;
            format!("Markdown, {} lines.", content.lines().count())
        }
        "docx" => {
            let bytes = docx_bytes(&blocks)?;
            std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
            format!("Word document, {} block(s).", blocks.len())
        }
        _ => {
            let bytes = render_pdf(browser, &html_doc(&name, &blocks)).await?;
            std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
            format!("PDF, {} block(s), {} kB.", blocks.len(), bytes.len() / 1024)
        }
    };
    Ok((saved(&path, &note), Some(path)))
}

fn sheet_create(
    args: &Value,
    dir: &Path,
) -> Result<(String, Option<std::path::PathBuf>), String> {
    let name = require_name(args)?;
    let rows = args
        .get("rows")
        .and_then(Value::as_array)
        .filter(|r| !r.is_empty())
        .ok_or("sheet_create needs a non-empty \"rows\" array")?;
    if rows.len() > MAX_ROWS {
        return Err(format!(
            "sheet_create takes at most {MAX_ROWS} rows; split the data up"
        ));
    }

    let mut sheet = text_arg(args, "sheet");
    if sheet.is_empty() {
        sheet = "Sheet1".to_string();
    }
    // Excel refuses these three names and caps sheet names at 31 characters.
    if matches!(sheet.as_str(), "History" | "Sheet" | "Workbook") {
        sheet = format!("{sheet}1");
    }
    let sheet: String = sheet.chars().take(31).collect();

    let mut book = rust_xlsxwriter::Workbook::new();
    let ws = book.add_worksheet();
    ws.set_name(&sheet).map_err(|e| e.to_string())?;

    let mut written = 0usize;
    for (r, row) in rows.iter().enumerate() {
        let Some(cells) = row.as_array() else {
            return Err(format!("row {} is not an array", r + 1));
        };
        if cells.len() > MAX_COLUMNS {
            return Err(format!("row {} has more than {MAX_COLUMNS} columns", r + 1));
        }
        for (c, cell) in cells.iter().enumerate() {
            let (r, c) = (r as u32, c as u16);
            match cell {
                Value::Null => {}
                Value::Bool(b) => {
                    ws.write_boolean(r, c, *b).map_err(|e| e.to_string())?;
                }
                Value::Number(n) => {
                    ws.write_number(r, c, n.as_f64().unwrap_or_default())
                        .map_err(|e| e.to_string())?;
                }
                // A formula is written as text: Excel evaluates it when the file
                // is opened. Nothing here pretends to be a spreadsheet engine.
                Value::String(s) if s.starts_with('=') => {
                    ws.write_formula(r, c, s.as_str()).map_err(|e| e.to_string())?;
                }
                Value::String(s) => {
                    ws.write_string(r, c, s.as_str()).map_err(|e| e.to_string())?;
                }
                other => {
                    ws.write_string(r, c, &other.to_string())
                        .map_err(|e| e.to_string())?;
                }
            }
            written += 1;
        }
    }

    let path = free_path(dir, &slug(&name), "xlsx");
    book.save(&path)
        .map_err(|e| format!("could not write the spreadsheet: {e}"))?;
    Ok((
        saved(
            &path,
            &format!("Sheet \"{sheet}\", {written} cell(s). Formulas are stored as written and evaluated by Excel when the file opens."),
        ),
        Some(path),
    ))
}

fn slides_create(
    args: &Value,
    dir: &Path,
) -> Result<(String, Option<std::path::PathBuf>), String> {
    use ppt_rs::generator::slide_content::SlideContent;

    let name = require_name(args)?;
    let slides = args
        .get("slides")
        .and_then(Value::as_array)
        .filter(|s| !s.is_empty())
        .ok_or("slides_create needs a non-empty \"slides\" array")?;
    if slides.len() > MAX_SLIDES {
        return Err(format!(
            "slides_create takes at most {MAX_SLIDES} slides; split the deck up"
        ));
    }

    let mut made = Vec::new();
    for (index, slide) in slides.iter().enumerate() {
        let at = index + 1;
        // A bare string is a section title slide, which is a shape a model
        // reaches for and would otherwise have to be rejected for.
        let (title, bullets) = match slide {
            Value::String(text) => (text.clone(), Vec::new()),
            Value::Object(_) => {
                let title = slide
                    .get("title")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                    .ok_or_else(|| format!("slide {at} needs a \"title\""))?
                    .to_string();
                let bullets = slide
                    .get("bullets")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_str)
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                (title, bullets)
            }
            _ => return Err(format!("slide {at} must be an object or a string")),
        };
        let mut content = SlideContent::new(title.as_str());
        for bullet in &bullets {
            content = content.add_bullet(bullet.as_str());
        }
        made.push(content);
    }

    let deck_title = {
        let explicit = text_arg(args, "title");
        if explicit.is_empty() {
            name.clone()
        } else {
            explicit
        }
    };
    let bytes = ppt_rs::create_pptx_with_content(deck_title.as_str(), made)
        .map_err(|e| format!("could not build the .pptx: {e}"))?;

    let path = free_path(dir, &slug(&name), "pptx");
    std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    Ok((
        saved(
            &path,
            &format!(
                "PowerPoint, {} slide(s). Layouts, images and animation are not supported — title and bullets only.",
                slides.len()
            ),
        ),
        Some(path),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_reader_understands_what_a_model_writes() {
        // Built line by line: a `\`-continued literal in the source would strip
        // the indentation that the nesting case is here to test.
        let md = [
            "# Title",
            "",
            "A paragraph.",
            "",
            "## Section",
            "- top bullet",
            "- second",
            "  - nested",
            "1. first",
            "2. second",
            "",
            "| Name | Qty |",
            "|------|-----|",
            "| Tea  | 3   |",
            "",
            "```rs",
            "let x = 1;",
            "# not a heading",
            "```",
            "",
        ]
        .join("\n");
        let blocks = parse_blocks(&md);
        assert_eq!(blocks[0], Block::Heading { level: 1, text: "Title".into() });
        assert_eq!(blocks[1], Block::Paragraph("A paragraph.".into()));
        assert_eq!(blocks[2], Block::Heading { level: 2, text: "Section".into() });
        assert_eq!(blocks[3], Block::Bullet { depth: 0, text: "top bullet".into() });
        assert_eq!(blocks[4], Block::Bullet { depth: 0, text: "second".into() });
        assert_eq!(blocks[5], Block::Bullet { depth: 1, text: "nested".into() });
        assert_eq!(blocks[6], Block::Numbered { text: "first".into() });
        assert_eq!(blocks[7], Block::Numbered { text: "second".into() });
        // The separator row is structure, not a row of data.
        assert_eq!(
            blocks[8],
            Block::Table(vec![
                vec!["Name".into(), "Qty".into()],
                vec!["Tea".into(), "3".into()],
            ])
        );
        // Inside a fence, a `#` line is code, and the fence marker is gone.
        assert_eq!(blocks[9], Block::Code("let x = 1;\n# not a heading".into()));
        assert_eq!(blocks.len(), 10);
    }

    #[test]
    fn hashtags_and_dashes_are_not_structure() {
        // `#hashtag` has no space, so it is prose. `-dash` is not a bullet.
        assert_eq!(
            parse_blocks("#hashtag\n-dash\ntext"),
            vec![
                Block::Paragraph("#hashtag".into()),
                Block::Paragraph("-dash".into()),
                Block::Paragraph("text".into()),
            ]
        );
        assert_eq!(parse_blocks(""), vec![]);
        assert_eq!(parse_blocks("   \n\n"), vec![]);
    }

    #[test]
    fn a_slug_is_safe_on_every_filesystem() {
        assert_eq!(slug("Q3 report"), "q3-report");
        assert_eq!(slug("  Report/../etc  "), "report-etc");
        assert_eq!(slug("../.."), "document");
        assert_eq!(slug("Résumé — 2026"), "r-sum-2026");
        // Capped, so a runaway title cannot make a path too long.
        assert!(slug(&"a".repeat(500)).len() <= 80);
    }

    #[test]
    fn a_second_document_does_not_overwrite_the_first() {
        let dir = std::env::temp_dir().join(format!("deskseek-docs-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = free_path(&dir, "report", "md");
        std::fs::write(&first, "one").unwrap();
        let second = free_path(&dir, "report", "md");
        assert_ne!(first, second);
        assert_eq!(second.file_name().unwrap(), "report-2.md");
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "one");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_html_page_escapes_the_model_text() {
        let html = html_doc("T", &[Block::Paragraph("<script>alert(1)</script>".into())]);
        assert!(!html.contains("<script>alert(1)</script>"), "raw markup must not survive");
        assert!(html.contains("&lt;script&gt;"));
        // A table becomes a real table, with the first row as the header.
        let table = html_doc(
            "T",
            &[Block::Table(vec![
                vec!["H".into()],
                vec!["<b>".into()],
            ])],
        );
        assert!(table.contains("<th>H</th>"));
        assert!(table.contains("<td>&lt;b&gt;</td>"));
    }

    #[test]
    fn the_emphasis_a_model_writes_becomes_runs_rather_than_asterisks() {
        let plain = |text: &str| Run {
            text: text.into(),
            bold: false,
            italic: false,
        };
        let bold = |text: &str| Run {
            text: text.into(),
            bold: true,
            italic: false,
        };
        let italic = |text: &str| Run {
            text: text.into(),
            bold: false,
            italic: true,
        };

        // A line with no marker is one plain run, and an empty line is no runs.
        assert_eq!(runs_of("Revenue rose."), vec![plain("Revenue rose.")]);
        assert_eq!(runs_of(""), Vec::<Run>::new());

        // The two markers, with the text around them kept in order.
        assert_eq!(
            runs_of("**Q1.** What is GDP?"),
            vec![bold("Q1."), plain(" What is GDP?")]
        );
        assert_eq!(runs_of("a *b* c"), vec![plain("a "), italic("b"), plain(" c")]);
        assert_eq!(runs_of("**a** *b*"), vec![bold("a"), plain(" "), italic("b")]);

        // Everything that is *not* emphasis stays as written: a stray asterisk is a
        // character, an unclosed marker loses nothing, and an underscore is never a
        // marker because it lives inside file names.
        assert_eq!(runs_of("****"), vec![plain("****")]);
        assert_eq!(runs_of("2 * 3 = 6"), vec![plain("2 * 3 = 6")]);
        assert_eq!(runs_of("**unclosed"), vec![plain("**unclosed")]);
        assert_eq!(runs_of("q3_file.docx"), vec![plain("q3_file.docx")]);
    }

    /// The body of a built `.docx`, so a test can say what Word will show rather
    /// than what the library was asked for.
    fn document_xml(bytes: &[u8]) -> String {
        use std::io::Read;
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("a real package");
        let mut part = zip
            .by_name("word/document.xml")
            .expect("a Word document has a body");
        let mut out = String::new();
        part.read_to_string(&mut out).expect("the body is XML");
        out
    }

    /// The run a piece of text is in, as the XML before it: a run's properties come
    /// before its text, so this is what says *that* text is bold or 14pt rather
    /// than that something in the document is.
    fn run_before<'a>(xml: &'a str, text: &str) -> &'a str {
        let at = xml.find(text).unwrap_or_else(|| panic!("{text:?} is not in the document"));
        let before = &xml[..at];
        &before[before.rfind("<w:r>").unwrap_or(0)..]
    }

    #[test]
    fn a_break_line_starts_a_page_and_an_indented_line_is_an_option() {
        assert_eq!(
            parse_blocks("A paragraph.\n\\pagebreak\n## 1. One?\n\n    A) an option"),
            vec![
                Block::Paragraph("A paragraph.".into()),
                Block::Break,
                Block::Heading {
                    level: 2,
                    text: "1. One?".into(),
                },
                Block::Indented("A) an option".into()),
            ]
        );

        // The indent is read as a fact about the line rather than kept in its text,
        // and a tab means what four spaces mean — a tab left in the text is a
        // character Word will not print.
        assert_eq!(
            parse_blocks("        deeply indented"),
            vec![Block::Indented("deeply indented".into())]
        );
        assert_eq!(
            parse_blocks("\tA) an option"),
            vec![Block::Indented("A) an option".into())]
        );

        // Three spaces is not an indent: it is a paragraph that happens to start
        // shifted, and it is trimmed like every other paragraph.
        assert_eq!(
            parse_blocks("   three"),
            vec![Block::Paragraph("three".into())]
        );
        // An indented *list* is still a list — this reader has always nested those.
        assert_eq!(
            parse_blocks("    - nested"),
            vec![Block::Bullet {
                depth: 2,
                text: "nested".into(),
            }]
        );
        // And the break is one exact line, so prose that mentions it is prose.
        assert_eq!(
            parse_blocks("written \\pagebreak in a sentence"),
            vec![Block::Paragraph("written \\pagebreak in a sentence".into())]
        );
    }

    /// The paragraph a piece of text is in, as the XML before it.
    ///
    /// Where `run_before` reads a *run*'s properties, this reads the paragraph's —
    /// an indent and a page break are written there, before the runs.
    fn paragraph_before<'a>(xml: &'a str, text: &str) -> &'a str {
        let at = xml
            .find(text)
            .unwrap_or_else(|| panic!("{text:?} is not in the document"));
        let before = &xml[..at];
        &before[before.rfind("<w:p>").unwrap_or(0)..]
    }

    #[test]
    fn a_break_starts_a_page_and_an_option_sits_in_from_the_margin() {
        let md = [
            "# Subject",
            "",
            "Made 14 October 2026",
            "",
            PAGE_BREAK,
            "## 1. One?",
            "",
            "    A) an option",
        ]
        .join("\n");
        let blocks = parse_blocks(&md);
        let xml = document_xml(&docx_bytes(&blocks).unwrap());

        // The break belongs to the block that follows it, not to a paragraph of its
        // own — an empty paragraph would print a blank line at the top of the page it
        // had just started. So between the break and the question there is no
        // paragraph end, and the title is on the near side of it.
        let at = xml
            .find("<w:pageBreakBefore")
            .unwrap_or_else(|| panic!("the break is not in the document: {xml}"));
        let after = &xml[at..];
        let question = after.find("1. One?").expect("the question follows the break");
        assert!(
            !after[..question].contains("</w:p>"),
            "the break is in a paragraph of its own: {after}"
        );
        assert!(xml.find("Subject").unwrap() < at, "the title came after the break");

        // The option is indented by half an inch of real margin — and the four
        // spaces it arrived with are not in the text, because in Word those would be
        // four characters wide rather than four spaces of margin.
        let option = paragraph_before(&xml, "A) an option");
        assert!(
            option.contains("<w:ind w:left=\"720\""),
            "the option is not indented: {option}"
        );
        assert!(!xml.contains("    A) an option"), "the option kept its spaces: {xml}");

        // The printed page says the same two things in its own language, and indents
        // an option by the same distance in the unit a stylesheet wants.
        let html = html_doc("T", &blocks);
        assert!(html.contains("break-before:page"), "{html}");
        assert!(
            html.contains("margin-left:36pt"),
            "the option is not indented: {html}"
        );
    }

    #[test]
    fn a_word_document_is_set_in_the_house_look() {
        let md = [
            "## 1. What is GDP?",
            "",
            "    A) A stock of assets",
            "    B) A flow of output",
            "",
            "**Note**: the key is a separate sheet.",
        ]
        .join("\n");
        let xml = document_xml(&docx_bytes(&parse_blocks(&md)).unwrap());

        // The font, everywhere: a generated document is Times New Roman rather than
        // whatever the library's default happens to be.
        assert!(xml.contains("Times New Roman"), "{xml}");

        // The question is bold at 14pt (14 points is 28 half-points) and an option
        // is 11pt and not bold — which is the whole request, held per run rather
        // than by looking for a size somewhere in the file.
        let question = run_before(&xml, "What is GDP?");
        assert!(question.contains("<w:b"), "the question is not bold: {question}");
        assert!(question.contains("28"), "the question is not 14pt: {question}");
        let option = run_before(&xml, "A) A stock of assets");
        assert!(option.contains("22"), "an option is not 11pt: {option}");
        assert!(!option.contains("<w:b"), "an option is bold: {option}");

        // And no markup reaches the page: the asterisks *and* the hashes were
        // markup, and the text they marked is what a reader sees.
        assert!(!xml.contains("**"), "asterisks reached the page: {xml}");
        assert!(!xml.contains("##"), "hashes reached the page: {xml}");
        assert!(xml.contains("Note"), "the emphasised text is missing: {xml}");
    }

    #[test]
    fn the_printable_page_carries_the_same_look_and_the_same_emphasis() {
        let html = html_doc(
            "T",
            &parse_blocks("## 1. Why?\n\n**Bold** and *italic*.\n\n```\na **b** c\n```"),
        );
        // Emphasis becomes tags rather than asterisks, and a code block is the one
        // place they are text — because there they are the content.
        assert!(html.contains("<strong>Bold</strong>"), "{html}");
        assert!(html.contains("<em>italic</em>"), "{html}");
        assert!(!html.contains("**Bold**"), "asterisks reached the page: {html}");
        assert!(html.contains("a **b** c"), "a code block lost its asterisks: {html}");

        // And the stylesheet is the same font and the same sizes the `.docx` sets,
        // so a document does not change its look with its extension.
        assert!(html.contains("'Times New Roman'"), "{html}");
        assert!(html.contains("font:11pt"), "{html}");
        assert!(html.contains("h2{font-size:14pt"), "{html}");
    }

    #[test]
    fn a_pdf_is_only_a_pdf_if_it_starts_like_one() {
        // Chromium exits zero even when it writes nothing useful, so the bytes
        // are the only honest check.
        assert!(looks_like_pdf(b"%PDF-1.7\n..."));
        assert!(!looks_like_pdf(b""));
        assert!(!looks_like_pdf(b"<html>"));
    }

    #[test]
    fn a_windows_path_becomes_a_url_chromium_accepts() {
        let url = file_url(Path::new(r"C:\Users\x\doc.html"));
        assert_eq!(url, "file:///C:/Users/x/doc.html");
        // A POSIX path keeps its single leading slash.
        assert_eq!(file_url(Path::new("/tmp/x.html")), "file:///tmp/x.html");
    }

    /// The path a document tool reports, and the check that the two ways of
    /// saying it agree.
    ///
    /// The sentence is what the model reads and what the window's Open button
    /// parses; the `Option<PathBuf>` beside it is what a notebook's output row
    /// stores, because a row that had to read the path back out of a sentence
    /// would be a second answer about a file that already exists. Asserting the
    /// sentence names the same path is what keeps the two from drifting apart.
    fn path_of(result: &(String, Option<PathBuf>)) -> PathBuf {
        let (text, path) = result;
        let path = path.clone().expect("a document writer must say where it wrote");
        let first = text.lines().next().unwrap_or_default();
        assert_eq!(
            first.trim_start_matches("Saved to: "),
            path.display().to_string(),
            "the sentence and the path disagree"
        );
        path
    }

    /// A ZIP's entry names live in its central directory uncompressed, so the
    /// part we asked for can be found without a zip reader.
    fn contains(haystack: &[u8], needle: &str) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window == needle.as_bytes())
    }

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("deskseek-docs-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn the_document_tools_reject_what_they_cannot_build() {
        let dir = temp_dir();
        // Each of these is a shape a model actually produces.
        for (tool, args) in [
            ("doc_create", json!({"format": "md", "content": "x"})),
            ("doc_create", json!({"name": "x", "format": "md"})),
            ("doc_create", json!({"name": "x", "format": "odt", "content": "x"})),
            ("sheet_create", json!({"name": "x"})),
            ("sheet_create", json!({"name": "x", "rows": []})),
            ("sheet_create", json!({"name": "x", "rows": [5]})),
            ("slides_create", json!({"name": "x", "slides": []})),
            ("slides_create", json!({"name": "x", "slides": [{"bullets": ["a"]}]})),
            ("slides_create", json!({"name": "x", "slides": [7]})),
            ("nonsense", json!({})),
        ] {
            assert!(
                execute(tool, &args, &dir, "").await.is_err(),
                "{tool} {args} should be rejected"
            );
        }
        // Nothing may be left behind by a rejected call.
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_missing_browser_is_explained_rather_than_guessed_at() {
        let dir = temp_dir();
        // A pinned browser that is not there resolves to nothing, so this is the
        // no-browser path whatever happens to be installed on this machine.
        let err = execute(
            "doc_create",
            &json!({"name": "x", "format": "pdf", "content": "# Hi"}),
            &dir,
            r"C:\definitely\not\here\nope.exe",
        )
        .await
        .unwrap_err();
        assert!(err.contains("browser"), "the message must name the problem: {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn it_writes_a_real_file_of_every_format() {
        let dir = temp_dir();
        let md = "# Q3 report\n\n- Tea: 3\n\n| A | B |\n|---|---|\n| 1 | 2 |";

        // Markdown is written exactly as given, under a slugged name.
        let out = execute("doc_create", &json!({"name": "Q3 Report", "format": "md", "content": md}), &dir, "")
            .await
            .unwrap();
        let path = path_of(&out);
        assert_eq!(path.file_name().unwrap().to_string_lossy(), "q3-report.md");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), md);

        // The three OOXML formats are real packages, each carrying the part that
        // proves it is the format the model asked for and not an empty zip.
        for (tool, args, part, magic) in [
            (
                "doc_create",
                json!({"name": "Q3 report", "format": "docx", "content": md}),
                "word/document.xml",
                b"PK\x03\x04".as_slice(),
            ),
            (
                "sheet_create",
                json!({"name": "Q3 sheet", "rows": [["Tea", 3], ["Coffee", "=A2+1"]]}),
                "xl/worksheets/sheet1.xml",
                b"PK\x03\x04".as_slice(),
            ),
            (
                "slides_create",
                json!({"name": "Q3 deck", "slides": [{"title": "Intro", "bullets": ["one", "two"]}]}),
                "ppt/slides/slide1.xml",
                b"PK\x03\x04".as_slice(),
            ),
        ] {
            let out = execute(tool, &args, &dir, "").await.unwrap();
            let path = path_of(&out);
            let bytes = std::fs::read(&path).unwrap();
            assert!(bytes.starts_with(magic), "{tool} did not write a zip: {path:?}");
            // A zip that was never closed has no end-of-central-directory record.
            assert!(
                contains(&bytes, "PK\x05\x06"),
                "{tool} wrote an unterminated zip: {path:?}"
            );
            assert!(contains(&bytes, part), "{tool} is missing {part}");
        }

        // The PDF needs a browser on the machine. Skip rather than fail where
        // there is none, like the other live tests.
        if crate::tools::browse::find_browser("").is_none() {
            eprintln!("no Chromium-family browser; skipping the PDF half");
        } else {
            let out = execute("doc_create", &json!({"name": "Q3 pdf", "format": "pdf", "content": md}), &dir, "")
                .await
                .unwrap();
            let path = path_of(&out);
            assert!(looks_like_pdf(&std::fs::read(&path).unwrap()), "{path:?} is not a PDF");
        }

        // Two documents with the same name do not overwrite each other.
        let again = execute("doc_create", &json!({"name": "Q3 Report", "format": "md", "content": "second"}), &dir, "")
            .await
            .unwrap();
        assert_eq!(path_of(&again).file_name().unwrap().to_string_lossy(), "q3-report-2.md");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), md);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
