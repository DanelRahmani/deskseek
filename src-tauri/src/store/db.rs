//! SQLite persistence for chats and messages.

use std::path::Path;

use rusqlite::{params, Connection};
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
    created_at        INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_messages_chat ON messages(chat_id, seq);
"#;

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

pub fn list_chats(conn: &Connection, kind: &str) -> rusqlite::Result<Vec<ChatRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, kind, workspace, model, thinking, effort, created_at, updated_at
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
            created_at: r.get(7)?,
            updated_at: r.get(8)?,
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
) -> rusqlite::Result<ChatRow> {
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now_ts();
    conn.execute(
        "INSERT INTO chats (id, title, kind, workspace, model, thinking, effort, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
        params![id, "New chat", kind, workspace, model, thinking as i64, effort, ts],
    )?;
    Ok(ChatRow {
        id,
        title: "New chat".to_string(),
        kind: kind.to_string(),
        workspace: workspace.map(str::to_string),
        model: model.to_string(),
        thinking,
        effort: effort.to_string(),
        created_at: ts,
        updated_at: ts,
    })
}

pub fn get_chat(conn: &Connection, id: &str) -> rusqlite::Result<ChatRow> {
    conn.query_row(
        "SELECT id, title, kind, workspace, model, thinking, effort, created_at, updated_at
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
                created_at: r.get(7)?,
                updated_at: r.get(8)?,
            })
        },
    )
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

pub fn touch_chat(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE chats SET updated_at = ?2 WHERE id = ?1",
        params![id, now_ts()],
    )?;
    Ok(())
}

pub fn delete_chat(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM messages WHERE chat_id = ?1", [id])?;
    conn.execute("DELETE FROM chats WHERE id = ?1", [id])?;
    Ok(())
}

pub fn list_messages(conn: &Connection, chat_id: &str) -> rusqlite::Result<Vec<MessageRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, chat_id, seq, role, content, reasoning_content, tool_calls_json, tool_call_id, created_at
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
            created_at: r.get(8)?,
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
) -> rusqlite::Result<MessageRow> {
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now_ts();
    let seq: i64 = conn.query_row(
        "SELECT COALESCE(MAX(seq), 0) + 1 FROM messages WHERE chat_id = ?1",
        [chat_id],
        |r| r.get(0),
    )?;
    conn.execute(
        "INSERT INTO messages (id, chat_id, seq, role, content, reasoning_content, tool_calls_json, tool_call_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![id, chat_id, seq, role, content, reasoning, tool_calls_json, tool_call_id, ts],
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
        created_at: ts,
    })
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
        let chat = create_chat(&conn, "chat", None, "deepseek-flash", true, "high").unwrap();
        insert_message(&conn, &chat.id, "user", Some("one"), None, None, None).unwrap();
        insert_message(&conn, &chat.id, "assistant", Some("two"), None, None, None).unwrap();
        let msgs = list_messages(&conn, &chat.id).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].seq, 1);
        assert_eq!(msgs[1].seq, 2);
        assert_eq!(msgs[0].role, "user");
    }

    #[test]
    fn chats_are_scoped_by_kind() {
        let conn = mem();
        create_chat(&conn, "chat", None, "deepseek-flash", true, "high").unwrap();
        create_chat(&conn, "coding", Some("C:/proj"), "deepseek-v4-pro", true, "high").unwrap();
        assert_eq!(list_chats(&conn, "chat").unwrap().len(), 1);
        assert_eq!(list_chats(&conn, "coding").unwrap().len(), 1);
        assert_eq!(list_chats(&conn, "nothing").unwrap().len(), 0);
    }

    #[test]
    fn deleting_a_chat_removes_its_messages() {
        let conn = mem();
        let chat = create_chat(&conn, "chat", None, "deepseek-flash", true, "high").unwrap();
        insert_message(&conn, &chat.id, "user", Some("hi"), None, None, None).unwrap();
        delete_chat(&conn, &chat.id).unwrap();
        assert!(list_messages(&conn, &chat.id).unwrap().is_empty());
        assert!(get_chat(&conn, &chat.id).is_err());
    }

    #[test]
    fn thinking_round_trips_as_a_bool() {
        let conn = mem();
        let chat = create_chat(&conn, "chat", None, "deepseek-flash", false, "low").unwrap();
        let back = get_chat(&conn, &chat.id).unwrap();
        assert!(!back.thinking);
        assert_eq!(back.effort, "low");
    }
}
