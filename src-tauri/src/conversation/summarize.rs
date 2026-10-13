//! End-of-chat summarising and folding. One cheap non-streaming call produces a
//! short summary plus proposed additions to `memory.md`, and the same summary is
//! what a fold replaces the transcript with. Nothing is written to the memory
//! file here — proposals wait for the user to accept them.

use serde::Deserialize;

use crate::api::client::Client;
use crate::memory::SUMMARY_SYSTEM_PROMPT;
use crate::store::{db, keychain};
use crate::AppState;

/// Trim the transcript so a very long chat still summarises cheaply.
const MAX_TRANSCRIPT_CHARS: usize = 48_000;

/// The model the app summarises with — its own cheapest, because neither
/// summarising nor folding needs reasoning. Named here so the price on the row it
/// writes is the price of the model that was actually called.
pub const SUMMARY_MODEL: &str = "deepseek-flash";

/// The fraction of the model's window at which a run folds its own chat.
///
/// Deliberately higher than the meter's warning at 0.8: the warning is free and a
/// fold spends a summariser call, so the automatic step waits until the next
/// request really is close to the ceiling.
pub const AUTO_COMPACT_AT: f64 = 0.9;

/// Whether a run should fold its chat before the next request.
///
/// The measure is the provider's own `prompt_tokens` for the request just sent,
/// not a character estimate, so this cannot drift from what the endpoint counted.
///
/// `stored` is how many messages the chat holds now. A transcript that has
/// already been folded is one message, and folding it again would replace the
/// fold with itself — spending a call every turn and never letting the
/// conversation grow back. So a lone message is never folded, and neither is a
/// chat whose window is unknown: neither is a reason to spend.
pub fn should_compact(prompt_tokens: u32, context: u32, stored: usize) -> bool {
    stored > 1 && context > 0 && prompt_tokens as f64 >= context as f64 * AUTO_COMPACT_AT
}

/// What a folded transcript is replaced with.
///
/// `folded` is how many messages went into the summary, said out loud so the
/// transcript records that history was folded rather than lost — the same reason
/// the manual `/compact` has never been silent about it.
pub fn compacted_note(summary: &str, folded: usize) -> String {
    format!(
        "Context compacted — {folded} earlier messages were folded into the summary below.\n\n\
         Summary of the conversation so far:\n\n{summary}\n\nContinue from here."
    )
}

/// Replaces a chat's transcript with one note carrying its summary.
///
/// The manual `/compact` and the automatic fold both come through here, so there
/// is one shape of folded transcript in the database rather than two that can
/// drift apart.
pub fn fold(conn: &rusqlite::Connection, chat_id: &str, summary: &str) -> rusqlite::Result<()> {
    let folded = db::delete_messages(conn, chat_id)?;
    db::insert_message(
        conn,
        chat_id,
        "user",
        Some(&compacted_note(summary, folded)),
        None,
        None,
        None,
        None,
    )?;
    Ok(())
}

#[derive(Debug, Deserialize, Default)]
pub struct Summary {
    #[serde(default)]
    topic: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    memory: Vec<ProposedMemory>,
}

/// One proposed memory.
///
/// The extractor is asked for `{"text", "target"}` objects. A plain string is
/// still accepted — an older or sloppier reply should not lose the fact — and is
/// filed as a fact, never as a profile entry, because only the tagged shape is
/// allowed to reach the always-injected file.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ProposedMemory {
    Tagged {
        text: String,
        #[serde(default)]
        target: String,
    },
    Plain(String),
}

impl ProposedMemory {
    fn into_draft(self) -> db::ProposalDraft {
        let (text, target) = match self {
            ProposedMemory::Tagged { text, target } => (text, target),
            ProposedMemory::Plain(text) => (text, String::new()),
        };
        // Only an explicit `profile` may reach the always-injected file, and only a
        // name the app knows may become its own category. Blank, misspelled or
        // invented targets land in `other`, so neither the injected file nor the
        // category list can grow by accident. `save_memory` applies the same rule.
        let target = if target.trim().eq_ignore_ascii_case(crate::memory_files::PROFILE) {
            crate::memory_files::PROFILE
        } else {
            crate::memory_files::canonical(&target).unwrap_or(crate::memory_files::FALLBACK)
        };
        db::ProposalDraft {
            text,
            target: target.to_string(),
        }
    }
}

/// Turns a topic into a safe, lowercase file-name fragment.
fn slugify(text: &str) -> String {
    let mut out = String::new();
    let mut pending_dash = false;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(ch.to_ascii_lowercase());
        } else {
            pending_dash = true;
        }
    }
    let trimmed: String = out.chars().take(48).collect();
    let trimmed = trimmed.trim_end_matches('-');
    if trimmed.is_empty() {
        "chat".to_string()
    } else {
        trimmed.to_string()
    }
}

/// `YYYYMMDD-topic.md`, suffixed with `-2`, `-3`… if that name is taken.
fn chatlog_path(dir: &std::path::Path, date: &str, topic: &str) -> std::path::PathBuf {
    let base = format!("{date}-{}", slugify(topic));
    let mut candidate = dir.join(format!("{base}.md"));
    let mut counter = 2;
    while candidate.exists() && counter < 100 {
        candidate = dir.join(format!("{base}-{counter}.md"));
        counter += 1;
    }
    candidate
}

fn write_chatlog(
    dir: &std::path::Path,
    date: &str,
    topic: &str,
    chat_title: &str,
    summary: &str,
    memory: &[String],
) -> std::io::Result<String> {
    std::fs::create_dir_all(dir)?;
    let path = chatlog_path(dir, date, topic);
    let mut body = format!("# {topic}\n\n_{date} — {chat_title}_\n\n{summary}\n");
    if !memory.is_empty() {
        body.push_str("\n## Remembered\n\n");
        for entry in memory {
            body.push_str(&format!("- {entry}\n"));
        }
    }
    std::fs::write(&path, body)?;
    Ok(path.display().to_string())
}

/// The model sometimes wraps JSON in a fenced block despite instructions.
fn strip_fences(raw: &str) -> &str {
    let trimmed = raw.trim();
    let without_open = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed);
    without_open.strip_suffix("```").unwrap_or(without_open).trim()
}

pub fn parse_summary(raw: &str) -> Result<Summary, String> {
    let body = strip_fences(raw);
    serde_json::from_str::<Summary>(body).map_err(|e| format!("the summariser returned invalid JSON: {e}"))
}

fn transcript(rows: &[db::MessageRow]) -> String {
    let mut out = String::new();
    for row in rows {
        let content = row.content.as_deref().unwrap_or("").trim();
        if content.is_empty() || row.role == "system" {
            continue;
        }
        let label = match row.role.as_str() {
            "user" => "User",
            "assistant" => "Assistant",
            "tool" => "Tool output",
            other => other,
        };
        out.push_str(&format!("{label}: {content}\n\n"));
    }
    if out.chars().count() > MAX_TRANSCRIPT_CHARS {
        // Keep the end: the most recent exchange is what matters most.
        let skip = out.chars().count() - MAX_TRANSCRIPT_CHARS;
        out = out.chars().skip(skip).collect();
    }
    out
}

/// Returns (summary, proposals_created, chatlog_path).
///
/// `propose_memory` is false when the caller only wants the transcript
/// compressed (`/compact`), so compacting does not silently queue memory notes.
pub async fn summarize_chat(
    state: &AppState,
    chat_id: &str,
    propose_memory: bool,
) -> Result<(String, usize, Option<String>), String> {
    let api_key = keychain::get_api_key()?.ok_or("No API key set")?;
    let settings = state.settings.lock().unwrap().clone();

    let (rows, chat_title) = {
        let conn = state.db.lock().unwrap();
        let rows = db::list_messages(&conn, chat_id).map_err(|e| e.to_string())?;
        let title = db::get_chat(&conn, chat_id)
            .map(|c| c.title)
            .unwrap_or_else(|_| "Untitled chat".to_string());
        (rows, title)
    };
    if rows.iter().all(|r| r.role != "user") {
        return Err("nothing to summarise yet".to_string());
    }

    let client = Client::new(state.http.clone(), api_key, settings.base_url.clone());
    let (raw, usage) = client
        .complete(
            SUMMARY_MODEL,
            SUMMARY_SYSTEM_PROMPT,
            &transcript(&rows),
            true,
        )
        .await
        .map_err(|e| e.to_string())?;
    // Paid for by the user, typed by nobody: onto the same bill the turns are on,
    // so "what did this cost" has one answer rather than one per feature.
    super::record_overhead(state, chat_id, db::SUMMARY, SUMMARY_MODEL, &usage);

    let mut parsed = parse_summary(&raw)?;
    let drafts: Vec<db::ProposalDraft> = std::mem::take(&mut parsed.memory)
        .into_iter()
        .map(ProposedMemory::into_draft)
        .collect();

    let created = {
        let conn = state.db.lock().unwrap();
        db::set_summary(&conn, chat_id, &parsed.summary).map_err(|e| e.to_string())?;
        if !propose_memory {
            Vec::new()
        } else {
            // Re-summarising replaces the pending set rather than stacking duplicates.
            let existing =
                db::list_proposals(&conn, chat_id, Some("pending")).map_err(|e| e.to_string())?;
            let ids: Vec<String> = existing.into_iter().map(|p| p.id).collect();
            db::set_proposal_status(&conn, &ids, "superseded").map_err(|e| e.to_string())?;
            db::insert_proposals(&conn, chat_id, &drafts).map_err(|e| e.to_string())?
        }
    };

    // Every summary also becomes a dated note in CHATLOG.
    let topic = if parsed.topic.trim().is_empty() {
        chat_title.clone()
    } else {
        parsed.topic.trim().to_string()
    };
    let date = chrono::Local::now().format("%Y%m%d").to_string();
    let remembered: Vec<String> = drafts.iter().map(|draft| draft.text.clone()).collect();
    let chatlog = write_chatlog(
        &state.paths.chatlog_dir(),
        &date,
        &topic,
        &chat_title,
        &parsed.summary,
        &remembered,
    )
    .ok()
    .map(|path| format!("{topic}\u{1f}{path}"));

    Ok((parsed.summary, created.len(), chatlog))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_plain_json_payload() {
        let s = parse_summary(r#"{"summary":"about rust","memory":["Likes Rust"]}"#).unwrap();
        assert_eq!(s.summary, "about rust");
        // An untagged plain string is still accepted, as a fact.
        assert_eq!(s.memory.len(), 1);
        let draft = s.memory.into_iter().next().unwrap().into_draft();
        assert_eq!(draft.text, "Likes Rust");
        assert_eq!(draft.target, "other");
    }

    #[test]
    fn a_tagged_entry_keeps_its_target() {
        let s = parse_summary(
            r#"{"summary":"x","memory":[{"text":"Replies in Dutch","target":"profile"},{"text":"Buying a house","target":"fact"}]}"#,
        )
        .unwrap();
        let drafts: Vec<db::ProposalDraft> = s
            .memory
            .into_iter()
            .map(ProposedMemory::into_draft)
            .collect();
        assert_eq!(drafts[0].target, "profile");
        assert_eq!(drafts[0].text, "Replies in Dutch");
        assert_eq!(drafts[1].target, "other");
    }

    #[test]
    fn an_invented_category_is_filed_as_other_and_a_real_one_is_honoured() {
        let invented = parse_summary(
            r#"{"summary":"x","memory":[{"text":"Something","target":"astrophysics"}]}"#,
        )
        .unwrap();
        let draft = invented.memory.into_iter().next().unwrap().into_draft();
        assert_eq!(draft.target, "other");

        let real = parse_summary(
            r#"{"summary":"x","memory":[{"text":"Studies IB","target":"Education"}]}"#,
        )
        .unwrap();
        let draft = real.memory.into_iter().next().unwrap().into_draft();
        assert_eq!(draft.target, "education");
    }

    #[test]
    fn an_entry_without_a_target_is_filed_as_other() {
        let s = parse_summary(r#"{"summary":"x","memory":[{"text":"Something"}]}"#).unwrap();
        let draft = s.memory.into_iter().next().unwrap().into_draft();
        assert_eq!(draft.target, "other");
    }

    #[test]
    fn only_an_explicit_profile_target_reaches_the_injected_file() {
        for reply in [
            r#"{"summary":"x","memory":[{"text":"a","target":"Profile"}]}"#,
            r#"{"summary":"x","memory":[{"text":"a","target":" profile "}]}"#,
        ] {
            let draft = parse_summary(reply)
                .unwrap()
                .memory
                .into_iter()
                .next()
                .unwrap()
                .into_draft();
            assert_eq!(draft.target, "profile", "{reply}");
        }

        // A misspelled or invented target must not become a profile entry.
        let draft = parse_summary(r#"{"summary":"x","memory":[{"text":"a","target":"user"}]}"#)
            .unwrap()
            .memory
            .into_iter()
            .next()
            .unwrap()
            .into_draft();
        assert_eq!(draft.target, "other");
    }

    #[test]
    fn parses_a_fenced_payload() {
        let s = parse_summary("```json\n{\"summary\":\"x\",\"memory\":[]}\n```").unwrap();
        assert_eq!(s.summary, "x");
        assert!(s.memory.is_empty());
    }

    #[test]
    fn a_missing_memory_key_is_not_an_error() {
        let s = parse_summary(r#"{"summary":"only a summary"}"#).unwrap();
        assert!(s.memory.is_empty());
    }

    #[test]
    fn invalid_json_is_reported() {
        assert!(parse_summary("not json at all").is_err());
    }

    #[test]
    fn a_topic_is_parsed_when_present() {
        let s = parse_summary(r#"{"topic":"Rust Ownership","summary":"x","memory":[]}"#).unwrap();
        assert_eq!(s.topic, "Rust Ownership");
    }

    #[test]
    fn slugs_are_lowercase_and_dash_separated() {
        assert_eq!(slugify("Rust Ownership Rules"), "rust-ownership-rules");
        assert_eq!(slugify("  Hello,  world!  "), "hello-world");
        assert_eq!(slugify("C++ vs. C#"), "c-vs-c");
    }

    #[test]
    fn a_topic_with_nothing_usable_falls_back_to_chat() {
        assert_eq!(slugify("!!!"), "chat");
        assert_eq!(slugify(""), "chat");
    }

    #[test]
    fn chatlog_name_is_date_then_topic() {
        let dir = std::env::temp_dir().join(format!("deskseek-log-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = chatlog_path(&dir, "20261011", "Rust Ownership");
        assert_eq!(
            path.file_name().unwrap().to_string_lossy(),
            "20261011-rust-ownership.md"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn chatlog_name_does_not_overwrite_an_existing_one() {
        let dir = std::env::temp_dir().join(format!("deskseek-log-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = write_chatlog(&dir, "20261011", "Rust", "t", "s", &[]).unwrap();
        let second = write_chatlog(&dir, "20261011", "Rust", "t", "s", &[]).unwrap();
        assert_ne!(first, second);
        assert!(second.ends_with("20261011-rust-2.md"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn chatlog_includes_the_remembered_entries() {
        let dir = std::env::temp_dir().join(format!("deskseek-log-{}", uuid::Uuid::new_v4()));
        let path = write_chatlog(
            &dir,
            "20261011",
            "Rust Ownership",
            "Borrow checker",
            "Talked about lifetimes.",
            &["Prefers British English".to_string()],
        )
        .unwrap();
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.contains("# Rust Ownership"));
        assert!(body.contains("Talked about lifetimes."));
        assert!(body.contains("Prefers British English"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn transcript_keeps_the_tail_when_too_long() {
        let rows: Vec<db::MessageRow> = (0..4)
            .map(|i| db::MessageRow {
                id: i.to_string(),
                chat_id: "c".into(),
                seq: i,
                role: "user".into(),
                content: Some("y".repeat(20_000)),
                reasoning_content: None,
                tool_calls_json: None,
                tool_call_id: None,
                attachments_json: None,
                created_at: 0,
                discarded: false,
            })
            .collect();
        let text = transcript(&rows);
        assert!(text.chars().count() <= MAX_TRANSCRIPT_CHARS);
    }

    #[test]
    fn a_chat_over_the_window_is_folded_once_and_not_again() {
        // 80% of the window is the meter's warning, not the fold's mark: a chat
        // that still has room is left alone. Nothing here asserts the exact
        // boundary — 0.9 * 1_000_000 is a floating-point question, not a fact
        // about the rule, and a test that pins it would be pinning arithmetic.
        assert!(!should_compact(800_000, 1_000_000, 40));
        assert!(should_compact(950_000, 1_000_000, 40));
        // Already folded, the transcript is the fold itself — folding it again
        // would spend a call a turn and never let the conversation grow back.
        assert!(!should_compact(950_000, 1_000_000, 1));
        // A window we do not know is not a reason to spend.
        assert!(!should_compact(999_999, 0, 40));
    }

    #[test]
    fn a_folded_note_says_how_much_it_folded() {
        let note = compacted_note("talked about lifetimes", 12);
        assert!(note.contains("12 earlier messages"), "{note}");
        assert!(note.contains("talked about lifetimes"), "{note}");
        assert!(note.contains("Continue from here."), "{note}");
    }
}
