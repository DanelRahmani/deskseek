//! Reading a document the user hands over: `.docx`, `.xlsx`, `.pptx` and `.pdf`.
//!
//! The model is text-only, so a document has to become text before it can be
//! read at all — and a Word file is a ZIP of XML, not text, which is why
//! `read_file` refused one outright: `read_to_string` was asked to decode
//! compressed bytes as UTF-8. This module is the other direction of `docs.rs`:
//! that one turns Markdown into a document, this one turns a document back into
//! the words in it. The same rule `docs.rs` states about its Markdown reader
//! applies here — this is deliberately *not* a file-format implementation. It
//! reads the text a person would see on the page and ignores formatting, images,
//! comments and revision marks, so `.docx` and the same words pasted into chat
//! cannot disagree about what the document says.
//!
//! A document whose words are a *picture* — a scan — yields nothing, and says so
//! rather than coming back as an empty document that looks like a bug.

use std::io::{Cursor, Read};

use quick_xml::events::Event;
use quick_xml::Reader;

/// Extensions this module can turn into text. The one list: `read_file`,
/// `attach_path` and `read_preview` all ask this rather than spelling the four
/// names a second time.
pub fn is_extractable(name: &str) -> bool {
    matches!(extension(name).as_str(), "docx" | "xlsx" | "pptx" | "pdf")
}

/// The largest container we will open at all. A `.pptx` with a video in it is
/// enormous while its text is tiny, but reading 500 MB into memory to discover
/// that is not worth doing.
pub const MAX_DOC_BYTES: u64 = 32 * 1024 * 1024;

/// How much text one document may hand back. A guard against a pathological
/// file, not a summary: a book is still a book.
const MAX_CHARS: usize = 120_000;

fn extension(name: &str) -> String {
    std::path::Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// The text of a document, from its bytes.
///
/// The extension decides, because that is what the user sees and what the
/// document tools write; the `%PDF` check is only there for a PDF that lost its
/// name. Returns the text alone — the caller frames it, because a `read_file`
/// result and an attachment are not the same thing to the model.
pub fn text_of(name: &str, bytes: &[u8]) -> Result<String, String> {
    let ext = extension(name);
    let text = match ext.as_str() {
        "docx" => container(name, bytes, &["word/document.xml"]),
        "xlsx" => xlsx_text(name, bytes),
        "pptx" => pptx_text(name, bytes),
        _ if bytes.starts_with(b"%PDF") => pdf_text(name, bytes),
        "pdf" => pdf_text(name, bytes),
        other => {
            return Err(format!(
                "'{name}' is a .{other} file, which DeskSeek cannot read as a document."
            ))
        }
    }?;
    Ok(tidy(cap(text)))
}

fn cap(text: String) -> String {
    if text.chars().count() <= MAX_CHARS {
        return text;
    }
    let cut: String = text.chars().take(MAX_CHARS).collect();
    format!("{cut}\n\n… (the rest of the document was not read)")
}

/// Collapse the blank-line noise an empty paragraph leaves behind, so the text
/// reads like the document rather than like its markup.
fn tidy(text: String) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blanks = 0;
    for line in text.replace('\r', "").lines() {
        let line = line.trim_end();
        if line.trim().is_empty() {
            blanks += 1;
            if blanks > 1 {
                continue;
            }
        } else {
            blanks = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim().to_string()
}

// ------------------------------------------------------------------ containers

/// Open a ZIP of XML and read the named parts as text.
fn container(name: &str, bytes: &[u8], parts: &[&str]) -> Result<String, String> {
    let mut archive = open(name, bytes)?;
    let mut out = String::new();
    for part in parts {
        let xml = part_text(&mut archive, part)?;
        out.push_str(&collect(&xml, b"t", b"p"));
    }
    Ok(out)
}

fn open<'a>(name: &str, bytes: &'a [u8]) -> Result<zip::ZipArchive<Cursor<&'a [u8]>>, String> {
    zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| format!("'{name}' could not be opened as an Office file: {e}"))
}

/// One entry, decoded. Office XML is UTF-8; `from_utf8_lossy` is the honest
/// fallback for a file some other tool wrote with a different encoding.
fn part_text<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    part: &str,
) -> Result<String, String> {
    let mut file = archive
        .by_name(part)
        .map_err(|_| format!("'{part}' is missing from the file"))?;
    let mut raw = Vec::new();
    file.read_to_end(&mut raw).map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&raw).into_owned())
}

// ----------------------------------------------------------------------- docx

/// Walk a Word or PowerPoint part and gather what a reader would see: the text
/// inside `text_local` elements, a newline at every `break_local` (a paragraph),
/// and the inline controls Word writes instead of whitespace.
///
/// Prefixes are ignored — `w:t` and `a:t` are both the text — because the local
/// name is the part that carries the meaning and the prefix is only a namespace
/// abbreviation the producer chose.
fn collect(xml: &str, text_local: &[u8], break_local: &[u8]) -> String {
    let mut reader = Reader::from_str(xml);
    let mut out = String::new();
    let mut in_text = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = e.local_name();
                let name = name.as_ref();
                if name == text_local {
                    in_text = true;
                } else if name == b"tab" {
                    out.push('\t');
                }
            }
            Ok(Event::End(e)) => {
                let name = e.local_name();
                let name = name.as_ref();
                if name == text_local {
                    in_text = false;
                } else if name == break_local {
                    out.push('\n');
                }
            }
            Ok(Event::Empty(e)) => {
                let name = e.local_name();
                match name.as_ref() {
                    b"tab" => out.push('\t'),
                    // A self-closing paragraph is the blank line a person sees.
                    b"br" => out.push('\n'),
                    name if name == break_local => out.push('\n'),
                    _ => {}
                }
            }
            Ok(Event::Text(e)) => {
                if in_text {
                    push_text(&mut out, &e);
                }
            }
            Ok(Event::GeneralRef(r)) => {
                if in_text {
                    push_ref(&mut out, &r);
                }
            }
            Ok(Event::CData(e)) => {
                if in_text {
                    out.push_str(&String::from_utf8_lossy(&e.into_inner()));
                }
            }
            Ok(Event::Eof) => break,
            // A malformed part is worth what was read before it went wrong.
            Err(_) => break,
            _ => {}
        }
    }
    out
}

/// One text event, with its entities resolved. `&amp;` in the file is `&` to the
/// reader that opens it, and the model should see what the reader sees.
fn push_text(out: &mut String, e: &quick_xml::events::BytesText<'_>) {
    let Ok(raw) = e.decode() else { return };
    match quick_xml::escape::unescape(&raw) {
        Ok(text) => out.push_str(&text),
        Err(_) => out.push_str(&raw),
    }
}

/// A general reference — `&amp;`, `&#38;` — arrives as its own event, *outside*
/// the text on either side of it. A reader that only collects `Text` therefore
/// silently drops the character: `a &amp; b` becomes `a  b`. The delimiters the
/// reader stripped are put back so the same `unescape` resolves it.
fn push_ref(out: &mut String, r: &quick_xml::events::BytesRef<'_>) {
    let Ok(name) = r.decode() else { return };
    let raw = format!("&{name};");
    match quick_xml::escape::unescape(&raw) {
        Ok(text) => out.push_str(&text),
        Err(_) => out.push_str(&raw),
    }
}

// ----------------------------------------------------------------------- pptx

fn pptx_text(name: &str, bytes: &[u8]) -> Result<String, String> {
    let mut archive = open(name, bytes)?;
    let mut slides: Vec<(u32, String)> = archive
        .file_names()
        .filter_map(|entry| slide_number(entry).map(|n| (n, entry.to_string())))
        .collect();
    slides.sort_by_key(|(n, _)| *n);
    if slides.is_empty() {
        return Err(format!("'{name}' has no slides in it"));
    }

    let mut out = String::new();
    for (number, entry) in slides {
        let xml = part_text(&mut archive, &entry)?;
        out.push_str(&format!("--- Slide {number} ---\n"));
        out.push_str(&collect(&xml, b"t", b"p"));
        out.push('\n');
    }
    Ok(out)
}

/// `ppt/slides/slide12.xml` is slide 12. Relationships and layouts share the
/// folder, so the shape of the name is the test, not the folder.
fn slide_number(entry: &str) -> Option<u32> {
    let rest = entry.strip_prefix("ppt/slides/slide")?;
    rest.strip_suffix(".xml")?.parse().ok()
}

// ----------------------------------------------------------------------- xlsx

fn xlsx_text(name: &str, bytes: &[u8]) -> Result<String, String> {
    let mut archive = open(name, bytes)?;

    // Shared strings are the file's own dictionary: a cell holds an index into
    // it, not the words. Without this every cell reads as a number.
    let shared = match part_text(&mut archive, "xl/sharedStrings.xml") {
        Ok(xml) => shared_strings(&xml),
        Err(_) => Vec::new(),
    };
    let names = match part_text(&mut archive, "xl/workbook.xml") {
        Ok(xml) => sheet_names(&xml),
        Err(_) => Vec::new(),
    };

    let mut sheets: Vec<(u32, String)> = archive
        .file_names()
        .filter_map(|entry| sheet_number(entry).map(|n| (n, entry.to_string())))
        .collect();
    sheets.sort_by_key(|(n, _)| *n);
    if sheets.is_empty() {
        return Err(format!("'{name}' has no worksheets in it"));
    }

    let mut out = String::new();
    for (number, entry) in sheets {
        let xml = part_text(&mut archive, &entry)?;
        let label = names
            .get(number as usize - 1)
            .cloned()
            .unwrap_or_else(|| format!("Sheet{number}"));
        out.push_str(&format!("--- {label} ---\n"));
        out.push_str(&sheet_text(&xml, &shared));
        out.push('\n');
    }
    Ok(out)
}

/// `xl/worksheets/sheet12.xml` is worksheet 12.
fn sheet_number(entry: &str) -> Option<u32> {
    let rest = entry.strip_prefix("xl/worksheets/sheet")?;
    rest.strip_suffix(".xml")?.parse().ok()
}

/// `<si>` elements in order — the dictionary every `t="s"` cell indexes into.
fn shared_strings(xml: &str) -> Vec<String> {
    let mut reader = Reader::from_str(xml);
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_text = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                if e.local_name().as_ref() == b"t" {
                    in_text = true;
                }
            }
            Ok(Event::End(e)) => {
                let name = e.local_name();
                match name.as_ref() {
                    b"t" => in_text = false,
                    b"si" => {
                        out.push(current.clone());
                        current.clear();
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(e)) => {
                if in_text {
                    push_text(&mut current, &e);
                }
            }
            Ok(Event::GeneralRef(r)) => {
                if in_text {
                    push_ref(&mut current, &r);
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    out
}

/// The `name` of every `<sheet>` in the workbook, in order — so a printed sheet
/// is called what the user called it rather than "Sheet1".
fn sheet_names(xml: &str) -> Vec<String> {
    let mut reader = Reader::from_str(xml);
    let mut out = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                if e.local_name().as_ref() == b"sheet" {
                    if let Some(name) = attribute(&e, b"name") {
                        out.push(name);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    out
}

/// A worksheet as tab-separated rows, the form a model reads most easily and a
/// person can paste into a spreadsheet.
fn sheet_text(xml: &str, shared: &[String]) -> String {
    let mut reader = Reader::from_str(xml);
    let mut out = String::new();
    let mut row: Vec<String> = Vec::new();
    let mut kind = String::new();
    let mut value = String::new();
    let mut in_value = false;
    let mut in_inline = false;
    let mut in_inline_text = false;
    let mut column: Option<usize> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = e.local_name();
                match name.as_ref() {
                    b"c" => {
                        // `t` says how to read the value; `r` is the cell's
                        // reference, e.g. "B7", which is the only way to keep
                        // columns lined up when Excel omits the empty cells.
                        kind = attribute(&e, b"t").unwrap_or_default();
                        value.clear();
                        column = attribute(&e, b"r").as_deref().and_then(column_index);
                    }
                    b"v" => in_value = true,
                    b"is" => in_inline = true,
                    b"t" if in_inline => in_inline_text = true,
                    _ => {}
                }
            }
            Ok(Event::Text(e)) => {
                if in_value || in_inline_text {
                    push_text(&mut value, &e);
                }
            }
            Ok(Event::GeneralRef(r)) => {
                if in_value || in_inline_text {
                    push_ref(&mut value, &r);
                }
            }
            Ok(Event::End(e)) => {
                let name = e.local_name();
                match name.as_ref() {
                    b"v" => in_value = false,
                    b"t" => in_inline_text = false,
                    b"is" => in_inline = false,
                    b"c" => {
                        if let Some(at) = column {
                            while row.len() < at {
                                row.push(String::new());
                            }
                        }
                        row.push(cell_text(&kind, &value, shared));
                    }
                    b"row" => {
                        out.push_str(&row.join("\t"));
                        out.push('\n');
                        row.clear();
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    out
}

/// One cell, resolved the way Excel would show it.
fn cell_text(kind: &str, value: &str, shared: &[String]) -> String {
    match kind {
        // A shared string is an index into the dictionary.
        "s" => value
            .trim()
            .parse::<usize>()
            .ok()
            .and_then(|index| shared.get(index).cloned())
            .unwrap_or_default(),
        "b" => match value.trim() {
            "1" => "TRUE".to_string(),
            "0" => "FALSE".to_string(),
            other => other.to_string(),
        },
        _ => value.to_string(),
    }
}

/// `B7` is column 2. Letters only — the row number is not a column.
fn column_index(reference: &str) -> Option<usize> {
    let letters: Vec<u8> = reference
        .bytes()
        .take_while(u8::is_ascii_alphabetic)
        .collect();
    if letters.is_empty() {
        return None;
    }
    let mut index = 0usize;
    for ch in letters {
        index = index * 26 + (ch.to_ascii_uppercase() - b'A' + 1) as usize;
    }
    Some(index - 1)
}

/// One attribute, with its entities resolved — `escape::unescape` by hand
/// rather than `Attribute::decode_and_unescape_value`, which wants a decoder the
/// callers here have no reason to carry: every part is already lossy UTF-8.
fn attribute(e: &quick_xml::events::BytesStart<'_>, key: &[u8]) -> Option<String> {
    let found = e
        .attributes()
        .flatten()
        .find(|a| a.key.local_name().as_ref() == key)?;
    let raw = String::from_utf8_lossy(&found.value).into_owned();
    match quick_xml::escape::unescape(&raw) {
        Ok(text) => Some(text.into_owned()),
        Err(_) => Some(raw),
    }
}

// ------------------------------------------------------------------------ pdf

/// ponytail: the whole PDF path is one call into `pdf-extract`. It is a parser
/// over untrusted bytes, and the release profile is `panic = "abort"`, so a
/// panic inside it takes the app down rather than failing one tool call —
/// `catch_unwind` cannot help under `abort`. The ceiling is a malformed PDF that
/// trips the crate; the upgrade path is to run extraction out-of-process if that
/// ever shows up in practice.
fn pdf_text(name: &str, bytes: &[u8]) -> Result<String, String> {
    let text = pdf_extract::extract_text_from_mem(bytes)
        .map_err(|e| format!("'{name}' could not be read as a PDF: {e}"))?;
    if text.trim().is_empty() {
        return Err(format!(
            "'{name}' has no text a reader can lift out — it is probably a scan, so its words \
             are a picture rather than characters."
        ));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The writer and the reader here are two pieces of code written from the
    /// same spec, so a round trip through the crates `docs.rs` writes with is the
    /// only test that can catch them disagreeing.
    #[test]
    fn a_word_document_comes_back_as_its_words() {
        use docx_rs::{Docx, Paragraph, Run};

        let doc = Docx::new()
            .add_paragraph(Paragraph::new().add_run(Run::new().add_text("Quarterly report")))
            .add_paragraph(Paragraph::new().add_run(Run::new().add_text("Revenue rose & held")));
        let mut bytes = Vec::new();
        doc.build().pack(Cursor::new(&mut bytes)).unwrap();

        let text = text_of("report.docx", &bytes).unwrap();
        assert!(text.contains("Quarterly report"), "{text}");
        assert!(text.contains("Revenue rose & held"), "entities resolved: {text}");
        // Two paragraphs, two lines — not one run-on line.
        assert!(text.contains("Quarterly report\nRevenue"), "{text}");
    }

    #[test]
    fn a_spreadsheet_comes_back_as_rows_and_columns() {
        let mut book = rust_xlsxwriter::Workbook::new();
        let sheet = book.add_worksheet();
        sheet.set_name("Sales").unwrap();
        sheet.write_string(0, 0, "Item").unwrap();
        sheet.write_string(0, 1, "Qty").unwrap();
        sheet.write_string(1, 0, "Tea").unwrap();
        sheet.write_number(1, 1, 3.0).unwrap();
        let bytes = book.save_to_buffer().unwrap();

        let text = text_of("sales.xlsx", &bytes).unwrap();
        assert!(text.contains("Sales"), "sheet name: {text}");
        assert!(text.contains("Item\tQty"), "{text}");
        assert!(text.contains("Tea\t3"), "{text}");
    }

    #[test]
    fn a_deck_comes_back_slide_by_slide() {
        use ppt_rs::generator::slide_content::SlideContent;

        let slides = vec![
            SlideContent::new("Agenda").add_bullet("First point"),
            SlideContent::new("Numbers").add_bullet("42"),
        ];
        let bytes = ppt_rs::create_pptx_with_content("Deck", slides).unwrap();

        let text = text_of("deck.pptx", &bytes).unwrap();
        assert!(text.contains("Slide 1"), "{text}");
        assert!(text.contains("Agenda"), "{text}");
        assert!(text.contains("First point"), "{text}");
        assert!(text.contains("Slide 2"), "{text}");
        // Slide 2 comes after slide 1, whatever order the archive stores them in.
        assert!(
            text.find("Agenda").unwrap() < text.find("Numbers").unwrap(),
            "slides out of order: {text}"
        );
    }

    #[test]
    fn the_four_document_extensions_are_the_whole_list() {
        for name in ["a.docx", "a.XLSX", "a.pptx", "a.pdf"] {
            assert!(is_extractable(name), "{name}");
        }
        for name in ["a.txt", "a.md", "a.png", "a.zip", "noextension"] {
            assert!(!is_extractable(name), "{name}");
        }
    }

    #[test]
    fn a_broken_file_says_so_instead_of_returning_nothing() {
        // A .docx that is not a ZIP: the model must be told it could not be
        // read, because an empty result reads as an empty document.
        let err = text_of("broken.docx", b"not a zip at all").unwrap_err();
        assert!(err.contains("broken.docx"), "{err}");

        // A file with no document extension at all is refused by name.
        let err = text_of("notes.rtf", b"{\\rtf1}").unwrap_err();
        assert!(err.contains(".rtf"), "{err}");
    }

    #[test]
    fn a_pdf_that_is_not_a_pdf_is_reported_as_a_pdf_problem() {
        // The extraction itself is the crate's job; what is ours is choosing the
        // path by extension and framing the failure. Garbage must not arrive as
        // an empty document.
        let err = text_of("scan.pdf", b"%PDF-1.4 not really").unwrap_err();
        assert!(err.contains("scan.pdf"), "{err}");
        assert!(err.contains("PDF"), "{err}");
    }

    #[test]
    fn a_cell_reference_is_a_column_and_not_a_row() {
        assert_eq!(column_index("A1"), Some(0));
        assert_eq!(column_index("B7"), Some(1));
        assert_eq!(column_index("Z1"), Some(25));
        assert_eq!(column_index("AA1"), Some(26));
        assert_eq!(column_index("7"), None);
        assert_eq!(column_index(""), None);
    }

    #[test]
    fn blank_paragraphs_do_not_swallow_the_document() {
        // Word writes an empty <w:p> for every blank line, so the raw text is
        // mostly newlines; what a person sees is the words.
        let xml = r#"<w:document><w:body>
            <w:p><w:r><w:t>One</w:t></w:r></w:p>
            <w:p/>
            <w:p/>
            <w:p><w:r><w:t>Two</w:t></w:r></w:p>
        </w:body></w:document>"#;
        let text = tidy(collect(xml, b"t", b"p"));
        assert_eq!(text, "One\n\nTwo");
    }

    #[test]
    fn a_tab_and_a_break_are_whitespace_not_text() {
        let xml = r#"<w:p><w:r><w:t>a</w:t><w:tab/><w:t>b</w:t><w:br/><w:t>c</w:t></w:r></w:p>"#;
        assert_eq!(tidy(collect(xml, b"t", b"p")), "a\tb\nc");
    }
}
