//! A notebook: its sources, what they contain, and the questions asked of them.
//!
//! The rule this file owns is **how much of a source reaches the model**. A
//! notebook is a place to put forty documents and ask about any of them, so the
//! prompt carries a *catalogue* — each source's name and what it contains — and
//! the question's own sources are the only text sent. Nothing here sends a whole
//! folder to the model because the folder happens to exist.
//!
//! Two smaller rules live here too, and both are the same fact seen twice: a
//! source's **fingerprint** is what its digest describes, so one comparison
//! answers both "has this changed on disk" (the rail's *stale*) and "is the digest
//! worth paying for again" (the answer is no, until it actually changes).
//!
//! 5.4 adds the pull half — `source_search` and `read_source` — on top of the
//! catalogue this file already builds. The push half here is what a chat does
//! with a folder, one budget at a time.

use std::path::Path;

use serde::Serialize;

use crate::api::client::Client;
use crate::api::types::Usage;
use crate::conversation::agent::push_instructions;
use crate::retrieval;
use crate::store::db;
use crate::store::settings::Settings;
use crate::AppState;

/// How much of one source the first pass of the budget sends.
///
/// ponytail: fixed rather than token-counted, because counting tokens needs the
/// model's own tokeniser and this app has never had one. The ceiling is that a
/// long document is truncated rather than summarised; the upgrade path is the
/// tools 5.4 adds, which fetch the part of a source a question is actually about
/// instead of paying for the beginning of all of them.
pub const ASK_SOURCE_CHARS: usize = 12_000;

/// The whole source section's budget, across every enabled source.
///
/// Deliberately generous but bounded: forty documents must not each get their
/// slice of a window that only holds a few.
pub const ASK_TOTAL_CHARS: usize = 48_000;

/// How much of a source the digest call reads. A digest is "what this contains",
/// which comes from the beginning and the shape of a document, not from all of it.
pub const DIGEST_CHARS: usize = 16_000;

/// The grounding contract, sent with every question.
const GROUNDED: &str = "\
The sources are listed above with what each one contains, and their text is given below.\n\n\
Answer from them. Cite the source you used by writing its name in square brackets exactly as it \
is written above — `[name]` for the whole of it, or `[name#Heading]` for one part. Never cite a \
source that is not in that list: a citation the reader cannot look up is worse than no citation \
at all. If the sources do not answer the question, say so plainly rather than filling the gap \
from memory, and never present a guess as something a source said.";

/// The instruction the digest call runs under.
const DIGEST_SYSTEM: &str = "\
You describe one document in two sentences, for a list a reader scans to decide what to open.\n\n\
Say what it is and what it covers — the subject, the period, the kind of document, and anything \
that makes it distinctive. No preamble, no \"this document\", no restating the file name. Never \
invent a fact that is not in the text.";

/// One source as the window reads it: the row, plus the one thing the row cannot
/// know — whether the file it points at is still the file its digest describes.
///
/// Built by `view`/`views` rather than assembled by each caller, so `state` is
/// decided in one place. That is also why `extract` is not here: the text of a
/// source never reaches the window.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceView {
    pub id: String,
    pub notebook_id: String,
    pub name: String,
    pub kind: String,
    pub origin: String,
    pub path: Option<String>,
    pub enabled: bool,
    pub digest: String,
    pub digest_at: Option<i64>,
    /// `ok` | `stale` | `missing`
    pub state: &'static str,
    /// How much text was read out of it — so a source that extracted to nothing
    /// says so rather than looking like one that answered every question.
    pub chars: usize,
    pub created_at: i64,
}

pub fn view(row: &db::SourceRow) -> SourceView {
    SourceView {
        id: row.id.clone(),
        notebook_id: row.notebook_id.clone(),
        name: row.name.clone(),
        kind: row.kind.clone(),
        origin: row.origin.clone(),
        path: row.path.clone(),
        enabled: row.enabled,
        digest: row.digest.clone(),
        digest_at: row.digest_at,
        state: state_of(row.path.as_deref().map(Path::new), &row.fingerprint),
        chars: row.extract.as_deref().map(|t| t.chars().count()).unwrap_or(0),
        created_at: row.created_at,
    }
}

pub fn views(rows: &[db::SourceRow]) -> Vec<SourceView> {
    rows.iter().map(view).collect()
}

/// The fingerprint of a file: its size and the second it was last written.
///
/// Size and mtime rather than a hash, because this is asked on every list and a
/// hash would read every document in the notebook to answer it. The ceiling is
/// real and named: an edit that leaves both the size and the timestamp alone —
/// a same-length overwrite in the same second — reads as unchanged. The upgrade
/// path is a content hash for the one file whose digest is about to be paid for,
/// where reading it is already happening.
pub fn fingerprint_of(path: &Path) -> Result<String, String> {
    let meta = std::fs::metadata(path)
        .map_err(|e| format!("could not read {}: {e}", path.display()))?;
    Ok(fingerprint_of_meta(&meta))
}

fn fingerprint_of_meta(meta: &std::fs::Metadata) -> String {
    let secs = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{}-{}", meta.len(), secs)
}

/// Whether a source is still the one its digest describes.
///
/// `missing` rather than `stale` when there is nothing to compare: a file that
/// has been deleted is a different problem from one that has been edited, and
/// "stale" would invite a re-sync that cannot work.
pub fn state_of(path: Option<&Path>, fingerprint: &str) -> &'static str {
    match path.and_then(|p| std::fs::metadata(p).ok()) {
        Some(meta) if fingerprint_of_meta(&meta) == fingerprint => "ok",
        Some(_) => "stale",
        None => "missing",
    }
}

/// Whether a source's digest has to be made again.
///
/// The rule the owner asked for, in one line: the outline is made the first time
/// a document is added and **not** each time it is looked at. It is the same
/// question as `state_of` — the fingerprint is what the digest describes — which
/// is why the two are stated once and compared here rather than stored twice.
pub fn needs_digest(stored: &str, current: &str) -> bool {
    stored != current
}

/// What a source contains, when no model could be asked.
///
/// Derived from the text itself, so it is free, deterministic and available on a
/// machine with no endpoint at all. It is an outline rather than a summary, and
/// that is the honest description of it: the shape of a document is what a reader
/// scanning a list needs to decide whether to open it.
pub fn outline(text: &str, bytes: u64) -> String {
    // Headings are counted as words by a plain split, which would make `# Title`
    // read as two words and inflate every figure a reader is using to guess how
    // long a document is. The marker is stripped before the count, and the same
    // strip is what makes the heading count and the word count agree about what a
    // heading is.
    let words: usize = text
        .lines()
        .map(|line| line.trim_start().trim_start_matches('#').trim_start())
        .map(|line| line.split_whitespace().count())
        .sum();
    let headings = text
        .lines()
        .filter(|line| line.trim_start().starts_with('#'))
        .count();
    let size = if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else {
        format!("{} KB", (bytes / 1024).max(1))
    };
    if words == 0 {
        return format!("No readable text in this file ({size}).");
    }
    let shape = if headings > 0 {
        format!("{words} words under {headings} heading{}", if headings == 1 { "" } else { "s" })
    } else {
        format!("{words} words, no headings")
    };
    format!("{shape}, {size}.")
}

/// What this source contains, in the model's own words — one cheap call.
///
/// Returns `None` on any failure rather than an error, and that is the point: a
/// document the user added must not fail to arrive because no endpoint answered.
/// The caller falls back to `outline`.
pub async fn digest(
    client: &Client,
    model: &str,
    name: &str,
    text: &str,
) -> Option<(String, Usage)> {
    let body: String = text.chars().take(DIGEST_CHARS).collect();
    if body.trim().is_empty() {
        return None;
    }
    let prompt = format!("File name: {name}\n\n{body}");
    let (raw, usage) = client
        .complete(model, DIGEST_SYSTEM, &prompt, false)
        .await
        .ok()?;
    let digest = raw.trim().to_string();
    (!digest.is_empty()).then_some((digest, usage))
}

/// The catalogue: what the model is told is available, without the contents.
///
/// Only the sources that are switched on, because that is the whole meaning of
/// the switch. A disabled source is not hidden from the user — the rail shows it
/// greyed — it is hidden from the model.
pub fn catalogue(sources: &[db::SourceRow]) -> String {
    let mut out = String::new();
    let mut any = false;
    for source in sources.iter().filter(|s| s.enabled) {
        any = true;
        let digest = source.digest.trim();
        if digest.is_empty() {
            out.push_str(&format!("- `{}` — not described yet.\n", source.name));
        } else {
            out.push_str(&format!("- `{}` — {digest}\n", source.name));
        }
    }
    if !any {
        return "No sources are switched on in this notebook.\n".to_string();
    }
    out
}

/// The names the model may cite, in catalogue order.
pub fn labels(sources: &[db::SourceRow]) -> Vec<String> {
    sources
        .iter()
        .filter(|s| s.enabled)
        .map(|s| retrieval::label(&s.name, None))
        .collect()
}

/// Splits a budget across sources in rounds, so one huge document cannot starve
/// the others and every source that has anything to say gets to say some of it.
///
/// In rounds rather than evenly: a notebook with one book and one note would give
/// the note and the book the same slice of an even split, which wastes the whole
/// budget on a file that needed a line of it.
pub fn budget(items: &[(String, String)], total: usize, per_source: usize) -> Vec<(String, String)> {
    let chars: Vec<Vec<char>> = items.iter().map(|(_, text)| text.chars().collect()).collect();
    let mut cut = vec![0usize; items.len()];
    let mut spent = 0usize;
    let mut round = 0usize;
    loop {
        let mut moved = false;
        for (i, text) in chars.iter().enumerate() {
            if spent >= total {
                break;
            }
            let ceiling = per_source.saturating_mul(round + 1);
            let want = text.len().min(ceiling);
            if want > cut[i] {
                let give = (want - cut[i]).min(total - spent);
                cut[i] += give;
                spent += give;
                moved = true;
            }
        }
        if !moved || spent >= total {
            break;
        }
        round += 1;
    }
    items
        .iter()
        .enumerate()
        .map(|(i, (name, _))| (name.clone(), chars[i][..cut[i]].iter().collect()))
        .collect()
}

/// The system prompt for one question, and the parts it is made of.
///
/// `base` is the whole thing; the catalogue and the sources above it. The
/// notebook's own rules are pushed by the same function a chat's and a project's
/// are, so "which instruction blocks apply" has one answer in this app and not
/// three — a notebook is filtered as a chat is, because it is a reading and
/// writing surface rather than a coding one.
fn system_prompt(notebook: &db::NotebookRow, sources: &[db::SourceRow]) -> String {
    let mut base = format!(
        "# Notebook: {}\n\nYou answer questions about one notebook, from the sources it holds. \
         Nothing else about this conversation exists — this is not a chat with a history.",
        notebook.title
    );
    push_instructions(&mut base, &mut Vec::new(), &notebook.instructions, "chat");

    base.push_str("\n\n# Sources\n\n");
    base.push_str(&catalogue(sources));
    base.push('\n');
    base.push_str(GROUNDED);

    let items: Vec<(String, String)> = sources
        .iter()
        .filter(|s| s.enabled)
        .map(|s| {
            (
                s.name.clone(),
                s.extract.clone().unwrap_or_default(),
            )
        })
        .collect();
    if items.is_empty() {
        return base;
    }
    let sliced = budget(&items, ASK_TOTAL_CHARS, ASK_SOURCE_CHARS);
    base.push_str("\n\n# What the sources say\n");
    for (name, text) in &sliced {
        if text.trim().is_empty() {
            continue;
        }
        base.push_str(&format!("\n\n## [{}]\n\n{text}", name));
    }
    base
}

/// One question, one answer, both written down.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Asked {
    pub question: db::NotebookMessageRow,
    pub answer: db::NotebookMessageRow,
    /// Citations that named a source the notebook does not hold. Recorded rather
    /// than hidden: the answer is kept as the model wrote it, and this is what
    /// says it overstepped.
    pub unresolved: Vec<String>,
}

/// Asks a notebook a question, from its enabled sources, and records both halves.
///
/// Deliberately not a turn: no tools, no approvals and no chat row. A question
/// here is a grounded read of a source set, and giving it the agent loop would
/// make every surface that reads a chat — the sidebar, the spend rows, the
/// scheduler, memory — learn about a second kind of conversation.
pub async fn ask(
    state: &AppState,
    notebook_id: &str,
    question: &str,
) -> Result<Asked, String> {
    let question = question.trim();
    if question.is_empty() {
        return Err("Type a question to ask this notebook.".to_string());
    }
    let (notebook, sources) = {
        let conn = state.db.lock().unwrap();
        let notebook = db::get_notebook(&conn, notebook_id)
            .map_err(|e| e.to_string())?
            .ok_or("That notebook has been deleted.")?;
        let sources = db::list_sources(&conn, notebook_id).map_err(|e| e.to_string())?;
        (notebook, sources)
    };
    let enabled: Vec<db::SourceRow> = sources.into_iter().filter(|s| s.enabled).collect();
    if enabled.is_empty() {
        return Err("This notebook has no sources switched on.".to_string());
    }

    let settings: Settings = state.settings.lock().unwrap().clone();
    let model = if notebook.model.trim().is_empty() {
        settings.model_for("chat")
    } else {
        notebook.model.clone()
    };
    let client = crate::conversation::client(state, &settings)?;
    let system = system_prompt(&notebook, &enabled);
    let (raw, usage) = client
        .complete(&model, &system, question, false)
        .await
        .map_err(|e| e.to_string())?;
    // Priced like every other call nobody typed: the notebook id stands in the
    // chat column, and the kind is what the spend view groups it under.
    crate::conversation::record_overhead(state, &settings, notebook_id, "notebook", &model, &usage);

    let answer = raw.trim().to_string();
    if answer.is_empty() {
        return Err("The answer came back empty — try again.".to_string());
    }
    let available = labels(&enabled);
    let cited = retrieval::cited(&answer);
    let unresolved = retrieval::unresolved(&answer, &available);

    let (question_row, answer_row) = {
        let conn = state.db.lock().unwrap();
        let question_row =
            db::append_notebook_message(&conn, notebook_id, "user", question, &[])
                .map_err(|e| e.to_string())?;
        let answer_row =
            db::append_notebook_message(&conn, notebook_id, "assistant", &answer, &cited)
                .map_err(|e| e.to_string())?;
        db::touch_notebook(&conn, notebook_id).map_err(|e| e.to_string())?;
        (question_row, answer_row)
    };

    Ok(Asked {
        question: question_row,
        answer: answer_row,
        unresolved,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::settings::Instruction;

    fn source(name: &str, digest: &str, enabled: bool) -> db::SourceRow {
        db::SourceRow {
            id: name.to_string(),
            notebook_id: "n".to_string(),
            name: name.to_string(),
            kind: "file".to_string(),
            origin: String::new(),
            path: None,
            enabled,
            extract: Some("body".to_string()),
            digest: digest.to_string(),
            digest_at: None,
            fingerprint: String::new(),
            created_at: 0,
        }
    }

    #[test]
    fn a_digest_is_made_once_and_not_every_time() {
        // Never described: work to do.
        assert!(needs_digest("", "120-77"));
        // Described, and the file has not moved: nothing to pay for again. This
        // is the owner's own rule and the reason the fingerprint is stored.
        assert!(!needs_digest("120-77", "120-77"));
        // Edited: the digest describes something the file no longer is.
        assert!(needs_digest("120-77", "131-88"));
        // And the same comparison is what the rail reads as `stale`.
        let dir = std::env::temp_dir().join(format!("deskseek-notebook-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("notes.md");
        std::fs::write(&file, "# One\n\nsome words\n").unwrap();
        let first = fingerprint_of(&file).unwrap();
        assert_eq!(state_of(Some(&file), &first), "ok");
        assert!(!needs_digest(&first, &first));
        // A write of a different length moves the fingerprint.
        std::fs::write(&file, "# One\n\nsome more words than before\n").unwrap();
        let second = fingerprint_of(&file).unwrap();
        assert_ne!(first, second);
        assert_eq!(state_of(Some(&file), &second), "ok");
        assert_eq!(state_of(Some(&file), &first), "stale");
        assert!(needs_digest(&first, &second));
        // A file that is gone is missing, not stale: re-syncing it cannot work.
        std::fs::remove_file(&file).unwrap();
        assert_eq!(state_of(Some(&file), &second), "missing");
        assert_eq!(state_of(None, &second), "missing");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_free_outline_describes_the_shape_of_a_document() {
        let text = "# Title\n\nwords here\n\n## Part\n\nmore words\n";
        let out = outline(text, 2048);
        assert!(out.starts_with("6 words under 2 headings, 2 KB."), "got {out}");
        // One heading reads as one heading, not "1 headings".
        let one = outline("# Only\n\na word\n", 100);
        assert!(one.contains("under 1 heading,"), "got {one}");
        // A document with no headings says so rather than claiming zero.
        let flat = outline("just prose and nothing else", 0);
        assert!(flat.contains("no headings"), "got {flat}");
        // And a scan with no text in it says that, which is the honest thing to
        // see beside a source that answers nothing.
        let empty = outline("   \n\n", 4_096);
        assert!(empty.starts_with("No readable text"), "got {empty}");
    }

    #[test]
    fn only_the_sources_that_are_switched_on_are_offered() {
        let sources = vec![
            source("q3.pdf", "Quarterly numbers.", true),
            source("secret.md", "Private notes.", false),
            source("notes.md", "", true),
        ];
        let listed = catalogue(&sources);
        assert!(listed.contains("`q3.pdf` — Quarterly numbers."));
        assert!(listed.contains("`notes.md` — not described yet."));
        assert!(!listed.contains("secret.md"), "a switched-off source was offered");
        assert_eq!(labels(&sources), vec!["q3.pdf", "notes.md"]);
        // Nothing switched on is said in words rather than as an empty list.
        let none = catalogue(&[source("secret.md", "Private.", false)]);
        assert!(none.contains("No sources are switched on"));
        assert!(labels(&[source("secret.md", "Private.", false)]).is_empty());
    }

    #[test]
    fn the_budget_is_shared_in_rounds_rather_than_divided_evenly() {
        let items = vec![
            ("small.md".to_string(), "a".repeat(10)),
            ("huge.pdf".to_string(), "b".repeat(1_000)),
        ];
        let sliced = budget(&items, 100, 60);
        let total: usize = sliced.iter().map(|(_, t)| t.chars().count()).sum();
        assert_eq!(total, 100, "the whole budget is spent");
        assert_eq!(sliced[0].1.chars().count(), 10, "a small source gives all it has");
        assert_eq!(sliced[1].1.chars().count(), 90, "the rest goes to the one that needs it");
        // Every source that has anything gets some of it, and nothing is invented.
        assert!(sliced.iter().all(|(_, t)| items.iter().any(|(_, full)| full.starts_with(t))));
    }

    #[test]
    fn the_notebooks_own_rules_reach_its_prompt() {
        let notebook = db::NotebookRow {
            id: "n".to_string(),
            title: "Thesis".to_string(),
            instructions: vec![
                Instruction {
                    id: "1".to_string(),
                    name: "Citations".to_string(),
                    scope: "both".to_string(),
                    body: "Always give the page.".to_string(),
                    enabled: true,
                },
                Instruction {
                    id: "2".to_string(),
                    name: "Switched off".to_string(),
                    scope: "both".to_string(),
                    body: "This must not appear.".to_string(),
                    enabled: false,
                },
                Instruction {
                    id: "3".to_string(),
                    name: "Coding only".to_string(),
                    scope: "coding".to_string(),
                    body: "This must not appear either.".to_string(),
                    enabled: true,
                },
            ],
            model: String::new(),
            created_at: 0,
            updated_at: 0,
        };
        let prompt = system_prompt(&notebook, &[source("q3.pdf", "Numbers.", true)]);
        assert!(prompt.contains("# Notebook: Thesis"), "the notebook is named");
        assert!(prompt.contains("Always give the page."));
        assert!(!prompt.contains("This must not appear"), "a rule that should not apply did");
        // The catalogue, the contract, and the text itself.
        assert!(prompt.contains("`q3.pdf` — Numbers."));
        assert!(prompt.contains("Never cite a source that is not in that list"));
        assert!(prompt.contains("## [q3.pdf]"));
    }
}
