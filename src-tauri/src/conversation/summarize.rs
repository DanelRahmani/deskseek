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
    summary: String,
    #[serde(default)]
    memory: Vec<String>,
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

/// Returns (summary, proposals_created).
///
/// `propose_memory` is false when the caller only wants the transcript
/// compressed (`/compact`), so compacting does not silently queue memory notes.
pub async fn summarize_chat(
    state: &AppState,
    chat_id: &str,
    propose_memory: bool,
) -> Result<(String, usize), String> {
    let api_key = keychain::get_api_key()?.ok_or("No API key set")?;
    let settings = state.settings.lock().unwrap().clone();

    let rows = {
        let conn = state.db.lock().unwrap();
        db::list_messages(&conn, chat_id).map_err(|e| e.to_string())?
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

    Ok((parsed.summary, created.len()))
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
