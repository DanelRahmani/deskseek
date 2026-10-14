//! What a citation is, and whether it resolves.
//!
//! One rule in one file, because it is the rule the whole "answer from these
//! sources, with citations" feature rests on: a label the model writes that names
//! nothing it was given is a fabrication, and it has to be catchable without
//! asking the model what it meant. So the label format lives here, the parser
//! that finds citations in an answer lives here, and the check that every one of
//! them names something real lives here.
//!
//! The chunk index and the query it serves arrive with 5.4. The label format and
//! this rule arrived first, in 4.1, because a notebook's sources carry labels
//! before anything is indexed — and a second home for "what a citation is" would
//! be exactly the drift this file exists to prevent.

/// The label one source is cited by: its name, and the heading when the citation
/// is about one part of it.
///
/// `report.pdf`, or `report.pdf#Method`. Deliberately the file's own name rather
/// than a number: a reader can see at a glance whether the source it names is one
/// the notebook holds, and so can the check below.
pub fn label(name: &str, heading: Option<&str>) -> String {
    match heading.map(str::trim).filter(|h| !h.is_empty()) {
        Some(heading) => format!("{}#{heading}", name.trim()),
        None => name.trim().to_string(),
    }
}

/// Looks like a source rather than prose.
///
/// ponytail: a heuristic, and the ceiling is that a source named with no dot and
/// no hash — a bare `README` — is not recognised as a citation. It is here
/// because the alternative is worse: with every bracketed span counted, `[sic]`,
/// an edit marker and a numbered footnote `[1]` all become citations naming
/// nothing, and a rule that fires on prose gets turned off. The upgrade path is
/// to pass the label list in and match against it, which this function already
/// receives at the call site.
fn looks_like_a_source(inner: &str) -> bool {
    !inner.is_empty()
        && !inner.chars().all(|c| c.is_ascii_digit())
        && (inner.contains('.') || inner.contains('#') || inner.contains('/'))
}

/// Every source an answer cites, in order of first mention and without repeats.
///
/// A markdown link — `[text](url)` — is not a citation: a bracket followed by a
/// paren is a link, and every answer that links something would otherwise be
/// reported as citing a source that does not exist.
pub fn cited(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '[' {
            i += 1;
            continue;
        }
        let Some(offset) = chars[i + 1..].iter().position(|c| *c == ']') else {
            break;
        };
        let close = i + 1 + offset;
        let inner: String = chars[i + 1..close].iter().collect();
        let inner = inner.trim().to_string();
        let is_link = chars.get(close + 1) == Some(&'(');
        if !is_link && !inner.contains('\n') && looks_like_a_source(&inner) {
            let known = out.iter().any(|seen| seen.eq_ignore_ascii_case(&inner));
            if !known {
                out.push(inner);
            }
        }
        i = close + 1;
    }
    out
}

/// The citations that name nothing in `available`.
///
/// This is the bug the feature's own spec names: a citation for a source that
/// was not there to be read. Compared without case, because whether the model
/// capitalised a file name is not the point.
///
/// A citation may name a *part* of a source — `report.pdf#Method` — so a label is
/// held against both the full label and the name before its hash.
pub fn unresolved(text: &str, available: &[String]) -> Vec<String> {
    let known = |citation: &str| {
        let name = citation.split('#').next().unwrap_or(citation).trim();
        available.iter().any(|label| {
            label.eq_ignore_ascii_case(citation)
                || label.eq_ignore_ascii_case(name)
                || label.split('#').next().unwrap_or(label).eq_ignore_ascii_case(name)
        })
    };
    cited(text).into_iter().filter(|c| !known(c)).collect()
}

// ------------------------------------------------------- the chunk index

use std::path::{Path, PathBuf};

/// How long one chunk may be before it is split at a line boundary.
///
/// Big enough to hold a section of prose and small enough that four of them are
/// an excerpt rather than a document: what a turn injects is the part of a file
/// that bears on the question, and a chunk is the unit that decision is made in.
pub const MAX_CHUNK_CHARS: usize = 2_000;

/// How many files one indexing pass will read.
pub const MAX_FILES: usize = 200;

/// How many bytes one pass will read. Whichever of the two runs out first stops
/// the pass, and the next turn carries on where it left off — because every file
/// already indexed is skipped for the price of a `stat`, so the budget advances
/// by itself without a bookmark to keep in step.
///
/// ponytail: 4 MB per turn, so the first question asked in a large folder reads a
/// few dozen documents and the rest arrive over the next few turns. Above that,
/// indexing would be a stall the user did not ask for. The upgrade path is a
/// background pass with progress, which this does not need while a folder of
/// documents is what a chat owns.
pub const MAX_PASS_BYTES: u64 = 4 * 1024 * 1024;

/// The largest single file the index will read at all.
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// How many chunks a turn injects, and the character budget for all of them.
pub const RECALL_CHUNKS: usize = 4;
pub const RECALL_CHARS: usize = 8_000;

/// Extensions the index reads as plain text, beside whatever `extract` can open.
///
/// A list rather than "anything not binary", because the alternative indexes a
/// `.log`, a lockfile and a minified bundle, and the first question then costs
/// more than the answer is worth. `extract::is_extractable` covers `.docx`,
/// `.xlsx`, `.pptx` and `.pdf`.
const TEXT_EXTENSIONS: [&str; 18] = [
    "md", "markdown", "txt", "csv", "tsv", "json", "yaml", "yml", "toml", "ini", "rs", "ts", "tsx",
    "js", "jsx", "py", "go", "sql",
];

/// Whether the index should read this file.
pub fn is_indexable(name: &str) -> bool {
    if crate::tools::extract::is_extractable(name) {
        return true;
    }
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    TEXT_EXTENSIONS.contains(&ext.as_str())
}

/// How a document becomes chunks: `(heading, text)` pairs, in order.
///
/// `headings` is false for everything that is not markdown, and that is not a
/// shortcut — a `#` in a Python file, a shell script or a YAML comment is a
/// comment, and treating those as headings would file every comment line as a
/// section and hand the model a document made of headings.
///
/// A section longer than `MAX_CHUNK_CHARS` is split at line boundaries and keeps
/// its heading, so the answer can still say which part of the document it read.
pub fn chunks_of(text: &str, headings: bool) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut heading = String::new();
    let mut body = String::new();

    for line in text.lines() {
        let marked = heading_of(line);
        if headings && marked.is_some() {
            push_chunk(&mut out, &heading, &body);
            heading = marked.unwrap_or_default();
            body.clear();
            continue;
        }
        // A single line longer than a chunk — a minified file, a one-line CSV —
        // is cut where the limit falls, because the alternative is indexing the
        // whole thing as one chunk and sending all of it or none of it.
        if body.chars().count() + line.chars().count() + 1 > MAX_CHUNK_CHARS && !body.is_empty() {
            push_chunk(&mut out, &heading, &body);
            body.clear();
        }
        push_line(&mut out, &heading, &mut body, line);
    }
    push_chunk(&mut out, &heading, &body);
    out
}

/// The heading a markdown line declares, if it declares one.
fn heading_of(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = &trimmed[hashes..];
    if !rest.starts_with(' ') && !rest.is_empty() {
        return None;
    }
    let title = rest.trim();
    (!title.is_empty()).then(|| title.to_string())
}

fn push_chunk(out: &mut Vec<(String, String)>, heading: &str, body: &str) {
    // A chunk with no words in it is not an excerpt of anything; indexing one
    // gives a question a match with nothing behind it.
    if body.trim().is_empty() {
        return;
    }
    out.push((heading.to_string(), body.trim().to_string()));
}

fn push_line(out: &mut Vec<(String, String)>, heading: &str, body: &mut String, line: &str) {
    let mut rest = line;
    // A line that will not fit is cut; the remainder stays in the body and the
    // next call continues it, so nothing is dropped.
    while rest.chars().count() > MAX_CHUNK_CHARS {
        let cut = rest
            .char_indices()
            .nth(MAX_CHUNK_CHARS)
            .map(|(i, _)| i)
            .unwrap_or(rest.len());
        let (head, tail) = rest.split_at(cut);
        body.push_str(head);
        push_chunk(out, heading, body);
        body.clear();
        rest = tail;
    }
    if !body.is_empty() {
        body.push('\n');
    }
    body.push_str(rest);
}

/// Brings the index up to date for these roots, and answers how many files it
/// read. Files that have not moved cost one `stat` each; files that have are
/// re-read and re-chunked; files that are gone have their chunks swept out.
pub fn index_roots(
    conn: &rusqlite::Connection,
    roots: &[PathBuf],
    budget: u64,
) -> Result<usize, String> {
    let mut spent = 0u64;
    let mut read = 0usize;
    for root in roots {
        let root_key = display(root);
        let walked: Vec<PathBuf> = crate::tools::walk::walk_files(root)
            .into_iter()
            .filter(|path| {
                path.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(is_indexable)
            })
            .take(MAX_FILES)
            .collect();
        let here: Vec<String> = walked.iter().map(|p| display(p)).collect();

        // A file that has been deleted or renamed must stop answering questions.
        for stale in crate::store::db::indexed_paths(conn, &root_key).map_err(|e| e.to_string())? {
            if !here.contains(&stale) {
                crate::store::db::delete_path_chunks(conn, &stale).map_err(|e| e.to_string())?;
            }
        }

        for path in &walked {
            if spent >= budget {
                return Ok(read);
            }
            let key = display(path);
            let Ok(meta) = std::fs::metadata(path) else {
                continue;
            };
            if meta.len() > MAX_FILE_BYTES {
                continue;
            }
            let fingerprint = crate::conversation::notebook::fingerprint_of(path)?;
            let known = crate::store::db::chunk_fingerprint(conn, &key).map_err(|e| e.to_string())?;
            if known.as_deref() == Some(fingerprint.as_str()) {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            let Ok(text) = read_text(path, &name) else {
                continue;
            };
            let chunks = chunks_of(&text, is_markdown(&name));
            if chunks.is_empty() {
                crate::store::db::delete_path_chunks(conn, &key).map_err(|e| e.to_string())?;
                continue;
            }
            spent += meta.len();
            read += 1;
            crate::store::db::replace_chunks(conn, &root_key, &key, &fingerprint, &chunks)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(read)
}

/// The best chunks across every root, best first.
///
/// One search per root and then one merge, because `bm25` only compares rows of
/// the search it came from — and the tiebreak is the label so two runs of the
/// same question cannot disagree about what to cite.
pub fn search(
    conn: &rusqlite::Connection,
    roots: &[PathBuf],
    query: &str,
    k: usize,
) -> Result<Vec<crate::store::db::ChunkHit>, String> {
    let mut hits: Vec<crate::store::db::ChunkHit> = Vec::new();
    for root in roots {
        let mut found = crate::store::db::search_chunks(conn, &display(root), query, k as i64)
            .map_err(|e| e.to_string())?;
        hits.append(&mut found);
    }
    hits.sort_by(|a, b| {
        a.score
            .total_cmp(&b.score)
            .then_with(|| label_for(&a.root, &a.path, &a.heading).cmp(&label_for(&b.root, &b.path, &b.heading)))
    });
    hits.truncate(k);
    Ok(hits)
}

/// The label a chunk is cited by: its path relative to the root it was found
/// under, then the heading.
///
/// Relative rather than absolute because a label is read by a person and written
/// by the model, and `C:\Users\…\documents\q3.pdf#Revenue` is neither: the
/// file's own name and the section is what identifies it in an answer.
pub fn label_for(root: &str, path: &str, heading: &str) -> String {
    let trimmed = path
        .strip_prefix(root)
        .unwrap_or(path)
        .trim_start_matches(['/', '\\']);
    let relative = if trimmed.is_empty() { path } else { trimmed };
    label(&relative.replace('\\', "/"), Some(heading))
}

fn display(path: &Path) -> String {
    path.display().to_string()
}

fn is_markdown(name: &str) -> bool {
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    ext == "md" || ext == "markdown"
}

/// The readable text of a file, the same way the notebook's sources are read.
fn read_text(path: &Path, name: &str) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("could not read {name}: {e}"))?;
    if crate::tools::extract::is_extractable(name) {
        return crate::tools::extract::text_of(name, &bytes);
    }
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn a_citation_names_its_source_and_the_part_it_used() {
        assert_eq!(label("report.pdf", None), "report.pdf");
        assert_eq!(label("report.pdf", Some("  ")), "report.pdf");
        assert_eq!(label("report.pdf", Some(" Method ")), "report.pdf#Method");
    }

    #[test]
    fn a_citation_that_names_nothing_is_caught() {
        let available = names(&["q3.pdf", "notes.md"]);
        // Two real ones and one invented: the invented one is what this rule is
        // for, and it is reported on its own.
        let answer = "Revenue rose [q3.pdf] and costs fell [notes.md#Costs], but \
                      the outlook is [strategy-2030.docx].";
        assert_eq!(unresolved(answer, &available), names(&["strategy-2030.docx"]));
        // Case is not the point.
        assert!(unresolved("See [Q3.PDF].", &available).is_empty());
        // A part of a source is still that source.
        assert!(unresolved("See [q3.pdf#Revenue].", &available).is_empty());
        // And an answer with no citations at all is not an offender.
        assert!(unresolved("Nothing here.", &available).is_empty());
    }

    #[test]
    fn prose_and_links_are_not_mistaken_for_citations() {
        let available = names(&["q3.pdf"]);
        // A link, an edit marker, a footnote number, and a bracketed aside.
        let answer = "See [the release notes](https://example.com/x), [sic], [1], and [Note].";
        assert!(cited(answer).is_empty(), "got {:?}", cited(answer));
        assert!(unresolved(answer, &available).is_empty());
        // The same answer with a real citation still finds it, once.
        let mixed = "[q3.pdf] and [q3.pdf] and [1] and [link](http://x)";
        assert_eq!(cited(mixed), names(&["q3.pdf"]));
    }

    #[test]
    fn a_document_becomes_chunks_at_its_headings() {
        let text = "# One\n\nalpha beta\n\n## Two\n\ngamma\n\n# Three\n\ndelta\n";
        let chunks = chunks_of(text, true);
        assert_eq!(chunks.len(), 3, "got {chunks:?}");
        assert_eq!(chunks[0].0, "One");
        assert!(chunks[0].1.contains("alpha beta"));
        assert_eq!(chunks[1].0, "Two");
        assert!(chunks[1].1.contains("gamma"));
        assert_eq!(chunks[2].0, "Three");
        // Text before the first heading is still a chunk, with no heading — a
        // document's opening paragraph is not thrown away because it has no
        // section above it.
        let opening = chunks_of("an introduction\n\n# One\n\nbody\n", true);
        assert_eq!(opening[0].0, "", "the preamble has no heading");
        assert!(opening[0].1.contains("an introduction"));
        // A section too long for one chunk is split and keeps its heading, so the
        // answer can still say which part of the document it read.
        let long = format!("# Big\n\n{}", "word ".repeat(MAX_CHUNK_CHARS));
        let split = chunks_of(&long, true);
        assert!(split.len() > 1, "a long section must be split");
        assert!(split.iter().all(|(heading, _)| heading == "Big"));
        assert!(split.iter().all(|(_, body)| body.chars().count() <= MAX_CHUNK_CHARS));
        // Nothing is lost in the split.
        let rejoined: String = split.iter().map(|(_, body)| body.as_str()).collect();
        assert_eq!(rejoined.matches("word").count(), MAX_CHUNK_CHARS);
        // Blank text is no chunk at all, rather than one that matches every query
        // with nothing behind it.
        assert!(chunks_of("   \n\n\n", true).is_empty());
    }

    #[test]
    fn only_markdown_is_chunked_by_its_hashes() {
        // A Python file's `#` is a comment, and treating those as headings would
        // hand the model a document made of comments.
        let python = "# a comment\nprint('x')\n# another\nprint('y')\n";
        let chunks = chunks_of(python, false);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].0, "");
        assert!(chunks[0].1.contains("a comment") && chunks[0].1.contains("print('y')"));
        // The same text read as markdown is two sections, which is what the flag
        // is for.
        let marked = chunks_of(python, true);
        assert_eq!(marked.len(), 2, "got {marked:?}");
        assert_eq!(marked[0].0, "a comment");
        assert_eq!(marked[1].0, "another");
        // A heading with nothing under it is no chunk: there is nothing in it to
        // answer from, and the file simply contributes nothing.
        assert!(chunks_of("# Only a heading\n", true).is_empty());
        assert!(is_markdown("notes.md") && is_markdown("README.MARKDOWN"));
        assert!(!is_markdown("main.py"));
        assert!(is_indexable("notes.md") && is_indexable("report.docx") && is_indexable("x.rs"));
        assert!(!is_indexable("photo.png") && !is_indexable("app.exe"));
    }

    /// A real database on a real file: the chunk tables are `CREATE TABLE`, and a
    /// fresh in-memory schema would prove nothing about `open()` building them.
    fn temp_db() -> (rusqlite::Connection, PathBuf) {
        let dir = std::env::temp_dir().join(format!("deskseek-chunks-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let conn = crate::store::db::open(&dir.join("deskseek.db")).unwrap();
        (conn, dir)
    }

    #[test]
    fn the_index_reads_a_file_once_and_again_only_when_it_moves() {
        let (conn, dir) = temp_db();
        std::fs::write(dir.join("notes.md"), "# Revenue\n\nrevenue rose sharply\n").unwrap();
        std::fs::write(dir.join("other.txt"), "unrelated words\n").unwrap();
        std::fs::write(dir.join("photo.png"), "not text\n").unwrap();
        let roots = [dir.clone()];

        // Two files are read; the picture is not one of them.
        assert_eq!(index_roots(&conn, &roots, MAX_PASS_BYTES).unwrap(), 2);
        // Nothing has moved, so a second pass reads nothing — which is the whole
        // reason the fingerprint is stored, and what makes the next turn cheap.
        assert_eq!(
            index_roots(&conn, &roots, MAX_PASS_BYTES).unwrap(),
            0,
            "an unchanged file was read again"
        );
        // The words are found, and the chunk says which file it came from.
        let hits = search(&conn, &roots, "revenue", 4).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].text.contains("rose sharply"));
        let label = label_for(&hits[0].root, &hits[0].path, &hits[0].heading);
        assert_eq!(label, "notes.md#Revenue", "a chunk is labelled by file and heading");

        // A file that changes is read again...
        std::fs::write(
            dir.join("notes.md"),
            "# Revenue\n\nrevenue fell this quarter instead\n",
        )
        .unwrap();
        assert_eq!(index_roots(&conn, &roots, MAX_PASS_BYTES).unwrap(), 1);
        let hits = search(&conn, &roots, "instead", 4).unwrap();
        assert!(hits[0].text.contains("instead"), "the new text must be indexed");
        // ...and the old text is gone rather than answering alongside it.
        assert!(
            search(&conn, &roots, "sharply", 4).unwrap().is_empty(),
            "a replaced chunk kept answering"
        );

        // A file that is deleted stops answering questions.
        std::fs::remove_file(dir.join("notes.md")).unwrap();
        assert_eq!(index_roots(&conn, &roots, MAX_PASS_BYTES).unwrap(), 0);
        assert!(
            search(&conn, &roots, "revenue", 4).unwrap().is_empty(),
            "a deleted file was still searchable"
        );

        // The budget stops a pass; the next one carries on, because every file
        // already read is skipped for the price of a stat.
        std::fs::write(dir.join("big.md"), "needle\n".repeat(2_000)).unwrap();
        assert_eq!(index_roots(&conn, &roots, 0).unwrap(), 0, "a spent budget reads nothing");
        assert_eq!(index_roots(&conn, &roots, MAX_PASS_BYTES).unwrap(), 1);
        assert!(!search(&conn, &roots, "needle", 4).unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
