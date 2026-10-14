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
source you were given and something you looked up.";

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
) -> Result<String, String> {
    match name {
        "source_search" => source_search(state, notebook_id, args).await,
        "read_source" => read_source(state, notebook_id, args),
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

/// The notebook's enabled sources, as `(name, path)`.
///
/// The filter is applied where the search happens as well as where the catalogue
/// is built, because a switch that took a source out of the prompt but left it
/// answering tool calls would not be a switch.
fn enabled_sources(state: &AppState, notebook_id: &str) -> Result<Vec<db::SourceRow>, String> {
    let conn = state.db.lock().unwrap();
    let rows = db::list_sources(&conn, notebook_id).map_err(|e| e.to_string())?;
    Ok(rows.into_iter().filter(|s| s.enabled).collect())
}

/// The passages of this notebook's sources that match a query.
async fn source_search(
    state: &AppState,
    notebook_id: &str,
    args: &serde_json::Value,
) -> Result<String, String> {
    let query = crate::conversation::agent::argument(args, "query");
    if query.is_empty() {
        return Err("source_search needs a query".to_string());
    }
    let sources = enabled_sources(state, notebook_id)?;
    if sources.is_empty() {
        return Err("This notebook has no sources switched on.".to_string());
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
) -> Result<String, String> {
    let wanted = crate::conversation::agent::argument(args, "name");
    if wanted.is_empty() {
        return Err("read_source needs a name".to_string());
    }
    let sources = enabled_sources(state, notebook_id)?;
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

/// One thing a notebook can be asked to produce.
///
/// Data rather than code, and deliberately: a generator is a title, the format
/// the writer takes, and the instruction that turns a question into that shape.
/// Everything else a generation needs — the catalogue, the two source tools, the
/// citation rule, the price — is the ask the notebook already does, so a seventh
/// generator is one more entry here and nothing anywhere else.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Generator {
    pub id: &'static str,
    pub title: &'static str,
    /// A format `tools::docs::DOC_FORMATS` accepts. Asserted against that list
    /// rather than trusted, because a menu entry naming a format nothing writes
    /// is invisible until somebody presses it.
    pub format: &'static str,
    /// The question. It is asked through the same path a hand-typed one is, so a
    /// generated output has citations like any other answer.
    pub prompt: &'static str,
}

/// What the menu offers, in the order a reader wants it: the study shapes, then
/// the summary shapes.
pub const GENERATORS: [Generator; 6] = [
    Generator {
        id: "flashcards",
        title: "Flashcards",
        // Word rather than Markdown: a card set is something you print and cut.
        format: "docx",
        prompt: "Turn these sources into flashcards. One card is `Q:` on a line, `A:` on the next, \
                 then a blank line. Cover the facts that matter rather than every sentence, and \
                 after each answer cite the source it came from, as `[name#Heading]`.",
    },
    Generator {
        id: "quiz",
        title: "Quiz",
        format: "md",
        prompt: "Write a quiz over these sources: ten questions, four options each, then an answer \
                 key at the end saying which option is right and why. Every answer in the key cites \
                 the source it came from as `[name#Heading]`, and a question the sources cannot \
                 settle is left out rather than invented.",
    },
    Generator {
        id: "briefing",
        title: "Briefing",
        format: "docx",
        prompt: "Write a one-page briefing on these sources: what they are, the five things that \
                 matter most, and what they do not settle. Cite each claim as `[name#Heading]`, and \
                 say plainly where the sources disagree rather than picking one.",
    },
    Generator {
        id: "timeline",
        title: "Timeline",
        format: "md",
        prompt: "Build a timeline of what these sources describe, earliest first, as a Markdown \
                 table with the columns when, what happened, and the source. Where a date is \
                 relative or uncertain, say so in the cell rather than tidying it into a year. \
                 Cite the source of each row as `[name#Heading]`.",
    },
    Generator {
        id: "study-guide",
        title: "Study guide",
        format: "md",
        prompt: "Write a study guide for these sources: what to know, what to practise, and the \
                 questions a reader should be able to answer afterwards. Cite the source of each \
                 section as `[name#Heading]`.",
    },
    Generator {
        id: "mind-map",
        title: "Mind map",
        format: "md",
        prompt: "Draw the structure of these sources as a mind map: a fenced ```mermaid mindmap \
                 block, then a short note under it on each branch, citing the source each branch \
                 comes from as `[name#Heading]`.",
    },
];

/// One generator by id, or `None` — an unknown id from the window is refused
/// rather than guessed at.
pub fn generator(id: &str) -> Option<&'static Generator> {
    GENERATORS.iter().find(|g| g.id == id)
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
            let result = match run_tool(state, notebook_id, &call.function.name, &args).await {
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
    fn every_generator_names_a_format_the_writer_takes() {
        for generator in GENERATORS {
            // The failure this exists for: a menu entry naming a format nothing
            // writes, which is invisible until somebody presses it.
            assert!(
                crate::tools::docs::DOC_FORMATS.contains(&generator.format),
                "{} names the format {}",
                generator.id,
                generator.format
            );
            assert!(!generator.title.trim().is_empty(), "{} has no title", generator.id);
            assert!(!generator.prompt.trim().is_empty(), "{} asks for nothing", generator.id);
            // Every prompt has to ask for citations, because an output that cites
            // nothing is the one thing the notebook's prompt promises it will not
            // produce.
            assert!(
                generator.prompt.contains("[name#Heading]"),
                "{} does not ask for a citation",
                generator.id
            );
            let duplicates = GENERATORS.iter().filter(|g| g.id == generator.id).count();
            assert_eq!(duplicates, 1, "{} is listed {duplicates} times", generator.id);
        }
        // An id from the window that is not one of these is refused, not guessed.
        assert!(generator("flashcards").is_some());
        assert!(generator("nonsense").is_none());
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
