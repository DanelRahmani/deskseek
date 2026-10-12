//! SQLite persistence for chats and messages.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::settings::Instruction;

pub fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRow {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub workspace: Option<String>,
    /// The folder a chat-mode chat owns for its own files, decided once when it
    /// is first needed and remembered here so it does not move when the chat is
    /// renamed. Deliberately *not* `workspace`: that column means "the folder the
    /// user chose" for coding chats and projects, and it is also what scopes
    /// memory, so giving every chat one would fragment memory into a file per
    /// chat.
    pub folder: Option<String>,
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
    /// The project this chat belongs to, or `None`. Membership only — the chat's
    /// own folder and memory do not depend on it.
    pub project_id: Option<String>,
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
    /// This turn has an answer in the discarded pile that can be brought back.
    /// Computed on read rather than stored: it is a property of the turn and the
    /// pile, and storing it twice is how the two would come to disagree.
    pub discarded: bool,
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS chats (
    id           TEXT PRIMARY KEY,
    title        TEXT NOT NULL,
    kind         TEXT NOT NULL DEFAULT 'chat',
    workspace    TEXT,
    -- The folder this chat owns for its own files. See `ChatRow::folder`. Added
    -- by ALTER for databases written before chats had one.
    folder       TEXT,
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
    -- The project this chat belongs to, or NULL. Membership only: the chat's own
    -- folder and memory do not depend on it. Also added by ALTER for databases
    -- written before projects existed.
    project_id   TEXT,
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

-- What one turn cost. `chats.cost_usd` is a running total with no dates, and a
-- usage column on `messages` would be one row per API round rather than per
-- turn, so neither can answer "what did this cost me this month".
--
-- `run_id` is the agent run's own id and is UNIQUE on purpose: the row is
-- upserted as each round of the turn finishes, which is what keeps one turn to
-- one row however the run ended. Written at the same moment and under the same
-- condition as `add_chat_cost`, so the history and the chat's total agree.
CREATE TABLE IF NOT EXISTS turn_usage (
    id                TEXT PRIMARY KEY,
    chat_id           TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
    run_id            TEXT NOT NULL UNIQUE,
    model             TEXT NOT NULL,
    -- API round-trips inside this turn: 3 means two tool loops and a final answer.
    rounds            INTEGER NOT NULL DEFAULT 1,
    prompt_tokens     INTEGER NOT NULL DEFAULT 0,
    completion_tokens INTEGER NOT NULL DEFAULT 0,
    -- Kept apart from the prompt total because they are billed at different
    -- rates; `prompt_tokens` is the sum of these two when the API reports both.
    cache_hit_tokens  INTEGER NOT NULL DEFAULT 0,
    cache_miss_tokens INTEGER NOT NULL DEFAULT 0,
    cost_usd          REAL NOT NULL DEFAULT 0,
    created_at        INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_turn_usage_time ON turn_usage(created_at);
CREATE INDEX IF NOT EXISTS idx_turn_usage_chat ON turn_usage(chat_id, created_at);

-- A tail a resend replaced, kept so it can be brought back. Without it, "ask
-- again" is destructive: the only way to see the other answer is to lose this
-- one. No message tree and no parent ids — a discarded tail belongs to the turn
-- it replaced, and swapping it back in is the whole feature.
--
-- The rows are stored whole, as JSON, because *moving* them is the invariant: an
-- assistant turn and the tool results it asked for have to travel together, and
-- screening one out instead of moving it is what produced a 400 the last two
-- times this area was touched.
CREATE TABLE IF NOT EXISTS discarded_tails (
    id         TEXT PRIMARY KEY,
    chat_id    TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
    -- The seq of the last message that was kept; everything above it is in rows.
    after_seq  INTEGER NOT NULL,
    rows       TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    -- One tail per turn. Discarding the same turn again replaces it, which is
    -- what makes Restore a toggle between two answers rather than a pile.
    UNIQUE (chat_id, after_seq)
);

-- A named container: a folder to work in and the chats that belong to it. The
-- chats are the members, so deleting a project must never delete them — see
-- `delete_project`.
CREATE TABLE IF NOT EXISTS projects (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    -- Where a new chat in this project starts. Optional: a project can be a
    -- grouping alone.
    folder     TEXT,
    -- Per-project rule blocks: a JSON array of `settings::Instruction`. A project
    -- owns its own rules, so they live here rather than in the settings blob, and
    -- every chat in the project inherits them without being touched.
    instructions TEXT NOT NULL DEFAULT '[]',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_projects_updated ON projects(updated_at);
CREATE INDEX IF NOT EXISTS idx_chats_project ON chats(project_id);

-- A prompt that runs itself on a calendar. `next_run_at` is stored rather than
-- derived so a due task is one indexed comparison, and so a task that was missed
-- because the app was closed does not fire the instant it opens: the ticker
-- advances before it fires, and a stale `next_run_at` just means it runs once.
CREATE TABLE IF NOT EXISTS scheduled_tasks (
    id          TEXT PRIMARY KEY,
    title       TEXT NOT NULL,
    prompt      TEXT NOT NULL,
    -- The chat to run in, or NULL to start a fresh one each time.
    chat_id     TEXT,
    -- Membership only, as with a chat: the run's folder and memory come from the
    -- chat it lands in, or from the project's folder when a fresh one is made.
    project_id  TEXT,
    freq        TEXT NOT NULL DEFAULT 'daily',
    -- Local wall-clock time, "HH:MM".
    time        TEXT NOT NULL DEFAULT '09:00',
    -- 0 = Monday. Read only by the weekly and monthly shapes.
    weekday     INTEGER NOT NULL DEFAULT 0,
    -- 1 = first … 4 = fourth. Read only by the monthly shape.
    nth         INTEGER NOT NULL DEFAULT 1,
    enabled     INTEGER NOT NULL DEFAULT 1,
    next_run_at INTEGER NOT NULL,
    last_run_at INTEGER,
    created_at  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_scheduled_due ON scheduled_tasks(next_run_at);
"#;

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;

    // Column migrations run BEFORE the schema batch, and the order is load
    // bearing: `SCHEMA` is one statement batch, so it either all applies or none
    // of it does, and it contains indexes over columns that an older database
    // only gains from the ALTERs below. Running the batch first meant
    // `CREATE INDEX ... ON chats(project_id)` hit a database written before
    // projects existed and took the whole app down at startup — an index on a
    // column is a hard dependency, unlike `CREATE TABLE IF NOT EXISTS`.
    //
    // A failure here is expected and ignored: on a fresh database the tables do
    // not exist yet ("no such table"), and on an upgraded one the column is
    // already there ("duplicate column name"). Neither is an error.
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
    // Which project a chat belongs to, if any. NULL for every chat made before
    // projects existed, which is exactly right: it belongs to none.
    let _ = conn.execute("ALTER TABLE chats ADD COLUMN project_id TEXT", []);
    // Where a chat-mode chat keeps its own files. NULL means "not worked out
    // yet"; the first file operation decides it, so an existing chat keeps
    // reading the shared documents folder until it next touches a file.
    let _ = conn.execute("ALTER TABLE chats ADD COLUMN folder TEXT", []);
    // A project's own instruction blocks arrived with 2.7. An older database
    // gets an empty array, which is exactly "no project rules yet".
    let _ = conn.execute(
        "ALTER TABLE projects ADD COLUMN instructions TEXT NOT NULL DEFAULT '[]'",
        [],
    );

    conn.execute_batch(SCHEMA)?;

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
        "SELECT id, title, kind, workspace, model, thinking, effort, permission_mode, cost_usd, created_at, updated_at, archived, agent_mode, project_id, folder
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
            project_id: r.get(13)?,
            folder: r.get(14)?,
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
        folder: None,
        model: model.to_string(),
        thinking,
        effort: effort.to_string(),
        permission_mode: permission_mode.to_string(),
        cost_usd: 0.0,
        agent_mode: "standard".to_string(),
        archived: false,
        project_id: None,
        created_at: ts,
        updated_at: ts,
    })
}

/// Copies a conversation: a row of its own, the same settings, and its own copies
/// of the messages. `None` when there is no such chat.
///
/// Two things are deliberately *not* copied:
///
/// - `cost_usd`. Nothing was paid for the copy — the money went on the original —
///   and a second chat claiming the same dollars would double-count in the spend
///   view, which is the one thing that view must not do. The same reasoning keeps
///   the per-turn usage history out of it.
/// - `folder`. The copy owns its own files; whoever duplicates a conversation
///   then copies the folder into it.
///
/// The per-chat switches (skills and design systems) *are* copied, because they
/// are how this conversation was set up rather than something it spent.
pub fn duplicate_chat(conn: &Connection, id: &str) -> rusqlite::Result<Option<ChatRow>> {
    let Ok(source) = get_chat(conn, id) else {
        return Ok(None);
    };
    // `insert_message` numbers each message as it goes, so replaying the source
    // in order reproduces the order exactly with fresh ids — no bespoke INSERT,
    // and no second place that decides what a message's seq means.
    let copy = create_chat(
        conn,
        &source.kind,
        source.workspace.as_deref(),
        &source.model,
        source.thinking,
        &source.effort,
        &source.permission_mode,
    )?;
    rename_chat(conn, &copy.id, &format!("{} (copy)", source.title))?;
    conn.execute(
        "UPDATE chats SET agent_mode = ?2, project_id = ?3 WHERE id = ?1",
        params![copy.id, source.agent_mode, source.project_id],
    )?;
    let skills = get_chat_skills(conn, id)?;
    if !skills.is_empty() {
        set_chat_skills(conn, &copy.id, &skills)?;
    }
    for m in list_messages(conn, id)? {
        insert_message(
            conn,
            &copy.id,
            &m.role,
            m.content.as_deref(),
            m.reasoning_content.as_deref(),
            m.tool_calls_json.as_deref(),
            m.tool_call_id.as_deref(),
            m.attachments_json.as_deref(),
        )?;
    }
    get_chat(conn, &copy.id).map(Some)
}

pub fn get_chat(conn: &Connection, id: &str) -> rusqlite::Result<ChatRow> {
    conn.query_row(
        "SELECT id, title, kind, workspace, model, thinking, effort, permission_mode, cost_usd, created_at, updated_at, archived, agent_mode, project_id, folder
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
                project_id: r.get(13)?,
                folder: r.get(14)?,
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

/// Sets a chat's title: from the first turn's text, or by hand from the sidebar.
///
/// `updated_at` is deliberately left alone — the same rule archiving and
/// `set_chat_folder` follow. The list is ordered by recency, and renaming is not
/// activity: bumping it would shuffle the row to the top as a side effect of the
/// user typing a better name.
pub fn rename_chat(conn: &Connection, id: &str, title: &str) -> rusqlite::Result<()> {
    conn.execute("UPDATE chats SET title = ?2 WHERE id = ?1", params![id, title])?;
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

/// Remembers the folder a chat owns, once, when it is first worked out.
///
/// `updated_at` is deliberately left alone — the same rule archiving follows.
/// Deciding where a chat's files live is bookkeeping, and bumping the timestamp
/// would reorder the sidebar as a side effect of opening a file panel.
pub fn set_chat_folder(conn: &Connection, id: &str, folder: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE chats SET folder = ?2 WHERE id = ?1",
        params![id, folder],
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
        "SELECT m.id, m.chat_id, m.seq, m.role, m.content, m.reasoning_content,
                m.tool_calls_json, m.tool_call_id, m.attachments_json, m.created_at,
                EXISTS (SELECT 1 FROM discarded_tails d
                        WHERE d.chat_id = m.chat_id AND d.after_seq = m.seq)
         FROM messages m WHERE m.chat_id = ?1 AND m.role = 'user' ORDER BY m.seq DESC LIMIT 1",
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
                discarded: r.get::<_, i64>(10)? != 0,
            })
        },
    )
    .optional()
}

/// Moves every message after `seq` into the discarded pile instead of dropping
/// it, so a re-run starts from that turn without destroying the answer it is
/// replacing.
///
/// Moving rather than screening out is what keeps the request well-formed: an
/// assistant turn whose tool calls have no results is a 400 waiting to happen.
/// Everything above `seq` travels together, in order, as it was.
pub fn discard_tail(conn: &Connection, chat_id: &str, seq: i64) -> Result<(), String> {
    let rows = rows_after(conn, chat_id, seq).map_err(|e| e.to_string())?;
    if rows.is_empty() {
        // Nothing to keep. Still clear any older pile for this turn, or Restore
        // would offer an answer that no longer belongs to it.
        conn.execute(
            "DELETE FROM discarded_tails WHERE chat_id = ?1 AND after_seq = ?2",
            params![chat_id, seq],
        )
        .map_err(|e| e.to_string())?;
        return Ok(());
    }
    let json = serde_json::to_string(&rows).map_err(|e| e.to_string())?;
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO discarded_tails (id, chat_id, after_seq, rows, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(chat_id, after_seq) DO UPDATE SET
             rows = excluded.rows, created_at = excluded.created_at",
        params![
            uuid::Uuid::new_v4().to_string(),
            chat_id,
            seq,
            json,
            now_ts()
        ],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "DELETE FROM messages WHERE chat_id = ?1 AND seq > ?2",
        params![chat_id, seq],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}

/// Puts a replaced tail back, and puts the tail it replaces into the pile.
///
/// The swap is the point: asking again is not destructive, so the two answers
/// are one click apart and neither is lost. It is also what makes the seq
/// arithmetic safe — everything above `after_seq` is cleared before the stored
/// rows go back, so their original seqs are free again.
pub fn restore_tail(conn: &Connection, chat_id: &str, after_seq: i64) -> Result<(), String> {
    let stored: String = conn
        .query_row(
            "SELECT rows FROM discarded_tails WHERE chat_id = ?1 AND after_seq = ?2",
            params![chat_id, after_seq],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("There is no replaced answer to bring back at that point.")?;
    let incoming: Vec<MessageRow> =
        serde_json::from_str(&stored).map_err(|e| format!("the kept answer is unreadable: {e}"))?;
    let current = rows_after(conn, chat_id, after_seq).map_err(|e| e.to_string())?;

    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    if current.is_empty() {
        tx.execute(
            "DELETE FROM discarded_tails WHERE chat_id = ?1 AND after_seq = ?2",
            params![chat_id, after_seq],
        )
        .map_err(|e| e.to_string())?;
    } else {
        let json = serde_json::to_string(&current).map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE discarded_tails SET rows = ?3, created_at = ?4
             WHERE chat_id = ?1 AND after_seq = ?2",
            params![chat_id, after_seq, json, now_ts()],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.execute(
        "DELETE FROM messages WHERE chat_id = ?1 AND seq > ?2",
        params![chat_id, after_seq],
    )
    .map_err(|e| e.to_string())?;
    for row in &incoming {
        tx.execute(
            "INSERT INTO messages (id, chat_id, seq, role, content, reasoning_content,
                                   tool_calls_json, tool_call_id, attachments_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                row.id,
                row.chat_id,
                row.seq,
                row.role,
                row.content,
                row.reasoning_content,
                row.tool_calls_json,
                row.tool_call_id,
                row.attachments_json,
                row.created_at,
            ],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}

/// Every message after `seq`, in order — the unit that moves between the
/// conversation and the discarded pile.
fn rows_after(conn: &Connection, chat_id: &str, seq: i64) -> rusqlite::Result<Vec<MessageRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, chat_id, seq, role, content, reasoning_content, tool_calls_json,
                tool_call_id, attachments_json, created_at
         FROM messages WHERE chat_id = ?1 AND seq > ?2 ORDER BY seq ASC",
    )?;
    let rows = stmt.query_map(params![chat_id, seq], |r| {
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
            discarded: false,
        })
    })?;
    rows.collect()
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
        "SELECT m.id, m.chat_id, m.seq, m.role, m.content, m.reasoning_content,
                m.tool_calls_json, m.tool_call_id, m.attachments_json, m.created_at,
                EXISTS (SELECT 1 FROM discarded_tails d
                        WHERE d.chat_id = m.chat_id AND d.after_seq = m.seq)
         FROM messages m WHERE m.chat_id = ?1 ORDER BY m.seq ASC",
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
            discarded: r.get::<_, i64>(10)? != 0,
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
        // A message that was just written has nothing replaced behind it.
        discarded: false,
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
///
/// Unrestricted on purpose: this is the user looking at their own history, from
/// the sidebar. `search_messages_scoped` is the same search for the *model*,
/// which has no business reading across folders.
pub fn search_messages(
    conn: &Connection,
    query: &str,
    limit: i64,
) -> rusqlite::Result<Vec<MessageHit>> {
    search_messages_scoped(conn, query, None, None, limit)
}

/// The same search, narrowed by the caller.
///
/// The model searches through this one. A side effect of the split is the point:
/// the scope is a parameter, so a tool call cannot forget to apply it and the
/// query exists in exactly one place.
///
/// `workspace` narrows to one folder and `kind` to one section. A project's
/// chats all work in the project's folder, so the folder already *is* the
/// project scope — adding the project id as a second filter would only be a
/// narrower way to ask the same question.
pub fn search_messages_scoped(
    conn: &Connection,
    query: &str,
    workspace: Option<&str>,
    kind: Option<&str>,
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
           AND (?2 IS NULL OR c.workspace = ?2)
           AND (?3 IS NULL OR c.kind = ?3)
         ORDER BY bm25(messages_fts)
         LIMIT ?4",
    )?;
    let rows = stmt.query_map(params![expr, workspace, kind, limit], |r| {
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

// ---------------------------------------------------------------- projects

/// A named container: a folder to work in, the chats that belong to it, and the
/// rules every chat in it inherits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRow {
    pub id: String,
    pub name: String,
    /// The folder new chats in this project start in. Optional.
    pub folder: Option<String>,
    /// Instruction blocks injected into every chat in the project. Empty means
    /// none, which is what every project had before 2.7.
    #[serde(default)]
    pub instructions: Vec<Instruction>,
    pub created_at: i64,
    pub updated_at: i64,
}

const PROJECT_COLS: &str = "id, name, folder, instructions, created_at, updated_at";

fn project_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ProjectRow> {
    // A malformed blob reads as "no instructions" rather than failing the whole
    // list: the panel is the way back to a good value, and losing sight of the
    // other projects over it would be worse than losing the rules.
    let raw: String = r.get(3)?;
    Ok(ProjectRow {
        id: r.get(0)?,
        name: r.get(1)?,
        folder: r.get(2)?,
        instructions: serde_json::from_str(&raw).unwrap_or_default(),
        created_at: r.get(4)?,
        updated_at: r.get(5)?,
    })
}

pub fn list_projects(conn: &Connection) -> rusqlite::Result<Vec<ProjectRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {PROJECT_COLS} FROM projects ORDER BY updated_at DESC"
    ))?;
    let rows = stmt.query_map([], project_row)?;
    rows.collect()
}

/// An empty name would make a project unfindable in the list, so it falls back
/// rather than being refused — the user can rename it straight after.
pub fn create_project(
    conn: &Connection,
    name: &str,
    folder: Option<&str>,
) -> rusqlite::Result<ProjectRow> {
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now_ts();
    let name = name.trim();
    let name = if name.is_empty() { "New project" } else { name };
    conn.execute(
        "INSERT INTO projects (id, name, folder, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?4)",
        params![id, name, folder, ts],
    )?;
    Ok(ProjectRow {
        id,
        name: name.to_string(),
        folder: folder.map(str::to_string),
        instructions: Vec::new(),
        created_at: ts,
        updated_at: ts,
    })
}

pub fn rename_project(conn: &Connection, id: &str, name: &str) -> rusqlite::Result<()> {
    let name = name.trim();
    let name = if name.is_empty() { "New project" } else { name };
    conn.execute(
        "UPDATE projects SET name = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, name, now_ts()],
    )?;
    Ok(())
}

pub fn set_project_folder(
    conn: &Connection,
    id: &str,
    folder: Option<&str>,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE projects SET folder = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, folder, now_ts()],
    )?;
    Ok(())
}

/// Removes the container only. The chats stay — deleting a project must not
/// delete conversations — they simply stop belonging to anything.
pub fn delete_project(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("UPDATE chats SET project_id = NULL WHERE project_id = ?1", [id])?;
    tx.execute("DELETE FROM projects WHERE id = ?1", [id])?;
    tx.commit()
}

/// Puts a chat in a project, or takes it out of one.
pub fn set_chat_project(
    conn: &Connection,
    id: &str,
    project_id: Option<&str>,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE chats SET project_id = ?2 WHERE id = ?1",
        params![id, project_id],
    )?;
    Ok(())
}

/// One project, or `None` when it has been deleted.
pub fn get_project(conn: &Connection, id: &str) -> rusqlite::Result<Option<ProjectRow>> {
    let mut stmt = conn.prepare(&format!("SELECT {PROJECT_COLS} FROM projects WHERE id = ?1"))?;
    stmt.query_row([id], project_row).optional()
}

/// Replaces a project's instruction blocks.
///
/// Deliberately does not touch `updated_at`: the project list is ordered by it,
/// so editing a rule must not reshuffle the list — the same reason renaming a
/// chat does not bump the chat's own timestamp.
pub fn set_project_instructions(
    conn: &Connection,
    id: &str,
    instructions: &[Instruction],
) -> rusqlite::Result<()> {
    let json = serde_json::to_string(instructions).unwrap_or_else(|_| "[]".to_string());
    conn.execute(
        "UPDATE projects SET instructions = ?2 WHERE id = ?1",
        params![id, json],
    )?;
    Ok(())
}

/// One project's all-time spend, summed across the chats that belong to it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSpend {
    pub project_id: String,
    pub cost_usd: f64,
    pub turns: i64,
}

/// Spend per project, in one query.
///
/// `spend_report` already groups a chat's turns; this is the same rows grouped
/// one level up through `chats.project_id`. All-time rather than a window,
/// because "what has this project cost me" is a total — and it is the same
/// number the sidebar shows per chat, added together.
pub fn project_spend(conn: &Connection) -> rusqlite::Result<Vec<ProjectSpend>> {
    let mut stmt = conn.prepare(
        "SELECT c.project_id, COALESCE(SUM(t.cost_usd), 0), COUNT(*)
         FROM chats c JOIN turn_usage t ON t.chat_id = c.id
         WHERE c.project_id IS NOT NULL
         GROUP BY c.project_id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(ProjectSpend {
            project_id: r.get(0)?,
            cost_usd: r.get(1)?,
            turns: r.get(2)?,
        })
    })?;
    rows.collect()
}

// ---------------------------------------------------------- scheduled tasks

/// One recurring task: a prompt, when it next runs, and where its run goes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledRow {
    pub id: String,
    pub title: String,
    pub prompt: String,
    /// The chat to run in, or `None` to start a fresh one each time.
    pub chat_id: Option<String>,
    pub project_id: Option<String>,
    pub freq: String,
    /// Local wall-clock "HH:MM".
    pub time: String,
    /// 0 = Monday. Read only by the weekly and monthly shapes.
    pub weekday: i64,
    /// 1 = first … 4 = fourth. Read only by the monthly shape.
    pub nth: i64,
    pub enabled: bool,
    pub next_run_at: i64,
    pub last_run_at: Option<i64>,
    pub created_at: i64,
}

const SCHEDULED_COLS: &str = "id, title, prompt, chat_id, project_id, freq, time, weekday, nth, \
                              enabled, next_run_at, last_run_at, created_at";

fn scheduled_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ScheduledRow> {
    Ok(ScheduledRow {
        id: r.get(0)?,
        title: r.get(1)?,
        prompt: r.get(2)?,
        chat_id: r.get(3)?,
        project_id: r.get(4)?,
        freq: r.get(5)?,
        time: r.get(6)?,
        weekday: r.get(7)?,
        nth: r.get(8)?,
        enabled: r.get::<_, i64>(9)? != 0,
        next_run_at: r.get(10)?,
        last_run_at: r.get(11)?,
        created_at: r.get(12)?,
    })
}

pub fn list_scheduled(conn: &Connection) -> rusqlite::Result<Vec<ScheduledRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {SCHEDULED_COLS} FROM scheduled_tasks ORDER BY next_run_at"
    ))?;
    let rows = stmt.query_map([], scheduled_row)?;
    rows.collect()
}

/// Adds a task. `next_run_at` comes from the caller, so the row has no clock of
/// its own and a test can schedule a task into the past.
#[allow(clippy::too_many_arguments)]
pub fn create_scheduled(
    conn: &Connection,
    title: &str,
    prompt: &str,
    chat_id: Option<&str>,
    project_id: Option<&str>,
    freq: &str,
    time: &str,
    weekday: i64,
    nth: i64,
    next_run_at: i64,
) -> rusqlite::Result<ScheduledRow> {
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now_ts();
    let title = title.trim();
    let title = if title.is_empty() { "Scheduled task" } else { title };
    conn.execute(
        "INSERT INTO scheduled_tasks
         (id, title, prompt, chat_id, project_id, freq, time, weekday, nth, enabled, next_run_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1, ?10, ?11)",
        params![id, title, prompt, chat_id, project_id, freq, time, weekday, nth, next_run_at, ts],
    )?;
    Ok(ScheduledRow {
        id,
        title: title.to_string(),
        prompt: prompt.to_string(),
        chat_id: chat_id.map(str::to_string),
        project_id: project_id.map(str::to_string),
        freq: freq.to_string(),
        time: time.to_string(),
        weekday,
        nth,
        enabled: true,
        next_run_at,
        last_run_at: None,
        created_at: ts,
    })
}

/// Rewrites a task, including when it next runs — a changed time has to move the
/// next run, or the edit would only take effect after the old one fired.
#[allow(clippy::too_many_arguments)]
pub fn update_scheduled(
    conn: &Connection,
    id: &str,
    title: &str,
    prompt: &str,
    chat_id: Option<&str>,
    project_id: Option<&str>,
    freq: &str,
    time: &str,
    weekday: i64,
    nth: i64,
    enabled: bool,
    next_run_at: i64,
) -> rusqlite::Result<()> {
    let title = title.trim();
    let title = if title.is_empty() { "Scheduled task" } else { title };
    conn.execute(
        "UPDATE scheduled_tasks
         SET title = ?2, prompt = ?3, chat_id = ?4, project_id = ?5, freq = ?6, time = ?7,
             weekday = ?8, nth = ?9, enabled = ?10, next_run_at = ?11
         WHERE id = ?1",
        params![
            id,
            title,
            prompt,
            chat_id,
            project_id,
            freq,
            time,
            weekday,
            nth,
            enabled as i64,
            next_run_at
        ],
    )?;
    Ok(())
}

pub fn delete_scheduled(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM scheduled_tasks WHERE id = ?1", [id])?;
    Ok(())
}

/// Enabled tasks whose time has come, earliest first.
pub fn due_scheduled(conn: &Connection, now: i64) -> rusqlite::Result<Vec<ScheduledRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {SCHEDULED_COLS} FROM scheduled_tasks
         WHERE enabled = 1 AND next_run_at <= ?1 ORDER BY next_run_at"
    ))?;
    let rows = stmt.query_map([now], scheduled_row)?;
    rows.collect()
}

/// Records a run and moves the task on. Called *before* the run starts: a run
/// that outlasts the tick interval must not be started again by the next tick.
pub fn mark_scheduled_ran(
    conn: &Connection,
    id: &str,
    next_run_at: i64,
    ran_at: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE scheduled_tasks SET next_run_at = ?2, last_run_at = ?3 WHERE id = ?1",
        params![id, next_run_at, ran_at],
    )?;
    Ok(())
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

/// Every artifact, newest first, across every chat — what the gallery lists.
/// Capped: the gallery is a browsing surface, not an export.
pub fn list_all_artifacts(conn: &Connection, limit: usize) -> rusqlite::Result<Vec<ArtifactRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {ARTIFACT_COLS} FROM artifacts ORDER BY updated_at DESC LIMIT ?1"
    ))?;
    let rows = stmt.query_map([limit as i64], artifact_row)?;
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

// ------------------------------------------------------------ usage history

/// One API round's usage, as the agent loop hands it over.
///
/// `rounds` is 1 for a normal call: the caller adds one round at a time and the
/// store accumulates, so it never has to hold the whole turn in memory.
pub struct TurnUsage<'a> {
    pub model: &'a str,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub cache_hit_tokens: u32,
    pub cache_miss_tokens: u32,
    pub cost_usd: f64,
}

/// Adds one round to its turn's row, creating the row on the first round.
///
/// An upsert rather than an append, and called next to `add_chat_cost` under the
/// same condition, so a run that is stopped or errors half-way leaves exactly
/// what it spent — in the history as well as in the chat's running total. That
/// agreement is the whole point: a spend view that disagrees with the chat it is
/// describing is worse than no spend view.
pub fn record_turn_usage(
    conn: &Connection,
    chat_id: &str,
    run_id: &str,
    round: &TurnUsage<'_>,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO turn_usage
             (id, chat_id, run_id, model, rounds, prompt_tokens, completion_tokens,
              cache_hit_tokens, cache_miss_tokens, cost_usd, created_at)
         VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(run_id) DO UPDATE SET
             rounds            = rounds + 1,
             prompt_tokens     = prompt_tokens + excluded.prompt_tokens,
             completion_tokens = completion_tokens + excluded.completion_tokens,
             cache_hit_tokens  = cache_hit_tokens + excluded.cache_hit_tokens,
             cache_miss_tokens = cache_miss_tokens + excluded.cache_miss_tokens,
             cost_usd          = cost_usd + excluded.cost_usd,
             model             = excluded.model",
        params![
            uuid::Uuid::new_v4().to_string(),
            chat_id,
            run_id,
            round.model,
            round.prompt_tokens,
            round.completion_tokens,
            round.cache_hit_tokens,
            round.cache_miss_tokens,
            round.cost_usd,
            now_ts(),
        ],
    )?;
    Ok(())
}

/// One day's spend, summed over every turn that finished that day.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaySpend {
    /// `YYYY-MM-DD`, in local time — the day the user would name.
    pub day: String,
    pub cost_usd: f64,
    pub turns: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSpend {
    pub model: String,
    pub cost_usd: f64,
    pub turns: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSpend {
    pub chat_id: String,
    pub title: String,
    pub cost_usd: f64,
    pub turns: i64,
}

/// Everything the spend panel shows, in one round trip.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpendReport {
    /// The window these figures cover, as a Unix timestamp.
    pub since: i64,
    pub total_usd: f64,
    pub turns: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    /// Newest day first, and only days that actually had spend.
    pub days: Vec<DaySpend>,
    /// Most expensive first.
    pub models: Vec<ModelSpend>,
    pub chats: Vec<ChatSpend>,
}

/// Spend since `since`, grouped the three ways the panel asks for it.
///
/// A day is a *local* day: the column is Unix seconds, and a spend view that
/// buckets by UTC midnight would put an evening's work on tomorrow's row for
/// anyone east of Greenwich.
pub fn spend_report(conn: &Connection, since: i64, chat_limit: usize) -> rusqlite::Result<SpendReport> {
    let (total_usd, turns, prompt_tokens, completion_tokens) = conn.query_row(
        "SELECT COALESCE(SUM(cost_usd), 0), COUNT(*),
                COALESCE(SUM(prompt_tokens), 0), COALESCE(SUM(completion_tokens), 0)
         FROM turn_usage WHERE created_at >= ?1",
        [since],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;

    let days = {
        let mut stmt = conn.prepare(
            "SELECT date(created_at, 'unixepoch', 'localtime') AS day,
                    SUM(cost_usd), COUNT(*), SUM(prompt_tokens), SUM(completion_tokens)
             FROM turn_usage WHERE created_at >= ?1
             GROUP BY day ORDER BY day DESC",
        )?;
        let rows = stmt.query_map([since], |r| {
            Ok(DaySpend {
                day: r.get(0)?,
                cost_usd: r.get(1)?,
                turns: r.get(2)?,
                prompt_tokens: r.get(3)?,
                completion_tokens: r.get(4)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    let models = {
        let mut stmt = conn.prepare(
            "SELECT model, SUM(cost_usd), COUNT(*) FROM turn_usage WHERE created_at >= ?1
             GROUP BY model ORDER BY SUM(cost_usd) DESC",
        )?;
        let rows = stmt.query_map([since], |r| {
            Ok(ModelSpend {
                model: r.get(0)?,
                cost_usd: r.get(1)?,
                turns: r.get(2)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    let chats = {
        // Joined to `chats` for the title: the panel shows conversations by name,
        // and a deleted chat's rows are gone with it (the foreign key cascades).
        let mut stmt = conn.prepare(
            "SELECT t.chat_id, c.title, SUM(t.cost_usd), COUNT(*)
             FROM turn_usage t JOIN chats c ON c.id = t.chat_id
             WHERE t.created_at >= ?1
             GROUP BY t.chat_id ORDER BY SUM(t.cost_usd) DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![since, chat_limit as i64], |r| {
            Ok(ChatSpend {
                chat_id: r.get(0)?,
                title: r.get(1)?,
                cost_usd: r.get(2)?,
                turns: r.get(3)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    Ok(SpendReport {
        since,
        total_usd,
        turns,
        prompt_tokens,
        completion_tokens,
        days,
        models,
        chats,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A **fresh** database: `SCHEMA` and nothing else.
    ///
    /// Note the blind spot this creates. It is the schema as `CREATE TABLE`
    /// builds it, so a column that only arrives via `ALTER` is present here from
    /// the start and the upgrade path is never exercised — a bug in `open()`'s
    /// ordering passed 251 tests and then took the app down on the user's real
    /// database. Anything about upgrading needs a file and `open()`; see
    /// `an_older_database_upgrades_instead_of_refusing_to_open`.
    fn mem() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        conn
    }

    #[test]
    fn an_older_database_upgrades_instead_of_refusing_to_open() {
        // Exactly the `chats` table as it was before projects existed: the same
        // columns, minus `project_id`. `SCHEMA` carries an index over that column,
        // so if `open()` ran the schema batch before the ALTERs it would abort
        // here — which is precisely what happened in the wild.
        let dir = std::env::temp_dir().join(format!("deskseek-upgrade-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("deskseek.db");
        {
            let old = Connection::open(&path).unwrap();
            old.execute_batch(
                "CREATE TABLE chats (
                    id           TEXT PRIMARY KEY,
                    title        TEXT NOT NULL,
                    kind         TEXT NOT NULL DEFAULT 'chat',
                    workspace    TEXT,
                    model        TEXT NOT NULL,
                    thinking     INTEGER NOT NULL DEFAULT 1,
                    effort       TEXT NOT NULL DEFAULT 'high',
                    permission_mode TEXT NOT NULL DEFAULT 'manual',
                    cost_usd     REAL NOT NULL DEFAULT 0,
                    agent_mode   TEXT NOT NULL DEFAULT 'standard',
                    archived     INTEGER NOT NULL DEFAULT 0,
                    created_at   INTEGER NOT NULL,
                    updated_at   INTEGER NOT NULL
                );
                -- And a project as it was before it could hold instructions, so
                -- the `instructions` ALTER is exercised on a real file rather
                -- than only on a fresh `CREATE TABLE`.
                CREATE TABLE projects (
                    id         TEXT PRIMARY KEY,
                    name       TEXT NOT NULL,
                    folder     TEXT,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                );
                INSERT INTO chats
                    (id, title, kind, model, created_at, updated_at)
                    VALUES ('old-1', 'A chat from before', 'chat', 'm', 1, 1);
                INSERT INTO projects (id, name, created_at, updated_at)
                    VALUES ('p-1', 'Before 2.7', 1, 1);",
            )
            .unwrap();
        }

        let conn = open(&path).expect("an older database must still open");

        // The column the index needs is there, and the old row kept its identity
        // while gaining exactly nothing: it belongs to no project, and its own
        // folder is decided the first time it needs one rather than guessed here.
        // (`get_chat` selects every column, so this row is also the guard that
        // the `folder` ALTER really ran on an upgraded database.)
        let kept = get_chat(&conn, "old-1").unwrap();
        assert_eq!(kept.title, "A chat from before");
        assert!(kept.project_id.is_none());
        assert!(kept.folder.is_none());

        // The `instructions` ALTER ran on the upgraded file: the old project kept
        // its name and gained an empty rule list rather than a malformed one.
        let projects = list_projects(&conn).unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].name, "Before 2.7");
        assert!(projects[0].instructions.is_empty());

        // The index that actually failed must really exist now. "Did not crash"
        // would also be satisfied by the batch silently skipping it.
        let built: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_chats_project'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(built, 1, "the index over the ALTERed column was not created");

        // And the new features work against the upgraded file, not just against
        // an in-memory schema.
        let chat = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        set_chat_project(&conn, &chat.id, None).unwrap();
        // The pre-2.7 project is still there — the upgrade adds a column, it does
        // not empty the table — and the new call paths run against the file.
        assert_eq!(list_projects(&conn).unwrap().len(), 1);
        assert!(list_scheduled(&conn).unwrap().is_empty());
        create_scheduled(&conn, "t", "p", None, None, "daily", "09:00", 0, 1, 1).unwrap();
        assert_eq!(list_scheduled(&conn).unwrap().len(), 1);

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
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
    fn renaming_a_chat_does_not_reorder_the_sidebar() {
        let conn = mem();
        let chat = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        let before = get_chat(&conn, &chat.id).unwrap().updated_at;

        rename_chat(&conn, &chat.id, "A better name").unwrap();

        assert_eq!(get_chat(&conn, &chat.id).unwrap().title, "A better name");
        // `list_chats` orders by `updated_at DESC`, so a bumped timestamp here
        // would move the row to the top the moment the user typed its new name.
        assert_eq!(get_chat(&conn, &chat.id).unwrap().updated_at, before);
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
        // And a query with nothing searchable in it is empty, not an error.
        for raw in ["", "   ", "a", "*", "-"] {
            assert!(
                search_messages_scoped(&conn, raw, None, None, 10).unwrap().is_empty(),
                "{raw:?} should have found nothing"
            );
        }
    }

    #[test]
    fn a_scoped_search_cannot_quote_another_workspace() {
        let conn = mem();
        // Two clients' folders, and one chat-mode conversation.
        let mine = create_chat(&conn, "coding", Some("C:/client-a"), "m", true, "high", "manual")
            .unwrap();
        let theirs =
            create_chat(&conn, "coding", Some("C:/client-b"), "m", true, "high", "manual").unwrap();
        let loose = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        insert_message(&conn, &mine.id, "user", Some("the invoice template"), None, None, None, None)
            .unwrap();
        insert_message(&conn, &theirs.id, "user", Some("their invoice template"), None, None, None, None)
            .unwrap();
        insert_message(&conn, &loose.id, "user", Some("my own invoice template"), None, None, None, None)
            .unwrap();

        // Unrestricted finds all three, which is what the sidebar asks for.
        assert_eq!(search_messages(&conn, "invoice", 10).unwrap().len(), 3);

        // Scoped to my folder, the other client's conversation is not a result —
        // and neither is the chat-mode one, which lives in its own place.
        let scoped = search_messages_scoped(&conn, "invoice", Some("C:/client-a"), Some("coding"), 10)
            .unwrap();
        assert_eq!(scoped.len(), 1, "got {scoped:?}");
        assert_eq!(scoped[0].chat_id, mine.id);

        // A chat-mode chat has no folder to draw a line at, so its scope is the
        // section: its own history, and none of the coding folders.
        let loose_hits =
            search_messages_scoped(&conn, "invoice", None, Some("chat"), 10).unwrap();
        assert_eq!(loose_hits.len(), 1);
        assert_eq!(loose_hits[0].chat_id, loose.id);

        // Hostile input goes through the same quoting on this path too.
        for raw in ["\"*\"", "- : (", "**"] {
            search_messages_scoped(&conn, raw, Some("C:/client-a"), Some("coding"), 10).unwrap();
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
    fn a_resend_moves_the_tail_aside_and_can_rewrite_the_turn() {
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
        assert!(!turn.discarded, "nothing has been replaced yet");

        set_message_content(&conn, &turn.id, "edited question").unwrap();
        discard_tail(&conn, &chat.id, turn.seq).unwrap();

        let left = list_messages(&conn, &chat.id).unwrap();
        assert_eq!(left.len(), 3, "only what followed the turn is gone");
        assert_eq!(left[2].content.as_deref(), Some("edited question"));
        assert_eq!(left[1].content.as_deref(), Some("older answer"), "the older turn stays");
        // And the turn that was replaced now says so, which is what draws the
        // button that brings it back.
        assert!(left[2].discarded, "the replaced answer is offered back");

        // Nothing to re-send when there is no user turn at all.
        let empty = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        assert!(last_user_message(&conn, &empty.id).unwrap().is_none());
    }

    #[test]
    fn a_replaced_answer_is_swapped_back_not_rerun() {
        let conn = mem();
        let chat = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        let turn = insert_message(&conn, &chat.id, "user", Some("the question"), None, None, None, None)
            .unwrap();
        insert_message(&conn, &chat.id, "assistant", Some("the first answer"), None, None, None, None)
            .unwrap();

        discard_tail(&conn, &chat.id, turn.seq).unwrap();
        // The resend that follows produces the second answer.
        insert_message(&conn, &chat.id, "assistant", Some("the second answer"), None, None, None, None)
            .unwrap();
        assert_eq!(last_answer(&conn, &chat.id), "the second answer");

        restore_tail(&conn, &chat.id, turn.seq).unwrap();
        assert_eq!(last_answer(&conn, &chat.id), "the first answer");
        // The flag is on the user turn the answer belonged to, because that is
        // the row the button is drawn on — not on the answer itself.
        let rows = list_messages(&conn, &chat.id).unwrap();
        assert!(rows[0].discarded, "the replaced answer is offered back on that turn");
        assert!(!rows[1].discarded, "the answer itself carries no button");

        // And back again: Restore is a toggle, not a one-way door.
        restore_tail(&conn, &chat.id, turn.seq).unwrap();
        assert_eq!(last_answer(&conn, &chat.id), "the second answer");

        // Asking for an answer that was never replaced is an error, not a
        // silent no-op.
        assert!(restore_tail(&conn, &chat.id, 9_999).is_err());
    }

    /// The invariant that has bitten twice: an assistant turn's tool calls and
    /// the tool results answering them are one unit. Something that deletes or
    /// restores a tail has to keep them together, or the next request is a 400.
    #[test]
    fn a_tail_moves_with_its_tool_results_intact() {
        let conn = mem();
        let chat = create_chat(&conn, "coding", Some("C:/p"), "m", true, "high", "manual").unwrap();
        let turn = insert_message(&conn, &chat.id, "user", Some("go"), None, None, None, None)
            .unwrap();
        let calls = r#"[{"id":"c1","type":"function","function":{"name":"read_file","arguments":"{}"}}]"#;
        insert_message(&conn, &chat.id, "assistant", None, None, Some(calls), None, None).unwrap();
        insert_message(&conn, &chat.id, "tool", Some("the file's text"), None, None, Some("c1"), None)
            .unwrap();
        insert_message(&conn, &chat.id, "assistant", Some("done"), None, None, None, None).unwrap();

        // Neither half may be left behind on its own: a caller with no result, or
        // a result with no caller, is a request the API refuses.
        fn paired(conn: &Connection, chat_id: &str, stage: &str) {
            let rows = list_messages(conn, chat_id).unwrap();
            let callers: Vec<&str> = rows
                .iter()
                .filter(|m| m.tool_calls_json.is_some())
                .map(|m| m.id.as_str())
                .collect();
            let answers: Vec<&str> =
                rows.iter().filter_map(|m| m.tool_call_id.as_deref()).collect();
            assert_eq!(callers.len(), answers.len(), "{stage}: {rows:?}");
            if !callers.is_empty() {
                assert_eq!(answers, vec!["c1"], "{stage}: the pair travelled together");
            }
        }

        paired(&conn, &chat.id, "in the conversation");
        discard_tail(&conn, &chat.id, turn.seq).unwrap();
        paired(&conn, &chat.id, "after being discarded");
        restore_tail(&conn, &chat.id, turn.seq).unwrap();
        paired(&conn, &chat.id, "after being restored");

        // Restored: the whole tail is back, in order, exactly once.
        let back = list_messages(&conn, &chat.id).unwrap();
        assert_eq!(back.len(), 4, "{back:?}");
        assert!(back[1].tool_calls_json.is_some());
        assert_eq!(back[2].tool_call_id.as_deref(), Some("c1"));
        assert_eq!(back[3].content.as_deref(), Some("done"));
    }

    /// The last assistant row's text, for the tests above.
    fn last_answer(conn: &Connection, chat_id: &str) -> String {
        list_messages(conn, chat_id)
            .unwrap()
            .into_iter()
            .rev()
            .find(|m| m.role == "assistant")
            .and_then(|m| m.content)
            .unwrap_or_default()
    }

    #[test]
    fn a_duplicate_is_its_own_conversation() {
        let conn = mem();
        let chat =
            create_chat(&conn, "chat", None, "deepseek-flash", true, "high", "manual").unwrap();
        rename_chat(&conn, &chat.id, "Invoice").unwrap();
        add_chat_cost(&conn, &chat.id, 1.5).unwrap();
        set_chat_skills(&conn, &chat.id, &["concise".to_string()]).unwrap();
        let calls =
            r#"[{"id":"c1","type":"function","function":{"name":"read_file","arguments":"{}"}}]"#;
        insert_message(&conn, &chat.id, "user", Some("a question"), None, None, None, None).unwrap();
        insert_message(&conn, &chat.id, "assistant", None, None, Some(calls), None, None).unwrap();
        insert_message(&conn, &chat.id, "tool", Some("its text"), None, None, Some("c1"), None)
            .unwrap();

        let copy = duplicate_chat(&conn, &chat.id).unwrap().unwrap();
        assert_ne!(copy.id, chat.id);
        assert_eq!(copy.title, "Invoice (copy)");
        assert_eq!(copy.model, "deepseek-flash");
        // Nothing was paid for the copy, and the original still holds its own.
        assert_eq!(copy.cost_usd, 0.0);
        assert_eq!(get_chat(&conn, &chat.id).unwrap().cost_usd, 1.5);
        // Its own folder, decided the first time it is needed.
        assert!(copy.folder.is_none());
        // The switches travel with the conversation; the spend does not.
        assert_eq!(get_chat_skills(&conn, &copy.id).unwrap(), vec!["concise".to_string()]);

        // The transcript came across, in order, with its own row ids and its
        // tool-call pairing intact.
        let copied = list_messages(&conn, &copy.id).unwrap();
        let original = list_messages(&conn, &chat.id).unwrap();
        assert_eq!(copied.len(), 3, "{copied:?}");
        assert_eq!(copied[0].content.as_deref(), Some("a question"));
        assert!(copied[1].tool_calls_json.is_some());
        assert_eq!(copied[2].tool_call_id.as_deref(), Some("c1"));
        assert_ne!(copied[0].id, original[0].id, "the rows are its own");

        // Editing the copy leaves the original alone.
        set_message_content(&conn, &copied[0].id, "changed").unwrap();
        assert_eq!(
            list_messages(&conn, &chat.id).unwrap()[0].content.as_deref(),
            Some("a question")
        );

        assert!(duplicate_chat(&conn, "no-such-chat").unwrap().is_none());
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

    #[test]
    fn deleting_a_project_keeps_its_chats_and_unassigns_them() {
        let conn = mem();
        let project = create_project(&conn, "Client work", Some("/repo")).unwrap();
        let inside = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        let outside = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        set_chat_project(&conn, &inside.id, Some(&project.id)).unwrap();

        assert_eq!(get_chat(&conn, &inside.id).unwrap().project_id.as_deref(), Some(project.id.as_str()));
        assert!(get_chat(&conn, &outside.id).unwrap().project_id.is_none());
        assert_eq!(list_projects(&conn).unwrap().len(), 1);

        delete_project(&conn, &project.id).unwrap();

        // The container goes; the conversation does not. It is only unassigned,
        // which is the whole reason deleting a project needs no confirmation.
        assert!(list_projects(&conn).unwrap().is_empty());
        assert!(get_chat(&conn, &inside.id).unwrap().project_id.is_none());
        assert!(get_chat(&conn, &outside.id).is_ok());
    }

    #[test]
    fn renaming_a_project_keeps_an_empty_name_from_hiding_it() {
        let conn = mem();
        let project = create_project(&conn, "  ", None).unwrap();
        assert_eq!(project.name, "New project");
        rename_project(&conn, &project.id, "   ").unwrap();
        assert_eq!(list_projects(&conn).unwrap()[0].name, "New project");
        rename_project(&conn, &project.id, "Real name").unwrap();
        assert_eq!(list_projects(&conn).unwrap()[0].name, "Real name");
    }

    #[test]
    fn a_projects_instructions_round_trip_without_resorting_the_list() {
        let conn = mem();
        let project = create_project(&conn, "Client work", Some("/repo")).unwrap();
        assert!(project.instructions.is_empty());

        let rules = vec![
            Instruction {
                id: "tone".to_string(),
                name: "Tone".to_string(),
                scope: "both".to_string(),
                body: "Answer in Dutch.".to_string(),
                enabled: true,
            },
            Instruction {
                id: "parked".to_string(),
                name: "Parked".to_string(),
                scope: "coding".to_string(),
                body: "Not yet.".to_string(),
                enabled: false,
            },
        ];
        let before = get_project(&conn, &project.id).unwrap().unwrap().updated_at;
        set_project_instructions(&conn, &project.id, &rules).unwrap();

        let read = get_project(&conn, &project.id).unwrap().unwrap();
        assert_eq!(read.instructions.len(), 2);
        assert_eq!(read.instructions[0].name, "Tone");
        assert!(!read.instructions[1].enabled);
        // The list is ordered by `updated_at`, so an edit that bumped it would
        // reshuffle the projects every time a rule was typed.
        assert_eq!(read.updated_at, before, "set_project_instructions bumped updated_at");
    }

    #[test]
    fn project_spend_adds_up_a_projects_chats_and_leaves_others_out() {
        let conn = mem();
        let project = create_project(&conn, "Client work", None).unwrap();
        let inside_a = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        let inside_b = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        let outside = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        set_chat_project(&conn, &inside_a.id, Some(&project.id)).unwrap();
        set_chat_project(&conn, &inside_b.id, Some(&project.id)).unwrap();

        // One-off costs, so the total is arithmetic that cannot drift.
        for (chat, cost) in [(&inside_a, 0.25), (&inside_b, 0.75), (&outside, 9.0)] {
            record_turn_usage(
                &conn,
                &chat.id,
                &uuid::Uuid::new_v4().to_string(),
                &TurnUsage {
                    model: "m",
                    prompt_tokens: 10,
                    completion_tokens: 5,
                    cache_hit_tokens: 0,
                    cache_miss_tokens: 0,
                    cost_usd: cost,
                },
            )
            .unwrap();
        }

        let spend = project_spend(&conn).unwrap();
        assert_eq!(spend.len(), 1, "only the project with a member should appear");
        assert_eq!(spend[0].project_id, project.id);
        assert_eq!(spend[0].turns, 2);
        assert!((spend[0].cost_usd - 1.0).abs() < 1e-9, "got {}", spend[0].cost_usd);
    }

    #[test]
    fn only_tasks_whose_time_has_come_are_due() {
        let conn = mem();
        let soon = create_scheduled(
            &conn, "Morning brief", "summarise the news", None, None, "daily", "09:00", 0, 1, 1_000,
        )
        .unwrap();
        let later = create_scheduled(
            &conn, "Weekly review", "check the books", None, None, "weekly", "17:00", 2, 1, 9_000,
        )
        .unwrap();

        let due = due_scheduled(&conn, 5_000).unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, soon.id);
        assert!(due_scheduled(&conn, 0).unwrap().is_empty());
        // Both, oldest first.
        let both = due_scheduled(&conn, 10_000).unwrap();
        assert_eq!(
            both.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            vec![soon.id.as_str(), later.id.as_str()]
        );
    }

    #[test]
    fn a_disabled_task_is_never_due() {
        let conn = mem();
        let task = create_scheduled(
            &conn, "Later", "do a thing", None, None, "daily", "09:00", 0, 1, 1,
        )
        .unwrap();
        update_scheduled(
            &conn, &task.id, "Later", "do a thing", None, None, "daily", "09:00", 0, 1, false, 1,
        )
        .unwrap();
        assert!(due_scheduled(&conn, 999_999).unwrap().is_empty());
        assert!(!list_scheduled(&conn).unwrap()[0].enabled);
    }

    #[test]
    fn advancing_a_task_records_the_run_and_moves_it_on() {
        let conn = mem();
        let task = create_scheduled(
            &conn, "Brief", "p", None, None, "daily", "09:00", 0, 1, 1_000,
        )
        .unwrap();
        assert!(task.last_run_at.is_none());

        mark_scheduled_ran(&conn, &task.id, 86_401_000, 1_000).unwrap();

        let after = &list_scheduled(&conn).unwrap()[0];
        assert_eq!(after.next_run_at, 86_401_000);
        assert_eq!(after.last_run_at, Some(1_000));
        // Already advanced, so the same due check does not fire it again.
        assert!(due_scheduled(&conn, 1_001).unwrap().is_empty());
    }

    #[test]
    fn updating_a_task_moves_its_next_run_and_keeps_one_row() {
        let conn = mem();
        let task = create_scheduled(
            &conn, "Old", "old prompt", None, None, "daily", "09:00", 0, 1, 1_000,
        )
        .unwrap();
        update_scheduled(
            &conn, &task.id, "New", "new prompt", None, None, "weekly", "08:30", 2, 1, true,
            555_000,
        )
        .unwrap();

        let all = list_scheduled(&conn).unwrap();
        assert_eq!(all.len(), 1, "an update must not add a second task");
        assert_eq!(all[0].title, "New");
        assert_eq!(all[0].prompt, "new prompt");
        assert_eq!(all[0].freq, "weekly");
        assert_eq!(all[0].time, "08:30");
        assert_eq!(all[0].weekday, 2);
        assert_eq!(all[0].next_run_at, 555_000);

        delete_scheduled(&conn, &task.id).unwrap();
        assert!(list_scheduled(&conn).unwrap().is_empty());
    }

    #[test]
    fn a_task_with_no_title_still_shows_up_in_the_list() {
        let conn = mem();
        create_scheduled(&conn, "  ", "p", None, None, "daily", "09:00", 0, 1, 1).unwrap();
        assert_eq!(list_scheduled(&conn).unwrap()[0].title, "Scheduled task");
    }

    fn round(model: &str, prompt: u32, completion: u32, cost: f64) -> TurnUsage<'_> {
        TurnUsage {
            model,
            prompt_tokens: prompt,
            completion_tokens: completion,
            cache_hit_tokens: 0,
            cache_miss_tokens: prompt,
            cost_usd: cost,
        }
    }

    #[test]
    fn every_round_of_a_turn_lands_in_one_row() {
        let conn = mem();
        let chat = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();

        // Three rounds of one turn: two tool loops and the answer.
        record_turn_usage(&conn, &chat.id, "run-1", &round("m", 100, 20, 0.25)).unwrap();
        record_turn_usage(&conn, &chat.id, "run-1", &round("m", 200, 30, 0.5)).unwrap();
        record_turn_usage(&conn, &chat.id, "run-1", &round("m", 50, 10, 0.25)).unwrap();

        let report = spend_report(&conn, 0, 10).unwrap();
        assert_eq!(report.turns, 1, "three rounds of one turn must not be three turns");
        assert_eq!(report.prompt_tokens, 350);
        assert_eq!(report.completion_tokens, 60);
        assert_eq!(report.total_usd, 1.0);

        // The round count is what says the row is a turn and not a call.
        let rounds: i64 = conn
            .query_row("SELECT rounds FROM turn_usage", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rounds, 3);
    }

    #[test]
    fn spend_is_grouped_by_day_model_and_chat() {
        let conn = mem();
        let chat = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        let now = now_ts();

        // Rows days apart, so the local-day boundaries never enter into it: two
        // turns today, one three days ago, one outside a 30-day window. The costs
        // are exact binary fractions, so the sums can be compared exactly.
        for (run, days_ago, model, cost) in [
            ("run-1", 0, "flash", 0.5),
            ("run-2", 0, "pro", 0.25),
            ("run-3", 3, "pro", 0.125),
            ("run-4", 40, "flash", 1.0),
        ] {
            record_turn_usage(&conn, &chat.id, run, &round(model, 1_000, 100, cost)).unwrap();
            conn.execute(
                "UPDATE turn_usage SET created_at = ?2 WHERE run_id = ?1",
                params![run, now - days_ago * 86_400],
            )
            .unwrap();
        }

        // Everything, and then the same thing through a 30-day window.
        let all = spend_report(&conn, 0, 10).unwrap();
        assert_eq!(all.turns, 4);
        assert_eq!(all.days.len(), 3, "today, three days ago, and 40 days ago");
        assert_eq!(all.total_usd, 1.875);

        let windowed = spend_report(&conn, now - 30 * 86_400, 10).unwrap();
        assert_eq!(windowed.turns, 3, "the 40-day-old turn is out of the window");
        assert_eq!(windowed.days.len(), 2);
        assert_eq!(windowed.total_usd, 0.875);
        // Newest day first, so the panel does not have to sort.
        assert!(windowed.days[0].day > windowed.days[1].day);

        // Per model, dearest first: one `flash` turn (0.5) beats two `pro` ones.
        assert_eq!(windowed.models.len(), 2);
        assert_eq!(windowed.models[0].model, "flash");
        assert_eq!(windowed.models[0].cost_usd, 0.5);
        assert_eq!(windowed.models[1].turns, 2);

        // Per chat, with the title a person recognises.
        assert_eq!(windowed.chats.len(), 1);
        assert_eq!(windowed.chats[0].chat_id, chat.id);
        assert_eq!(windowed.chats[0].turns, 3);
    }

    /// Walk a payload and fail on any snake_case key.
    fn assert_camel_case_keys(value: &serde_json::Value, path: &str) {
        match value {
            serde_json::Value::Object(map) => {
                for (key, child) in map {
                    assert!(!key.contains('_'), "{path}.{key} is snake_case on the wire");
                    assert_camel_case_keys(child, &format!("{path}.{key}"));
                }
            }
            serde_json::Value::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    assert_camel_case_keys(item, &format!("{path}[{i}]"));
                }
            }
            _ => {}
        }
    }

    #[test]
    fn the_spend_payload_is_camel_case_on_the_wire() {
        // The spend structs are `rename_all = "camelCase"`, so the frontend is
        // written against `totalUsd`, not `total_usd`. When it asked for the
        // latter it got `undefined`, formatted it, and took the whole window
        // down — a TypeScript type cannot check the wire, so the wire is
        // asserted here instead.
        let report = SpendReport {
            since: 1,
            total_usd: 1.25,
            turns: 3,
            prompt_tokens: 900,
            completion_tokens: 60,
            days: vec![DaySpend {
                day: "2026-10-12".into(),
                cost_usd: 1.25,
                turns: 3,
                prompt_tokens: 900,
                completion_tokens: 60,
            }],
            models: vec![ModelSpend {
                model: "flash".into(),
                cost_usd: 1.25,
                turns: 3,
            }],
            chats: vec![ChatSpend {
                chat_id: "chat-1".into(),
                title: "A conversation".into(),
                cost_usd: 1.25,
                turns: 3,
            }],
        };

        let value = serde_json::to_value(&report).unwrap();
        // Named keys first, so the walk below cannot pass over an empty object.
        assert_eq!(value["totalUsd"], serde_json::json!(1.25));
        assert_eq!(value["promptTokens"], serde_json::json!(900));
        assert_eq!(value["days"][0]["costUsd"], serde_json::json!(1.25));
        assert_eq!(value["chats"][0]["chatId"], serde_json::json!("chat-1"));
        assert_camel_case_keys(&value, "report");
    }

    #[test]
    fn an_older_database_gains_the_usage_table() {
        // The new table arrives with `CREATE TABLE IF NOT EXISTS` inside the
        // schema batch rather than from an ALTER, so it is worth proving the
        // batch still applies to a file written before it existed — the batch is
        // all-or-nothing, and one bad statement would take every table with it.
        let dir = std::env::temp_dir().join(format!("deskseek-usage-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("deskseek.db");
        {
            let old = Connection::open(&path).unwrap();
            old.execute_batch(
                "CREATE TABLE chats (
                    id           TEXT PRIMARY KEY,
                    title        TEXT NOT NULL,
                    kind         TEXT NOT NULL DEFAULT 'chat',
                    workspace    TEXT,
                    model        TEXT NOT NULL,
                    thinking     INTEGER NOT NULL DEFAULT 1,
                    effort       TEXT NOT NULL DEFAULT 'high',
                    permission_mode TEXT NOT NULL DEFAULT 'manual',
                    cost_usd     REAL NOT NULL DEFAULT 0,
                    agent_mode   TEXT NOT NULL DEFAULT 'standard',
                    archived     INTEGER NOT NULL DEFAULT 0,
                    project_id   TEXT,
                    folder       TEXT,
                    created_at   INTEGER NOT NULL,
                    updated_at   INTEGER NOT NULL
                );
                INSERT INTO chats (id, title, kind, model, created_at, updated_at)
                    VALUES ('old-1', 'From before usage existed', 'chat', 'flash', 1, 1);",
            )
            .unwrap();
        }

        let conn = open(&path).expect("an older database must still open");

        let chat = get_chat(&conn, "old-1").unwrap();
        record_turn_usage(&conn, &chat.id, "run-1", &round("flash", 10, 5, 0.25)).unwrap();
        let report = spend_report(&conn, 0, 10).unwrap();
        assert_eq!(report.turns, 1);
        assert_eq!(report.total_usd, 0.25);
        // The chat's own title comes from the join, which also proves `chats`
        // survived the batch on this file.
        assert_eq!(report.chats[0].title, "From before usage existed");

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
