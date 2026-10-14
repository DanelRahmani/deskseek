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

use serde::{Deserialize, Serialize};

use crate::api::client::Client;
use crate::api::types::{ChatMessage, ChatRequest, Content, StreamOptions, Usage};
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
The sources are listed above with what each one contains, and the text of the ones that matter is \
given below.\n\n\
Answer from them. Cite the source you used by writing its name in square brackets exactly as it \
is written above — `[name]` for the whole of it, or `[name#Heading]` for one part. Never cite a \
source that is not in that list: a citation the reader cannot look up is worse than no citation \
at all. If the sources do not answer the question, say so plainly rather than filling the gap \
from memory, and never present a guess as something a source said.\n\n\
When the list above is not enough, `source_search` finds the passages that bear on a question and \
`read_source` reads one whole source. Those two, and reading the public web through \
`fetch_url`/`web_search`, are the only ways you have of finding anything out — and when an answer \
comes from the web rather than from a source, say so, because that is the difference between a \
source you were given and something you looked up.\n\n\
When the reader asks for something to *study from* — flashcards, a quiz — call `make_study_set` \
with the items themselves rather than describing them in your answer. What it makes is then handed \
to them to take, one item at a time, so writing the questions out as well is the same work twice.";

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
         Earlier questions and their answers are part of this conversation, and a new question is \
         answered the same way — from the sources.",
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

/// How many tool rounds one question may take.
///
/// Four: enough for a search, a read, a second search and an answer, and few
/// enough that a model which keeps looking cannot spend the user's money in a
/// loop that never ends.
const MAX_ROUNDS: usize = 4;

/// How many earlier lines of a notebook's conversation travel with a new
/// question, so that a follow-up is a follow-up rather than a fresh start.
const HISTORY_MESSAGES: usize = 8;

/// One tool call, run for a notebook.
///
async fn run_tool(
    state: &AppState,
    notebook_id: &str,
    name: &str,
    args: &serde_json::Value,
    scope: &[String],
) -> Result<String, String> {
    match name {
        "source_search" => source_search(state, notebook_id, args, scope).await,
        "read_source" => read_source(state, notebook_id, args, scope),
        // The one call a notebook may make that leaves anything behind — and it
        // leaves it in the notebook's own store rather than on the disk, because a
        // study set is a thing the window draws rather than a file. That is exactly
        // what `ToolClass::Internal` means, and it is why this loop still needs no
        // approval path: nothing here can touch the machine.
        "make_study_set" => make_study_set(state, notebook_id, args),
        // The internet tools read the public web rather than a folder, and the
        // app already owns how they are run — settings, the browser, the search
        // provider — so this defers to that rather than a second copy.
        "fetch_url" | "browser_read" | "web_search" => {
            crate::conversation::agent::run_browse_tool(state, name, args).await
        }
        other => Err(format!(
            "'{other}' is not available in a notebook. Its sources are the only files it can \
             read."
        )),
    }
}

/// Makes a study set from a `make_study_set` call, and stores it.
///
/// Everything that makes the set *usable* is checked before a row exists: a set
/// stored with a question whose right option cannot be recognised would be a
/// question nobody could get right, and the reader would have no way to tell it
/// from their own mistake. A refusal costs the model one more call inside the same
/// question, which is why the messages in `study_set` say what was wrong.
fn make_study_set(
    state: &AppState,
    notebook_id: &str,
    args: &serde_json::Value,
) -> Result<String, String> {
    let set = study_set(args)?;
    let stored = {
        let conn = state.db.lock().unwrap();
        let row = db::add_notebook_set(&conn, notebook_id, &set.kind, &set.title, &set.items)
            .map_err(|e| e.to_string())?;
        db::touch_notebook(&conn, notebook_id).map_err(|e| e.to_string())?;
        row
    };
    Ok(format!(
        "Made \"{}\" — {} {}. It is in this notebook's Study list now, where the reader takes it \
         one item at a time, so do not write the items out in your answer as well.",
        stored.title,
        stored.items.len(),
        if set.kind == "quiz" {
            "quiz questions"
        } else {
            "cards"
        }
    ))
}

/// The sources a question may read: the ones switched on, and — when the reader
/// named some — only those.
///
/// One function, because "switched off" and "not asked for this time" are the same
/// rule applied twice, and it has to hold where the search happens as well as where
/// the catalogue is built: a source ticked out of the form but still answering a
/// tool call would not be out of scope, it would be hidden. Shared with the fact
/// table, which has to hold every row's citation against the same set the question
/// was given.
pub fn scoped_sources(
    state: &AppState,
    notebook_id: &str,
    scope: &[String],
) -> Result<Vec<db::SourceRow>, String> {
    let conn = state.db.lock().unwrap();
    let rows = db::list_sources(&conn, notebook_id).map_err(|e| e.to_string())?;
    Ok(in_scope(rows, scope))
}

/// The rule itself, without a database in the way — so it can be tested on its own
/// rather than only through a call that happens to reach one.
pub fn in_scope(rows: Vec<db::SourceRow>, scope: &[String]) -> Vec<db::SourceRow> {
    rows.into_iter()
        .filter(|source| source.enabled)
        .filter(|source| {
            scope.is_empty()
                || scope
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(&source.name))
        })
        .collect()
}

/// What to say when a question has nothing it may read.
///
/// Two different situations wear the same empty list — nothing is switched on, and
/// none of the ones this question asked for is — and saying the first when it is
/// the second sends the reader to the wrong switch.
pub fn no_sources(scope: &[String]) -> String {
    if scope.is_empty() {
        "This notebook has no sources switched on.".to_string()
    } else {
        format!(
            "None of the sources this was asked for is switched on: {}.",
            scope.join(", ")
        )
    }
}

/// The passages of this notebook's sources that match a query.
async fn source_search(
    state: &AppState,
    notebook_id: &str,
    args: &serde_json::Value,
    scope: &[String],
) -> Result<String, String> {
    let query = crate::conversation::agent::argument(args, "query");
    if query.is_empty() {
        return Err("source_search needs a query".to_string());
    }
    let sources = scoped_sources(state, notebook_id, scope)?;
    if sources.is_empty() {
        return Err(no_sources(scope));
    }
    let root = state.paths.notebook_sources_dir(notebook_id);
    let roots = [root];
    let hits = {
        let conn = state.db.lock().unwrap();
        crate::retrieval::index_roots(&conn, &roots, crate::retrieval::MAX_PASS_BYTES)?;
        let found = crate::retrieval::search(&conn, &roots, &query, crate::retrieval::RECALL_CHUNKS)?;
        live_hits(found, &sources)
    };
    if hits.is_empty() {
        return Ok(format!(
            "Nothing in this notebook's sources matches \"{query}\". The sources are: {}.",
            sources
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let mut out = String::new();
    for hit in &hits {
        out.push_str(&format!(
            "\n## [{}]\n\n{}\n",
            crate::retrieval::label_for(&hit.root, &hit.path, &hit.heading),
            hit.text.trim()
        ));
    }
    Ok(out)
}

/// One source in full, by the name the catalogue shows.
fn read_source(
    state: &AppState,
    notebook_id: &str,
    args: &serde_json::Value,
    scope: &[String],
) -> Result<String, String> {
    let wanted = crate::conversation::agent::argument(args, "name");
    if wanted.is_empty() {
        return Err("read_source needs a name".to_string());
    }
    let sources = scoped_sources(state, notebook_id, scope)?;
    let found = sources
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case(&wanted))
        .ok_or_else(|| {
            format!(
                "This notebook has no source called '{wanted}'. Its sources are: {}.",
                sources
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
    let text = found.extract.clone().unwrap_or_default();
    if text.trim().is_empty() {
        return Err(format!("'{}' has no readable text in it.", found.name));
    }
    Ok(format!("## [{}]\n\n{text}", found.name))
}

/// The hits that belong to a source the notebook still has switched on.
///
/// The switch has to hold here as well as in the catalogue, and this is the
/// place it does: a source taken out of the prompt but still answering tool
/// calls would not be switched off, it would be hidden. It takes the notebook's
/// whole source list rather than a pre-filtered one, so a caller that forgets to
/// filter cannot quietly re-enable a source — the filter is here, once.
fn live_hits(hits: Vec<db::ChunkHit>, sources: &[db::SourceRow]) -> Vec<db::ChunkHit> {
    let live: Vec<&str> = sources
        .iter()
        .filter(|s| s.enabled)
        .filter_map(|s| s.path.as_deref())
        .collect();
    hits.into_iter()
        .filter(|hit| live.contains(&hit.path.as_str()))
        .collect()
}

/// One message of a notebook's own conversation, as the wire reads it.
fn plain(role: &str, content: &str) -> ChatMessage {
    ChatMessage {
        role: role.to_string(),
        content: Some(Content::text(content)),
        ..Default::default()
    }
}

/// What pressing a menu entry does.
///
/// The menu is one list and its entries do different work, so *what* an entry
/// makes is data rather than something the window has to work out from an id: a
/// document is written and listed, a workbook is built out of rows, and a set is
/// stored and taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Document,
    Table,
    /// Stored rather than written: a quiz or a card set the window runs. It has no
    /// format because there is no file, which is what the menu reads to stop
    /// promising one.
    Set,
    /// Two documents rather than one — a paper, and the key to it — which is why the
    /// plan carries a second question.
    Exam,
}

/// How many of something an entry can be asked for.
///
/// The noun travels with the numbers so a form can label the field without a
/// second list saying which entry wants which word, and so "40" cannot end up
/// under a heading that says "cards".
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Count {
    pub noun: &'static str,
    pub default: u32,
    /// Zero here means "none is a real answer", which is what makes an optional
    /// second list expressible without a second kind of field.
    pub min: u32,
    /// The ceiling, stated once — so the form can refuse a number the builder
    /// would refuse anyway, rather than the model discovering it.
    pub max: u32,
}

/// One thing a notebook can be asked to produce.
///
/// Data rather than code, and deliberately: an entry is a title, what it makes,
/// what it may be asked for, and the instruction that turns a question into that
/// shape. Everything else a generation needs — the catalogue, the source tools,
/// the citation rule, the price — is the ask the notebook already does, so a new
/// entry is one more entry here and nothing anywhere else.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Generator {
    pub id: &'static str,
    pub title: &'static str,
    pub action: Action,
    /// The formats it will write, the first being the default — **empty** for an
    /// entry that writes no file, which is how the menu stops claiming a format an
    /// entry does not produce. Every one of these is asserted against the writer
    /// that would have to take it, because a format nothing writes is invisible
    /// until somebody presses it.
    pub formats: &'static [&'static str],
    /// The question, before the reader's own numbers and their own line are added.
    /// It is asked through the same path a hand-typed one is, so a generated
    /// output has citations like any other answer.
    pub prompt: &'static str,
    /// What this entry can be asked to count, or `None` when a number means
    /// nothing to it.
    pub count: Option<Count>,
    /// A second list of its own — the exam's open questions are the one that will
    /// fill this — so a form is drawn from what the entry declares rather than
    /// from a table of special cases in the window.
    pub open: Option<Count>,
}

/// What the menu offers, in the order a reader wants it: the study shapes, the
/// summary shapes, the exam, and the table.
pub const GENERATORS: [Generator; 8] = [
    Generator {
        id: "flashcards",
        title: "Flashcards",
        action: Action::Set,
        // No format at all: what this makes is a set the window flips, and the
        // label saying `.docx` was the menu promising something it does not write.
        formats: &[],
        prompt: "Make flashcards the reader can flip, from these sources. Call `make_study_set` \
                 with kind `flashcards`, a short title, and one item per card — the front as \
                 `prompt`, the back as `answer`, and no options. Pass `count` as the number asked \
                 for, name the source of every card that has one, and leave out a fact the sources \
                 cannot settle rather than inventing it. The set is the answer: do not write the \
                 cards out in your reply as well.",
        count: Some(Count {
            noun: "cards",
            default: 20,
            min: 1,
            max: MAX_STUDY_ITEMS as u32,
        }),
        open: None,
    },
    Generator {
        id: "quiz",
        title: "Quiz",
        action: Action::Set,
        formats: &[],
        // No number in the instruction itself — and no answer key, because the
        // window marks the answers: the item that says which option is right *is*
        // the key. Nor any word about the *order* of the options, because the window
        // deals that fresh every time the set is taken (`lib/quiz.ts`), and asking a
        // model for a random order is asking the one thing it cannot do.
        prompt: "Make a quiz the reader can take, from these sources. Call `make_study_set` with \
                 kind `quiz`, a short title, and one item per question — the question, four \
                 options, and which option is right written into `answer` exactly as that option is \
                 spelled. Pass `count` as the number asked for, name the source of every question \
                 that has one, and leave out a question the sources cannot settle rather than \
                 inventing it. The set is the answer: do not write the questions out in your reply \
                 as well.",
        count: Some(Count {
            noun: "questions",
            default: 10,
            min: 1,
            max: MAX_STUDY_ITEMS as u32,
        }),
        open: None,
    },
    Generator {
        id: "briefing",
        title: "Briefing",
        action: Action::Document,
        formats: &["docx"],
        prompt: "Write a one-page briefing on these sources: what they are, the five things that \
                 matter most, and what they do not settle. Cite each claim as `[name#Heading]`, and \
                 say plainly where the sources disagree rather than picking one.",
        count: None,
        open: None,
    },
    Generator {
        id: "timeline",
        title: "Timeline",
        action: Action::Document,
        formats: &["md"],
        prompt: "Build a timeline of what these sources describe, earliest first, as a Markdown \
                 table with the columns when, what happened, and the source. Where a date is \
                 relative or uncertain, say so in the cell rather than tidying it into a year. \
                 Cite the source of each row as `[name#Heading]`.",
        count: None,
        open: None,
    },
    Generator {
        id: "study-guide",
        title: "Study guide",
        action: Action::Document,
        formats: &["md"],
        prompt: "Write a study guide for these sources: what to know, what to practise, and the \
                 questions a reader should be able to answer afterwards. Cite the source of each \
                 section as `[name#Heading]`.",
        count: None,
        open: None,
    },
    Generator {
        id: "mind-map",
        title: "Mind map",
        action: Action::Document,
        formats: &["md"],
        prompt: "Draw the structure of these sources as a mind map: a fenced ```mermaid mindmap \
                 block, then a short note under it on each branch, citing the source each branch \
                 comes from as `[name#Heading]`.",
        count: None,
        open: None,
    },
    Generator {
        id: "exam",
        title: "Exam",
        action: Action::Exam,
        // Word or PDF, chosen in the form: a paper is something you print and hand
        // out, and which of the two you hand out is the reader's call.
        formats: &["docx", "pdf"],
        prompt: "Write an exam paper on these sources, to be printed and sat. Number the questions \
                 and letter the options A to D. The paper carries no answers anywhere — no key, no \
                 asterisk, no hint — because the key is written separately, and no citations \
                 either, because they belong on the key rather than on the paper a reader is \
                 sitting. Cover what the sources actually say, and leave out a question they cannot \
                 settle rather than inventing it.",
        count: Some(Count {
            noun: "multiple-choice questions",
            default: 40,
            min: 1,
            max: 60,
        }),
        open: Some(Count {
            noun: "open questions",
            default: 0,
            min: 0,
            max: 20,
        }),
    },
    Generator {
        id: "fact-table",
        title: "Fact table",
        // In the menu like everything else now that every entry takes the same
        // options — it was a button of its own only while an entry was a fixed
        // prompt and a format, which is exactly what it is not.
        action: Action::Table,
        formats: &["xlsx"],
        prompt: "Build a fact table across these sources. One row per fact that can be compared \
                 between them, under a header row. The last column is the source each row came \
                 from, written as its name exactly as the source list spells it. Where the sources \
                 disagree, add a row saying so rather than picking one; where a number has a unit \
                 or a date, put it in the cell as the source states it.",
        count: None,
        open: None,
    },
];

/// One generator by id, or `None` — an unknown id from the window is refused
/// rather than guessed at.
pub fn generator(id: &str) -> Option<&'static Generator> {
    GENERATORS.iter().find(|g| g.id == id)
}

/// What the reader asked for, as the window sends it.
///
/// Every field optional, because the form draws only the ones an entry declares —
/// and `Default` so a caller that asks for nothing at all is the plain question
/// the entry's own prompt describes.
/// The second question of an entry that makes two documents: the key to the paper
/// it has just written.
///
/// Short, and it does not repeat the questions, because the paper is the *previous
/// turn of the notebook's own conversation* — which is what that conversation is for.
/// It carries no counts either: it asks for the same questions in the same order,
/// which the paper the model just wrote already states.
const EXAM_KEY_PROMPT: &str = "\
Now write the answer key for the paper above, as a document of its own. One line per question, in \
the same order and with the same numbers, saying which option is right and why in a sentence. Cite \
the source of each answer as `[name#Heading]`, and do not repeat the questions themselves.";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Options {
    pub count: Option<u32>,
    pub open: Option<u32>,
    pub format: Option<String>,
    /// The sources it may read, by name. Empty means every source that is switched
    /// on, which is what a question typed into the box gets.
    pub sources: Vec<String>,
    /// One line to steer with, appended as typed and never interpreted.
    pub steer: String,
}

/// A checked request: what to ask, what to write it as, and what may be read.
///
/// The numbers the reader asked for are not fields here: they are in the question,
/// which is the only thing that reads them. An entry that needs them back — a set,
/// whose count is held against what was really made — adds them when it has
/// somewhere to use them, rather than carrying a value nothing looks at.
#[derive(Debug, Clone)]
pub struct Plan {
    pub entry: &'static Generator,
    /// The format to write, or empty for an entry that writes no file.
    pub format: String,
    /// The sources it may read, by name — empty for "everything switched on".
    pub sources: Vec<String>,
    pub question: String,
    /// The second ask, for the entry that makes two documents: the exam's key. `None`
    /// everywhere else, and read by the command rather than ignored.
    pub then: Option<String>,
}

/// One entry's request, checked against the notebook — or the reason it is not.
///
/// The one place a question is built, so a count is phrased once. The source names
/// are checked against the notebook because they arrive from the window *as
/// names*: a name nobody holds would otherwise be a scope that quietly reads
/// everything, which is the opposite of what the reader asked for.
pub fn plan(entry: &'static Generator, options: &Options, held: &[String]) -> Result<Plan, String> {
    let count = number(entry.count, options.count, &entry.title)?;
    let open = number(entry.open, options.open, &entry.title)?;
    let format = if entry.formats.is_empty() {
        String::new()
    } else {
        let wanted = options.format.as_deref().unwrap_or(entry.formats[0]);
        if !entry.formats.contains(&wanted) {
            return Err(format!(
                "{} is written as {} rather than as {wanted}.",
                entry.title,
                entry.formats.join(" or ")
            ));
        }
        wanted.to_string()
    };
    let mut sources = Vec::new();
    for name in &options.sources {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        let found = held
            .iter()
            .find(|held| held.eq_ignore_ascii_case(name))
            .ok_or_else(|| {
                format!(
                    "This notebook has no source called '{name}'. Its sources are: {}.",
                    held.join(", ")
                )
            })?;
        sources.push(found.clone());
    }
    let question = question_for(entry, count, open, &sources, &options.steer);
    let then = follow_up_for(entry, open);
    Ok(Plan {
        entry,
        format,
        sources,
        question,
        then,
    })
}

/// The second ask, for the entry that makes two documents.
fn follow_up_for(entry: &Generator, open: Option<u32>) -> Option<String> {
    if entry.action != Action::Exam {
        return None;
    }
    let mut out = String::from(EXAM_KEY_PROMPT);
    if let (Some(open), Some(slot)) = (open, entry.open) {
        out.push_str(&format!(
            "\n\nThen give a model answer for each of the {open} {}. Each one is worth a mark of its \
             own, and none of them is answered by choosing.",
            slot.noun
        ));
    }
    Some(out)
}

/// A number an entry declares a field for, inside that field's own bounds.
///
/// Zero is `None` rather than a refusal: "none of them" is a real answer for a
/// list that may be empty, and the entry says so by declaring a minimum of zero.
fn number(slot: Option<Count>, asked: Option<u32>, what: &str) -> Result<Option<u32>, String> {
    let Some(slot) = slot else {
        // A number for an entry with no field for it is dropped rather than
        // refused: the form cannot send one, so refusing would only ever be
        // reached by a caller that built its own options.
        return Ok(None);
    };
    let value = asked.unwrap_or(slot.default);
    if value < slot.min || value > slot.max {
        return Err(format!(
            "{what} takes between {} and {} {}.",
            slot.min, slot.max, slot.noun
        ));
    }
    Ok(if value == 0 { None } else { Some(value) })
}

/// The question: the entry's instruction, then what the reader asked for.
///
/// Sentences rather than placeholders, because the entry's prompt is written prose
/// and a template language is a second way to write an instruction badly. Order
/// matters and is deliberate: how many, what else, from which sources, and then
/// the reader's own line last, where the model reads it as the thing being asked
/// for rather than as background.
fn question_for(
    entry: &Generator,
    count: Option<u32>,
    open: Option<u32>,
    sources: &[String],
    steer: &str,
) -> String {
    let mut out = String::from(entry.prompt);
    if let (Some(count), Some(slot)) = (count, entry.count) {
        out.push_str(&format!("\n\nMake exactly {count} {}.", slot.noun));
    }
    if let (Some(open), Some(slot)) = (open, entry.open) {
        out.push_str(&format!(
            "\n\nThen add exactly {open} {}. Each one is answered in full rather than by choosing, \
             and each answer is worth a mark of its own.",
            slot.noun
        ));
    }
    if !sources.is_empty() {
        out.push_str(&format!(
            "\n\nUse only these sources, and leave anything they do not contain out: {}.",
            sources.join(", ")
        ));
    }
    if !steer.trim().is_empty() {
        out.push_str(&format!("\n\n{}", steer.trim()));
    }
    out
}

/// What a study set may be. A closed set, for the same reason `ARTIFACT_KINDS` is
/// one: a kind the window cannot draw is a set nobody can take, and the model
/// would never learn why — so the tool's own schema reads *this* list rather than
/// repeating it.
pub const STUDY_KINDS: [&str; 2] = ["quiz", "flashcards"];

/// How many items one set may hold. Long enough for a quiz over a chapter, and
/// bounded so a model cannot spend the reader's money filling a list nobody will
/// finish.
pub const MAX_STUDY_ITEMS: usize = 40;

/// How many options a quiz question may offer: two is the smallest question worth
/// asking, and six is as many as a reader can hold in mind at once.
const MIN_OPTIONS: usize = 2;
const MAX_OPTIONS: usize = 6;

/// A study set that has been checked, ready for the store to hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StudySet {
    pub kind: String,
    pub title: String,
    pub items: Vec<db::StudyItem>,
}

/// One `make_study_set` call, checked and normalised — or the reason it is not a
/// set the reader could take.
///
/// The rule worth its own sentence is that **options are what make a quiz**.
/// Present, the item is answered by picking one, so the answer has to be one of
/// them and is stored spelled exactly as that option is — because the window marks
/// a choice by comparing the two, and anything looser would be a question whose
/// right option is marked wrong. Absent, the item is a flashcard and the answer is
/// the back of it. A refusal here is the model's to read inside the same question,
/// which is why every message says what was wrong rather than only that something
/// was.
pub fn study_set(args: &serde_json::Value) -> Result<StudySet, String> {
    let kind = crate::conversation::agent::argument(args, "kind");
    if !STUDY_KINDS.contains(&kind.as_str()) {
        return Err(format!(
            "'{kind}' is not a kind of study set — it is one of {}.",
            STUDY_KINDS.join(" or ")
        ));
    }
    let title = crate::conversation::agent::argument(args, "title");
    if title.is_empty() {
        return Err("a study set needs a title — it is the line the reader picks it out by".to_string());
    }
    let raws = args
        .get("items")
        .and_then(|value| value.as_array())
        .ok_or("a study set needs `items`: one object per question or card")?;
    if raws.is_empty() {
        return Err("that set has no items in it".to_string());
    }
    if raws.len() > MAX_STUDY_ITEMS {
        return Err(format!(
            "a set holds at most {MAX_STUDY_ITEMS} items, and that one has {}.",
            raws.len()
        ));
    }
    // The number the reader asked for, when they asked for one — refused rather
    // than counted and shrugged at, because "ten questions" that arrives as eight is
    // the one thing a reader notices without being told.
    if let Some(wanted) = args.get("count").and_then(|value| value.as_u64()) {
        let wanted = wanted as usize;
        if wanted == 0 || wanted > MAX_STUDY_ITEMS {
            return Err(format!(
                "a set holds between 1 and {MAX_STUDY_ITEMS} items, and {wanted} were asked for."
            ));
        }
        if raws.len() != wanted {
            // Worded so that no number is followed by a noun: "1 items" is the
            // kind of line a reader stops trusting the whole message over.
            return Err(format!(
                "that set came back with {} of the {wanted} items that were asked for — send \
                 exactly {wanted}.",
                raws.len()
            ));
        }
    }
    let mut items = Vec::with_capacity(raws.len());
    for raw in raws {
        items.push(study_item(&kind, raw)?);
    }
    Ok(StudySet {
        kind,
        title,
        items,
    })
}

/// One item, held to the kind of set it is in.
fn study_item(kind: &str, raw: &serde_json::Value) -> Result<db::StudyItem, String> {
    let prompt = crate::conversation::agent::argument(raw, "prompt");
    let answer = crate::conversation::agent::argument(raw, "answer");
    if prompt.is_empty() {
        return Err("an item has no question in it".to_string());
    }
    if answer.is_empty() {
        return Err(format!("the item \"{prompt}\" has no answer"));
    }
    let options = strings_of(raw, "options");
    let why = crate::conversation::agent::argument(raw, "why");
    // The label the answer came from, kept as it was written: whether the notebook
    // still holds a source by that name is decided where every other citation is
    // decided — by the reader, against the source list — so nothing is resolved
    // here and nothing is thrown away either.
    let source = crate::conversation::agent::argument(raw, "source");

    if kind != "quiz" {
        if !options.is_empty() {
            return Err(format!(
                "a flashcard has no options, and \"{prompt}\" has {} — a card is answered by \
                 knowing it rather than by picking one",
                options.len()
            ));
        }
        return Ok(db::StudyItem {
            prompt,
            answer,
            options,
            why,
            source,
        });
    }

    if options.len() < MIN_OPTIONS {
        return Err(format!(
            "the quiz question \"{prompt}\" offers {} options, and a question answered by \
             picking one needs at least {MIN_OPTIONS}",
            options.len()
        ));
    }
    if options.len() > MAX_OPTIONS {
        return Err(format!(
            "the quiz question \"{prompt}\" offers {} options, and {MAX_OPTIONS} is as many as a \
             reader can hold at once",
            options.len()
        ));
    }
    for (index, option) in options.iter().enumerate() {
        if options[..index]
            .iter()
            .any(|seen| seen.eq_ignore_ascii_case(option))
        {
            return Err(format!(
                "the quiz question \"{prompt}\" offers \"{option}\" twice, and two options that \
                 read the same cannot be told apart"
            ));
        }
    }
    let answer = options
        .iter()
        .find(|option| option.eq_ignore_ascii_case(&answer))
        .cloned()
        .ok_or_else(|| {
            format!(
                "the answer to \"{prompt}\" is not one of its options — it has to be spelled as \
                 one of them, so that the right choice can be recognised: {}",
                options.join(", ")
            )
        })?;
    Ok(db::StudyItem {
        prompt,
        answer,
        options,
        why,
        source,
    })
}

/// The non-empty strings of an array field, trimmed.
fn strings_of(raw: &serde_json::Value, key: &str) -> Vec<String> {
    raw.get(key)
        .and_then(|value| value.as_array())
        .map(|list| {
            list.iter()
                .map(|value| value.as_str().unwrap_or_default().trim().to_string())
                .filter(|text| !text.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// The first table in an answer, as its rows of cells.
///
/// The reader is the document writer's own block parser, so "what a table is" has
/// one answer in this app rather than two — and a fence is handled by that parser
/// too, which matters because an answer about a Markdown source may contain a
/// table inside a code block that is an example rather than the result.
pub fn table_rows(markdown: &str) -> Option<Vec<Vec<String>>> {
    crate::tools::docs::parse_blocks(markdown)
        .into_iter()
        .find_map(|block| match block {
            crate::tools::docs::Block::Table(rows) if rows.len() > 1 => Some(rows),
            _ => None,
        })
}

/// One cell as a workbook should hold it.
///
/// Excel treats a string beginning with `=` as a formula, and a fact copied out of
/// a document is never a formula to run: a source whose text contains `=1+1`, or
/// something worse, would otherwise arrive in the spreadsheet as an instruction.
/// The apostrophe is the marker Excel itself uses for "this is text", so the cell
/// reads as what the document said rather than as arithmetic.
pub fn cell_text(cell: &str) -> String {
    if cell.trim_start().starts_with('=') {
        return format!("'{cell}");
    }
    cell.to_string()
}

/// The rows whose last cell does not name a source the notebook holds.
///
/// The same rule the answer's own citations go through — a citation naming
/// something that was never there is the bug this feature is built to catch — and
/// it is applied per row because a table's citation column is *the* thing that
/// makes it a fact table rather than a list of claims.
pub fn unlabelled_rows(rows: &[Vec<String>], sources: &[String]) -> Vec<usize> {
    rows.iter()
        .enumerate()
        .filter(|(_, row)| {
            let Some(cell) = row.last() else {
                return true;
            };
            let cell = cell.trim();
            // Empty, or naming something no source goes by — including a cell
            // that names nothing at all, which `cited` reports as no citations
            // and which is exactly as unusable as an invented name.
            cell.is_empty()
                || crate::retrieval::cited(cell).is_empty()
                || !crate::retrieval::unresolved(cell, sources).is_empty()
        })
        .map(|(index, _)| index)
        .collect()
}

/// Asks a notebook a question, from its enabled sources, and records both halves.
///
/// A bounded read-only loop rather than a turn: the catalogue and the passages
/// that match are in the prompt, the notebook's two tools and the internet ones
/// are offered, and there are no writes, no approvals and no chat row. Giving
/// this the agent loop instead would make every surface that reads a chat — the
/// sidebar, the spend rows, the scheduler, memory — learn about a second kind of
/// conversation to gain nothing a notebook needs.
pub async fn ask(
    state: &AppState,
    notebook_id: &str,
    question: &str,
    scope: &[String],
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
    let enabled = in_scope(sources, scope);
    if enabled.is_empty() {
        return Err(no_sources(scope));
    }

    let settings: Settings = state.settings.lock().unwrap().clone();
    let model = if notebook.model.trim().is_empty() {
        settings.model_for("chat")
    } else {
        notebook.model.clone()
    };
    let client = crate::conversation::client(state, &settings)?;
    let system = system_prompt(&notebook, &enabled);
    let tools: Vec<crate::api::types::Tool> = crate::tools::tools_for_notebook()
        .into_iter()
        .map(|(tool, _)| tool)
        .collect();

    // What was already asked and answered, so a follow-up lands on top of it.
    let history: Vec<ChatMessage> = {
        let conn = state.db.lock().unwrap();
        let rows = db::list_notebook_messages(&conn, notebook_id).map_err(|e| e.to_string())?;
        let start = rows.len().saturating_sub(HISTORY_MESSAGES);
        rows[start..]
            .iter()
            .map(|row| plain(&row.role, &row.content))
            .collect()
    };

    let cancel = crate::runs::Cancel::new();
    let mut usage = Usage::default();
    let mut turns: Vec<ChatMessage> = Vec::new();
    let mut answer = String::new();
    for _ in 0..MAX_ROUNDS {
        let mut messages = vec![plain("system", &system)];
        messages.extend(history.iter().cloned());
        messages.extend(turns.iter().cloned());
        messages.push(plain("user", question));
        let request = ChatRequest {
            model: model.clone(),
            messages,
            stream: true,
            stream_options: Some(StreamOptions {
                include_usage: true,
            }),
            // A grounded read of a folder is not the place to pay for reasoning,
            // and the notebook has no effort control to consult.
            thinking: None,
            reasoning_effort: None,
            max_tokens: None,
            tools: Some(tools.clone()),
        };
        let outcome = client
            .stream_chat(&request, &cancel, |_| {})
            .await
            .map_err(|failure| failure.message)?;
        crate::api::provider::merge_usage(&mut usage, &outcome.usage);

        let calls = outcome.message.tool_calls.clone().unwrap_or_default();
        if calls.is_empty() {
            answer = outcome
                .message
                .content
                .as_ref()
                .and_then(Content::as_text)
                .unwrap_or_default()
                .trim()
                .to_string();
            break;
        }
        // The assistant's call, then one result per call, is the shape every
        // dialect reads back — the same pairing the agent loop sends.
        turns.push(outcome.message);
        for call in calls {
            let args: serde_json::Value =
                serde_json::from_str(&call.function.arguments).unwrap_or(serde_json::Value::Null);
            let result = match run_tool(state, notebook_id, &call.function.name, &args, scope).await {
                Ok(text) => text,
                // A refused or failed call is the model's to read and recover
                // from, not an error that loses the question.
                Err(reason) => format!("That call failed: {reason}"),
            };
            turns.push(ChatMessage {
                role: "tool".to_string(),
                content: Some(Content::text(result)),
                tool_call_id: Some(call.id.clone()),
                ..Default::default()
            });
        }
    }
    // Priced like every other call nobody typed: the notebook id stands in the
    // chat column, and the kind is what the spend view groups it under.
    crate::conversation::record_overhead(state, &settings, notebook_id, "notebook", &model, &usage);

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
    fn a_fact_table_has_to_say_which_source_each_row_came_from() {
        let answer = "\
Here it is:\n\n\
| Fact | Value | Source |\n\
| --- | --- | --- |\n\
| Revenue | 12.4m | [q3.pdf#Revenue] |\n\
| Headcount | 40 | [notes.md] |\n\n\
That is everything the sources settle.";
        let rows = table_rows(answer).unwrap();
        assert_eq!(rows.len(), 3, "the header and two facts: {rows:?}");
        assert_eq!(rows[0][0], "Fact");
        // The header row is not a fact, so it is not asked to cite anything — the
        // check runs over the rows below it.
        let sources = vec!["q3.pdf".to_string(), "notes.md".to_string()];
        assert!(unlabelled_rows(&rows[1..], &sources).is_empty());

        // A row naming a source the notebook does not hold, and a row naming
        // nothing at all, are both caught — with the row number a reader counts.
        let bad = vec![
            vec!["Revenue".to_string(), "12.4m".to_string(), "[q3.pdf]".to_string()],
            vec!["Headcount".to_string(), "40".to_string(), "[somewhere-else.docx]".to_string()],
            vec!["Costs".to_string(), "3m".to_string(), "".to_string()],
        ];
        assert_eq!(unlabelled_rows(&bad, &sources), vec![1, 2]);

        // An answer with no table at all is not a fact table, and one whose only
        // table is inside a fence is an example rather than the result.
        assert!(table_rows("no table here, just prose").is_none());
        assert!(table_rows("| A | B |\n| --- | --- |\n").is_none(), "a header alone is no table");
        let fenced = format!("```\n{}\n```\n\nNothing outside it.", answer);
        assert!(
            table_rows(&fenced).is_none(),
            "a table inside a fence is an example, not the answer"
        );
    }

    #[test]
    fn a_cell_that_begins_with_an_equals_sign_is_text_not_a_formula() {
        // The one thing a fact out of a document must never become: something the
        // spreadsheet runs when it is opened.
        assert_eq!(cell_text("=1+1"), "'=1+1");
        assert_eq!(cell_text("  =SUM(A1:A9)"), "'  =SUM(A1:A9)");
        // Everything else is left exactly as the source wrote it, including a
        // number, a date and a cell that merely mentions an equals sign.
        assert_eq!(cell_text("12.4m"), "12.4m");
        assert_eq!(cell_text("2026-03-01"), "2026-03-01");
        assert_eq!(cell_text("A = B"), "A = B");
        assert_eq!(cell_text(""), "");
    }

    #[test]
    fn every_menu_entry_writes_something_the_writer_takes() {
        for entry in GENERATORS {
            match entry.action {
                Action::Document => {
                    assert!(!entry.formats.is_empty(), "{} writes nothing at all", entry.id);
                    // The failure this exists for: a menu entry naming a format
                    // nothing writes, which is invisible until somebody presses it.
                    for format in entry.formats {
                        assert!(
                            crate::tools::docs::DOC_FORMATS.contains(format),
                            "{} names the format {format}",
                            entry.id
                        );
                    }
                    // A document that cites nothing is the one thing the notebook's
                    // prompt promises it will not produce.
                    assert!(
                        entry.prompt.contains("[name#Heading]"),
                        "{} does not ask for a citation",
                        entry.id
                    );
                }
                // The workbook writer is the one that makes a spreadsheet, and it is
                // not in `DOC_FORMATS` because it is not a document — so its format
                // is named here rather than agreed with a list, and the source column
                // is what takes the place of a citation.
                Action::Table => {
                    assert_eq!(entry.formats, &["xlsx"], "{} writes no workbook", entry.id);
                    assert!(
                        entry.prompt.contains("source list spells it"),
                        "{} does not say how a row names its source",
                        entry.id
                    );
                }
                // A set writes nothing at all, so the label may not promise a file
                // — and it has to name the tool and the count, because those are the
                // two things the model has to get right for the reader to end up
                // with something they can take.
                Action::Set => {
                    assert!(entry.formats.is_empty(), "{} names a format it does not write", entry.id);
                    assert!(
                        entry.prompt.contains("make_study_set"),
                        "{} does not say which tool makes it",
                        entry.id
                    );
                    assert!(
                        entry.prompt.contains("`count`"),
                        "{} does not pass on the number asked for",
                        entry.id
                    );
                    assert!(entry.count.is_some(), "{} has no number to pass", entry.id);
                }
                // An exam is two documents, both written by the same writer, so both
                // of its formats have to be ones that writer takes — and the paper may
                // not carry an answer anywhere, because the key is a document of its
                // own.
                Action::Exam => {
                    assert!(!entry.formats.is_empty(), "{} writes nothing at all", entry.id);
                    for format in entry.formats {
                        assert!(
                            crate::tools::docs::DOC_FORMATS.contains(format),
                            "{} names the format {format}",
                            entry.id
                        );
                    }
                    assert!(
                        entry.prompt.contains("no answers anywhere"),
                        "{} does not keep the answers off the paper",
                        entry.id
                    );
                    assert!(
                        entry.count.is_some() && entry.open.is_some(),
                        "{} declares only one of its two numbers",
                        entry.id
                    );
                }
            }
            assert!(!entry.title.trim().is_empty(), "{} has no title", entry.id);
            assert!(!entry.prompt.trim().is_empty(), "{} asks for nothing", entry.id);
            // A count the form cannot draw a field for, or a default outside its own
            // bounds, is a number nobody could ever ask for — and the default is what
            // an untouched form sends.
            for slot in [entry.count, entry.open].into_iter().flatten() {
                assert!(!slot.noun.trim().is_empty(), "{} counts nothing", entry.id);
                assert!(
                    slot.default >= slot.min && slot.default <= slot.max,
                    "{} defaults to {} {} outside {}..={}",
                    entry.id,
                    slot.default,
                    slot.noun,
                    slot.min,
                    slot.max
                );
            }
            let duplicates = GENERATORS.iter().filter(|other| other.id == entry.id).count();
            assert_eq!(duplicates, 1, "{} is listed {duplicates} times", entry.id);
        }
        // An id from the window that is not one of these is refused, not guessed.
        assert!(generator("flashcards").is_some());
        assert!(generator("fact-table").is_some(), "the table is an entry too");
        assert!(generator("nonsense").is_none());
    }

    #[test]
    fn what_the_reader_asked_for_is_what_gets_asked() {
        let held = vec!["Macro I.docx".to_string(), "Macro II.docx".to_string()];
        let quiz = generator("quiz").unwrap();

        // Nothing touched: the question is the entry's own prompt with its own
        // default count, phrased exactly once, and no claim about a scope nobody
        // set.
        let plain = plan(quiz, &Options::default(), &held).unwrap();
        // Quiz is a *set* now, so there is no format to name: that empty string is
        // what stops the menu promising a file the entry does not write.
        assert_eq!(plain.format, "");
        assert_eq!(plain.question.matches("Make exactly").count(), 1, "{}", plain.question);
        assert!(plain.question.contains("Make exactly 10 questions"), "{}", plain.question);
        assert!(
            !plain.question.contains("Use only these sources"),
            "an unscoped question must not claim a scope: {}",
            plain.question
        );

        // The three things that were impossible before: how many, from which
        // documents, and a line of the reader's own.
        let asked = plan(
            quiz,
            &Options {
                count: Some(25),
                sources: vec!["Macro II.docx".to_string()],
                steer: "Only the national-accounts material.".to_string(),
                ..Default::default()
            },
            &held,
        )
        .unwrap();
        assert_eq!(asked.sources, vec!["Macro II.docx".to_string()]);
        assert!(asked.question.contains("Make exactly 25 questions"), "{}", asked.question);
        assert!(
            asked.question.contains(
                "Use only these sources, and leave anything they do not contain out: Macro II.docx."
            ),
            "{}",
            asked.question
        );
        assert!(
            asked.question.ends_with("Only the national-accounts material."),
            "the reader's own line goes last, where it reads as the ask: {}",
            asked.question
        );
        // And the count is not the default: a prompt that also said "ten questions"
        // would be asked for twenty-five and mean ten.
        assert!(!asked.question.contains("ten questions"), "{}", asked.question);

        // The fact table takes a scope and a steer with no numbers at all, and its
        // format is the one the workbook writer makes.
        let table = plan(
            generator("fact-table").unwrap(),
            &Options {
                sources: vec!["Macro I.docx".to_string()],
                steer: "One row per country.".to_string(),
                ..Default::default()
            },
            &held,
        )
        .unwrap();
        assert_eq!(table.format, "xlsx");
        assert!(!table.question.contains("Make exactly"), "{}", table.question);

        // A number for an entry with no field for it is dropped rather than refused:
        // the form cannot send one, so a refusal would only ever be reachable from a
        // caller that built its own options.
        let briefing = plan(
            generator("briefing").unwrap(),
            &Options {
                count: Some(9),
                ..Default::default()
            },
            &held,
        )
        .unwrap();
        assert!(!briefing.question.contains("Make exactly"), "{}", briefing.question);

        // Bounds, format and names, each refused with something the reader can act
        // on — the last of them because a name nobody holds would otherwise be a
        // scope that quietly reads everything.
        let too_many = plan(
            quiz,
            &Options {
                count: Some(999),
                ..Default::default()
            },
            &held,
        )
        .expect_err("a count past the ceiling is refused");
        assert!(too_many.contains("between 1 and 40 questions"), "{too_many}");
        let none_at_all = plan(
            quiz,
            &Options {
                count: Some(0),
                ..Default::default()
            },
            &held,
        )
        .expect_err("no questions at all is not a quiz");
        assert!(none_at_all.contains("between 1 and 40 questions"), "{none_at_all}");

        // The format rules belong to the entries that write a file: a document that
        // is asked for a format it does not write is refused, and its own format is
        // what comes back when it is asked for nothing.
        let timeline = generator("timeline").unwrap();
        assert_eq!(plan(timeline, &Options::default(), &held).unwrap().format, "md");
        let wrong_format = plan(
            timeline,
            &Options {
                format: Some("docx".to_string()),
                ..Default::default()
            },
            &held,
        )
        .expect_err("a format the entry does not write is refused");
        assert!(wrong_format.contains("written as md"), "{wrong_format}");
        // And a format sent to an entry that writes nothing is dropped for the same
        // reason a count is: the form cannot send one, so refusing would only ever
        // be reached by a caller that built its own options.
        assert_eq!(
            plan(
                quiz,
                &Options {
                    format: Some("docx".to_string()),
                    ..Default::default()
                },
                &held
            )
            .unwrap()
            .format,
            "",
            "a set writes nothing, whatever format it is sent"
        );
        let unknown = plan(
            quiz,
            &Options {
                sources: vec!["Nope.docx".to_string()],
                ..Default::default()
            },
            &held,
        )
        .expect_err("a source the notebook does not hold is refused");
        assert!(unknown.contains("no source called 'Nope.docx'"), "{unknown}");
        assert!(
            unknown.contains("Macro I.docx"),
            "the refusal names what it does hold: {unknown}"
        );
    }

    #[test]
    fn a_source_out_of_scope_is_not_read_and_the_others_still_are() {
        let all = vec![
            source("a.pdf", "Numbers.", true),
            source("b.pdf", "Words.", true),
            source("c.pdf", "Off.", false),
        ];

        // Nothing named: everything switched on, which is what a typed question gets.
        let everything = in_scope(all.clone(), &[]);
        assert_eq!(everything.len(), 2, "the switched-off one is not in it");
        assert!(catalogue(&everything).contains("a.pdf"));
        assert_eq!(labels(&everything).len(), 2);

        // One named: only that one — and the catalogue, which is what the model is
        // *told* it may read, describes that one and not the others.
        let scoped = in_scope(all.clone(), &["b.pdf".to_string()]);
        assert_eq!(scoped.len(), 1);
        assert!(catalogue(&scoped).contains("b.pdf"));
        assert!(
            !catalogue(&scoped).contains("a.pdf"),
            "an out-of-scope source must not be described: {}",
            catalogue(&scoped)
        );
        assert_eq!(labels(&scoped).len(), 1, "and it is not citable either");

        // Case is not the point, the same way it is not anywhere else here.
        assert_eq!(in_scope(all.clone(), &["B.PDF".to_string()]).len(), 1);

        // A switched-off source cannot be brought back by naming it: the two filters
        // are one rule, and the second cannot undo the first.
        assert!(in_scope(all, &["c.pdf".to_string()]).is_empty());

        // And the message for an empty scope says which of the two situations it is,
        // because a reader sent to the wrong switch has been told nothing.
        assert_eq!(no_sources(&[]), "This notebook has no sources switched on.");
        assert!(no_sources(&["c.pdf".to_string()]).contains("switched on: c.pdf"));
    }

    /// The call shape the two tests below are about, so that they are tests of the
    /// rules rather than of JSON plumbing.
    fn quiz_args() -> serde_json::Value {
        serde_json::json!({
            "kind": "quiz",
            "title": "Revenue quiz",
            "items": [{
                "prompt": "How much did revenue rise?",
                "options": ["9%", "12%", "15%"],
                "answer": "9%",
                "why": "The summary states it.",
                "source": "q3.pdf"
            }]
        })
    }

    #[test]
    fn a_quiz_question_is_only_usable_when_its_answer_is_one_of_its_options() {
        // The one that works, first: every refusal below refuses a shape that is
        // otherwise fine, so a passing call is what says the *rule* is being tested
        // rather than the plumbing.
        let ok = study_set(&quiz_args()).expect("a well-formed quiz is a set");
        assert_eq!(ok.kind, "quiz");
        assert_eq!(ok.title, "Revenue quiz");
        assert_eq!(ok.items.len(), 1);
        assert_eq!(ok.items[0].options.len(), 3);
        assert_eq!(ok.items[0].source, "q3.pdf");

        // An answer that is not one of the options is a question nobody could get
        // right, because the window marks a choice against the answer. The refusal
        // names the options, so the model can repair it inside the same question.
        let mut outside = quiz_args();
        outside["items"][0]["answer"] = serde_json::json!("fifteen percent");
        let reason = study_set(&outside).expect_err("an answer outside the options is refused");
        assert!(reason.contains("not one of its options"), "{reason}");
        assert!(reason.contains("9%, 12%, 15%"), "{reason}");

        // And it is stored spelled as the option is — the same rule seen from the
        // other side: a model that got the case wrong would otherwise ship a
        // question whose right option is marked wrong.
        let mut cased = quiz_args();
        cased["items"][0]["options"] = serde_json::json!(["GDP", "9%"]);
        cased["items"][0]["answer"] = serde_json::json!("gdp");
        assert_eq!(study_set(&cased).unwrap().items[0].answer, "GDP");

        // Two options that read the same cannot be told apart.
        let mut twins = quiz_args();
        twins["items"][0]["options"] = serde_json::json!(["9%", "9%"]);
        let reason = study_set(&twins).expect_err("two identical options are refused");
        assert!(reason.contains("twice"), "{reason}");

        // One option is not a question, and seven is a list nobody reads to the end.
        let mut thin = quiz_args();
        thin["items"][0]["options"] = serde_json::json!(["9%"]);
        assert!(study_set(&thin).is_err(), "one option is not a quiz question");
        let mut wide = quiz_args();
        wide["items"][0]["options"] = serde_json::json!(["a", "b", "c", "d", "e", "f", "g"]);
        assert!(study_set(&wide).is_err(), "seven options is not a question");

        // A flashcard carrying options is a quiz that says it is a card: the kind is
        // what decides how the window draws the set, so it decides what an item may
        // hold.
        let mut card = quiz_args();
        card["kind"] = serde_json::json!("flashcards");
        let reason = study_set(&card).expect_err("a flashcard may not carry options");
        assert!(reason.contains("no options"), "{reason}");

        // And the shape of the call itself: an unknown kind, no title, an empty
        // title, no items, no items field, and a list past the cap.
        let refused = |args: serde_json::Value| study_set(&args).is_err();
        assert!(refused(serde_json::json!({
            "kind": "crossword", "title": "x", "items": []
        })));
        assert!(refused(serde_json::json!({"kind": "quiz", "items": []})));
        assert!(refused(serde_json::json!({"kind": "quiz", "title": "  ", "items": []})));
        assert!(refused(serde_json::json!({"kind": "quiz", "title": "x"})));
        assert!(refused(serde_json::json!({"kind": "quiz", "title": "x", "items": []})));
        let too_many: Vec<serde_json::Value> = (0..=MAX_STUDY_ITEMS)
            .map(|i| serde_json::json!({"prompt": format!("question {i}"), "answer": "a"}))
            .collect();
        assert!(
            refused(serde_json::json!({
                "kind": "flashcards", "title": "All of them", "items": too_many
            })),
            "a set past the cap is refused"
        );
    }

    #[test]
    fn an_exam_is_a_paper_and_a_key_and_the_paper_carries_no_answers() {
        let held = vec!["Macro I.docx".to_string()];
        let exam = generator("exam").unwrap();

        // The defaults are the whole request: forty questions, and no open ones.
        assert_eq!(exam.count.unwrap().default, 40);
        assert_eq!(exam.open.unwrap().default, 0);

        // Nothing touched: forty multiple-choice questions, no open ones, and a second
        // ask that refers to the paper rather than repeating it.
        let asked = plan(exam, &Options::default(), &held).unwrap();
        assert_eq!(asked.format, "docx");
        assert!(
            asked.question.contains("Make exactly 40 multiple-choice questions"),
            "{}",
            asked.question
        );
        assert!(
            !asked.question.contains("open question"),
            "no open questions were asked for: {}",
            asked.question
        );
        let key = asked.then.as_deref().expect("an exam has a key to write");
        assert!(key.contains("answer key"), "{key}");
        assert!(key.contains("the paper above"), "{key}");
        assert!(
            !key.contains("open question"),
            "a key for a paper with no open questions must not ask for one: {key}"
        );
        assert!(
            !key.contains("Make exactly"),
            "the key answers the paper's numbers, not numbers of its own: {key}"
        );

        // Open questions bring a sentence to both halves: the paper asks for them, and
        // the key answers them.
        let with_open = plan(
            exam,
            &Options {
                count: Some(12),
                open: Some(5),
                ..Default::default()
            },
            &held,
        )
        .unwrap();
        assert!(
            with_open.question.contains("Make exactly 12 multiple-choice questions"),
            "{}",
            with_open.question
        );
        assert!(
            with_open.question.contains("add exactly 5 open questions"),
            "{}",
            with_open.question
        );
        let key = with_open.then.as_deref().unwrap();
        assert!(key.contains("each of the 5 open questions"), "{key}");

        // No other entry makes a second document, which is what keeps `then` honest
        // rather than a field every entry carries and none of them uses.
        for entry in GENERATORS.iter().filter(|one| one.id != "exam") {
            assert!(
                plan(entry, &Options::default(), &held).unwrap().then.is_none(),
                "{} makes a second document nobody asked for",
                entry.id
            );
        }
    }

    #[test]
    fn a_set_is_the_size_that_was_asked_for_or_it_is_refused() {
        // One item for a count of one, which is the case that must keep working:
        // the rule is a mismatch, not a number.
        let mut args = quiz_args();
        args["count"] = serde_json::json!(1);
        assert_eq!(study_set(&args).unwrap().items.len(), 1);

        // Ten asked for and one sent is exactly what a reader notices without
        // being told, so it is refused with both numbers — and the message counts
        // nothing with a noun after it, because "1 items" is where a reader stops
        // trusting the message.
        args["count"] = serde_json::json!(10);
        let reason = study_set(&args).expect_err("a set that is not the size asked for is refused");
        assert!(reason.contains("1 of the 10 items"), "{reason}");

        // Leaving the number out is not a mismatch: a question typed into the box
        // never asks for one.
        let mut unasked = quiz_args();
        unasked["count"] = serde_json::Value::Null;
        assert!(study_set(&unasked).is_ok(), "no number asked for is not a mismatch");

        // And a number nobody could satisfy is refused before anything is built,
        // so the model is told the range rather than guessing at it.
        let mut zero = quiz_args();
        zero["count"] = serde_json::json!(0);
        let reason = study_set(&zero).expect_err("none of them is not a set");
        assert!(reason.contains("between 1 and 40"), "{reason}");
        let mut many = quiz_args();
        many["count"] = serde_json::json!(400);
        let reason = study_set(&many).expect_err("past the ceiling is refused");
        assert!(reason.contains("between 1 and 40"), "{reason}");
    }

    #[test]
    fn a_flashcard_holds_a_front_and_a_back_and_nothing_more() {
        let args = serde_json::json!({
            "kind": "flashcards",
            "title": "Key numbers",
            "items": [
                {"prompt": "Revenue", "answer": "9% year on year", "source": "q3.pdf"},
                {"prompt": "Margin", "answer": "41%"}
            ]
        });
        let set = study_set(&args).expect("cards with no options are a set");
        assert_eq!(set.kind, "flashcards");
        assert_eq!(set.items.len(), 2);
        assert!(set.items[0].options.is_empty(), "a card has no options");
        assert_eq!(set.items[0].why, "", "a card does not have to explain itself");
        assert_eq!(set.items[1].source, "", "and it does not have to name a source");

        // A card with no back is a prompt with nothing to reveal, and an item with
        // no front is not a question at all. Both refusals name the item, because
        // the model has to know which one to repair.
        let mut headless = args.clone();
        headless["items"][0]["answer"] = serde_json::json!("   ");
        let reason = study_set(&headless).expect_err("a card with no answer is refused");
        assert!(reason.contains("Revenue"), "{reason}");
        let mut frontless = args.clone();
        frontless["items"][1]["prompt"] = serde_json::json!("");
        assert!(study_set(&frontless).is_err(), "an item with no question is refused");
    }

    #[test]
    fn a_source_that_is_switched_off_cannot_be_searched() {
        let mut live = source("q3.pdf", "Numbers.", true);
        live.path = Some("C:/sources/q3.pdf".to_string());
        let mut off = source("secret.md", "Private.", false);
        off.path = Some("C:/sources/secret.md".to_string());
        // A hit with no source row at all — a file that was removed from the
        // notebook while its chunks were still in the index.
        let stray = db::ChunkHit {
            root: "C:/sources".to_string(),
            path: "C:/sources/gone.md".to_string(),
            heading: String::new(),
            text: "text".to_string(),
            score: 1.0,
        };
        let hit = |path: &str| db::ChunkHit {
            root: "C:/sources".to_string(),
            path: path.to_string(),
            heading: String::new(),
            text: "text".to_string(),
            score: 1.0,
        };
        let found = vec![
            hit("C:/sources/q3.pdf"),
            hit("C:/sources/secret.md"),
            stray,
        ];
        let kept = live_hits(found, &[live, off]);
        assert_eq!(kept.len(), 1, "only the switched-on source may answer");
        assert!(kept[0].path.ends_with("q3.pdf"));
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
