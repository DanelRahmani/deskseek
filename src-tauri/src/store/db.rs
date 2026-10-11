//! SQLite persistence for chats and messages.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

pub fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRow {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub workspace: Option<String>,
    pub model: String,
    pub thinking: bool,
    pub effort: String,
    /// `manual` | `auto` | `skip` — see `tools::guard::PermissionMode`.
    pub permission_mode: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageRow {
    pub id: String,
    pub chat_id: String,
    pub seq: i64,
    pub role: String,
    pub content: Option<String>,
    pub reasoning_content: Option<String>,
    pub tool_calls_json: Option<String>,
    pub tool_call_id: Option<String>,
    pub attachments_json: Option<String>,
    pub created_at: i64,
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS chats (
    id           TEXT PRIMARY KEY,
    title        TEXT NOT NULL,
    kind         TEXT NOT NULL DEFAULT 'chat',
    workspace    TEXT,
    model        TEXT NOT NULL,
    thinking     INTEGER NOT NULL DEFAULT 1,
    effort       TEXT NOT NULL DEFAULT 'high',
    permission_mode TEXT NOT NULL DEFAULT 'manual',
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS messages (
    id                TEXT PRIMARY KEY,
    chat_id           TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
    seq               INTEGER NOT NULL,
    role              TEXT NOT NULL,
    content           TEXT,
    reasoning_content TEXT,
    tool_calls_json   TEXT,
    tool_call_id      TEXT,
    attachments_json  TEXT,
    created_at        INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_messages_chat ON messages(chat_id, seq);

CREATE TABLE IF NOT EXISTS chat_summaries (
    chat_id    TEXT PRIMARY KEY,
    summary    TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS memory_proposals (
    id         TEXT PRIMARY KEY,
    chat_id    TEXT NOT NULL,
    text       TEXT NOT NULL,
    status     TEXT NOT NULL DEFAULT 'pending',
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS chat_skills (
    chat_id  TEXT NOT NULL,
    skill_id TEXT NOT NULL,
    PRIMARY KEY (chat_id, skill_id)
);

CREATE TABLE IF NOT EXISTS tool_permissions (
    workspace  TEXT NOT NULL,
    tool       TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (workspace, tool)
);
"#;

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
    conn.execute_batch(SCHEMA)?;
    // Additive migrations for databases written by earlier versions. A failure
    // here just means the column is already present.
    let _ = conn.execute("ALTER TABLE messages ADD COLUMN attachments_json TEXT", []);
    let _ = conn.execute(
        "ALTER TABLE chats ADD COLUMN permission_mode TEXT NOT NULL DEFAULT 'manual'",
        [],
    );
    Ok(conn)
}

pub fn list_chats(conn: &Connection, kind: &str) -> rusqlite::Result<Vec<ChatRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, kind, workspace, model, thinking, effort, permission_mode, created_at, updated_at
         FROM chats WHERE kind = ?1 ORDER BY updated_at DESC",
    )?;
    let rows = stmt.query_map([kind], |r| {
        Ok(ChatRow {
            id: r.get(0)?,
            title: r.get(1)?,
            kind: r.get(2)?,
            workspace: r.get(3)?,
            model: r.get(4)?,
            thinking: r.get::<_, i64>(5)? != 0,
            effort: r.get(6)?,
            permission_mode: r.get(7)?,
            created_at: r.get(8)?,
            updated_at: r.get(9)?,
        })
    })?;
    rows.collect()
}

pub fn create_chat(
    conn: &Connection,
    kind: &str,
    workspace: Option<&str>,
    model: &str,
    thinking: bool,
    effort: &str,
    permission_mode: &str,
) -> rusqlite::Result<ChatRow> {
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now_ts();
    conn.execute(
        "INSERT INTO chats (id, title, kind, workspace, model, thinking, effort, permission_mode, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
        params![id, "New chat", kind, workspace, model, thinking as i64, effort, permission_mode, ts],
    )?;
    Ok(ChatRow {
        id,
        title: "New chat".to_string(),
        kind: kind.to_string(),
        workspace: workspace.map(str::to_string),
        model: model.to_string(),
        thinking,
        effort: effort.to_string(),
        permission_mode: permission_mode.to_string(),
        created_at: ts,
        updated_at: ts,
    })
}

pub fn get_chat(conn: &Connection, id: &str) -> rusqlite::Result<ChatRow> {
    conn.query_row(
        "SELECT id, title, kind, workspace, model, thinking, effort, permission_mode, created_at, updated_at
         FROM chats WHERE id = ?1",
        [id],
        |r| {
            Ok(ChatRow {
                id: r.get(0)?,
                title: r.get(1)?,
                kind: r.get(2)?,
                workspace: r.get(3)?,
                model: r.get(4)?,
                thinking: r.get::<_, i64>(5)? != 0,
                effort: r.get(6)?,
                permission_mode: r.get(7)?,
                created_at: r.get(8)?,
                updated_at: r.get(9)?,
            })
        },
    )
}

pub fn set_chat_permission_mode(
    conn: &Connection,
    id: &str,
    mode: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE chats SET permission_mode = ?2 WHERE id = ?1",
        params![id, mode],
    )?;
    Ok(())
}

pub fn set_chat_model(
    conn: &Connection,
    id: &str,
    model: &str,
    thinking: bool,
    effort: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE chats SET model = ?2, thinking = ?3, effort = ?4 WHERE id = ?1",
        params![id, model, thinking as i64, effort],
    )?;
    Ok(())
}

pub fn rename_chat(conn: &Connection, id: &str, title: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE chats SET title = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, title, now_ts()],
    )?;
    Ok(())
}

pub fn set_chat_workspace(
    conn: &Connection,
    id: &str,
    workspace: Option<&str>,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE chats SET workspace = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, workspace, now_ts()],
    )?;
    Ok(())
}

pub fn touch_chat(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE chats SET updated_at = ?2 WHERE id = ?1",
        params![id, now_ts()],
    )?;
    Ok(())
}

pub fn delete_chat(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM messages WHERE chat_id = ?1", [id])?;
    conn.execute("DELETE FROM memory_proposals WHERE chat_id = ?1", [id])?;
    conn.execute("DELETE FROM chat_skills WHERE chat_id = ?1", [id])?;
    conn.execute("DELETE FROM chat_summaries WHERE chat_id = ?1", [id])?;
    conn.execute("DELETE FROM chats WHERE id = ?1", [id])?;
    Ok(())
}

/// Drops every message in a chat — used when the conversation is compacted.
pub fn delete_messages(conn: &Connection, chat_id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM messages WHERE chat_id = ?1", [chat_id])?;
    Ok(())
}

pub fn list_messages(conn: &Connection, chat_id: &str) -> rusqlite::Result<Vec<MessageRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, chat_id, seq, role, content, reasoning_content, tool_calls_json, tool_call_id, attachments_json, created_at
         FROM messages WHERE chat_id = ?1 ORDER BY seq ASC",
    )?;
    let rows = stmt.query_map([chat_id], |r| {
        Ok(MessageRow {
            id: r.get(0)?,
            chat_id: r.get(1)?,
            seq: r.get(2)?,
            role: r.get(3)?,
            content: r.get(4)?,
            reasoning_content: r.get(5)?,
            tool_calls_json: r.get(6)?,
            tool_call_id: r.get(7)?,
            attachments_json: r.get(8)?,
            created_at: r.get(9)?,
        })
    })?;
    rows.collect()
}

#[allow(clippy::too_many_arguments)]
pub fn insert_message(
    conn: &Connection,
    chat_id: &str,
    role: &str,
    content: Option<&str>,
    reasoning: Option<&str>,
    tool_calls_json: Option<&str>,
    tool_call_id: Option<&str>,
    attachments_json: Option<&str>,
) -> rusqlite::Result<MessageRow> {
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now_ts();
    let seq: i64 = conn.query_row(
        "SELECT COALESCE(MAX(seq), 0) + 1 FROM messages WHERE chat_id = ?1",
        [chat_id],
        |r| r.get(0),
    )?;
    conn.execute(
        "INSERT INTO messages (id, chat_id, seq, role, content, reasoning_content, tool_calls_json, tool_call_id, attachments_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![id, chat_id, seq, role, content, reasoning, tool_calls_json, tool_call_id, attachments_json, ts],
    )?;
    Ok(MessageRow {
        id,
        chat_id: chat_id.to_string(),
        seq,
        role: role.to_string(),
        content: content.map(str::to_string),
        reasoning_content: reasoning.map(str::to_string),
        tool_calls_json: tool_calls_json.map(str::to_string),
        tool_call_id: tool_call_id.map(str::to_string),
        attachments_json: attachments_json.map(str::to_string),
        created_at: ts,
    })
}

// ---------------------------------------------------------------- summaries

pub fn set_summary(conn: &Connection, chat_id: &str, summary: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO chat_summaries (chat_id, summary, created_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(chat_id) DO UPDATE SET summary = ?2, created_at = ?3",
        params![chat_id, summary, now_ts()],
    )?;
    Ok(())
}

pub fn get_summary(conn: &Connection, chat_id: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT summary FROM chat_summaries WHERE chat_id = ?1",
        [chat_id],
        |r| r.get(0),
    )
    .optional()
}

// --------------------------------------------------------- memory proposals

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryProposalRow {
    pub id: String,
    pub chat_id: String,
    pub text: String,
    pub status: String,
    pub created_at: i64,
}

pub fn insert_proposals(
    conn: &Connection,
    chat_id: &str,
    texts: &[String],
) -> rusqlite::Result<Vec<MemoryProposalRow>> {
    let ts = now_ts();
    let mut out = Vec::new();
    for text in texts {
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO memory_proposals (id, chat_id, text, status, created_at)
             VALUES (?1, ?2, ?3, 'pending', ?4)",
            params![id, chat_id, text, ts],
        )?;
        out.push(MemoryProposalRow {
            id,
            chat_id: chat_id.to_string(),
            text: text.to_string(),
            status: "pending".to_string(),
            created_at: ts,
        });
    }
    Ok(out)
}

fn map_proposal(r: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryProposalRow> {
    Ok(MemoryProposalRow {
        id: r.get(0)?,
        chat_id: r.get(1)?,
        text: r.get(2)?,
        status: r.get(3)?,
        created_at: r.get(4)?,
    })
}

pub fn list_proposals(
    conn: &Connection,
    chat_id: &str,
    status: Option<&str>,
) -> rusqlite::Result<Vec<MemoryProposalRow>> {
    let mut out = Vec::new();
    match status {
        Some(status) => {
            let mut stmt = conn.prepare(
                "SELECT id, chat_id, text, status, created_at FROM memory_proposals
                 WHERE chat_id = ?1 AND status = ?2 ORDER BY created_at DESC",
            )?;
            let rows = stmt.query_map(params![chat_id, status], map_proposal)?;
            for row in rows {
                out.push(row?);
            }
        }
        None => {
            let mut stmt = conn.prepare(
                "SELECT id, chat_id, text, status, created_at FROM memory_proposals
                 WHERE chat_id = ?1 ORDER BY created_at DESC",
            )?;
            let rows = stmt.query_map([chat_id], map_proposal)?;
            for row in rows {
                out.push(row?);
            }
        }
    }
    Ok(out)
}

pub fn proposals_by_ids(conn: &Connection, ids: &[String]) -> rusqlite::Result<Vec<MemoryProposalRow>> {
    let mut out = Vec::new();
    for id in ids {
        let row = conn
            .query_row(
                "SELECT id, chat_id, text, status, created_at FROM memory_proposals WHERE id = ?1",
                [id],
                map_proposal,
            )
            .optional()?;
        if let Some(row) = row {
            out.push(row);
        }
    }
    Ok(out)
}

pub fn set_proposal_status(conn: &Connection, ids: &[String], status: &str) -> rusqlite::Result<()> {
    for id in ids {
        conn.execute(
            "UPDATE memory_proposals SET status = ?2 WHERE id = ?1",
            params![id, status],
        )?;
    }
    Ok(())
}

// ------------------------------------------------------------------- skills

pub fn set_chat_skills(conn: &Connection, chat_id: &str, skill_ids: &[String]) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM chat_skills WHERE chat_id = ?1", [chat_id])?;
    for id in skill_ids {
        conn.execute(
            "INSERT OR IGNORE INTO chat_skills (chat_id, skill_id) VALUES (?1, ?2)",
            params![chat_id, id],
        )?;
    }
    Ok(())
}

pub fn get_chat_skills(conn: &Connection, chat_id: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT skill_id FROM chat_skills WHERE chat_id = ?1")?;
    let rows = stmt.query_map([chat_id], |r| r.get::<_, String>(0))?;
    rows.collect()
}

// -------------------------------------------------------------- permissions

pub fn allow_tool(conn: &Connection, workspace: &str, tool: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO tool_permissions (workspace, tool, created_at) VALUES (?1, ?2, ?3)",
        params![workspace, tool, now_ts()],
    )?;
    Ok(())
}

pub fn revoke_tool(conn: &Connection, workspace: &str, tool: &str) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM tool_permissions WHERE workspace = ?1 AND tool = ?2",
        params![workspace, tool],
    )?;
    Ok(())
}

pub fn is_tool_allowed(conn: &Connection, workspace: &str, tool: &str) -> rusqlite::Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tool_permissions WHERE workspace = ?1 AND tool = ?2",
        params![workspace, tool],
        |r| r.get(0),
    )?;
    Ok(count > 0)
}

pub fn list_allowed_tools(conn: &Connection, workspace: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT tool FROM tool_permissions WHERE workspace = ?1 ORDER BY tool")?;
    let rows = stmt.query_map([workspace], |r| r.get::<_, String>(0))?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        conn
    }

    #[test]
    fn messages_are_sequenced_and_ordered() {
        let conn = mem();
        let chat = create_chat(&conn, "chat", None, "deepseek-flash", true, "high", "manual").unwrap();
        insert_message(&conn, &chat.id, "user", Some("one"), None, None, None, None).unwrap();
        insert_message(&conn, &chat.id, "assistant", Some("two"), None, None, None, None).unwrap();
        let msgs = list_messages(&conn, &chat.id).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].seq, 1);
        assert_eq!(msgs[1].seq, 2);
        assert_eq!(msgs[0].role, "user");
    }

    #[test]
    fn chats_are_scoped_by_kind() {
        let conn = mem();
        create_chat(&conn, "chat", None, "deepseek-flash", true, "high", "manual").unwrap();
        create_chat(&conn, "coding", Some("C:/proj"), "deepseek-v4-pro", true, "high", "manual")
            .unwrap();
        assert_eq!(list_chats(&conn, "chat").unwrap().len(), 1);
        assert_eq!(list_chats(&conn, "coding").unwrap().len(), 1);
        assert_eq!(list_chats(&conn, "nothing").unwrap().len(), 0);
    }

    #[test]
    fn deleting_a_chat_removes_its_messages() {
        let conn = mem();
        let chat = create_chat(&conn, "chat", None, "deepseek-flash", true, "high", "manual").unwrap();
        insert_message(&conn, &chat.id, "user", Some("hi"), None, None, None, None).unwrap();
        delete_chat(&conn, &chat.id).unwrap();
        assert!(list_messages(&conn, &chat.id).unwrap().is_empty());
        assert!(get_chat(&conn, &chat.id).is_err());
    }

    #[test]
    fn thinking_round_trips_as_a_bool() {
        let conn = mem();
        let chat = create_chat(&conn, "chat", None, "deepseek-flash", false, "low", "manual").unwrap();
        let back = get_chat(&conn, &chat.id).unwrap();
        assert!(!back.thinking);
        assert_eq!(back.effort, "low");
    }

    #[test]
    fn permission_mode_round_trips_and_can_be_changed() {
        let conn = mem();
        let chat = create_chat(&conn, "chat", None, "deepseek-flash", true, "high", "auto")
            .unwrap();
        assert_eq!(chat.permission_mode, "auto");
        assert_eq!(get_chat(&conn, &chat.id).unwrap().permission_mode, "auto");
        set_chat_permission_mode(&conn, &chat.id, "skip").unwrap();
        assert_eq!(get_chat(&conn, &chat.id).unwrap().permission_mode, "skip");
    }

    #[test]
    fn proposals_start_pending_and_can_be_marked() {
        let conn = mem();
        let chat = create_chat(&conn, "chat", None, "deepseek-flash", true, "high", "manual").unwrap();
        let made = insert_proposals(
            &conn,
            &chat.id,
            &["User is called Dan.".to_string(), "  ".to_string()],
        )
        .unwrap();
        assert_eq!(made.len(), 1, "blank entries are dropped");
        let pending = list_proposals(&conn, &chat.id, Some("pending")).unwrap();
        assert_eq!(pending.len(), 1);

        set_proposal_status(&conn, &[made[0].id.clone()], "accepted").unwrap();
        assert!(list_proposals(&conn, &chat.id, Some("pending")).unwrap().is_empty());
        let accepted = proposals_by_ids(&conn, &[made[0].id.clone()]).unwrap();
        assert_eq!(accepted[0].status, "accepted");
        assert_eq!(accepted[0].text, "User is called Dan.");
    }

    #[test]
    fn deleting_a_chat_clears_its_proposals_and_skills() {
        let conn = mem();
        let chat = create_chat(&conn, "chat", None, "deepseek-flash", true, "high", "manual").unwrap();
        insert_proposals(&conn, &chat.id, &["fact".to_string()]).unwrap();
        set_chat_skills(&conn, &chat.id, &["concise".to_string()]).unwrap();
        delete_chat(&conn, &chat.id).unwrap();
        assert!(list_proposals(&conn, &chat.id, None).unwrap().is_empty());
        assert!(get_chat_skills(&conn, &chat.id).unwrap().is_empty());
    }

    #[test]
    fn skills_are_replaced_not_appended() {
        let conn = mem();
        let chat = create_chat(&conn, "chat", None, "deepseek-flash", true, "high", "manual").unwrap();
        set_chat_skills(&conn, &chat.id, &["a".to_string(), "b".to_string()]).unwrap();
        set_chat_skills(&conn, &chat.id, &["b".to_string()]).unwrap();
        assert_eq!(get_chat_skills(&conn, &chat.id).unwrap(), vec!["b".to_string()]);
    }

    #[test]
    fn tool_permissions_are_scoped_to_a_workspace() {
        let conn = mem();
        assert!(!is_tool_allowed(&conn, "/repo-a", "run_command").unwrap());
        allow_tool(&conn, "/repo-a", "run_command").unwrap();
        assert!(is_tool_allowed(&conn, "/repo-a", "run_command").unwrap());
        assert!(!is_tool_allowed(&conn, "/repo-b", "run_command").unwrap());
        assert_eq!(list_allowed_tools(&conn, "/repo-a").unwrap().len(), 1);
        revoke_tool(&conn, "/repo-a", "run_command").unwrap();
        assert!(!is_tool_allowed(&conn, "/repo-a", "run_command").unwrap());
    }

    #[test]
    fn summaries_round_trip_and_overwrite() {
        let conn = mem();
        let chat = create_chat(&conn, "chat", None, "deepseek-flash", true, "high", "manual").unwrap();
        assert!(get_summary(&conn, &chat.id).unwrap().is_none());
        set_summary(&conn, &chat.id, "first").unwrap();
        set_summary(&conn, &chat.id, "second").unwrap();
        assert_eq!(get_summary(&conn, &chat.id).unwrap().as_deref(), Some("second"));
    }
}
