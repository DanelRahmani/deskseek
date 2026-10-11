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
    /// Accumulated USD for this chat, summed from each turn's usage.
    pub cost_usd: f64,
    /// How much rope this chat has: `ask`, `standard`, `plan` or `agent`. See
    /// `tools::AgentMode` — it decides the tool list as well as the prompt.
    pub agent_mode: String,
    /// Out of the way in the sidebar, and nothing else: the messages, the spend
    /// and the folder are exactly where they were.
    pub archived: bool,
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
    -- Accumulated USD spent across this chat's turns. Written as each run
    -- finishes, so the total survives a reload; the live usage in the UI is
    -- only ever the last turn's.
    cost_usd     REAL NOT NULL DEFAULT 0,
    -- How much rope this chat has: 'ask' (no tools), 'standard' (everything, with
    -- the approval mode deciding what is asked), 'plan' (read-only) or 'agent'
    -- (runs unattended). See tools::AgentMode.
    agent_mode   TEXT NOT NULL DEFAULT 'standard',
    -- Kept out of the sidebar's way without deleting anything.
    archived     INTEGER NOT NULL DEFAULT 0,
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
    -- Where an accepted proposal goes: 'profile' (the always-injected
    -- MEMORY.md) or one of the memory categories, each of which is its own file.
    -- Anything else is coerced to 'other' before it is stored.
    target     TEXT NOT NULL DEFAULT 'other',
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

CREATE TABLE IF NOT EXISTS artifacts (
    id              TEXT PRIMARY KEY,
    chat_id         TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
    message_id      TEXT,
    kind            TEXT NOT NULL,
    title           TEXT NOT NULL,
    current_version INTEGER NOT NULL DEFAULT 1,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_artifacts_chat ON artifacts(chat_id, updated_at);

CREATE TABLE IF NOT EXISTS artifact_versions (
    artifact_id TEXT NOT NULL REFERENCES artifacts(id) ON DELETE CASCADE,
    version     INTEGER NOT NULL,
    content     TEXT NOT NULL,
    created_by  TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    PRIMARY KEY (artifact_id, version)
);

-- Memory used to live here: a `memory_facts` table with an FTS5 index over it,
-- queried per turn. It is now one markdown file per category, under `memory/`,
-- with the profile in `MEMORY.md` — see src/memory_files.rs for why. The tables
-- are gone rather than kept around: this app has no users on an older layout, so
-- there is nothing to migrate and no reason to carry a store nothing writes to.

-- Full-text search over what was actually said, so a conversation can be found
-- by its contents and not only by its title. External-content, exactly like
-- memory_facts_fts above: the text lives once in `messages`, and the triggers
-- keep the index in step so no insert path can forget to update it.
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
    content,
    content='messages',
    content_rowid='rowid'
);

CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, content) VALUES (new.rowid, new.content);
END;

CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, content)
    VALUES ('delete', old.rowid, old.content);
END;

CREATE TRIGGER IF NOT EXISTS messages_au AFTER UPDATE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, content)
    VALUES ('delete', old.rowid, old.content);
    INSERT INTO messages_fts(rowid, content) VALUES (new.rowid, new.content);
END;

-- Pre-images of the files an agent run is about to change, so a run that went
-- wrong can be undone instead of unpicked by hand. `previous` is the content
-- before the run touched the file, or NULL when the file did not exist yet — a
-- rewind deletes a file the run created.
--
-- One row per (run, path): rewinding means back to where the run started, not to
-- an intermediate step, so a second write to the same file in one run must not
-- overwrite the earlier pre-image. That is what the UNIQUE key is for.
CREATE TABLE IF NOT EXISTS file_checkpoints (
    id         TEXT PRIMARY KEY,
    chat_id    TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
    run_id     TEXT NOT NULL,
    path       TEXT NOT NULL,
    previous   TEXT,
    created_at INTEGER NOT NULL,
    UNIQUE (run_id, path)
);

CREATE INDEX IF NOT EXISTS idx_checkpoints_chat ON file_checkpoints(chat_id, created_at);
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
    // Existing proposals predate the profile/fact split; everything so far went
    // to MEMORY.md, so that is what the default has to be for old rows.
    let _ = conn.execute(
        "ALTER TABLE memory_proposals ADD COLUMN target TEXT NOT NULL DEFAULT 'profile'",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE chats ADD COLUMN cost_usd REAL NOT NULL DEFAULT 0",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE chats ADD COLUMN agent_mode TEXT NOT NULL DEFAULT 'standard'",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE chats ADD COLUMN archived INTEGER NOT NULL DEFAULT 0",
        [],
    );
    // Databases written before the search index existed have no rows in it. The
    // triggers only fire on new writes, so an empty index over a non-empty table
    // means "first launch after the upgrade" and gets one build. `rebuild` is
    // FTS5's own external-content command; running it only in that case keeps a
    // normal launch free.
    let indexed: i64 = conn.query_row("SELECT count(*) FROM messages_fts", [], |r| r.get(0))?;
    let said: i64 = conn.query_row(
        "SELECT count(*) FROM messages WHERE content IS NOT NULL",
        [],
        |r| r.get(0),
    )?;
    if indexed == 0 && said > 0 {
        conn.execute("INSERT INTO messages_fts(messages_fts) VALUES('rebuild')", [])?;
    }
    Ok(conn)
}

/// Every chat of a section, archived ones included: the sidebar is what splits
/// them, and a tab restored from the last session has to find its chat even when
/// it is archived.
pub fn list_chats(conn: &Connection, kind: &str) -> rusqlite::Result<Vec<ChatRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, kind, workspace, model, thinking, effort, permission_mode, cost_usd, created_at, updated_at, archived, agent_mode
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
            cost_usd: r.get(8)?,
            created_at: r.get(9)?,
            updated_at: r.get(10)?,
            archived: r.get::<_, i64>(11)? != 0,
            agent_mode: r.get(12)?,
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
        cost_usd: 0.0,
        agent_mode: "standard".to_string(),
        archived: false,
        created_at: ts,
        updated_at: ts,
    })
}

pub fn get_chat(conn: &Connection, id: &str) -> rusqlite::Result<ChatRow> {
    conn.query_row(
        "SELECT id, title, kind, workspace, model, thinking, effort, permission_mode, cost_usd, created_at, updated_at, archived, agent_mode
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
                cost_usd: r.get(8)?,
                created_at: r.get(9)?,
                updated_at: r.get(10)?,
                archived: r.get::<_, i64>(11)? != 0,
                agent_mode: r.get(12)?,
            })
        },
    )
}

/// Takes a chat out of the sidebar's way without deleting anything, or puts it
/// back. `updated_at` is deliberately left alone: archiving is not activity, and
/// bumping it would reorder the list on the way back in.
pub fn set_chat_archived(conn: &Connection, id: &str, archived: bool) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE chats SET archived = ?2 WHERE id = ?1",
        params![id, archived as i64],
    )?;
    Ok(())
}

/// Adds one turn's spend to a chat's running total, so it survives a reload.
pub fn add_chat_cost(conn: &Connection, id: &str, cost: f64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE chats SET cost_usd = cost_usd + ?2 WHERE id = ?1",
        params![id, cost],
    )?;
    Ok(())
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

/// How much rope this chat has. Stored as the mode's own id, so a value this app
/// does not recognise is normalised at the edge (`AgentMode::parse`) instead of
/// being written down as-is.
pub fn set_chat_mode(conn: &Connection, id: &str, mode: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE chats SET agent_mode = ?2 WHERE id = ?1",
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

/// The newest user turn: where a regenerate or an edit starts from.
pub fn last_user_message(
    conn: &Connection,
    chat_id: &str,
) -> rusqlite::Result<Option<MessageRow>> {
    conn.query_row(
        "SELECT id, chat_id, seq, role, content, reasoning_content, tool_calls_json, tool_call_id, attachments_json, created_at
         FROM messages WHERE chat_id = ?1 AND role = 'user' ORDER BY seq DESC LIMIT 1",
        [chat_id],
        |r| {
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
        },
    )
    .optional()
}

/// Drops every message after `seq`, so a re-run starts from that turn instead of
/// stacking a second question on top of the first answer. Deleting the tail
/// rather than screening it out is what keeps the request well-formed: an
/// assistant turn whose tool calls have no results is a 400 waiting to happen.
pub fn delete_messages_after(conn: &Connection, chat_id: &str, seq: i64) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM messages WHERE chat_id = ?1 AND seq > ?2",
        params![chat_id, seq],
    )?;
    Ok(())
}

/// Rewrites a turn's text in place — the edit half of edit-and-resend.
pub fn set_message_content(conn: &Connection, id: &str, content: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE messages SET content = ?2 WHERE id = ?1",
        params![id, content],
    )?;
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

// ---------------------------------------------------------------- search

/// One message that matched a search, with enough context to show a result row.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageHit {
    pub chat_id: String,
    pub chat_title: String,
    pub chat_kind: String,
    pub message_id: String,
    pub role: String,
    /// The matched window, with the search terms bracketed.
    pub snippet: String,
}

/// Search what was actually said, across every chat.
///
/// The snippet shows the matched words in context, so a hit explains itself
/// rather than repeating a title the user can already see. Results are ordered
/// by relevance (`bm25`, best first) rather than recency: a search is a lookup,
/// not a feed.
pub fn search_messages(
    conn: &Connection,
    query: &str,
    limit: i64,
) -> rusqlite::Result<Vec<MessageHit>> {
    let Some(expr) = fts_query(query) else {
        return Ok(Vec::new());
    };
    let mut stmt = conn.prepare(
        "SELECT m.chat_id, c.title, c.kind, m.id, m.role,
                snippet(messages_fts, 0, '[', ']', ' … ', 12)
         FROM messages_fts
         JOIN messages m ON m.rowid = messages_fts.rowid
         JOIN chats c ON c.id = m.chat_id
         WHERE messages_fts MATCH ?1
         ORDER BY bm25(messages_fts)
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![expr, limit], |r| {
        Ok(MessageHit {
            chat_id: r.get(0)?,
            chat_title: r.get(1)?,
            chat_kind: r.get(2)?,
            message_id: r.get(3)?,
            role: r.get(4)?,
            snippet: r.get(5)?,
        })
    })?;
    rows.collect()
}

/// Turn free text into an FTS5 MATCH expression, or None when there is nothing
/// worth searching for.
///
/// Every term is quoted, because a bare `*`, `-`, `:` or `(` in the box is FTS5
/// syntax and would be a parse error rather than a search. The trailing `*`
/// makes it a prefix match, so a result appears while the word is still being
/// typed. A one-character term is dropped: prefix-matching `a` would return most
/// of the database. (A single-character CJK query is therefore not searchable;
/// two characters are, which is the practical case.)
fn fts_query(raw: &str) -> Option<String> {
    let terms: Vec<String> = raw
        .split_whitespace()
        .map(|t| t.replace('"', "\"\""))
        .filter(|t| t.chars().count() >= 2 && t.chars().any(|c| c.is_alphanumeric()))
        .map(|t| format!("\"{t}\"*"))
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" "))
    }
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
    /// `profile` (the always-injected MEMORY.md) or `fact` (the retrieved tier).
    pub target: String,
    pub created_at: i64,
}

/// One proposed memory, and where accepting it should file it.
pub struct ProposalDraft {
    pub text: String,
    pub target: String,
}

pub fn insert_proposals(
    conn: &Connection,
    chat_id: &str,
    drafts: &[ProposalDraft],
) -> rusqlite::Result<Vec<MemoryProposalRow>> {
    let ts = now_ts();
    let mut out = Vec::new();
    for draft in drafts {
        let text = draft.text.trim();
        if text.is_empty() {
            continue;
        }
        // The destination is already validated where proposals are parsed
        // (`summarize::into_draft`), so this stores what it is given rather than
        // applying its own rule: one validator, so the two cannot disagree.
        let target = &draft.target;
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO memory_proposals (id, chat_id, text, status, target, created_at)
             VALUES (?1, ?2, ?3, 'pending', ?4, ?5)",
            params![id, chat_id, text, target, ts],
        )?;
        out.push(MemoryProposalRow {
            id,
            chat_id: chat_id.to_string(),
            text: text.to_string(),
            status: "pending".to_string(),
            target: target.to_string(),
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
        target: r.get(4)?,
        created_at: r.get(5)?,
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
                "SELECT id, chat_id, text, status, target, created_at FROM memory_proposals
                 WHERE chat_id = ?1 AND status = ?2 ORDER BY created_at DESC",
            )?;
            let rows = stmt.query_map(params![chat_id, status], map_proposal)?;
            for row in rows {
                out.push(row?);
            }
        }
        None => {
            let mut stmt = conn.prepare(
                "SELECT id, chat_id, text, status, target, created_at FROM memory_proposals
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
                "SELECT id, chat_id, text, status, target, created_at FROM memory_proposals WHERE id = ?1",
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

/// Every grant there is, as `(workspace, tool)`. The list above answers "what may
/// this folder do"; nothing answered "what have I ever allowed", which is the
/// question a user asks when they want it taken back.
pub fn all_allowed_tools(conn: &Connection) -> rusqlite::Result<Vec<(String, String)>> {
    let mut stmt =
        conn.prepare("SELECT workspace, tool FROM tool_permissions ORDER BY workspace, tool")?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect()
}

// --------------------------------------------------------------- artifacts

/// One artifact: a titled, versioned document the panel renders. `kind` picks
/// the renderer (`html` / `svg` / `mermaid` / `markdown` / `code`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRow {
    pub id: String,
    pub chat_id: String,
    pub message_id: Option<String>,
    pub kind: String,
    pub title: String,
    pub current_version: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactVersionRow {
    pub version: i64,
    pub created_by: String,
    pub created_at: i64,
}

/// The artifact plus the one version the panel is showing and the list to pick
/// another from. Sent whole so opening one is a single round trip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactDetail {
    pub artifact: ArtifactRow,
    pub content: String,
    pub versions: Vec<ArtifactVersionRow>,
}

fn artifact_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ArtifactRow> {
    Ok(ArtifactRow {
        id: r.get(0)?,
        chat_id: r.get(1)?,
        message_id: r.get(2)?,
        kind: r.get(3)?,
        title: r.get(4)?,
        current_version: r.get(5)?,
        created_at: r.get(6)?,
        updated_at: r.get(7)?,
    })
}

const ARTIFACT_COLS: &str =
    "id, chat_id, message_id, kind, title, current_version, created_at, updated_at";

/// Store a new artifact and its first version. `created_by` is `assistant` for
/// the model and `user` for an edit made in the panel.
pub fn create_artifact(
    conn: &Connection,
    chat_id: &str,
    message_id: Option<&str>,
    kind: &str,
    title: &str,
    content: &str,
    created_by: &str,
) -> rusqlite::Result<ArtifactRow> {
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now_ts();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO artifacts (id, chat_id, message_id, kind, title, current_version, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?6)",
        params![id, chat_id, message_id, kind, title, ts],
    )?;
    tx.execute(
        "INSERT INTO artifact_versions (artifact_id, version, content, created_by, created_at)
         VALUES (?1, 1, ?2, ?3, ?4)",
        params![id, content, created_by, ts],
    )?;
    tx.commit()?;
    Ok(ArtifactRow {
        id,
        chat_id: chat_id.to_string(),
        message_id: message_id.map(str::to_string),
        kind: kind.to_string(),
        title: title.to_string(),
        current_version: 1,
        created_at: ts,
        updated_at: ts,
    })
}

/// Append a version and make it current. Nothing is ever overwritten, so an
/// edit and a rollback are the same operation.
pub fn add_artifact_version(
    conn: &Connection,
    artifact_id: &str,
    content: &str,
    created_by: &str,
) -> rusqlite::Result<ArtifactRow> {
    let ts = now_ts();
    let tx = conn.unchecked_transaction()?;
    let next: i64 = tx.query_row(
        "SELECT current_version + 1 FROM artifacts WHERE id = ?1",
        [artifact_id],
        |r| r.get(0),
    )?;
    tx.execute(
        "INSERT INTO artifact_versions (artifact_id, version, content, created_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![artifact_id, next, content, created_by, ts],
    )?;
    tx.execute(
        "UPDATE artifacts SET current_version = ?2, updated_at = ?3 WHERE id = ?1",
        params![artifact_id, next, ts],
    )?;
    tx.commit()?;
    get_artifact(conn, artifact_id)
}

pub fn get_artifact(conn: &Connection, id: &str) -> rusqlite::Result<ArtifactRow> {
    conn.query_row(
        &format!("SELECT {ARTIFACT_COLS} FROM artifacts WHERE id = ?1"),
        [id],
        artifact_row,
    )
}

/// The most recent artifact in this chat with this exact title, if any. This is
/// what makes a second `artifact_create` with the same title a new *version*
/// rather than a second document.
pub fn find_artifact_by_title(
    conn: &Connection,
    chat_id: &str,
    title: &str,
) -> rusqlite::Result<Option<ArtifactRow>> {
    conn.query_row(
        &format!(
            "SELECT {ARTIFACT_COLS} FROM artifacts
             WHERE chat_id = ?1 AND title = ?2 ORDER BY updated_at DESC LIMIT 1"
        ),
        params![chat_id, title],
        artifact_row,
    )
    .optional()
}

pub fn list_artifacts(conn: &Connection, chat_id: &str) -> rusqlite::Result<Vec<ArtifactRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {ARTIFACT_COLS} FROM artifacts WHERE chat_id = ?1 ORDER BY updated_at DESC"
    ))?;
    let rows = stmt.query_map([chat_id], artifact_row)?;
    rows.collect()
}

/// The artifact, one version's content (`None` = current), and every version's
/// metadata — newest first, which is the order the dropdown shows.
pub fn get_artifact_detail(
    conn: &Connection,
    id: &str,
    version: Option<i64>,
) -> rusqlite::Result<ArtifactDetail> {
    let artifact = get_artifact(conn, id)?;
    let wanted = version.unwrap_or(artifact.current_version);
    let content: String = conn.query_row(
        "SELECT content FROM artifact_versions WHERE artifact_id = ?1 AND version = ?2",
        params![id, wanted],
        |r| r.get(0),
    )?;
    let mut stmt = conn.prepare(
        "SELECT version, created_by, created_at FROM artifact_versions
         WHERE artifact_id = ?1 ORDER BY version DESC",
    )?;
    let versions = stmt
        .query_map([id], |r| {
            Ok(ArtifactVersionRow {
                version: r.get(0)?,
                created_by: r.get(1)?,
                created_at: r.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(ArtifactDetail {
        artifact,
        content,
        versions,
    })
}

// ------------------------------------------------------------- checkpoints

/// Records a file's pre-image before an agent run changes it.
///
/// Only the first write to a path within a run is kept (the UNIQUE key plus
/// INSERT OR IGNORE), because a rewind means back to where the run started — a
/// later write in the same run must not replace the earlier pre-image with an
/// intermediate state.
pub fn record_checkpoint(
    conn: &Connection,
    chat_id: &str,
    run_id: &str,
    path: &str,
    previous: Option<&str>,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO file_checkpoints (id, chat_id, run_id, path, previous, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            uuid::Uuid::new_v4().to_string(),
            chat_id,
            run_id,
            path,
            previous,
            now_ts()
        ],
    )?;
    Ok(())
}

/// Undoes the most recent run's file changes and returns what was restored.
///
/// One run at a time, newest first, and it says so: the pre-images of an older
/// run are only correct once the runs after it have been undone, so jumping
/// straight back would promise a state that cannot be reconstructed. An empty
/// result means there was nothing left to undo.
///
pub fn rewind_last_run(conn: &Connection, chat_id: &str) -> Result<Vec<String>, String> {
    let run_id: Option<String> = conn
        .query_row(
            "SELECT run_id FROM file_checkpoints WHERE chat_id = ?1
             ORDER BY created_at DESC, rowid DESC LIMIT 1",
            [chat_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let Some(run_id) = run_id else {
        return Ok(Vec::new());
    };

    let rows: Vec<(String, Option<String>)> = {
        let mut stmt = conn
            .prepare(
                "SELECT path, previous FROM file_checkpoints
                 WHERE run_id = ?1 ORDER BY created_at ASC, rowid ASC",
            )
            .map_err(|e| e.to_string())?;
        let mapped = stmt
            .query_map([&run_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?;
        mapped.collect::<rusqlite::Result<_>>().map_err(|e| e.to_string())?
    };

    let mut restored = Vec::new();
    for (path, previous) in rows {
        let file = std::path::Path::new(&path);
        match previous {
            // The run created it, so undoing means taking it away again. A file
            // already gone is not an error: the goal state is "absent".
            None => match std::fs::remove_file(file) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("could not remove {path}: {e}")),
            },
            Some(content) => {
                if let Some(parent) = file.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                std::fs::write(file, content)
                    .map_err(|e| format!("could not restore {path}: {e}"))?;
            }
        }
        restored.push(path);
    }
    conn.execute("DELETE FROM file_checkpoints WHERE run_id = ?1", [&run_id])
        .map_err(|e| e.to_string())?;
    Ok(restored)
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
    fn archiving_hides_a_chat_from_nothing_but_the_sidebar() {
        let conn = mem();
        let kept = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        let shelved = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        let before = get_chat(&conn, &shelved.id).unwrap().updated_at;

        set_chat_archived(&conn, &shelved.id, true).unwrap();

        // Both are still listed — the sidebar is what splits them, and a tab
        // restored from the last session has to find its chat — and the flag says
        // which is which.
        let all = list_chats(&conn, "chat").unwrap();
        assert_eq!(all.len(), 2);
        let archived: Vec<&str> = all
            .iter()
            .filter(|c| c.archived)
            .map(|c| c.id.as_str())
            .collect();
        assert_eq!(archived, vec![shelved.id.as_str()]);
        assert!(!get_chat(&conn, &kept.id).unwrap().archived);

        // Archiving is not activity. A bumped timestamp would reorder the list the
        // moment the chat came back out of the archive.
        assert_eq!(get_chat(&conn, &shelved.id).unwrap().updated_at, before);

        set_chat_archived(&conn, &shelved.id, false).unwrap();
        assert!(!get_chat(&conn, &shelved.id).unwrap().archived);
    }

    #[test]
    fn an_artifact_keeps_every_version() {
        let conn = mem();
        let chat = create_chat(&conn, "chat", None, "deepseek-flash", true, "high", "manual").unwrap();
        let art = create_artifact(&conn, &chat.id, Some("m1"), "html", "Report", "<p>v1</p>", "assistant")
            .unwrap();
        assert_eq!(art.current_version, 1);

        let edited = add_artifact_version(&conn, &art.id, "<p>v2</p>", "user").unwrap();
        assert_eq!(edited.current_version, 2);
        assert_eq!(edited.title, "Report");

        // The current content is v2, and v1 is still there to roll back to.
        let detail = get_artifact_detail(&conn, &art.id, None).unwrap();
        assert_eq!(detail.content, "<p>v2</p>");
        assert_eq!(detail.versions.len(), 2);
        // Newest first, which is the order the dropdown wants.
        assert_eq!(detail.versions[0].version, 2);
        assert_eq!(detail.versions[1].version, 1);
        assert_eq!(detail.versions[0].created_by, "user");

        let old = get_artifact_detail(&conn, &art.id, Some(1)).unwrap();
        assert_eq!(old.content, "<p>v1</p>");
        // Asking for an old version does not move the current pointer.
        assert_eq!(old.artifact.current_version, 2);
    }

    #[test]
    fn artifacts_are_listed_per_chat() {
        let conn = mem();
        let a = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        let b = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        create_artifact(&conn, &a.id, None, "markdown", "Notes", "# hi", "assistant").unwrap();
        create_artifact(&conn, &b.id, None, "markdown", "Other", "# yo", "assistant").unwrap();
        assert_eq!(list_artifacts(&conn, &a.id).unwrap().len(), 1);
        assert_eq!(list_artifacts(&conn, &a.id).unwrap()[0].title, "Notes");
    }

    #[test]
    fn a_title_lookup_finds_the_same_chat_only() {
        let conn = mem();
        let a = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        let b = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        let made = create_artifact(&conn, &a.id, None, "html", "Report", "v1", "assistant").unwrap();

        let found = find_artifact_by_title(&conn, &a.id, "Report").unwrap();
        assert_eq!(found.map(|r| r.id), Some(made.id));
        // A different chat is a different document, and an unseen title is none.
        assert!(find_artifact_by_title(&conn, &b.id, "Report").unwrap().is_none());
        assert!(find_artifact_by_title(&conn, &a.id, "Nothing").unwrap().is_none());
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
    fn a_search_finds_a_message_and_brackets_the_match() {
        let conn = mem();
        let a = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        let b = create_chat(&conn, "coding", None, "m", true, "high", "manual").unwrap();
        insert_message(&conn, &a.id, "user", Some("where is the parser bug"), None, None, None, None)
            .unwrap();
        insert_message(&conn, &b.id, "assistant", Some("the parser lives here"), None, None, None, None)
            .unwrap();

        // Both sections, prefix match, and the snippet says why it matched.
        let hits = search_messages(&conn, "pars", 10).unwrap();
        assert_eq!(hits.len(), 2, "got {hits:?}");
        assert!(hits.iter().all(|h| h.snippet.contains("[pars")));
        assert!(hits.iter().any(|h| h.chat_kind == "coding"));

        // A row with no content is not a hit, and a nonsense query is empty.
        assert!(search_messages(&conn, "x", 10).unwrap().is_empty());
        assert!(search_messages(&conn, "absentword", 10).unwrap().is_empty());
    }

    #[test]
    fn an_fts_metacharacter_is_a_search_and_not_a_syntax_error() {
        let conn = mem();
        let chat = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        insert_message(&conn, &chat.id, "user", Some("quotes and \"stars\""), None, None, None, None)
            .unwrap();
        // Each of these would be a MATCH parse error if it were not quoted.
        for raw in ["\"stars\"", "a - b", "a:b", "(a)", "**"] {
            search_messages(&conn, raw, 10).unwrap();
        }
    }

    #[test]
    fn a_rewind_restores_a_file_and_deletes_the_one_the_run_created() {
        let conn = mem();
        let chat = create_chat(&conn, "coding", Some("C:/proj"), "m", true, "high", "manual")
            .unwrap();
        let dir = std::env::temp_dir().join(format!("deskseek-rewind-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let edited = dir.join("main.rs");
        let created = dir.join("new.rs");
        std::fs::write(&edited, "original").unwrap();

        // A run that rewrites one file and creates another.
        record_checkpoint(&conn, &chat.id, "run-1", &edited.to_string_lossy(), Some("original"))
            .unwrap();
        record_checkpoint(&conn, &chat.id, "run-1", &created.to_string_lossy(), None).unwrap();
        std::fs::write(&edited, "changed").unwrap();
        std::fs::write(&created, "brand new").unwrap();

        let restored = rewind_last_run(&conn, &chat.id).unwrap();
        assert_eq!(restored.len(), 2);
        assert_eq!(std::fs::read_to_string(&edited).unwrap(), "original");
        assert!(!created.exists(), "a file the run created is removed again");
        // The run is spent, so a second rewind has nothing to do.
        assert!(rewind_last_run(&conn, &chat.id).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_the_first_pre_image_of_a_run_is_kept() {
        let conn = mem();
        let chat = create_chat(&conn, "coding", None, "m", true, "high", "manual").unwrap();
        // Two writes to one path in the same run: the first is the pre-image of
        // the run, and the second must not replace it.
        record_checkpoint(&conn, &chat.id, "run-1", "/tmp/f", Some("first")).unwrap();
        record_checkpoint(&conn, &chat.id, "run-1", "/tmp/f", Some("second")).unwrap();
        let kept: Option<String> = conn
            .query_row("SELECT previous FROM file_checkpoints WHERE run_id = 'run-1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(kept.as_deref(), Some("first"));
    }

    #[test]
    fn a_rewind_undoes_one_run_at_a_time() {
        let conn = mem();
        let chat = create_chat(&conn, "coding", None, "m", true, "high", "manual").unwrap();
        let dir = std::env::temp_dir().join(format!("deskseek-rewind2-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a");
        let b = dir.join("b");
        record_checkpoint(&conn, &chat.id, "run-1", &a.to_string_lossy(), None).unwrap();
        record_checkpoint(&conn, &chat.id, "run-2", &b.to_string_lossy(), None).unwrap();
        // Newest run first, and only that one.
        assert_eq!(
            rewind_last_run(&conn, &chat.id).unwrap(),
            vec![b.to_string_lossy().to_string()]
        );
        assert_eq!(
            rewind_last_run(&conn, &chat.id).unwrap(),
            vec![a.to_string_lossy().to_string()]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_resend_drops_the_tail_and_can_rewrite_the_turn() {
        let conn = mem();
        let chat = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        insert_message(&conn, &chat.id, "user", Some("older question"), None, None, None, None)
            .unwrap();
        insert_message(&conn, &chat.id, "assistant", Some("older answer"), None, None, None, None)
            .unwrap();
        insert_message(&conn, &chat.id, "user", Some("newer question"), None, None, None, None)
            .unwrap();
        insert_message(&conn, &chat.id, "assistant", Some("newer answer"), None, None, None, None)
            .unwrap();

        let turn = last_user_message(&conn, &chat.id).unwrap().unwrap();
        assert_eq!(turn.content.as_deref(), Some("newer question"));

        set_message_content(&conn, &turn.id, "edited question").unwrap();
        delete_messages_after(&conn, &chat.id, turn.seq).unwrap();

        let left = list_messages(&conn, &chat.id).unwrap();
        assert_eq!(left.len(), 3, "only what followed the turn is gone");
        assert_eq!(left[2].content.as_deref(), Some("edited question"));
        assert_eq!(left[1].content.as_deref(), Some("older answer"), "the older turn stays");
        // Nothing to re-send when there is no user turn at all.
        let empty = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        assert!(last_user_message(&conn, &empty.id).unwrap().is_none());
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
            &[
                ProposalDraft {
                    text: "User is called Dan.".to_string(),
                    target: "profile".to_string(),
                },
                ProposalDraft {
                    text: "  ".to_string(),
                    target: "fact".to_string(),
                },
            ],
        )
        .unwrap();
        assert_eq!(made.len(), 1, "blank entries are dropped");
        let pending = list_proposals(&conn, &chat.id, Some("pending")).unwrap();
        assert_eq!(pending.len(), 1);
        // The destination survives the round trip, or an accepted profile fact
        // would silently end up in the retrieved tier instead.
        assert_eq!(pending[0].target, "profile");

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
        insert_proposals(
            &conn,
            &chat.id,
            &[ProposalDraft {
                text: "fact".to_string(),
                target: "fact".to_string(),
            }],
        )
        .unwrap();
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
