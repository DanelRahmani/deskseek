//! End-of-chat summarising. One cheap non-streaming call produces a short
//! summary plus proposed additions to `memory.md`. Nothing is written to the
//! memory file here — proposals wait for the user to accept them.

use serde::Deserialize;

use crate::api::client::Client;
use crate::memory::SUMMARY_SYSTEM_PROMPT;
use crate::store::{db, keychain};
use crate::AppState;

/// Trim the transcript so a very long chat still summarises cheaply.
const MAX_TRANSCRIPT_CHARS: usize = 48_000;

#[derive(Debug, Deserialize, Default)]
pub struct Summary {
    #[serde(default)]
    topic: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    memory: Vec<String>,
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
    let raw = client
        .complete(
            "deepseek-flash",
            SUMMARY_SYSTEM_PROMPT,
            &transcript(&rows),
            true,
        )
        .await
        .map_err(|e| e.to_string())?;

    let parsed = parse_summary(&raw)?;

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
            db::insert_proposals(&conn, chat_id, &parsed.memory).map_err(|e| e.to_string())?
        }
    };

    // Every summary also becomes a dated note in CHATLOG.
    let topic = if parsed.topic.trim().is_empty() {
        chat_title.clone()
    } else {
        parsed.topic.trim().to_string()
    };
    let date = chrono::Local::now().format("%Y%m%d").to_string();
    let chatlog = write_chatlog(
        &state.paths.chatlog_dir(),
        &date,
        &topic,
        &chat_title,
        &parsed.summary,
        &parsed.memory,
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
        assert_eq!(s.memory, vec!["Likes Rust".to_string()]);
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
            })
            .collect();
        let text = transcript(&rows);
        assert!(text.chars().count() <= MAX_TRANSCRIPT_CHARS);
    }
}
