//! SQLite persistence for chats and messages.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::settings::Instruction;
use crate::triggers;

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
    /// This chat does not feed memory, and its prompt carries none. Handled by
    /// `memory::enabled` so the read and the write agree.
    pub incognito: bool,
    /// What this chat may send out: `all` (nothing declared), `read-only` or
    /// `none`. See `tools::guard::Egress` — it is consulted before the mode and
    /// before the user is asked.
    pub egress: String,
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
    -- This chat does not feed memory: `save_memory` writes nothing from it and
    -- its prompt carries no memory at all — one rule, in `memory::enabled`.
    -- Added by ALTER for databases written before the switch existed. Not
    -- activity, so flipping it does not bump `updated_at`.
    incognito    INTEGER NOT NULL DEFAULT 0,
    -- What this chat may send out: 'all' (nothing declared), 'read-only' (it may
    -- read the web, not send to it) or 'none'. See tools::guard::Egress. Added by
    -- ALTER for databases written before the switch existed.
    egress       TEXT NOT NULL DEFAULT 'all',
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
    -- What the grant covers, when the tool acts on something nameable: a site, a
    -- program, a folder inside the workspace, an app. Empty means the whole tool,
    -- which is what every grant written before targets existed meant.
    target     TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (workspace, tool, target)
);

-- One line per decision the app made on the user's behalf, written as it
-- happens. `tool_permissions` answers "what is switched on now"; this answers
-- "what happened then" — and the *answer* is the part that leaves no trace
-- anywhere else, because a denial and a guard refusal both produce no tool
-- result of their own. The calls themselves are already in the transcript; this
-- is the other half.
--
-- `chat_id` deliberately has **no foreign key**, alone among these tables: the
-- record has to outlive the thing it was about, or deleting a chat would erase
-- the evidence of what it was allowed to do. The join is a LEFT one for the same
-- reason, and a gone chat simply reads as unnamed.
--
-- `args_digest` is a hash rather than the arguments: an approval can cover a
-- command carrying a secret, and an audit trail is not the place to keep one. It
-- is enough to tell two approvals of the same tool apart.
CREATE TABLE IF NOT EXISTS approval_log (
    id          TEXT PRIMARY KEY,
    chat_id     TEXT NOT NULL,
    tool        TEXT NOT NULL,
    -- `allowed` | `denied` | `blocked` (the guard refused without asking).
    decision    TEXT NOT NULL,
    -- How or why: `once`, `session`, `always`, `stopped`, `denied`, or the
    -- guard's own reason for a block.
    note        TEXT NOT NULL DEFAULT '',
    args_digest TEXT NOT NULL DEFAULT '',
    workspace   TEXT NOT NULL DEFAULT '',
    created_at  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_approval_log_time ON approval_log(created_at);

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
-- The run a chat has in flight right now, if any. `runs.rs` holds the same fact
-- in memory so the stop button has something to pull, but memory dies with the
-- process — a build that is killed mid-run would otherwise leave no trace of
-- what it was doing. This row is that trace: written when a run starts, deleted
-- when it ends however it ends (finished, stopped, errored), so a row still
-- there at startup is a turn that never got to close. One row per chat, matching
-- `runs.rs`'s "one live run per chat".
CREATE TABLE IF NOT EXISTS in_flight_runs (
    chat_id    TEXT PRIMARY KEY REFERENCES chats(id) ON DELETE CASCADE,
    run_id     TEXT NOT NULL,
    started_at INTEGER NOT NULL
);

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
    -- Who spent it: 'turn' for a turn the user typed (or a task ran), 'summary'
    -- for the summariser, 'revision' for an in-place rewrite. A call the user did
    -- not type is still a call the user paid for, so it is a row in here rather
    -- than a number nobody sees. Added by ALTER for databases written before the
    -- overhead calls were priced.
    kind              TEXT NOT NULL DEFAULT 'turn',
    -- Whether the money on this row is known. An endpoint with no published rate
    -- records its tokens with this 0: the tokens happened, the price did not, and
    -- a bill that quietly drops them would be the second, quieter story this
    -- table exists to prevent.
    priced            INTEGER NOT NULL DEFAULT 1,
    -- API round-trips inside this turn: 3 means two tool loops and a final answer.
    rounds            INTEGER NOT NULL DEFAULT 1,
    prompt_tokens     INTEGER NOT NULL DEFAULT 0,
    completion_tokens INTEGER NOT NULL DEFAULT 0,
    -- Kept apart from the prompt total because they are billed at different
    -- rates; `prompt_tokens` is the sum of these two when the API reports both.
    cache_hit_tokens  INTEGER NOT NULL DEFAULT 0,
    cache_miss_tokens INTEGER NOT NULL DEFAULT 0,
    cost_usd          REAL NOT NULL DEFAULT 0,
    -- The scheduled task that started this turn, or NULL for one the user began.
    -- On the usage row rather than in a table of its own on purpose: a task's
    -- spend and the spend view are then the same rows, so they cannot tell two
    -- stories. Added by ALTER for databases written before the scheduler could
    -- tag a run; NULL is right for every existing row, they were all typed.
    task_id           TEXT,
    created_at        INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_turn_usage_time ON turn_usage(created_at);
CREATE INDEX IF NOT EXISTS idx_turn_usage_chat ON turn_usage(chat_id, created_at);
-- Over an ALTER-added column, which is only safe because the ALTERs run before
-- this batch — see `open`. Removing a task leaves its runs behind (no foreign
-- key), so a task's history outlives the task, exactly like the decision log.
CREATE INDEX IF NOT EXISTS idx_turn_usage_task ON turn_usage(task_id, created_at);

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
    created_at  INTEGER NOT NULL,
    -- What makes this task due. 'cadence' is the calendar in `schedule.rs`;
    -- 'file' and 'git' wait for something to change instead.
    kind        TEXT NOT NULL DEFAULT 'cadence',
    -- What a trigger watches: a folder, or a repository.
    target      TEXT NOT NULL DEFAULT '',
    -- Glob, or ref name. Blank means "any file" and "HEAD".
    filter      TEXT NOT NULL DEFAULT '',
    -- Seconds between a trigger's checks. Read only by a trigger.
    interval_secs INTEGER NOT NULL DEFAULT 300,
    -- The last thing a trigger saw. NULL means it has never looked, and that is
    -- what makes a first look arm rather than fire.
    last_state  TEXT,
    last_checked_at INTEGER,
    -- What the last check found or did, in words, for the panel.
    last_note   TEXT NOT NULL DEFAULT '',
    -- The model this task runs on, or '' for the chat's own. A pin is a model id
    -- and nothing more: the endpoint is whatever is default when the task fires,
    -- because a task outlives any particular one. Appended last on purpose, so a
    -- database written before this column and a fresh one have it in the same
    -- place — the ALTER can only ever append.
    model       TEXT NOT NULL DEFAULT ''
);

CREATE INDEX IF NOT EXISTS idx_scheduled_due ON scheduled_tasks(next_run_at);

-- A request a run parked on — an approval or a question — recorded so it can be
-- answered from the Scheduled panel instead of in a chat nobody is looking at.
--
-- `payload` is the exact event the chat was sent, so the panel draws the same
-- prompt the composer would have rather than a second description of it that
-- could drift. A row lives exactly as long as the park: answered, denied or
-- cancelled it is deleted, and one a crash left behind is marked `lost` on the
-- next launch — the run it belonged to is gone, so there is nothing left to
-- resume, but the user still gets to see what it wanted.
CREATE TABLE IF NOT EXISTS parked_requests (
    id         TEXT PRIMARY KEY,
    chat_id    TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
    -- `approval` | `question`
    kind       TEXT NOT NULL,
    payload    TEXT NOT NULL DEFAULT '{}',
    -- `pending` | `lost`
    status     TEXT NOT NULL DEFAULT 'pending',
    created_at INTEGER NOT NULL
);

-- A notebook: a title, its own rules, and its own folder of sources. It is not a
-- chat and deliberately has no row in `chats` — a notebook's question is not a
-- turn, and giving it a chat would make every surface that reads a chat (the
-- sidebar, the spend rows, the scheduler, memory) learn about a second kind of
-- thing for no gain. Its sources are files in `notebooks/<id>/sources`, so the
-- notebook owns its copies and deleting it takes them with it.
CREATE TABLE IF NOT EXISTS notebooks (
    id           TEXT PRIMARY KEY,
    title        TEXT NOT NULL,
    -- The notebook's own instruction blocks, in the same shape a project's rules
    -- take, so the same editor and the same rule about which of them apply are
    -- reused rather than reinvented. A malformed blob reads as none.
    instructions TEXT NOT NULL DEFAULT '[]',
    -- The model a question in this notebook runs on, or '' for the chat default —
    -- the same "a pin is a model id and nothing more" rule a scheduled task uses.
    -- Appended last on purpose, so a database written before this column and a
    -- fresh one hold it in the same place.
    model        TEXT NOT NULL DEFAULT '',
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL
);

-- One document a notebook holds.
--
-- `fingerprint` is what the *digest* describes — the file's size and mtime when
-- the digest was made — and it is the single answer to two questions: whether the
-- source has changed since (a changed file reads as stale rather than being
-- silently reused) and whether the digest is worth paying for again. One column
-- because it is one fact, and two would be two answers that can disagree.
--
-- `extract` is the source's text, and it is the one copy: the chunk index 5.4
-- builds is derived from it and rebuildable, which is deliberately the same
-- relationship `messages_fts` has with `messages`. `enabled` is filtered at read
-- time and never deleted, which is what makes the switch reversible.
CREATE TABLE IF NOT EXISTS sources (
    id          TEXT PRIMARY KEY,
    notebook_id TEXT NOT NULL REFERENCES notebooks(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    -- `file` | `url`
    kind        TEXT NOT NULL DEFAULT 'file',
    -- Where it came from: the path the user picked, or the URL it was read from.
    origin      TEXT NOT NULL DEFAULT '',
    -- The copy this notebook owns, under its own sources folder.
    path        TEXT,
    enabled     INTEGER NOT NULL DEFAULT 1,
    extract     TEXT,
    -- What this source contains, in the model's words when one could be reached
    -- and in the derived outline's when none could — never empty for a source
    -- that has been added, because the catalogue line is what the model reads.
    digest      TEXT NOT NULL DEFAULT '',
    digest_at   INTEGER,
    fingerprint TEXT NOT NULL DEFAULT '',
    created_at  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sources_notebook ON sources(notebook_id, enabled, created_at);

-- One question asked of a notebook and the answer it got. The answer's citations
-- are stored as the labels that resolved, so a later reader sees what the model
-- actually stood behind rather than re-parsing prose.
CREATE TABLE IF NOT EXISTS notebook_messages (
    id             TEXT PRIMARY KEY,
    notebook_id    TEXT NOT NULL REFERENCES notebooks(id) ON DELETE CASCADE,
    seq            INTEGER NOT NULL,
    -- `user` | `assistant`
    role           TEXT NOT NULL,
    content        TEXT NOT NULL,
    citations_json TEXT,
    created_at     INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_notebook_messages ON notebook_messages(notebook_id, seq);

-- Something a notebook produced: a flashcard set, a briefing, a spreadsheet. The
-- target is a written file's path or an artifact's id, because both writers
-- already exist and a second kind of output store would be a second answer to
-- where an output lives.
CREATE TABLE IF NOT EXISTS notebook_outputs (
    id          TEXT PRIMARY KEY,
    notebook_id TEXT NOT NULL REFERENCES notebooks(id) ON DELETE CASCADE,
    kind        TEXT NOT NULL,
    title       TEXT NOT NULL,
    target      TEXT NOT NULL DEFAULT '',
    created_at  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_notebook_outputs ON notebook_outputs(notebook_id, created_at);

-- One chunk of one document, so a question can be answered from the part that
-- bears on it rather than from everything in the folder.
--
-- `root` is the folder the walk started from, and it is what scopes a search: a
-- chat searches its own folder and its workspace, a notebook searches its own
-- sources folder, and neither can see the other's rows.
--
-- `text` is the one copy of the chunk. The FTS index beside it is external-content
-- over this table, exactly like `messages_fts` over `messages`, so reindexing is
-- always possible from the rows themselves and no trigger can leave the index
-- saying something the table does not.
--
-- `fingerprint` is the file's size and mtime when these chunks were made, which is
-- the same key a source's digest uses: one rule for "has this file moved", read by
-- the index to decide whether a file is worth reading again.
CREATE TABLE IF NOT EXISTS chunks (
    id          TEXT PRIMARY KEY,
    root        TEXT NOT NULL,
    path        TEXT NOT NULL,
    -- The heading the chunk sits under, or '' for a document's preamble.
    heading     TEXT NOT NULL DEFAULT '',
    ordinal     INTEGER NOT NULL,
    text        TEXT NOT NULL,
    fingerprint TEXT NOT NULL DEFAULT '',
    indexed_at  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_chunks_path ON chunks(path);
CREATE INDEX IF NOT EXISTS idx_chunks_root ON chunks(root, path);

CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
    text,
    content='chunks',
    content_rowid='rowid'
);

CREATE TRIGGER IF NOT EXISTS chunks_ai AFTER INSERT ON chunks BEGIN
    INSERT INTO chunks_fts(rowid, text) VALUES (new.rowid, new.text);
END;

CREATE TRIGGER IF NOT EXISTS chunks_ad AFTER DELETE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, text) VALUES ('delete', old.rowid, old.text);
END;

CREATE TRIGGER IF NOT EXISTS chunks_au AFTER UPDATE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, text) VALUES ('delete', old.rowid, old.text);
    INSERT INTO chunks_fts(rowid, text) VALUES (new.rowid, new.text);
END;
"#;

/// The column names of a table, or nothing when the table does not exist yet.
///
/// `PRAGMA` rather than a probe query, because the two answers this has to tell
/// apart — "no such table" and "no such column" — are the same error from a
/// probe, and an upgrade path that confuses them either rebuilds a table it
/// should not or misses the one it should.
fn table_columns(conn: &Connection, table: &str) -> Vec<String> {
    let mut stmt = match conn.prepare(&format!("PRAGMA table_info({table})")) {
        Ok(stmt) => stmt,
        Err(_) => return Vec::new(),
    };
    let rows = stmt.query_map([], |r| r.get::<_, String>(1));
    rows.map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default()
}

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
    // A chat that does not feed memory arrived with 8.3. An older database gets 0
    // for every chat, which is exactly "nothing was private before".
    let _ = conn.execute(
        "ALTER TABLE chats ADD COLUMN incognito INTEGER NOT NULL DEFAULT 0",
        [],
    );
    // A chat's declared network policy arrived with 9.3. An older database gets
    // 'all', which is exactly "nothing was declared before".
    let _ = conn.execute(
        "ALTER TABLE chats ADD COLUMN egress TEXT NOT NULL DEFAULT 'all'",
        [],
    );
    // Tagging a turn with the scheduled task that started it arrived with 8.6.
    // NULL for every existing row, which is right: they were all typed by hand.
    let _ = conn.execute("ALTER TABLE turn_usage ADD COLUMN task_id TEXT", []);
    // Saying who spent a row, and whether its price is known, arrived with 9.5.
    // Every existing row is a turn the user typed with a price we do publish,
    // which is exactly what the two defaults say.
    let _ = conn.execute(
        "ALTER TABLE turn_usage ADD COLUMN kind TEXT NOT NULL DEFAULT 'turn'",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE turn_usage ADD COLUMN priced INTEGER NOT NULL DEFAULT 1",
        [],
    );
    // A project's own instruction blocks arrived with 2.7. An older database
    // gets an empty array, which is exactly "no project rules yet".
    let _ = conn.execute(
        "ALTER TABLE projects ADD COLUMN instructions TEXT NOT NULL DEFAULT '[]'",
        [],
    );
    // Triggers arrived with 7.2. Every task an older database holds is a cadence,
    // which is exactly what the default says, and none of them has a state —
    // "never looked" being the thing that arms a trigger rather than an empty
    // string that would read as a fingerprint nobody could match.
    let _ = conn.execute(
        "ALTER TABLE scheduled_tasks ADD COLUMN kind TEXT NOT NULL DEFAULT 'cadence'",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE scheduled_tasks ADD COLUMN target TEXT NOT NULL DEFAULT ''",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE scheduled_tasks ADD COLUMN filter TEXT NOT NULL DEFAULT ''",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE scheduled_tasks ADD COLUMN interval_secs INTEGER NOT NULL DEFAULT 300",
        [],
    );
    let _ = conn.execute("ALTER TABLE scheduled_tasks ADD COLUMN last_state TEXT", []);
    let _ = conn.execute("ALTER TABLE scheduled_tasks ADD COLUMN last_checked_at INTEGER", []);
    let _ = conn.execute(
        "ALTER TABLE scheduled_tasks ADD COLUMN last_note TEXT NOT NULL DEFAULT ''",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE scheduled_tasks ADD COLUMN model TEXT NOT NULL DEFAULT ''",
        [],
    );

    // `tool_permissions` gained a `target` in 9.2, and SQLite cannot widen a
    // primary key in place: the old key was (workspace, tool) and the new one is
    // the triple. So a database written before it is renamed out of the way here,
    // the schema batch below makes the new shape, and the rows are carried over
    // just after as whole-tool grants — which is exactly what they meant. A
    // database that already has the column, and a fresh one, skip every step.
    let carrying_grants = {
        let columns = table_columns(&conn, "tool_permissions");
        !columns.is_empty() && !columns.iter().any(|c| c == "target")
    };
    if carrying_grants {
        conn.execute("ALTER TABLE tool_permissions RENAME TO tool_permissions_legacy", [])?;
    }

    conn.execute_batch(SCHEMA)?;

    if carrying_grants {
        conn.execute_batch(
            "INSERT INTO tool_permissions (workspace, tool, target, created_at)
                 SELECT workspace, tool, '', created_at FROM tool_permissions_legacy;
             DROP TABLE tool_permissions_legacy;",
        )?;
    }

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

/// Flushes the write-ahead log into the main file.
///
/// WAL mode keeps recent commits in `deskseek.db-wal`, so a copy of just the
/// database file can be missing everything since the last checkpoint — which is
/// exactly the writing a backup is meant to preserve.
pub fn checkpoint(conn: &Connection) {
    let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
}

/// Puts a restored database file in place of the live one.
///
/// The connection cannot simply be reopened on a file that was replaced
/// underneath it: on Windows an open file cannot be overwritten at all, and the
/// connection is held for the life of the app. So it is parked on an in-memory
/// database first — which releases the handle — then the file and its WAL
/// sidecars are swapped, and the restored file is opened in its place.
///
/// The caller must hold the `db` lock: for the moment between the park and the
/// reopen, this connection is not the profile.
pub fn swap_file(conn: &mut Connection, live: &Path, restored: &Path) -> Result<(), String> {
    checkpoint(conn);
    *conn = Connection::open_in_memory().map_err(|e| e.to_string())?;
    // The sidecars belong to the database that is going away; left behind they
    // would be replayed over the restored file.
    for suffix in ["-wal", "-shm"] {
        let sidecar = Path::new(&format!("{}{suffix}", live.display())).to_path_buf();
        let _ = std::fs::remove_file(sidecar);
    }
    std::fs::copy(restored, live)
        .map_err(|e| format!("could not restore the database: {e}"))?;
    *conn = open(live).map_err(|e| format!("the restored database could not be opened: {e}"))?;
    Ok(())
}

/// Every chat of a section, archived ones included: the sidebar is what splits
/// them, and a tab restored from the last session has to find its chat even when
/// it is archived.
pub fn list_chats(conn: &Connection, kind: &str) -> rusqlite::Result<Vec<ChatRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, kind, workspace, model, thinking, effort, permission_mode, cost_usd, created_at, updated_at, archived, agent_mode, project_id, folder, incognito, egress
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
            incognito: r.get::<_, i64>(15)? != 0,
            egress: r.get(16)?,
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
        incognito: false,
        egress: "all".to_string(),
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
    // `incognito` comes with it: the copy is the same conversation, so it must
    // not quietly start feeding memory when the original does not. The declared
    // network policy comes with it for the same reason — a copy of an off-the-grid
    // chat must not be the one that starts talking.
    conn.execute(
        "UPDATE chats SET agent_mode = ?2, project_id = ?3, incognito = ?4, egress = ?5 WHERE id = ?1",
        params![
            copy.id,
            source.agent_mode,
            source.project_id,
            source.incognito as i64,
            source.egress
        ],
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
        "SELECT id, title, kind, workspace, model, thinking, effort, permission_mode, cost_usd, created_at, updated_at, archived, agent_mode, project_id, folder, incognito, egress
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
                incognito: r.get::<_, i64>(15)? != 0,
                egress: r.get(16)?,
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

/// Turns "this chat does not feed memory" on or off. Like archiving, the switch
/// is not activity, so `updated_at` is left alone and the sidebar keeps its order.
pub fn set_chat_incognito(conn: &Connection, id: &str, incognito: bool) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE chats SET incognito = ?2 WHERE id = ?1",
        params![id, incognito as i64],
    )?;
    Ok(())
}

/// What this chat is allowed to send out — `all`, `read-only` or `none`. Not
/// activity either, so `updated_at` stays where it is.
pub fn set_chat_egress(conn: &Connection, id: &str, egress: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE chats SET egress = ?2 WHERE id = ?1",
        params![id, egress],
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
///
/// Returns how many went, so the note that replaces them can say how much was
/// folded rather than leaving the user to wonder where the history went.
pub fn delete_messages(conn: &Connection, chat_id: &str) -> rusqlite::Result<usize> {
    conn.execute("DELETE FROM messages WHERE chat_id = ?1", [chat_id])
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
    offset: i64,
    limit: i64,
) -> rusqlite::Result<Vec<MessageHit>> {
    search_messages_scoped(conn, query, None, None, offset, limit)
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
///
/// `offset` pages through a common word's results, which is why the ordering has
/// a tiebreak: `bm25` alone leaves equally-scored rows in whatever order the
/// query planner happens to walk, so page two could repeat a row from page one
/// and drop another entirely. `m.id` is arbitrary but *stable*, which is the
/// only property paging needs.
pub fn search_messages_scoped(
    conn: &Connection,
    query: &str,
    workspace: Option<&str>,
    kind: Option<&str>,
    offset: i64,
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
         ORDER BY bm25(messages_fts), m.id
         LIMIT ?4 OFFSET ?5",
    )?;
    let rows = stmt.query_map(params![expr, workspace, kind, limit, offset.max(0)], |r| {
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

/// Records that a tool may run without asking, in one folder, for one target.
///
/// `target` is what the call acts on — a site, a program, a folder, an app — and
/// an empty one means the whole tool. The triple is the key, so "always allow
/// `cargo`" and "always allow `npm`" are two grants that do not overwrite each
/// other, and neither one answers for the other.
pub fn allow_tool(
    conn: &Connection,
    workspace: &str,
    tool: &str,
    target: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO tool_permissions (workspace, tool, target, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![workspace, tool, target, now_ts()],
    )?;
    Ok(())
}

pub fn revoke_tool(
    conn: &Connection,
    workspace: &str,
    tool: &str,
    target: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM tool_permissions WHERE workspace = ?1 AND tool = ?2 AND target = ?3",
        params![workspace, tool, target],
    )?;
    Ok(())
}

/// Whether a call is already covered by a grant.
///
/// A grant with an empty target covers the whole tool and therefore every
/// target; a grant with one covers exactly its own and nothing else. `target` is
/// this call's own, empty when it has none — so a call that names no place
/// answers only to a whole-tool grant, which is what stops a grant for one
/// folder from covering a command.
pub fn is_tool_allowed(
    conn: &Connection,
    workspace: &str,
    tool: &str,
    target: &str,
) -> rusqlite::Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tool_permissions
          WHERE workspace = ?1 AND tool = ?2 AND (target = '' OR target = ?3)",
        params![workspace, tool, target],
        |r| r.get(0),
    )?;
    Ok(count > 0)
}

pub fn list_allowed_tools(conn: &Connection, workspace: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT tool FROM tool_permissions WHERE workspace = ?1 ORDER BY tool",
    )?;
    let rows = stmt.query_map([workspace], |r| r.get::<_, String>(0))?;
    rows.collect()
}

/// Every grant there is, as `(workspace, tool, target)`. The list above answers
/// "what may this folder do"; nothing answered "what have I ever allowed", which
/// is the question a user asks when they want it taken back.
pub fn all_allowed_tools(conn: &Connection) -> rusqlite::Result<Vec<(String, String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT workspace, tool, target FROM tool_permissions ORDER BY workspace, tool, target",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
    rows.collect()
}

// ------------------------------------------------------------- approvals log

/// A decision as it is written down: `allowed`, `denied` or `blocked`.
///
/// A bare string rather than an enum, because it only ever crosses to the UI and
/// the three words are the schema.
pub const ALLOWED: &str = "allowed";
pub const DENIED: &str = "denied";
pub const BLOCKED: &str = "blocked";

/// One line of the audit trail, as the settings panel lists it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRecord {
    pub id: String,
    pub chat_id: String,
    /// The chat's title, joined rather than copied.
    pub chat_title: String,
    pub tool: String,
    pub decision: String,
    pub note: String,
    pub args_digest: String,
    pub created_at: i64,
}

/// Writes one decision down. Append-only: nothing edits or removes a line except
/// the retention prune, so the record cannot disagree with itself.
pub fn log_decision(
    conn: &Connection,
    chat_id: &str,
    tool: &str,
    decision: &str,
    note: &str,
    args_digest: &str,
    workspace: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO approval_log
             (id, chat_id, tool, decision, note, args_digest, workspace, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            uuid::Uuid::new_v4().to_string(),
            chat_id,
            tool,
            decision,
            note,
            args_digest,
            workspace,
            now_ts()
        ],
    )?;
    Ok(())
}

/// The most recent decisions, newest first, for the panel that shows them.
pub fn recent_decisions(conn: &Connection, limit: usize) -> rusqlite::Result<Vec<ApprovalRecord>> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.chat_id, COALESCE(c.title, ''), a.tool, a.decision, a.note,
                a.args_digest, a.created_at
           FROM approval_log a
           LEFT JOIN chats c ON c.id = a.chat_id
          ORDER BY a.created_at DESC, a.rowid DESC
          LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit as i64], |r| {
        Ok(ApprovalRecord {
            id: r.get(0)?,
            chat_id: r.get(1)?,
            chat_title: r.get(2)?,
            tool: r.get(3)?,
            decision: r.get(4)?,
            note: r.get(5)?,
            args_digest: r.get(6)?,
            created_at: r.get(7)?,
        })
    })?;
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

// ---------------------------------------------------------- parked requests

/// A park the Scheduled panel lists: an approval or a question a run is waiting
/// on, with enough of the chat to name it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParkedRow {
    pub id: String,
    pub chat_id: String,
    /// Joined from `chats` rather than copied, so renaming a chat renames it
    /// here too.
    pub chat_title: String,
    pub kind: String,
    /// The event payload the chat received, kept verbatim.
    pub payload: serde_json::Value,
    pub status: String,
    pub created_at: i64,
}

/// Records that a run is parked on a request.
///
/// `payload` is the same value that went out as the event, which is what lets
/// the panel reuse the chat's own prompt rather than inventing one.
pub fn park_request(
    conn: &Connection,
    id: &str,
    chat_id: &str,
    kind: &str,
    payload: &serde_json::Value,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO parked_requests
             (id, chat_id, kind, payload, status, created_at)
         VALUES (?1, ?2, ?3, ?4, 'pending', ?5)",
        params![id, chat_id, kind, payload.to_string(), now_ts()],
    )?;
    Ok(())
}

/// Every park, newest first — the pending ones and the ones a crash left
/// behind. Deliberately unwindowed: it is a handful of rows, and a "needs you"
/// list that hides items is worse than no list.
pub fn list_parked(conn: &Connection) -> rusqlite::Result<Vec<ParkedRow>> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.chat_id, c.title, p.kind, p.payload, p.status, p.created_at
         FROM parked_requests p JOIN chats c ON c.id = p.chat_id
         ORDER BY p.created_at DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        let payload: String = r.get(4)?;
        Ok(ParkedRow {
            id: r.get(0)?,
            chat_id: r.get(1)?,
            chat_title: r.get(2)?,
            kind: r.get(3)?,
            payload: serde_json::from_str(&payload).unwrap_or(serde_json::Value::Null),
            status: r.get(5)?,
            created_at: r.get(6)?,
        })
    })?;
    rows.collect()
}

/// Removes one park. Idempotent on purpose: both the run that wakes up and
/// whoever answered it call this, and whichever is second is not an error.
pub fn forget_parked(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM parked_requests WHERE id = ?1", [id])?;
    Ok(())
}

/// Marks everything a previous process was parked on as `lost`, and returns how
/// many rows that was.
///
/// A park only means something while the run that made it is in memory, so after
/// a restart there is nothing to resume. Showing the row as lost is the honest
/// outcome: the user still learns what a run wanted, and the panel offers to
/// open the chat or dismiss the notice rather than a button that would do
/// nothing.
pub fn lose_parked_in_flight(conn: &Connection) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE parked_requests SET status = 'lost' WHERE status = 'pending'",
        [],
    )
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
    /// `"cadence"` for the calendar, `"file"` or `"git"` for a trigger.
    pub kind: String,
    /// What a trigger watches: a folder, or a repository. Empty for a cadence.
    pub target: String,
    /// Glob, or ref name.
    pub filter: String,
    pub interval_secs: i64,
    /// The last fingerprint a trigger saw. `None` means it has never looked.
    pub last_state: Option<String>,
    pub last_checked_at: Option<i64>,
    /// What the last check found, in words.
    pub last_note: String,
    /// The model this task runs on, or empty for the chat's own.
    pub model: String,
}

const SCHEDULED_COLS: &str = "id, title, prompt, chat_id, project_id, freq, time, weekday, nth, \
                              enabled, next_run_at, last_run_at, created_at, kind, target, filter, \
                              interval_secs, last_state, last_checked_at, last_note, model";

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
        kind: r.get(13)?,
        target: r.get(14)?,
        filter: r.get(15)?,
        interval_secs: r.get(16)?,
        last_state: r.get(17)?,
        last_checked_at: r.get(18)?,
        last_note: r.get(19)?,
        model: r.get(20)?,
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
/// its own and a test can schedule a task into the past. `spec` is the whole
/// watching half — which change the task waits for — and is a cadence unless the
/// caller says otherwise. `model` is a pin, and blank means the chat's own.
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
    spec: &triggers::Spec,
    model: &str,
    next_run_at: i64,
) -> rusqlite::Result<ScheduledRow> {
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now_ts();
    let title = title.trim();
    let title = if title.is_empty() { "Scheduled task" } else { title };
    let model = model.trim();
    let kind = spec.kind_id();
    conn.execute(
        "INSERT INTO scheduled_tasks
         (id, title, prompt, chat_id, project_id, freq, time, weekday, nth, enabled, next_run_at,
          created_at, kind, target, filter, interval_secs, model)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
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
            next_run_at,
            ts,
            kind,
            spec.target,
            spec.filter,
            spec.interval_secs,
            model
        ],
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
        kind: kind.to_string(),
        target: spec.target.clone(),
        filter: spec.filter.clone(),
        interval_secs: spec.interval_secs,
        last_state: None,
        last_checked_at: None,
        last_note: String::new(),
        model: model.to_string(),
    })
}

/// Rewrites a task, including when it next runs — a changed time has to move the
/// next run, or the edit would only take effect after the old one fired.
///
/// `last_state` is cleared on every edit. The stored fingerprint described the
/// target and the filter that were there a moment ago, so carrying it forward
/// would compare a new folder against an old folder's contents and fire on the
/// difference, which is not a change in anything the user is watching.
///
/// `model` is a pin, and blank means the chat's own — an edit that clears it is
/// a real edit, which is why it is written rather than left alone.
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
    spec: &triggers::Spec,
    model: &str,
    next_run_at: i64,
) -> rusqlite::Result<()> {
    let title = title.trim();
    let title = if title.is_empty() { "Scheduled task" } else { title };
    conn.execute(
        "UPDATE scheduled_tasks
         SET title = ?2, prompt = ?3, chat_id = ?4, project_id = ?5, freq = ?6, time = ?7,
             weekday = ?8, nth = ?9, enabled = ?10, next_run_at = ?11, kind = ?12, target = ?13,
             filter = ?14, interval_secs = ?15, last_state = NULL, model = ?16
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
            next_run_at,
            spec.kind_id(),
            spec.target,
            spec.filter,
            spec.interval_secs,
            model.trim()
        ],
    )?;
    Ok(())
}

/// A task's model pin, when it has one and the task still exists.
///
/// `None` means "no pin", which is also what a task deleted between its run
/// starting and this read gets: a pin that vanished is not a reason to fail a
/// turn, and the chat's own model is the answer either way.
pub fn scheduled_model(conn: &Connection, id: &str) -> Option<String> {
    conn.query_row("SELECT model FROM scheduled_tasks WHERE id = ?1", [id], |r| {
        r.get::<_, String>(0)
    })
    .optional()
    .ok()
    .flatten()
    .map(|model| model.trim().to_string())
    .filter(|model| !model.is_empty())
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

/// Moves a trigger's next *check* on, without touching what it saw.
///
/// Called before the check runs rather than after, for the same reason
/// `mark_scheduled_ran` is: a check slower than the interval must not be started
/// again by the tick behind it.
pub fn touch_scheduled(
    conn: &Connection,
    id: &str,
    next_run_at: i64,
    checked_at: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE scheduled_tasks SET next_run_at = ?2, last_checked_at = ?3 WHERE id = ?1",
        params![id, next_run_at, checked_at],
    )?;
    Ok(())
}

/// Says what a check found, for a look that did not move the fingerprint.
pub fn set_scheduled_note(conn: &Connection, id: &str, note: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE scheduled_tasks SET last_note = ?2 WHERE id = ?1",
        params![id, note],
    )?;
    Ok(())
}

/// Stores the fingerprint a trigger just saw, with the note that goes with it.
///
/// Both in one statement: the state is what the next tick compares against, so it
/// must not be able to land without the sentence that explains it.
pub fn set_scheduled_state(
    conn: &Connection,
    id: &str,
    state: &str,
    note: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE scheduled_tasks SET last_state = ?2, last_note = ?3 WHERE id = ?1",
        params![id, state, note],
    )?;
    Ok(())
}

/// How many runs a task has started since a moment — the trigger cap's one
/// source.
///
/// Counted from the run history rather than from a counter on the task, because
/// the history is already the record of what a task did and a second count could
/// quietly disagree with it.
pub fn fires_since(conn: &Connection, task_id: &str, since: i64) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM turn_usage WHERE task_id = ?1 AND created_at > ?2",
        params![task_id, since],
        |row| row.get(0),
    )
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

// --------------------------------------------------------- in-flight runs

/// Records that a run has started for `chat_id`.
///
/// Deliberately not inside `runs.rs`: that registry is in memory, and the whole
/// point of this row is to outlive the process. It is written next to
/// `Runs::begin` and cleared next to `Runs::finish`, so the durable marker and
/// the live one cannot disagree about which runs exist.
pub fn mark_run_started(conn: &Connection, chat_id: &str, run_id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO in_flight_runs (chat_id, run_id, started_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(chat_id) DO UPDATE SET run_id = excluded.run_id, started_at = excluded.started_at",
        params![chat_id, run_id, now_ts()],
    )?;
    Ok(())
}

/// Clears the marker. Every ending comes through here — a run that finished, one
/// stopped, one that errored — because they all mean "not in flight any more".
/// Only a process that died before reaching it leaves the row behind.
pub fn mark_run_ended(conn: &Connection, chat_id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM in_flight_runs WHERE chat_id = ?1", [chat_id])?;
    Ok(())
}

/// One run that was in flight when the process died.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InterruptedRun {
    pub chat_id: String,
    pub run_id: String,
    pub started_at: i64,
    /// Whether that run left file pre-images, so *Rewind* is offered only when it
    /// would actually put something back.
    pub can_rewind: bool,
}

/// The runs a previous process left in flight, newest first.
///
/// Meant to be read once at startup: while a run is live its marker is present
/// on purpose, so asking later in the session would report the run the user is
/// watching. Both actions the UI offers work off this: *Retry* re-sends the last
/// user turn (which drops the dangling tool calls a killed run left), and
/// *Rewind* is `rewind_last_run` over this chat.
pub fn interrupted_runs(conn: &Connection) -> rusqlite::Result<Vec<InterruptedRun>> {
    let mut stmt = conn.prepare(
        "SELECT r.chat_id, r.run_id, r.started_at,
                EXISTS (SELECT 1 FROM file_checkpoints c
                        WHERE c.chat_id = r.chat_id AND c.run_id = r.run_id)
         FROM in_flight_runs r
         JOIN chats c ON c.id = r.chat_id
         ORDER BY r.started_at DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(InterruptedRun {
            chat_id: r.get(0)?,
            run_id: r.get(1)?,
            started_at: r.get(2)?,
            can_rewind: r.get::<_, i64>(3)? != 0,
        })
    })?;
    rows.collect()
}

// --------------------------------------------------------------- maintenance

/// How many runs' worth of file checkpoints to keep per chat.
///
/// A rewind only ever reaches the *newest* run — the pre-images of an older one
/// are only correct once the runs after it have been undone, which is why
/// `rewind_last_run` refuses to jump back. Keeping ten runs is therefore already
/// generous; keeping every run the app has ever done is pure growth.
const CHECKPOINT_RUNS_KEPT: i64 = 10;

/// How many versions of one artifact to keep, newest first.
///
/// The panel offers every stored version as a reading surface, so this is a real
/// (small) loss rather than dead weight: the body of a document twenty revisions
/// ago is not something anyone opens.
const ARTIFACT_VERSIONS_KEPT: i64 = 20;

/// How many approval decisions to keep.
///
/// The log is append-only by design, so it is the one table whose growth is not
/// a bug — but "append-only" is not a reason to keep a year of it. A thousand
/// lines is far more than anyone reads back.
const DECISIONS_KEPT: i64 = 1000;

/// A prune only pays for a `VACUUM` past this many rows. `VACUUM` rewrites the
/// whole file, so doing it for a handful of rows would make the cure cost more
/// than the disease.
const VACUUM_AFTER_ROWS: usize = 500;

/// What a prune threw away.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PruneReport {
    pub checkpoints: usize,
    pub versions: usize,
    pub decisions: usize,
}

impl PruneReport {
    pub fn total(&self) -> usize {
        self.checkpoints + self.versions + self.decisions
    }
}

/// Drops the file checkpoints and artifact versions nothing can reach any more.
///
/// Two tables grow for ever and neither is a log: `file_checkpoints` keeps a
/// pre-image per (run, path) and a rewind can only walk back one run, and
/// `artifact_versions` keeps every revision of every artifact. Both are pruned to
/// a fixed depth rather than by age, because what matters is what is still
/// reachable, not how old it is.
pub fn prune(conn: &Connection) -> rusqlite::Result<PruneReport> {
    let chats: Vec<String> = {
        let mut stmt = conn.prepare("SELECT DISTINCT chat_id FROM file_checkpoints")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let mut checkpoints = 0usize;
    for chat_id in chats {
        checkpoints += conn.execute(
            "DELETE FROM file_checkpoints
              WHERE chat_id = ?1
                AND run_id NOT IN (
                    SELECT run_id FROM file_checkpoints
                     WHERE chat_id = ?1
                     GROUP BY run_id
                     ORDER BY max(created_at) DESC, max(rowid) DESC
                     LIMIT ?2
                )",
            params![chat_id, CHECKPOINT_RUNS_KEPT],
        )?;
    }
    // Per artifact, because a version number only means anything next to its own
    // siblings: the newest twenty of each, and never `current_version`, which is
    // the maximum by construction.
    let versions = conn.execute(
        "DELETE FROM artifact_versions
          WHERE version <= (
              SELECT max(v.version) - ?1 FROM artifact_versions v
               WHERE v.artifact_id = artifact_versions.artifact_id
          )",
        params![ARTIFACT_VERSIONS_KEPT],
    )?;
    let decisions = conn.execute(
        "DELETE FROM approval_log
          WHERE id NOT IN (
              SELECT id FROM approval_log
               ORDER BY created_at DESC, rowid DESC
               LIMIT ?1
          )",
        params![DECISIONS_KEPT],
    )?;
    Ok(PruneReport {
        checkpoints,
        versions,
        decisions,
    })
}

/// Reclaims the space a prune freed, when there is enough of it to be worth the
/// rewrite. Returns whether it ran.
pub fn vacuum_if_worth_it(conn: &Connection, removed: usize) -> bool {
    if removed < VACUUM_AFTER_ROWS {
        return false;
    }
    // Cannot run inside a transaction, and does not need one: it either finishes
    // or leaves the file as it was.
    conn.execute_batch("VACUUM;").is_ok()
}

/// The database's own size, from its page count rather than the file on disk: in
/// WAL mode there is a side file as well, and the pages are what the tables
/// actually occupy.
pub fn size_bytes(conn: &Connection) -> u64 {
    let pages: i64 = conn
        .query_row("PRAGMA page_count", [], |r| r.get(0))
        .unwrap_or(0);
    let size: i64 = conn
        .query_row("PRAGMA page_size", [], |r| r.get(0))
        .unwrap_or(1);
    (pages.max(0) as u64).saturating_mul(size.max(0) as u64)
}

// ------------------------------------------------------------ usage history

/// Who spent a row in `turn_usage`. Bare strings, like the decision log's three
/// words: they are the schema, and they only ever cross to the UI.
pub const TURN: &str = "turn";
pub const SUMMARY: &str = "summary";
pub const REVISION: &str = "revision";
pub const REVIEW: &str = "review";

/// One API round's usage, as the agent loop hands it over.
///
/// `rounds` is 1 for a normal call: the caller adds one round at a time and the
/// store accumulates, so it never has to hold the whole turn in memory.
///
/// `kind` says who spent it — `turn`, `summary`, `revision` — and `priced` says
/// whether the money is known. An endpoint with no published rate still records
/// its tokens, with `priced: false`, because a bill that silently drops them is
/// the second, quieter story this table exists to prevent.
pub struct TurnUsage<'a> {
    pub model: &'a str,
    pub kind: &'a str,
    pub priced: bool,
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
///
/// `task_id` is set only by the scheduler, and is carried through every round of
/// the same run (`COALESCE`, so a later round cannot clear it).
pub fn record_turn_usage(
    conn: &Connection,
    chat_id: &str,
    run_id: &str,
    round: &TurnUsage<'_>,
    task_id: Option<&str>,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO turn_usage
             (id, chat_id, run_id, model, kind, priced, rounds, prompt_tokens, completion_tokens,
              cache_hit_tokens, cache_miss_tokens, cost_usd, task_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
         ON CONFLICT(run_id) DO UPDATE SET
             rounds            = rounds + 1,
             prompt_tokens     = prompt_tokens + excluded.prompt_tokens,
             completion_tokens = completion_tokens + excluded.completion_tokens,
             cache_hit_tokens  = cache_hit_tokens + excluded.cache_hit_tokens,
             cache_miss_tokens = cache_miss_tokens + excluded.cache_miss_tokens,
             cost_usd          = cost_usd + excluded.cost_usd,
             model             = excluded.model,
             priced            = MIN(priced, excluded.priced),
             task_id           = COALESCE(excluded.task_id, task_id)",
        params![
            uuid::Uuid::new_v4().to_string(),
            chat_id,
            run_id,
            round.model,
            round.kind,
            round.priced as i64,
            round.prompt_tokens,
            round.completion_tokens,
            round.cache_hit_tokens,
            round.cache_miss_tokens,
            round.cost_usd,
            task_id,
            now_ts(),
        ],
    )?;
    Ok(())
}

/// One scheduled run, as the Scheduled panel lists it: when it ran, what it spent
/// and which chat it landed in.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRunRow {
    pub run_id: String,
    pub chat_id: String,
    /// Joined rather than copied; empty when the chat has since been deleted.
    pub chat_title: String,
    pub model: String,
    pub rounds: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub cost_usd: f64,
    pub created_at: i64,
}

/// The most recent runs of one task, newest first.
///
/// The same `turn_usage` rows `spend_report` sums, which is the whole reason the
/// task id lives there: a task's runs and the spend view are two readings of one
/// number, never two stories. A deleted chat is a LEFT join, so a run whose chat
/// is gone still counts.
pub fn task_runs(
    conn: &Connection,
    task_id: &str,
    limit: usize,
) -> rusqlite::Result<Vec<TaskRunRow>> {
    let mut stmt = conn.prepare(
        "SELECT t.run_id, t.chat_id, COALESCE(c.title, ''), t.model, t.rounds,
                t.prompt_tokens, t.completion_tokens, t.cost_usd, t.created_at
           FROM turn_usage t
           LEFT JOIN chats c ON c.id = t.chat_id
          WHERE t.task_id = ?1
          ORDER BY t.created_at DESC, t.rowid DESC
          LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![task_id, limit as i64], |r| {
        Ok(TaskRunRow {
            run_id: r.get(0)?,
            chat_id: r.get(1)?,
            chat_title: r.get(2)?,
            model: r.get(3)?,
            rounds: r.get(4)?,
            prompt_tokens: r.get(5)?,
            completion_tokens: r.get(6)?,
            cost_usd: r.get(7)?,
            created_at: r.get(8)?,
        })
    })?;
    rows.collect()
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
    /// How many rows — a turn, a summary, a revision.
    pub turns: i64,
    /// False when any row of this model was never priced, so the panel can say
    /// the money is unknown rather than presenting a part-sum as the whole bill.
    pub priced: bool,
}

/// Who spent it: the turns the user typed, the summaries, the in-place
/// revisions. Together these *are* the total, which is what makes the note "every
/// spender is on the bill" checkable rather than a promise.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KindSpend {
    /// `turn` | `summary` | `revision`
    pub kind: String,
    pub cost_usd: f64,
    pub calls: i64,
    /// False when a call of this kind could not be priced.
    pub priced: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSpend {
    pub chat_id: String,
    pub title: String,
    pub cost_usd: f64,
    pub turns: i64,
}

/// One section's share of the tokens, split into the two kinds that behave
/// differently: a coding chat reading files burns input, a conversation burns
/// output.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionTokens {
    /// `chat` | `coding`
    pub kind: String,
    pub turns: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
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
    /// By section, and together they *are* the totals above — which is the whole
    /// reason the panel can draw them as slices.
    pub sections: Vec<SectionTokens>,
    /// By who spent it, and together these are `total_usd` — the turns the user
    /// typed, the summaries and the revisions, so a call the user did not type is
    /// visible rather than folded into a figure nobody can explain.
    pub kinds: Vec<KindSpend>,
}

/// One hour of the local day, and how many turns began in it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HourCount {
    /// 0..23, local time — the hour the user would name.
    pub hour: i64,
    pub turns: i64,
}

/// A week's (or a month's) shape: what the work was, not what it cost.
///
/// A read model over `turn_usage`, never a store of its own — everything here is
/// derived from rows the spend view already reads. The totals, the days and the
/// conversations are taken *from* `spend_report` rather than queried again, so
/// "the recap agrees with the spend view" is structural rather than a promise two
/// queries keep by hand. Only the hour histogram is new, because the spend view
/// has no use for it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recap {
    pub since: i64,
    /// The same four figures `spend_report` reports for this window.
    pub total_usd: f64,
    pub turns: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    /// Newest day first, only days that had a turn.
    pub days: Vec<DaySpend>,
    /// Busiest hour first.
    pub hours: Vec<HourCount>,
    /// Dearest conversation first — the closest this has to "what you worked on".
    pub chats: Vec<ChatSpend>,
}

/// The shape of a window, built on `spend_report`'s numbers.
///
/// One extra query, for the hours: `strftime('%H', …, 'localtime')` buckets by the
/// user's own clock, for the same reason the days are local — an evening's work
/// must not be filed under tomorrow.
pub fn recap(conn: &Connection, since: i64, chat_limit: usize) -> rusqlite::Result<Recap> {
    let report = spend_report(conn, since, chat_limit)?;
    let hours = {
        let mut stmt = conn.prepare(
            "SELECT CAST(strftime('%H', created_at, 'unixepoch', 'localtime') AS INTEGER),
                    COUNT(*)
               FROM turn_usage WHERE created_at >= ?1
              GROUP BY 1 ORDER BY COUNT(*) DESC, 1",
        )?;
        let rows = stmt.query_map([since], |r| {
            Ok(HourCount {
                hour: r.get(0)?,
                turns: r.get(1)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    Ok(Recap {
        since: report.since,
        total_usd: report.total_usd,
        turns: report.turns,
        prompt_tokens: report.prompt_tokens,
        completion_tokens: report.completion_tokens,
        days: report.days,
        hours,
        chats: report.chats,
    })
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
            "SELECT model, SUM(cost_usd), COUNT(*), MIN(priced) FROM turn_usage WHERE created_at >= ?1
             GROUP BY model ORDER BY SUM(cost_usd) DESC",
        )?;
        let rows = stmt.query_map([since], |r| {
            Ok(ModelSpend {
                model: r.get(0)?,
                cost_usd: r.get(1)?,
                turns: r.get(2)?,
                priced: r.get::<_, i64>(3)? != 0,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    let kinds = {
        let mut stmt = conn.prepare(
            "SELECT kind, SUM(cost_usd), COUNT(*), MIN(priced) FROM turn_usage WHERE created_at >= ?1
             GROUP BY kind ORDER BY SUM(cost_usd) DESC, kind",
        )?;
        let rows = stmt.query_map([since], |r| {
            Ok(KindSpend {
                kind: r.get(0)?,
                cost_usd: r.get(1)?,
                calls: r.get(2)?,
                priced: r.get::<_, i64>(3)? != 0,
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

    let sections = {
        // The same rows grouped by the section the chat belongs to. Money answers
        // "how much"; this answers "where did it go", which is the half of the
        // bill a cheaper prompt can actually change.
        let mut stmt = conn.prepare(
            "SELECT c.kind, COUNT(*), COALESCE(SUM(t.prompt_tokens), 0),
                    COALESCE(SUM(t.completion_tokens), 0)
             FROM turn_usage t JOIN chats c ON c.id = t.chat_id
             WHERE t.created_at >= ?1
             GROUP BY c.kind ORDER BY c.kind",
        )?;
        let rows = stmt.query_map([since], |r| {
            Ok(SectionTokens {
                kind: r.get(0)?,
                turns: r.get(1)?,
                prompt_tokens: r.get(2)?,
                completion_tokens: r.get(3)?,
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
        sections,
        kinds,
    })
}

// ----------------------------------------------------------- the audit export

/// The version of the exported file, from its first release.
///
/// A file outlives the code that wrote it, so it says which shape it is in — the
/// same rule the chat bundle follows.
pub const AUDIT_VERSION: u32 = 1;

/// What leaves the app: the decisions and the bill, in one file.
///
/// Both halves are the *same reads the panels make*, not a second pair of
/// queries. An export that can disagree with the screen it exports is a
/// liability, and the equality is what the test asserts rather than promises.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Audit {
    pub version: u32,
    pub exported_at: i64,
    /// The window the spend covers, so a reader of the file knows.
    pub since: i64,
    /// Newest first, capped exactly like the panel's list — see the ceiling on
    /// `decision_limit` below.
    pub decisions: Vec<ApprovalRecord>,
    pub spend: SpendReport,
}

/// Assemble the export for one window.
///
/// `args_digest` is all this carries of a call's arguments, because the log
/// carries nothing else: an approval can cover a command holding a secret, and
/// this is a file that gets mailed around.
///
/// ponytail: `decision_limit` is the panel's own cap, so the file is the panel's
/// view of the log rather than a dump of the table. A person who wants every row
/// ever written wants `sqlite3` or a paging UI, not a JSON file to read — the
/// upgrade path if that changes is a `since` on the decisions query and a page in
/// the panel to match it.
pub fn audit(
    conn: &Connection,
    since: i64,
    decision_limit: usize,
    chat_limit: usize,
) -> rusqlite::Result<Audit> {
    Ok(Audit {
        version: AUDIT_VERSION,
        exported_at: now_ts(),
        since,
        decisions: recent_decisions(conn, decision_limit)?,
        spend: spend_report(conn, since, chat_limit)?,
    })
}

// --------------------------------------------------------------- notebooks

/// A notebook: a title, its own rules, its own folder of sources, and the model
/// its questions run on.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotebookRow {
    pub id: String,
    pub title: String,
    /// The notebook's own rules, in the same shape a project's take — so the
    /// rule about which of them apply is the one `agent::push_instructions`
    /// already owns, rather than a second answer here.
    #[serde(default)]
    pub instructions: Vec<Instruction>,
    /// The model a question runs on, or `''` for the chat default — the same pin
    /// a scheduled task carries, and blank for the same reason.
    #[serde(default)]
    pub model: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// One document a notebook holds.
///
/// Deliberately without a `state`: whether a source is current, stale or gone is
/// a question about the file on disk, and `conversation::notebook` owns both that
/// rule and the payload the window reads. A `state` field here would be a second
/// copy of it, written by whichever reader remembered to.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRow {
    pub id: String,
    pub notebook_id: String,
    pub name: String,
    /// `file` | `url`
    pub kind: String,
    pub origin: String,
    pub path: Option<String>,
    pub enabled: bool,
    /// The source's text. Deliberately not serialised: it is the same field the
    /// model is sent, capped and labelled, and a version of it on the wire would
    /// be a second copy of every document in the window.
    #[serde(skip)]
    pub extract: Option<String>,
    pub digest: String,
    pub digest_at: Option<i64>,
    /// The fingerprint the *digest* describes. See the table's comment.
    pub fingerprint: String,
    pub created_at: i64,
}

/// One question and one answer. The answer's citations are the labels that
/// resolved, stored rather than re-parsed from the prose.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotebookMessageRow {
    pub id: String,
    pub notebook_id: String,
    pub seq: i64,
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub citations: Vec<String>,
    pub created_at: i64,
}

/// Something a notebook produced. `target` is a written file's path or an
/// artifact's id, because both writers already exist.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotebookOutputRow {
    pub id: String,
    pub notebook_id: String,
    pub kind: String,
    pub title: String,
    pub target: String,
    pub created_at: i64,
}

const NOTEBOOK_COLS: &str = "id, title, instructions, model, created_at, updated_at";
const SOURCE_COLS: &str = "id, notebook_id, name, kind, origin, path, enabled, extract, digest, \
     digest_at, fingerprint, created_at";
const NOTEBOOK_MESSAGE_COLS: &str = "id, notebook_id, seq, role, content, citations_json, created_at";
const NOTEBOOK_OUTPUT_COLS: &str = "id, notebook_id, kind, title, target, created_at";

fn notebook_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<NotebookRow> {
    // A malformed blob reads as "no rules" rather than failing the whole list,
    // the same way a project's does.
    let raw: String = r.get(2)?;
    Ok(NotebookRow {
        id: r.get(0)?,
        title: r.get(1)?,
        instructions: serde_json::from_str(&raw).unwrap_or_default(),
        model: r.get(3)?,
        created_at: r.get(4)?,
        updated_at: r.get(5)?,
    })
}

fn source_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<SourceRow> {
    Ok(SourceRow {
        id: r.get(0)?,
        notebook_id: r.get(1)?,
        name: r.get(2)?,
        kind: r.get(3)?,
        origin: r.get(4)?,
        path: r.get(5)?,
        enabled: r.get::<_, i64>(6)? != 0,
        extract: r.get(7)?,
        digest: r.get(8)?,
        digest_at: r.get(9)?,
        fingerprint: r.get(10)?,
        created_at: r.get(11)?,
    })
}

fn notebook_message_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<NotebookMessageRow> {
    let raw: Option<String> = r.get(5)?;
    Ok(NotebookMessageRow {
        id: r.get(0)?,
        notebook_id: r.get(1)?,
        seq: r.get(2)?,
        role: r.get(3)?,
        content: r.get(4)?,
        citations: raw
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default(),
        created_at: r.get(6)?,
    })
}

fn notebook_output_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<NotebookOutputRow> {
    Ok(NotebookOutputRow {
        id: r.get(0)?,
        notebook_id: r.get(1)?,
        kind: r.get(2)?,
        title: r.get(3)?,
        target: r.get(4)?,
        created_at: r.get(5)?,
    })
}

/// Newest activity first, like the sidebar: asking a notebook a question is
/// activity, so it moves the row up.
pub fn list_notebooks(conn: &Connection) -> rusqlite::Result<Vec<NotebookRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {NOTEBOOK_COLS} FROM notebooks ORDER BY updated_at DESC, rowid DESC"
    ))?;
    let rows = stmt.query_map([], notebook_row)?;
    rows.collect()
}

/// One notebook, or `None` when it has been deleted.
pub fn get_notebook(conn: &Connection, id: &str) -> rusqlite::Result<Option<NotebookRow>> {
    let mut stmt = conn.prepare(&format!("SELECT {NOTEBOOK_COLS} FROM notebooks WHERE id = ?1"))?;
    stmt.query_row([id], notebook_row).optional()
}

/// An empty title would make a notebook unfindable in the list, so it falls back
/// rather than being refused — the user can rename it straight after, exactly as
/// a project does.
pub fn create_notebook(conn: &Connection, title: &str) -> rusqlite::Result<NotebookRow> {
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now_ts();
    let title = title.trim();
    let title = if title.is_empty() { "New notebook" } else { title };
    conn.execute(
        "INSERT INTO notebooks (id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
        params![id, title, ts],
    )?;
    Ok(NotebookRow {
        id,
        title: title.to_string(),
        instructions: Vec::new(),
        model: String::new(),
        created_at: ts,
        updated_at: ts,
    })
}

pub fn rename_notebook(conn: &Connection, id: &str, title: &str) -> rusqlite::Result<()> {
    let title = title.trim();
    let title = if title.is_empty() { "New notebook" } else { title };
    conn.execute(
        "UPDATE notebooks SET title = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, title, now_ts()],
    )?;
    Ok(())
}

/// The model this notebook's questions run on, or `''` for the chat default.
pub fn set_notebook_model(conn: &Connection, id: &str, model: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE notebooks SET model = ?2 WHERE id = ?1",
        params![id, model.trim()],
    )?;
    Ok(())
}

/// Replaces a notebook's rules.
///
/// Deliberately does not touch `updated_at`: the list is ordered by it, so
/// editing a rule must not reshuffle the list — the same reason renaming a chat
/// does not bump the chat's own timestamp.
pub fn set_notebook_instructions(
    conn: &Connection,
    id: &str,
    instructions: &[Instruction],
) -> rusqlite::Result<()> {
    let json = serde_json::to_string(instructions).unwrap_or_else(|_| "[]".to_string());
    conn.execute(
        "UPDATE notebooks SET instructions = ?2 WHERE id = ?1",
        params![id, json],
    )?;
    Ok(())
}

/// Removes the notebook and, by the foreign keys, its sources, its questions and
/// its outputs. The files under `notebooks/<id>/` are removed by the caller,
/// which is the one place that knows where they are.
pub fn delete_notebook(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM notebooks WHERE id = ?1", [id])?;
    Ok(())
}

/// Moves a notebook to the top of the list without changing its title or rules.
pub fn touch_notebook(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE notebooks SET updated_at = ?2 WHERE id = ?1",
        params![id, now_ts()],
    )?;
    Ok(())
}

/// Every source a notebook holds, in the order they were added — which is the
/// order the rail shows them in and the order a catalogue reads in.
///
/// `rowid` rather than `created_at`: a timestamp here has one-second resolution, so
/// adding three documents in a row ties, and the tiebreak then has to be something
/// that is not the id — a uuid is random, and `ORDER BY created_at, id` shuffled a
/// notebook's sources into an arbitrary order. SQLite's rowid is the insertion
/// order, which is exactly what this is.
pub fn list_sources(conn: &Connection, notebook_id: &str) -> rusqlite::Result<Vec<SourceRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {SOURCE_COLS} FROM sources WHERE notebook_id = ?1 ORDER BY rowid"
    ))?;
    let rows = stmt.query_map([notebook_id], source_row)?;
    rows.collect()
}

pub fn get_source(conn: &Connection, id: &str) -> rusqlite::Result<Option<SourceRow>> {
    let mut stmt = conn.prepare(&format!("SELECT {SOURCE_COLS} FROM sources WHERE id = ?1"))?;
    stmt.query_row([id], source_row).optional()
}

/// Adds a source, already copied and already read. The digest arrives separately
/// because generating it may need a model call, and a row that exists without one
/// is still a source the user added.
#[allow(clippy::too_many_arguments)]
pub fn create_source(
    conn: &Connection,
    notebook_id: &str,
    name: &str,
    kind: &str,
    origin: &str,
    path: Option<&str>,
    extract: &str,
    fingerprint: &str,
) -> rusqlite::Result<SourceRow> {
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now_ts();
    conn.execute(
        "INSERT INTO sources
            (id, notebook_id, name, kind, origin, path, enabled, extract, fingerprint, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?8, ?9)",
        params![id, notebook_id, name, kind, origin, path, extract, fingerprint, ts],
    )?;
    Ok(SourceRow {
        id,
        notebook_id: notebook_id.to_string(),
        name: name.to_string(),
        kind: kind.to_string(),
        origin: origin.to_string(),
        path: path.map(str::to_string),
        enabled: true,
        extract: (!extract.is_empty()).then(|| extract.to_string()),
        digest: String::new(),
        digest_at: None,
        fingerprint: fingerprint.to_string(),
        created_at: ts,
    })
}

/// Switches a source on or off. Nothing else moves: the extract, the digest and
/// the file all stay, which is what makes the switch reversible.
pub fn set_source_enabled(conn: &Connection, id: &str, enabled: bool) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE sources SET enabled = ?2 WHERE id = ?1",
        params![id, enabled as i64],
    )?;
    Ok(())
}

/// Records what a source now holds, and the fingerprint the digest will describe.
///
/// Both in one statement because they are one fact: a fingerprint written without
/// its text, or a text without its fingerprint, would read as a source that just
/// changed when it did not — and that is a digest regenerated for nothing.
pub fn set_source_extract(
    conn: &Connection,
    id: &str,
    extract: &str,
    fingerprint: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE sources SET extract = ?2, fingerprint = ?3 WHERE id = ?1",
        params![id, extract, fingerprint],
    )?;
    Ok(())
}

/// Records the digest, and when it was made. Deliberately does not touch the
/// fingerprint: that one belongs to the content, and this answers a question
/// about it.
pub fn set_source_digest(conn: &Connection, id: &str, digest: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE sources SET digest = ?2, digest_at = ?3 WHERE id = ?1",
        params![id, digest, now_ts()],
    )?;
    Ok(())
}

pub fn remove_source(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM sources WHERE id = ?1", [id])?;
    Ok(())
}

/// Appends one line to a notebook's conversation, at the end. `seq` is worked out
/// here rather than by the caller so two appends cannot claim the same place.
pub fn append_notebook_message(
    conn: &Connection,
    notebook_id: &str,
    role: &str,
    content: &str,
    citations: &[String],
) -> rusqlite::Result<NotebookMessageRow> {
    let seq: i64 = conn.query_row(
        "SELECT COALESCE(MAX(seq), 0) + 1 FROM notebook_messages WHERE notebook_id = ?1",
        [notebook_id],
        |r| r.get(0),
    )?;
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now_ts();
    let citations_json = (!citations.is_empty())
        .then(|| serde_json::to_string(citations).unwrap_or_else(|_| "[]".to_string()));
    conn.execute(
        "INSERT INTO notebook_messages (id, notebook_id, seq, role, content, citations_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, notebook_id, seq, role, content, citations_json, ts],
    )?;
    Ok(NotebookMessageRow {
        id,
        notebook_id: notebook_id.to_string(),
        seq,
        role: role.to_string(),
        content: content.to_string(),
        citations: citations.to_vec(),
        created_at: ts,
    })
}

pub fn list_notebook_messages(
    conn: &Connection,
    notebook_id: &str,
) -> rusqlite::Result<Vec<NotebookMessageRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {NOTEBOOK_MESSAGE_COLS} FROM notebook_messages WHERE notebook_id = ?1 ORDER BY seq"
    ))?;
    let rows = stmt.query_map([notebook_id], notebook_message_row)?;
    rows.collect()
}

pub fn add_notebook_output(
    conn: &Connection,
    notebook_id: &str,
    kind: &str,
    title: &str,
    target: &str,
) -> rusqlite::Result<NotebookOutputRow> {
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now_ts();
    conn.execute(
        "INSERT INTO notebook_outputs (id, notebook_id, kind, title, target, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, notebook_id, kind, title, target, ts],
    )?;
    Ok(NotebookOutputRow {
        id,
        notebook_id: notebook_id.to_string(),
        kind: kind.to_string(),
        title: title.to_string(),
        target: target.to_string(),
        created_at: ts,
    })
}

pub fn list_notebook_outputs(
    conn: &Connection,
    notebook_id: &str,
) -> rusqlite::Result<Vec<NotebookOutputRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {NOTEBOOK_OUTPUT_COLS} FROM notebook_outputs WHERE notebook_id = ?1 \
         ORDER BY created_at DESC, rowid DESC"
    ))?;
    let rows = stmt.query_map([notebook_id], notebook_output_row)?;
    rows.collect()
}

pub fn remove_notebook_output(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM notebook_outputs WHERE id = ?1", [id])?;
    Ok(())
}

// ------------------------------------------------------------------ chunks

/// One indexed piece of one document, as a search returns it.
#[derive(Debug, Clone)]
pub struct ChunkHit {
    pub root: String,
    pub path: String,
    /// The heading the chunk sits under, or empty for a document's preamble.
    pub heading: String,
    pub text: String,
    /// `bm25`, which is lower for a better match. Carried because the caller
    /// searches one root at a time and has to put the roots back in one order.
    pub score: f64,
}

/// The fingerprint a file's chunks were made from, or `None` when it has none.
///
/// One row is enough: every chunk of a file is written in the same transaction
/// from the same read, so they cannot disagree about what they were made from.
pub fn chunk_fingerprint(conn: &Connection, path: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT fingerprint FROM chunks WHERE path = ?1 LIMIT 1",
        [path],
        |r| r.get(0),
    )
    .optional()
}

/// Every file the index holds under one root — so a file that has been deleted
/// can be swept out of the index rather than answering questions forever.
pub fn indexed_paths(conn: &Connection, root: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT DISTINCT path FROM chunks WHERE root = ?1")?;
    let rows = stmt.query_map([root], |r| r.get(0))?;
    rows.collect()
}

/// Replaces one file's chunks, and answers how many it now has.
///
/// A delete and an insert in one transaction rather than an update per chunk: a
/// file that shrank would otherwise leave its old tail in the index, answering
/// questions with text that is no longer in it.
pub fn replace_chunks(
    conn: &Connection,
    root: &str,
    path: &str,
    fingerprint: &str,
    chunks: &[(String, String)],
) -> rusqlite::Result<usize> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM chunks WHERE path = ?1", [path])?;
    let ts = now_ts();
    for (ordinal, (heading, text)) in chunks.iter().enumerate() {
        tx.execute(
            "INSERT INTO chunks (id, root, path, heading, ordinal, text, fingerprint, indexed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                uuid::Uuid::new_v4().to_string(),
                root,
                path,
                heading,
                ordinal as i64,
                text,
                fingerprint,
                ts
            ],
        )?;
    }
    tx.commit()?;
    Ok(chunks.len())
}

pub fn delete_path_chunks(conn: &Connection, path: &str) -> rusqlite::Result<usize> {
    conn.execute("DELETE FROM chunks WHERE path = ?1", [path])
}

/// The best chunks in **one** root for a query, best first.
///
/// One root at a time on purpose: `bm25` is only comparable between rows of the
/// same search, so the alternative — one query filtered by a set of roots — would
/// either need dynamic SQL or let a large corpus crowd out a small one. The caller
/// asks per root and merges, which is a loop rather than a second rule.
///
/// The tiebreak is `path, ordinal` for the reason the message search has one:
/// `bm25` alone leaves equally-scored rows in whatever order the query planner
/// walks, and two runs of the same question would then cite different chunks.
pub fn search_chunks(
    conn: &Connection,
    root: &str,
    query: &str,
    limit: i64,
) -> rusqlite::Result<Vec<ChunkHit>> {
    let Some(expr) = fts_query(query) else {
        return Ok(Vec::new());
    };
    let mut stmt = conn.prepare(
        "SELECT c.root, c.path, c.heading, c.text, bm25(chunks_fts)
         FROM chunks_fts
         JOIN chunks c ON c.rowid = chunks_fts.rowid
         WHERE chunks_fts MATCH ?1 AND c.root = ?2
         ORDER BY bm25(chunks_fts), c.path, c.ordinal
         LIMIT ?3",
    )?;
    let rows = stmt.query_map(params![expr, root, limit.max(1)], |r| {
        Ok(ChunkHit {
            root: r.get(0)?,
            path: r.get(1)?,
            heading: r.get(2)?,
            text: r.get(3)?,
            score: r.get(4)?,
        })
    })?;
    rows.collect()
}

#[cfg(test)]
mod tests {    use super::*;

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

    /// `create_scheduled` with the watching half left at a cadence, which is what
    /// every test here but the trigger ones wants.
    fn schedule(
        conn: &Connection,
        title: &str,
        prompt: &str,
        freq: &str,
        time: &str,
        weekday: i64,
        nth: i64,
        next_run_at: i64,
    ) -> rusqlite::Result<ScheduledRow> {
        create_scheduled(
            conn,
            title,
            prompt,
            None,
            None,
            freq,
            time,
            weekday,
            nth,
            &triggers::Spec::default(),
            // No pin: the chat's own model, which is what every task had before
            // a task could name one.
            "",
            next_run_at,
        )
    }

    /// `update_scheduled` with the watching half left at a cadence.
    #[allow(clippy::too_many_arguments)]
    fn reschedule(
        conn: &Connection,
        id: &str,
        title: &str,
        prompt: &str,
        freq: &str,
        time: &str,
        weekday: i64,
        nth: i64,
        enabled: bool,
        next_run_at: i64,
    ) -> rusqlite::Result<()> {
        update_scheduled(
            conn,
            id,
            title,
            prompt,
            None,
            None,
            freq,
            time,
            weekday,
            nth,
            enabled,
            &triggers::Spec::default(),
            "",
            next_run_at,
        )
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
                -- And `turn_usage` from before a run could be tagged with the task
                -- that started it, so the `task_id` ALTER — and the index `SCHEMA`
                -- builds over it — are exercised on a real file rather than only
                -- on a fresh CREATE TABLE.
                CREATE TABLE turn_usage (
                    id                TEXT PRIMARY KEY,
                    chat_id           TEXT NOT NULL,
                    run_id            TEXT NOT NULL UNIQUE,
                    model             TEXT NOT NULL,
                    rounds            INTEGER NOT NULL DEFAULT 1,
                    prompt_tokens     INTEGER NOT NULL DEFAULT 0,
                    completion_tokens INTEGER NOT NULL DEFAULT 0,
                    cache_hit_tokens  INTEGER NOT NULL DEFAULT 0,
                    cache_miss_tokens INTEGER NOT NULL DEFAULT 0,
                    cost_usd          REAL NOT NULL DEFAULT 0,
                    created_at        INTEGER NOT NULL
                );
                INSERT INTO chats
                    (id, title, kind, model, created_at, updated_at)
                    VALUES ('old-1', 'A chat from before', 'chat', 'm', 1, 1);
                INSERT INTO projects (id, name, created_at, updated_at)
                    VALUES ('p-1', 'Before 2.7', 1, 1);
                -- And a grant from before a grant could name a target, whose
                -- primary key was the pair rather than the triple. SQLite cannot
                -- widen a key in place, so `open()` has to carry this row over
                -- by hand — and only a real file proves it did.
                CREATE TABLE tool_permissions (
                    workspace  TEXT NOT NULL,
                    tool       TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    PRIMARY KEY (workspace, tool)
                );
                INSERT INTO tool_permissions (workspace, tool, created_at)
                    VALUES ('/repo', 'git_push', 1);
                -- And a scheduled task from before a task could watch something,
                -- so the seven trigger ALTERs are exercised on a real file rather
                -- than only on a fresh CREATE TABLE.
                CREATE TABLE scheduled_tasks (
                    id          TEXT PRIMARY KEY,
                    title       TEXT NOT NULL,
                    prompt      TEXT NOT NULL,
                    chat_id     TEXT,
                    project_id  TEXT,
                    freq        TEXT NOT NULL DEFAULT 'daily',
                    time        TEXT NOT NULL DEFAULT '09:00',
                    weekday     INTEGER NOT NULL DEFAULT 0,
                    nth         INTEGER NOT NULL DEFAULT 1,
                    enabled     INTEGER NOT NULL DEFAULT 1,
                    next_run_at INTEGER NOT NULL,
                    last_run_at INTEGER,
                    created_at  INTEGER NOT NULL
                );
                INSERT INTO scheduled_tasks
                    (id, title, prompt, freq, time, weekday, nth, enabled, next_run_at, created_at)
                    VALUES ('task-1', 'Before triggers', 'p', 'daily', '09:00', 0, 1, 1, 1, 1);",
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
        // The `incognito` ALTER ran too: an old chat is not private, which is
        // exactly what a default of 0 means. (`get_chat` selects every column, so
        // this is a real read of a column only the ALTER could have added.)
        assert!(!kept.incognito, "an upgraded chat must not be incognito");
        // And the `egress` ALTER ran: a chat written before the switch declares
        // no network policy, which is what a default of 'all' means. `get_chat`
        // selects every column, so this is a real read of a column only the ALTER
        // could have added.
        assert_eq!(kept.egress, "all", "an upgraded chat must declare no policy");

        // The permission table was rebuilt rather than altered, and the grant the
        // user gave before targets existed survived as a whole-tool grant — which
        // is exactly what it meant when they gave it.
        assert!(is_tool_allowed(&conn, "/repo", "git_push", "").unwrap());
        assert!(is_tool_allowed(&conn, "/repo", "git_push", "anywhere").unwrap());
        assert_eq!(
            all_allowed_tools(&conn).unwrap(),
            vec![("/repo".to_string(), "git_push".to_string(), String::new())]
        );
        // And the old shape is gone: a second grant for the same tool now fits,
        // which the pair-keyed table could not have held.
        allow_tool(&conn, "/repo", "git_push", "origin").unwrap();
        assert_eq!(all_allowed_tools(&conn).unwrap().len(), 2);

        // The `instructions` ALTER ran on the upgraded file: the old project kept
        // its name and gained an empty rule list rather than a malformed one.
        let projects = list_projects(&conn).unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].name, "Before 2.7");
        assert!(projects[0].instructions.is_empty());

        // The notebook tables arrived whole in Phase 13, so there is nothing to
        // carry over — but a database written before them must still open, and the
        // four tables must really be there afterwards rather than the batch having
        // silently skipped them.
        for table in ["notebooks", "sources", "notebook_messages", "notebook_outputs"] {
            assert!(
                !table_columns(&conn, table).is_empty(),
                "an upgraded database has no {table} table"
            );
        }

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

        // Same for `task_id`: the ALTER added it, `SCHEMA` indexed it, and both
        // had to happen in that order or the batch would have aborted on this
        // file. Reading through `task_runs` is also the guard that the column is
        // really selectable here.
        assert!(task_runs(&conn, "no such task", 10).unwrap().is_empty());
        let task_index: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_turn_usage_task'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(task_index, 1, "the index over the ALTERed task_id column was not created");

        // And the new features work against the upgraded file, not just against
        // an in-memory schema.
        let chat = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        set_chat_project(&conn, &chat.id, None).unwrap();
        // The pre-2.7 project is still there — the upgrade adds a column, it does
        // not empty the table — and the new call paths run against the file.
        assert_eq!(list_projects(&conn).unwrap().len(), 1);
        // A task written before triggers existed is a cadence — which is what a
        // default of 'cadence' says — and has no fingerprint, because "never
        // looked" is the state that arms a trigger and not something an upgrade
        // should invent. `list_scheduled` selects every column, so this is a real
        // read of seven columns only the ALTERs could have added.
        let upgraded = &list_scheduled(&conn).unwrap()[0];
        assert_eq!(upgraded.title, "Before triggers");
        assert_eq!(upgraded.kind, "cadence");
        assert_eq!(upgraded.target, "");
        assert_eq!(upgraded.filter, "");
        assert_eq!(upgraded.interval_secs, 300);
        assert!(upgraded.last_state.is_none());
        assert!(upgraded.last_checked_at.is_none());
        assert_eq!(upgraded.last_note, "");
        // And the `model` ALTER ran: a task written before a task could name a
        // model has no pin, which is what a default of '' says. `list_scheduled`
        // selects every column, so this is a real read of a column only the ALTER
        // could have added.
        assert_eq!(upgraded.model, "");
        assert!(scheduled_model(&conn, "task-1").is_none());
        // And the new features work against the upgraded file too.
        schedule(&conn, "t", "p", "daily", "09:00", 0, 1, 1).unwrap();
        assert_eq!(list_scheduled(&conn).unwrap().len(), 2);

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
        let hits = search_messages(&conn, "pars", 0, 10).unwrap();
        assert_eq!(hits.len(), 2, "got {hits:?}");
        assert!(hits.iter().all(|h| h.snippet.contains("[pars")));
        assert!(hits.iter().any(|h| h.chat_kind == "coding"));

        // A row with no content is not a hit, and a nonsense query is empty.
        assert!(search_messages(&conn, "x", 0, 10).unwrap().is_empty());
        assert!(search_messages(&conn, "absentword", 0, 10).unwrap().is_empty());
    }

    #[test]
    fn an_fts_metacharacter_is_a_search_and_not_a_syntax_error() {
        let conn = mem();
        let chat = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        insert_message(&conn, &chat.id, "user", Some("quotes and \"stars\""), None, None, None, None)
            .unwrap();
        // Each of these would be a MATCH parse error if it were not quoted.
        for raw in ["\"stars\"", "a - b", "a:b", "(a)", "**"] {
            search_messages(&conn, raw, 0, 10).unwrap();
        }
        // And a query with nothing searchable in it is empty, not an error.
        for raw in ["", "   ", "a", "*", "-"] {
            assert!(
                search_messages_scoped(&conn, raw, None, None, 0, 10)
                    .unwrap()
                    .is_empty(),
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
        assert_eq!(search_messages(&conn, "invoice", 0, 10).unwrap().len(), 3);

        // Scoped to my folder, the other client's conversation is not a result —
        // and neither is the chat-mode one, which lives in its own place.
        let scoped = search_messages_scoped(
            &conn,
            "invoice",
            Some("C:/client-a"),
            Some("coding"),
            0,
            10,
        )
        .unwrap();
        assert_eq!(scoped.len(), 1, "got {scoped:?}");
        assert_eq!(scoped[0].chat_id, mine.id);

        // A chat-mode chat has no folder to draw a line at, so its scope is the
        // section: its own history, and none of the coding folders.
        let loose_hits =
            search_messages_scoped(&conn, "invoice", None, Some("chat"), 0, 10).unwrap();
        assert_eq!(loose_hits.len(), 1);
        assert_eq!(loose_hits[0].chat_id, loose.id);

        // Hostile input goes through the same quoting on this path too.
        for raw in ["\"*\"", "- : (", "**"] {
            search_messages_scoped(&conn, raw, Some("C:/client-a"), Some("coding"), 0, 10)
                .unwrap();
        }
    }

    /// The stop condition for 4.4: a common word must not simply hand back the
    /// newest rows. `bm25` scores a short document that is *about* the term above
    /// a long one that merely mentions it, so the top hit is the useful one and
    /// not the most recent one.
    #[test]
    fn a_common_word_puts_the_message_about_it_above_the_one_that_mentions_it() {
        let conn = mem();
        let chat = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        // Oldest first: the long, diluted message is written before the short one,
        // so recency order and relevance order disagree.
        insert_message(
            &conn,
            &chat.id,
            "user",
            Some(
                "notes about the quarter: the invoice went out late, the invoice \
                 number was wrong, we chased the invoice twice, and the invoice \
                 template needs changing",
            ),
            None,
            None,
            None,
            None,
        )
        .unwrap();
        let short = insert_message(
            &conn,
            &chat.id,
            "user",
            Some("invoice"),
            None,
            None,
            None,
            None,
        )
        .unwrap();

        let hits = search_messages(&conn, "invoice", 0, 10).unwrap();
        assert_eq!(hits.len(), 2, "got {hits:?}");
        assert_eq!(
            hits[0].message_id, short.id,
            "the shortest message about the term wins, not the newest: {hits:?}"
        );
    }

    /// Paging must be a partition of the ranked list: every page joins onto the
    /// next with no row repeated and none skipped. That only holds if the order
    /// is total, which is why the query carries a tiebreak — two rows scored the
    /// same are otherwise free to swap between calls, and a swap across a page
    /// boundary shows one row twice and hides another for ever.
    #[test]
    fn pages_of_a_search_join_up_without_gaps_or_repeats() {
        let conn = mem();
        let chat = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        for _ in 0..7 {
            // Deliberately identical texts, so every row scores exactly the same.
            insert_message(&conn, &chat.id, "user", Some("invoice"), None, None, None, None)
                .unwrap();
        }

        let all = search_messages(&conn, "invoice", 0, 20).unwrap();
        assert_eq!(all.len(), 7);

        let mut paged: Vec<String> = Vec::new();
        for offset in (0..7).step_by(3) {
            paged.extend(
                search_messages(&conn, "invoice", offset as i64, 3)
                    .unwrap()
                    .into_iter()
                    .map(|h| h.message_id),
            );
        }
        let ids: Vec<String> = all.into_iter().map(|h| h.message_id).collect();
        assert_eq!(paged, ids, "paging is the same list in the same order");

        // And the same page asked for twice is the same page, not a reshuffle.
        let first = search_messages(&conn, "invoice", 0, 2).unwrap();
        let again = search_messages(&conn, "invoice", 0, 2).unwrap();
        assert_eq!(
            first.iter().map(|h| &h.message_id).collect::<Vec<_>>(),
            again.iter().map(|h| &h.message_id).collect::<Vec<_>>()
        );
        // Past the end is empty, not an error.
        assert!(search_messages(&conn, "invoice", 99, 3).unwrap().is_empty());
    }

    #[test]
    fn a_rewind_restores_a_file_and_deletes_the_one_the_run_created() {        let conn = mem();
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
    fn a_run_in_flight_is_only_visible_until_it_ends() {
        let conn = mem();
        let chat = create_chat(&conn, "coding", Some("C:/proj"), "m", true, "high", "manual")
            .unwrap();

        assert!(interrupted_runs(&conn).unwrap().is_empty(), "nothing has run yet");

        mark_run_started(&conn, &chat.id, "run-1").unwrap();
        let found = interrupted_runs(&conn).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].chat_id, chat.id);
        assert_eq!(found[0].run_id, "run-1");
        assert!(!found[0].can_rewind, "this run touched no files");

        // A run that ends clears its own marker, whichever way it ended.
        mark_run_ended(&conn, &chat.id).unwrap();
        assert!(interrupted_runs(&conn).unwrap().is_empty());
    }

    #[test]
    fn an_interrupted_run_says_whether_rewinding_would_help() {
        let conn = mem();
        let chat = create_chat(&conn, "coding", Some("C:/proj"), "m", true, "high", "manual")
            .unwrap();
        mark_run_started(&conn, &chat.id, "run-1").unwrap();
        // The checkpoint is keyed by the run, so a pre-image from a *different*
        // run must not be mistaken for this one's.
        record_checkpoint(&conn, &chat.id, "run-2", "C:/proj/other.rs", Some("x")).unwrap();
        assert!(!interrupted_runs(&conn).unwrap()[0].can_rewind);

        record_checkpoint(&conn, &chat.id, "run-1", "C:/proj/main.rs", Some("x")).unwrap();
        assert!(interrupted_runs(&conn).unwrap()[0].can_rewind);
    }

    #[test]
    fn a_second_run_replaces_the_first_marker_rather_than_stacking() {
        let conn = mem();
        let chat = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        mark_run_started(&conn, &chat.id, "run-1").unwrap();
        mark_run_started(&conn, &chat.id, "run-2").unwrap();
        let found = interrupted_runs(&conn).unwrap();
        assert_eq!(found.len(), 1, "one row per chat, like the live registry");
        assert_eq!(found[0].run_id, "run-2");
    }

    /// The run ids still holding checkpoints, oldest first by insertion.
    fn checkpoint_runs(conn: &Connection, chat_id: &str) -> Vec<String> {
        let mut stmt = conn
            .prepare(
                "SELECT run_id FROM file_checkpoints WHERE chat_id = ?1
                 GROUP BY run_id ORDER BY max(rowid) ASC",
            )
            .unwrap();
        let rows = stmt.query_map([chat_id], |r| r.get(0)).unwrap();
        rows.collect::<rusqlite::Result<_>>().unwrap()
    }

    #[test]
    fn a_prune_keeps_the_runs_a_rewind_can_still_reach() {
        let conn = mem();
        let chat = create_chat(&conn, "coding", Some("C:/proj"), "m", true, "high", "manual")
            .unwrap();
        // More runs than the depth, each writing its own path. Every one lands in
        // the same second, which is the point: "newest" cannot be read off the
        // clock, so insertion order has to decide it.
        for i in 0..14 {
            record_checkpoint(
                &conn,
                &chat.id,
                &format!("run-{i:02}"),
                &format!("C:/proj/f{i}.rs"),
                Some("before"),
            )
            .unwrap();
        }

        let report = prune(&conn).unwrap();
        assert_eq!(report.checkpoints, 4, "14 runs pruned down to 10");
        assert_eq!(report.versions, 0);

        let kept = checkpoint_runs(&conn, &chat.id);
        assert_eq!(kept.len(), 10);
        assert!(!kept.contains(&"run-00".to_string()), "the oldest went");
        assert_eq!(kept.last().unwrap(), "run-13", "the newest stayed");

        // And the newest run's own pre-image is untouched, which is what makes it
        // still rewindable — the one thing the prune must not have broken.
        let newest_pre_images: i64 = conn
            .query_row(
                "SELECT count(*) FROM file_checkpoints WHERE run_id = 'run-13'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(newest_pre_images, 1);
    }

    #[test]
    fn a_prune_keeps_the_newest_versions_of_each_artifact() {
        let conn = mem();
        let chat = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        let artifact =
            create_artifact(&conn, &chat.id, None, "markdown", "Notes", "v1", "assistant").unwrap();
        for n in 2..=30 {
            add_artifact_version(&conn, &artifact.id, &format!("v{n}"), "assistant").unwrap();
        }
        // A second artifact must be pruned on its own terms, not against the
        // first one's numbering.
        let other =
            create_artifact(&conn, &chat.id, None, "markdown", "Other", "only", "assistant").unwrap();

        let report = prune(&conn).unwrap();
        assert_eq!(report.versions, 10, "30 versions pruned down to 20");

        let live = get_artifact_detail(&conn, &artifact.id, None).unwrap();
        assert_eq!(live.artifact.current_version, 30);
        assert_eq!(live.content, "v30", "the current version is still readable");
        assert_eq!(live.versions.len(), 20, "and it is the newest twenty listed");
        // Twenty kept out of thirty: versions 1..=10 went, so 11 is the oldest
        // that is still there — and the boundary is worth pinning, because an
        // off-by-one here is silent.
        assert_eq!(
            get_artifact_detail(&conn, &artifact.id, Some(11)).unwrap().content,
            "v11",
            "the oldest kept version"
        );
        assert!(get_artifact_detail(&conn, &artifact.id, Some(10)).is_err());
        // Untouched: one version is below the depth.
        assert!(get_artifact_detail(&conn, &other.id, None).is_ok());
    }

    #[test]
    fn pruning_nothing_is_not_an_error() {
        let conn = mem();
        let report = prune(&conn).unwrap();
        assert_eq!(report.total(), 0);
        assert!(!vacuum_if_worth_it(&conn, report.total()), "nothing to reclaim");
    }

    #[test]
    fn a_decision_is_written_down_and_read_back_newest_first() {
        let conn = mem();
        let chat = create_chat(&conn, "coding", Some("C:/proj"), "m", true, "high", "manual")
            .unwrap();
        log_decision(&conn, &chat.id, "run_command", ALLOWED, "once", "aaaa", "C:/proj").unwrap();
        log_decision(&conn, &chat.id, "write_file", DENIED, "denied", "bbbb", "C:/proj").unwrap();
        // A guard refusal never reaches a prompt, and that is exactly why it has
        // to be written down somewhere.
        log_decision(&conn, &chat.id, "run_command", BLOCKED, "formatting the system drive", "cccc", "C:/proj")
            .unwrap();

        // All three land in the same second, so "newest first" is insertion
        // order here — the same tie the prune has to break.
        let rows = recent_decisions(&conn, 10).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].decision, BLOCKED);
        assert_eq!(rows[0].note, "formatting the system drive");
        assert_eq!(rows[2].decision, ALLOWED);
        assert_eq!(rows[2].note, "once");
        assert_eq!(rows[2].chat_title, chat.title, "the title is joined, not copied");
        assert_eq!(rows[2].args_digest, "aaaa");
        // The limit is a limit, not the whole table.
        assert_eq!(recent_decisions(&conn, 2).unwrap().len(), 2);
    }

    #[test]
    fn the_log_outlives_the_chat_it_was_about() {
        let conn = mem();
        let chat = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        log_decision(&conn, &chat.id, "write_file", ALLOWED, "always", "aaaa", "").unwrap();

        delete_chat(&conn, &chat.id).unwrap();

        // The one deliberate exception to "deleting a chat takes everything with
        // it": an audit trail you can erase is not one.
        let rows = recent_decisions(&conn, 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].decision, ALLOWED);
        assert_eq!(rows[0].chat_title, "", "the chat is gone, so it reads as unnamed");
    }

    #[test]
    fn the_append_only_log_still_has_a_ceiling() {
        let conn = mem();
        let chat = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        for n in 0..1001 {
            log_decision(&conn, &chat.id, "write_file", ALLOWED, "once", &format!("{n:04}"), "")
                .unwrap();
        }

        let report = prune(&conn).unwrap();
        assert_eq!(report.decisions, 1, "one row past the ceiling");
        let rows = recent_decisions(&conn, 5000).unwrap();
        assert_eq!(rows.len(), 1000);
        // The newest survived, which is the end anyone reads from.
        assert_eq!(rows[0].args_digest, "1000");
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
    fn a_grant_covers_its_own_target_and_nothing_else() {
        let conn = mem();
        assert!(!is_tool_allowed(&conn, "/repo-a", "run_command", "cargo").unwrap());
        allow_tool(&conn, "/repo-a", "run_command", "cargo").unwrap();
        assert!(is_tool_allowed(&conn, "/repo-a", "run_command", "cargo").unwrap());
        // Exactly its own target: another program still asks...
        assert!(!is_tool_allowed(&conn, "/repo-a", "run_command", "npm").unwrap());
        // ...a call that names no target at all still asks (which is the rule
        // that stops a grant for one folder from covering a command)...
        assert!(!is_tool_allowed(&conn, "/repo-a", "run_command", "").unwrap());
        // ...and so does the same program in another folder.
        assert!(!is_tool_allowed(&conn, "/repo-b", "run_command", "cargo").unwrap());
        // A second target does not overwrite the first: the triple is the key.
        allow_tool(&conn, "/repo-a", "run_command", "npm").unwrap();
        assert!(is_tool_allowed(&conn, "/repo-a", "run_command", "cargo").unwrap());
        assert!(is_tool_allowed(&conn, "/repo-a", "run_command", "npm").unwrap());
        assert_eq!(all_allowed_tools(&conn).unwrap().len(), 2);
        // Revoking one asks again and leaves the other alone.
        revoke_tool(&conn, "/repo-a", "run_command", "cargo").unwrap();
        assert!(!is_tool_allowed(&conn, "/repo-a", "run_command", "cargo").unwrap());
        assert!(is_tool_allowed(&conn, "/repo-a", "run_command", "npm").unwrap());
        assert_eq!(
            list_allowed_tools(&conn, "/repo-a").unwrap(),
            vec!["run_command".to_string()]
        );
    }

    #[test]
    fn a_whole_tool_grant_still_covers_every_target_of_it() {
        let conn = mem();
        // What every grant meant before targets existed, and what a tool whose
        // arguments name no place still means: the tool itself.
        allow_tool(&conn, "/repo-a", "git_push", "").unwrap();
        assert!(is_tool_allowed(&conn, "/repo-a", "git_push", "").unwrap());
        assert!(is_tool_allowed(&conn, "/repo-a", "git_push", "anywhere at all").unwrap());
        // And it is still one tool, in one folder.
        assert!(!is_tool_allowed(&conn, "/repo-a", "git_commit", "").unwrap());
        assert!(!is_tool_allowed(&conn, "/repo-b", "git_push", "").unwrap());
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
                    kind: TURN,
                    priced: true,
                    prompt_tokens: 10,
                    completion_tokens: 5,
                    cache_hit_tokens: 0,
                    cache_miss_tokens: 0,
                    cost_usd: cost,
                },
                None,
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
    fn a_parked_request_lists_with_its_chat_and_is_forgettable() {
        let conn = mem();
        let chat = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        rename_chat(&conn, &chat.id, "Nightly build").unwrap();

        let payload = serde_json::json!({ "requestId": "r1", "tool": "run_command" });
        park_request(&conn, "r1", &chat.id, "approval", &payload).unwrap();

        let listed = list_parked(&conn).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].chat_title, "Nightly build", "the title is joined, not copied");
        assert_eq!(listed[0].status, "pending");
        assert_eq!(listed[0].payload["tool"], "run_command");

        // Forgetting twice is not an error: the run that wakes up and whoever
        // answered both come through here.
        forget_parked(&conn, "r1").unwrap();
        forget_parked(&conn, "r1").unwrap();
        assert!(list_parked(&conn).unwrap().is_empty());
    }

    #[test]
    fn a_park_left_by_a_crash_is_marked_lost_rather_than_deleted() {
        let conn = mem();
        let chat = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        park_request(&conn, "r1", &chat.id, "question", &serde_json::json!({})).unwrap();

        assert_eq!(lose_parked_in_flight(&conn).unwrap(), 1);
        let listed = list_parked(&conn).unwrap();
        assert_eq!(listed.len(), 1, "a lost park is still shown, not swept away");
        assert_eq!(listed[0].status, "lost");
        // Running it again finds nothing pending, so a second launch is quiet.
        assert_eq!(lose_parked_in_flight(&conn).unwrap(), 0);
    }

    #[test]
    fn deleting_a_chat_takes_its_parked_request_with_it() {
        let conn = mem();
        // `mem()` never turns foreign keys on; `open()` does, and the cascade is
        // the clause under test, so it is turned on here rather than assumed.
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        let chat = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        park_request(&conn, "r1", &chat.id, "approval", &serde_json::json!({})).unwrap();
        assert_eq!(list_parked(&conn).unwrap().len(), 1);

        // A park names a chat it cannot outlive: deleting the conversation must
        // not leave a "needs you" row pointing at nothing.
        conn.execute("DELETE FROM chats WHERE id = ?1", [&chat.id]).unwrap();
        assert!(list_parked(&conn).unwrap().is_empty());
    }

    #[test]
    fn only_tasks_whose_time_has_come_are_due() {
        let conn = mem();
        let soon = schedule(
            &conn, "Morning brief", "summarise the news", "daily", "09:00", 0, 1, 1_000,
        )
        .unwrap();
        let later = schedule(
            &conn, "Weekly review", "check the books", "weekly", "17:00", 2, 1, 9_000,
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
        let task = schedule(
            &conn, "Later", "do a thing", "daily", "09:00", 0, 1, 1,
        )
        .unwrap();
        reschedule(
            &conn, &task.id, "Later", "do a thing", "daily", "09:00", 0, 1, false, 1,
        )
        .unwrap();
        assert!(due_scheduled(&conn, 999_999).unwrap().is_empty());
        assert!(!list_scheduled(&conn).unwrap()[0].enabled);
    }

    #[test]
    fn advancing_a_task_records_the_run_and_moves_it_on() {
        let conn = mem();
        let task = schedule(
            &conn, "Brief", "p", "daily", "09:00", 0, 1, 1_000,
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
        let task = schedule(
            &conn, "Old", "old prompt", "daily", "09:00", 0, 1, 1_000,
        )
        .unwrap();
        reschedule(
            &conn, &task.id, "New", "new prompt", "weekly", "08:30", 2, 1, true,
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
        schedule(&conn, "  ", "p", "daily", "09:00", 0, 1, 1).unwrap();
        assert_eq!(list_scheduled(&conn).unwrap()[0].title, "Scheduled task");
    }

    #[test]
    fn a_trigger_keeps_what_it_watches_and_forgets_its_state_when_edited() {
        let conn = mem();
        let watching = triggers::Spec::new("file", "C:\\inbox", "*.csv", 120);
        let task = create_scheduled(
            &conn,
            "Watch inbox",
            "brief me",
            None,
            None,
            "daily",
            "09:00",
            0,
            1,
            &watching,
            "",
            1_000,
        )
        .unwrap();
        assert_eq!(task.kind, "file");
        assert_eq!(task.target, "C:\\inbox");
        assert_eq!(task.filter, "*.csv");
        assert_eq!(task.interval_secs, 120);
        assert!(
            task.last_state.is_none(),
            "a trigger that has just been made has never looked"
        );

        set_scheduled_state(&conn, &task.id, "a.csv:1", "armed").unwrap();
        let stored = &list_scheduled(&conn).unwrap()[0];
        assert_eq!(stored.last_state.as_deref(), Some("a.csv:1"));
        assert_eq!(stored.last_note, "armed");

        // An edit points the trigger somewhere else, and the fingerprint that
        // described the old folder goes with it: carried forward it would report
        // the new folder's contents as a change against the old folder's.
        update_scheduled(
            &conn,
            &task.id,
            "Watch repo",
            "brief me",
            None,
            None,
            "daily",
            "09:00",
            0,
            1,
            true,
            &triggers::Spec::new("git", "C:\\src", "main", 60),
            "deepseek-flash",
            2_000,
        )
        .unwrap();
        let after = &list_scheduled(&conn).unwrap()[0];
        assert_eq!(after.kind, "git");
        assert_eq!(after.target, "C:\\src");
        assert_eq!(after.filter, "main");
        assert!(after.last_state.is_none(), "an edit re-arms the trigger");
        // A pin given on an edit is written, not ignored: clearing one is a real
        // edit too, and leaving the column alone would make that impossible.
        assert_eq!(after.model, "deepseek-flash");
        assert_eq!(scheduled_model(&conn, &task.id).as_deref(), Some("deepseek-flash"));
    }

    #[test]
    fn a_task_can_pin_a_model_and_no_pin_reads_as_none() {
        let conn = mem();
        let task = schedule(&conn, "Nightly triage", "triage", "daily", "03:00", 0, 1, 1).unwrap();
        // Nothing was pinned, so the chat's own model is the answer — which is
        // what every task made before a task could name one gets.
        assert_eq!(task.model, "");
        assert_eq!(scheduled_model(&conn, &task.id), None);
        // A task that does not exist is the same answer: a pin that vanished is
        // not a reason to fail a turn.
        assert_eq!(scheduled_model(&conn, "no such task"), None);

        update_scheduled(
            &conn,
            &task.id,
            "Nightly triage",
            "triage",
            None,
            None,
            "daily",
            "03:00",
            0,
            1,
            true,
            &triggers::Spec::default(),
            "deepseek-flash",
            1_000,
        )
        .unwrap();
        assert_eq!(scheduled_model(&conn, &task.id).as_deref(), Some("deepseek-flash"));

        // And blanking it puts the task back on the chat's model rather than on a
        // model id that is the empty string.
        reschedule(&conn, &task.id, "Nightly triage", "triage", "daily", "03:00", 0, 1, true, 1_000)
            .unwrap();
        assert_eq!(scheduled_model(&conn, &task.id), None);
    }

    #[test]
    fn the_trigger_cap_counts_runs_inside_its_window_and_no_others() {
        let conn = mem();
        let chat = create_chat(&conn, "chat", None, "m", true, "high", "auto").unwrap();
        let task = create_scheduled(
            &conn,
            "Watch",
            "brief me",
            None,
            None,
            "daily",
            "09:00",
            0,
            1,
            &triggers::Spec::new("file", "C:\\inbox", "", 300),
            "",
            1_000,
        )
        .unwrap();

        // Three runs of this task, one of them older than the window.
        for (index, run) in ["r-1", "r-2", "r-3"].iter().enumerate() {
            record_turn_usage(
                &conn,
                &chat.id,
                run,
                &TurnUsage {
                    model: "m",
                    kind: TURN,
                    priced: true,
                    prompt_tokens: 1,
                    completion_tokens: 1,
                    cache_hit_tokens: 0,
                    cache_miss_tokens: 0,
                    cost_usd: 0.0,
                },
                Some(&task.id),
            )
            .unwrap();
            if index == 0 {
                conn.execute(
                    "UPDATE turn_usage SET created_at = 10 WHERE run_id = ?1",
                    [run],
                )
                .unwrap();
            }
        }

        assert_eq!(
            fires_since(&conn, &task.id, 1_000).unwrap(),
            2,
            "the run from before the hour is not counted"
        );
        assert_eq!(fires_since(&conn, &task.id, 0).unwrap(), 3);
        // Another task's runs are not this task's, which is the whole reason the
        // cap can be read off the run history instead of kept as a counter.
        assert_eq!(fires_since(&conn, "someone-else", 0).unwrap(), 0);
    }

    fn round(model: &str, prompt: u32, completion: u32, cost: f64) -> TurnUsage<'_> {
        TurnUsage {
            model,
            kind: TURN,
            priced: true,
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
        record_turn_usage(&conn, &chat.id, "run-1", &round("m", 100, 20, 0.25), None).unwrap();
        record_turn_usage(&conn, &chat.id, "run-1", &round("m", 200, 30, 0.5), None).unwrap();
        record_turn_usage(&conn, &chat.id, "run-1", &round("m", 50, 10, 0.25), None).unwrap();

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
    fn a_task_lists_its_runs_and_they_agree_with_the_spend_view() {
        let conn = mem();
        let chat = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();

        // Two runs of one task, and one ordinary turn that must not appear in its
        // list. One-off costs, so the totals below are sums of literals.
        record_turn_usage(&conn, &chat.id, "run-task-1", &round("m", 100, 20, 0.25), Some("t-1"))
            .unwrap();
        record_turn_usage(&conn, &chat.id, "run-task-2", &round("m", 200, 30, 0.5), Some("t-1"))
            .unwrap();
        record_turn_usage(&conn, &chat.id, "run-human", &round("m", 50, 10, 0.75), None).unwrap();

        let runs = task_runs(&conn, "t-1", 10).unwrap();
        assert_eq!(runs.len(), 2, "only the task's own runs");
        // Newest first, and the round trip through the row is what is asserted.
        assert_eq!(runs[0].run_id, "run-task-2");
        assert_eq!(runs[0].cost_usd, 0.5);
        assert_eq!(runs[0].rounds, 1);
        assert_eq!(runs[0].chat_title, "New chat");
        assert_eq!(runs[1].run_id, "run-task-1");

        // The task's runs are the same rows the spend view sums, so leaving the
        // ordinary turn out of the list and including it in the window are the
        // two halves of one agreement rather than two stories.
        let task_total: f64 = runs.iter().map(|r| r.cost_usd).sum();
        assert_eq!(task_total, 0.75);
        let report = spend_report(&conn, 0, 10).unwrap();
        assert_eq!(report.turns, 3);
        assert!((report.total_usd - (task_total + 0.75)).abs() < 1e-9, "{}", report.total_usd);
        assert!(
            runs.iter().all(|r| r.cost_usd != 0.75),
            "the turn the user typed must not be in the task's list"
        );

        // A task that has never run lists nothing rather than failing, and the
        // limit is honoured newest-first.
        assert!(task_runs(&conn, "t-2", 10).unwrap().is_empty());
        assert_eq!(task_runs(&conn, "t-1", 1).unwrap().len(), 1);
        assert_eq!(task_runs(&conn, "t-1", 1).unwrap()[0].run_id, "run-task-2");
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
            record_turn_usage(&conn, &chat.id, run, &round(model, 1_000, 100, cost), None).unwrap();
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
    fn the_recap_adds_up_and_agrees_with_the_spend_view() {
        let conn = mem();
        let chat = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();

        // One-off costs on different local days, so both groupings have something
        // to group, and round numbers so the sums are literals.
        for (run, days_ago, cost) in [("r1", 0, 0.5), ("r2", 0, 0.25), ("r3", 2, 0.125)] {
            record_turn_usage(&conn, &chat.id, run, &round("m", 100, 20, cost), None).unwrap();
            conn.execute(
                "UPDATE turn_usage SET created_at = ?2 WHERE run_id = ?1",
                params![run, now_ts() - days_ago * 86_400],
            )
            .unwrap();
        }

        let report = spend_report(&conn, 0, 10).unwrap();
        // Not bound to `recap`: a local of that name would shadow the function
        // for the rest of the test, including the empty-window call below.
        let shape = recap(&conn, 0, 10).unwrap();

        // The two views cannot tell different stories, and not by promise: the
        // recap takes its totals, days and conversations *from* `spend_report`
        // rather than querying again. Asserted anyway, because it is the item's
        // whole guarantee and a later rewrite must not quietly break it.
        assert_eq!(shape.turns, report.turns);
        assert_eq!(shape.total_usd, report.total_usd);
        assert_eq!(shape.prompt_tokens, report.prompt_tokens);
        assert_eq!(shape.days.len(), report.days.len());
        assert_eq!(shape.chats.len(), report.chats.len());

        // And the shape adds up to the numbers: the days and the hours are the
        // same turns, grouped two more ways. This is the part that can actually
        // break, so it is the part worth asserting.
        assert_eq!(shape.turns, 3);
        assert_eq!(shape.days.iter().map(|d| d.turns).sum::<i64>(), shape.turns);
        assert_eq!(shape.hours.iter().map(|h| h.turns).sum::<i64>(), shape.turns);
        let day_cost: f64 = shape.days.iter().map(|d| d.cost_usd).sum();
        assert!((day_cost - shape.total_usd).abs() < 1e-9, "{day_cost}");
        // Busiest hour first, and only hours that had a turn.
        assert!(shape.hours.windows(2).all(|w| w[0].turns >= w[1].turns));
        assert!(shape.hours.iter().all(|h| (0..24).contains(&h.hour)));
        assert_eq!(shape.hours.iter().map(|h| h.turns).sum::<i64>(), 3);

        // An empty window is empty — which is what lets the panel draw nothing
        // instead of a chart of zeros.
        let empty = recap(&conn, now_ts() + 86_400, 10).unwrap();
        assert_eq!(empty.turns, 0);
        assert!(empty.days.is_empty() && empty.hours.is_empty() && empty.chats.is_empty());
    }

    #[test]
    fn the_exported_audit_is_what_the_panels_show_and_carries_no_arguments() {
        let conn = mem();
        let chat = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        record_turn_usage(&conn, &chat.id, "r1", &round("m", 100, 20, 0.5), None).unwrap();
        // One turn in the window and one before it, so the equality below is
        // about the window as well as about the rows.
        record_turn_usage(&conn, &chat.id, "r0", &round("m", 10, 10, 9.0), None).unwrap();
        conn.execute("UPDATE turn_usage SET created_at = 100 WHERE run_id = 'r0'", [])
            .unwrap();

        // A decision whose arguments held a secret. Only the digest is written
        // down, which is the rule this file has to keep as it leaves the machine:
        // an approval can cover a command carrying a credential.
        let secret = serde_json::json!({ "command": "deploy --token hunter2" });
        let digest = crate::approvals::args_digest(&secret);
        log_decision(&conn, &chat.id, "run_command", ALLOWED, "once", &digest, "/work").unwrap();
        log_decision(&conn, &chat.id, "write_file", DENIED, "denied", "", "/work").unwrap();

        let since = now_ts() - 86_400;
        let export = audit(&conn, since, 100, 10).unwrap();

        // The export *is* the panels' reads rather than a second query that could
        // drift from them. Asserted anyway: that equality is the item's whole
        // guarantee, and a later rewrite could quietly break it.
        assert_eq!(
            serde_json::to_value(&export.decisions).unwrap(),
            serde_json::to_value(recent_decisions(&conn, 100).unwrap()).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&export.spend).unwrap(),
            serde_json::to_value(spend_report(&conn, since, 10).unwrap()).unwrap()
        );
        // The window is the caller's: the spend covers one turn, not two.
        assert_eq!(export.since, since);
        assert_eq!(export.spend.turns, 1);

        let text = serde_json::to_string_pretty(&export).unwrap();
        assert!(text.contains("run_command") && text.contains("write_file"));
        assert!(text.contains(&digest), "the digest is what the log holds");
        assert!(!text.contains("hunter2"), "an argument got into the export");
        assert!(!text.contains("deploy"), "an argument got into the export");
        assert_eq!(export.version, AUDIT_VERSION);
        // The call with arguments has a digest; the one without has none, and
        // neither has the arguments themselves.
        let digests: Vec<&str> = export
            .decisions
            .iter()
            .map(|row| row.args_digest.as_str())
            .collect();
        assert!(digests.contains(&digest.as_str()));
        assert!(digests.iter().any(|d| d.is_empty()));

        // The file is read by a person now and by whatever reads this format
        // later, so the keys have to be the camelCase ones it will look for.
        let shape = serde_json::to_value(&export).unwrap();
        assert_eq!(shape["exportedAt"], serde_json::json!(export.exported_at));
        assert!(shape["decisions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["argsDigest"].is_string()));
        assert_camel_case_keys(&shape, "audit");
    }

    #[test]
    fn a_notebook_keeps_what_it_holds_and_a_delete_takes_it_all() {
        let conn = mem();
        let notebook = create_notebook(&conn, "Thesis").unwrap();
        assert_eq!(notebook.title, "Thesis");
        // An empty title falls back rather than making the row unfindable, the
        // same rule a project follows.
        assert_eq!(create_notebook(&conn, "   ").unwrap().title, "New notebook");

        for (name, fingerprint) in [("a.pdf", "10-1"), ("b.md", "20-2"), ("c.txt", "30-3")] {
            create_source(
                &conn,
                &notebook.id,
                name,
                "file",
                "C:/origin",
                Some("C:/copy"),
                "body",
                fingerprint,
            )
            .unwrap();
        }
        let sources = list_sources(&conn, &notebook.id).unwrap();
        assert_eq!(sources.len(), 3, "all three are listed");
        assert_eq!(sources[0].name, "a.pdf", "in the order they were added");
        assert!(sources[0].enabled, "a new source is switched on");

        // The switch is reversible and keeps everything else — which is the whole
        // reason it is a column and not a delete.
        set_source_enabled(&conn, &sources[1].id, false).unwrap();
        let again = list_sources(&conn, &notebook.id).unwrap();
        assert!(!again[1].enabled);
        assert_eq!(
            again[1].extract.as_deref(),
            Some("body"),
            "switching a source off kept its text"
        );

        // A question, an answer, and something produced.
        append_notebook_message(&conn, &notebook.id, "user", "what changed?", &[]).unwrap();
        let answer = append_notebook_message(
            &conn,
            &notebook.id,
            "assistant",
            "Revenue rose [a.pdf].",
            &["a.pdf".to_string()],
        )
        .unwrap();
        assert_eq!(answer.seq, 2, "the second line goes after the first");
        assert_eq!(answer.citations, vec!["a.pdf".to_string()]);
        add_notebook_output(&conn, &notebook.id, "markdown", "Cards", "C:/copy/cards.md").unwrap();

        // The text and the fingerprint the digest describes are two facts, and
        // both round-trip — the second is what says whether the digest is stale.
        set_source_extract(&conn, &sources[0].id, "new body", "11-9").unwrap();
        set_source_digest(&conn, &sources[0].id, "A quarter of numbers.").unwrap();
        let row = get_source(&conn, &sources[0].id).unwrap().unwrap();
        assert_eq!(row.extract.as_deref(), Some("new body"));
        assert_eq!(row.fingerprint, "11-9");
        assert_eq!(row.digest, "A quarter of numbers.");
        assert!(row.digest_at.is_some(), "a digest records when it was made");

        // Removing one source takes only itself.
        remove_source(&conn, &sources[2].id).unwrap();
        assert_eq!(list_sources(&conn, &notebook.id).unwrap().len(), 2);

        // The notebook's own rows go together, by the foreign keys rather than by
        // a sweep somebody has to remember to write.
        delete_notebook(&conn, &notebook.id).unwrap();
        assert!(list_sources(&conn, &notebook.id).unwrap().is_empty());
        assert!(list_notebook_messages(&conn, &notebook.id).unwrap().is_empty());
        assert!(list_notebook_outputs(&conn, &notebook.id).unwrap().is_empty());
        assert!(get_notebook(&conn, &notebook.id).unwrap().is_none());
    }

    #[test]
    fn the_notebook_payloads_are_camel_case_and_never_carry_a_documents_text() {
        let conn = mem();
        let notebook = create_notebook(&conn, "Thesis").unwrap();
        let source = create_source(
            &conn,
            &notebook.id,
            "a.pdf",
            "file",
            "C:/a.pdf",
            Some("C:/copy/a.pdf"),
            "the whole document",
            "10-1",
        )
        .unwrap();
        let answer = append_notebook_message(
            &conn,
            &notebook.id,
            "assistant",
            "Yes [a.pdf].",
            &["a.pdf".to_string()],
        )
        .unwrap();
        let output =
            add_notebook_output(&conn, &notebook.id, "markdown", "Cards", "C:/copy/cards.md")
                .unwrap();

        for (label, value) in [
            ("notebook", serde_json::to_value(&notebook).unwrap()),
            ("source", serde_json::to_value(&source).unwrap()),
            ("message", serde_json::to_value(&answer).unwrap()),
            ("output", serde_json::to_value(&output).unwrap()),
        ] {
            assert_camel_case_keys(&value, label);
        }

        // The names the window actually reads, spelled the way it reads them.
        let shape = serde_json::to_value(&source).unwrap();
        assert!(shape["notebookId"].is_string());
        assert!(shape["digestAt"].is_null());
        assert_eq!(shape["enabled"], serde_json::json!(true));
        // And the document itself is not in it: a source's payload carries what it
        // contains, never what it says.
        assert!(shape.get("extract").is_none(), "{shape}");
        assert!(!shape.to_string().contains("the whole document"));
        // The citations a notebook's answer resolved ride with it, as a list.
        let answered = serde_json::to_value(&answer).unwrap();
        assert_eq!(answered["citations"], serde_json::json!(["a.pdf"]));
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
                priced: true,
            }],
            chats: vec![ChatSpend {
                chat_id: "chat-1".into(),
                title: "A conversation".into(),
                cost_usd: 1.25,
                turns: 3,
            }],
            sections: vec![SectionTokens {
                kind: "coding".into(),
                turns: 3,
                prompt_tokens: 900,
                completion_tokens: 60,
            }],
            kinds: vec![KindSpend {
                kind: TURN.into(),
                cost_usd: 1.25,
                calls: 3,
                priced: true,
            }],
        };

        let value = serde_json::to_value(&report).unwrap();
        // Named keys first, so the walk below cannot pass over an empty object.
        assert_eq!(value["totalUsd"], serde_json::json!(1.25));
        assert_eq!(value["promptTokens"], serde_json::json!(900));
        assert_eq!(value["days"][0]["costUsd"], serde_json::json!(1.25));
        assert_eq!(value["chats"][0]["chatId"], serde_json::json!("chat-1"));
        assert_eq!(value["sections"][0]["promptTokens"], serde_json::json!(900));
        assert_eq!(value["kinds"][0]["costUsd"], serde_json::json!(1.25));
        assert_eq!(value["models"][0]["priced"], serde_json::json!(true));
        assert_camel_case_keys(&value, "report");

        // The recap is the same family under the same rule, and its `hours` array
        // is the one shape nothing else in the app has — so it is walked too.
        let recap = Recap {
            since: 1,
            total_usd: 1.25,
            turns: 3,
            prompt_tokens: 900,
            completion_tokens: 60,
            days: Vec::new(),
            hours: vec![HourCount { hour: 9, turns: 3 }],
            chats: vec![ChatSpend {
                chat_id: "chat-1".into(),
                title: "A conversation".into(),
                cost_usd: 1.25,
                turns: 3,
            }],
        };
        let recap_value = serde_json::to_value(&recap).unwrap();
        assert_eq!(recap_value["totalUsd"], serde_json::json!(1.25));
        assert_eq!(recap_value["promptTokens"], serde_json::json!(900));
        assert_eq!(recap_value["hours"][0]["turns"], serde_json::json!(3));
        assert_eq!(recap_value["chats"][0]["chatId"], serde_json::json!("chat-1"));
        assert_camel_case_keys(&recap_value, "recap");
    }

    #[test]
    fn the_sections_are_a_split_of_the_totals_rather_than_a_second_story() {
        let conn = mem();
        let coding = create_chat(&conn, "coding", Some("/repo"), "m", true, "high", "manual").unwrap();
        let chat = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();

        // One-off rows with round numbers, so every expectation below is a sum of
        // literals rather than a restatement of the recurrence rules.
        let turn = |chat_id: &str, prompt: u32, completion: u32| {
            record_turn_usage(
                &conn,
                chat_id,
                &uuid::Uuid::new_v4().to_string(),
                &TurnUsage {
                    model: "m",
                    kind: TURN,
                    priced: true,
                    prompt_tokens: prompt,
                    completion_tokens: completion,
                    cache_hit_tokens: 0,
                    cache_miss_tokens: 0,
                    cost_usd: 0.5,
                },
                None,
            )
            .unwrap();
        };
        turn(&coding.id, 900, 100);
        turn(&coding.id, 900, 100);
        turn(&chat.id, 200, 400);

        let report = spend_report(&conn, 0, 10).unwrap();
        assert_eq!(report.prompt_tokens, 2000);
        assert_eq!(report.completion_tokens, 600);
        assert_eq!(report.turns, 3);

        let coding_row = report.sections.iter().find(|s| s.kind == "coding").expect("a coding group");
        let chat_row = report.sections.iter().find(|s| s.kind == "chat").expect("a chat group");
        assert_eq!((coding_row.turns, coding_row.prompt_tokens, coding_row.completion_tokens), (2, 1800, 200));
        assert_eq!((chat_row.turns, chat_row.prompt_tokens, chat_row.completion_tokens), (1, 200, 400));
        // Coding is the input-heavy half and a conversation the output-heavy one —
        // the reason the split is worth showing at all.
        assert!(coding_row.prompt_tokens > chat_row.prompt_tokens);
        assert!(chat_row.completion_tokens > coding_row.completion_tokens);

        // The panel draws these as slices of the totals above them, so they have
        // to *be* the totals — the same rule that keeps `spend_report` and the
        // sidebar agreeing.
        assert_eq!(
            report.sections.iter().map(|s| s.prompt_tokens).sum::<i64>(),
            report.prompt_tokens,
        );
        assert_eq!(
            report.sections.iter().map(|s| s.completion_tokens).sum::<i64>(),
            report.completion_tokens,
        );
        assert_eq!(report.sections.iter().map(|s| s.turns).sum::<i64>(), report.turns);
    }

    #[test]
    fn an_overhead_call_is_on_the_same_bill_as_the_turn_that_caused_it() {
        let conn = mem();
        let chat = create_chat(&conn, "chat", None, "m", true, "high", "manual").unwrap();
        // One turn, and the summary the automatic fold made of it: two rows, one
        // bill. Round numbers, so the expectations below are sums of literals.
        record_turn_usage(&conn, &chat.id, "run-1", &round("m", 100, 20, 0.5), None).unwrap();
        let overhead = |model: &'static str, kind: &'static str, priced: bool, prompt: u32| TurnUsage {
            model,
            kind,
            priced,
            prompt_tokens: prompt,
            completion_tokens: 10,
            cache_hit_tokens: 0,
            cache_miss_tokens: prompt,
            cost_usd: if priced { 0.2 } else { 0.0 },
        };
        record_turn_usage(&conn, &chat.id, "summary:abc", &overhead("m", SUMMARY, true, 40), None)
            .unwrap();

        let report = spend_report(&conn, 0, 10).unwrap();
        // The call nobody typed is in the total and named in the split, which is
        // the whole point of the row.
        assert_eq!(report.total_usd, 0.7);
        let turn_row = report.kinds.iter().find(|k| k.kind == TURN).expect("the turn");
        let summary_row = report.kinds.iter().find(|k| k.kind == SUMMARY).expect("the summary");
        assert_eq!((turn_row.cost_usd, turn_row.calls), (0.5, 1));
        assert_eq!((summary_row.cost_usd, summary_row.calls), (0.2, 1));
        // And together they *are* the total, which is what makes the split
        // checkable rather than decorative.
        let summed: f64 = report.kinds.iter().map(|k| k.cost_usd).sum();
        assert!((summed - report.total_usd).abs() < 1e-9, "the kinds are the bill");

        // An unpriced call is on the bill too: its tokens counted, its money
        // unknown rather than zero, and no figure invented for it.
        record_turn_usage(
            &conn,
            &chat.id,
            "summary:def",
            &overhead("a-model-with-no-price", SUMMARY, false, 1_000),
            None,
        )
        .unwrap();
        let report = spend_report(&conn, 0, 10).unwrap();
        let unpriced = report
            .models
            .iter()
            .find(|m| m.model == "a-model-with-no-price")
            .expect("an unpriced model is still on the bill");
        assert!(!unpriced.priced, "and it says the money is unknown");
        assert_eq!(unpriced.turns, 1);
        assert_eq!(report.prompt_tokens, 100 + 40 + 1_000);
        assert_eq!(report.total_usd, 0.7, "an unknown price must not be counted as free money");
        assert!(
            !report.kinds.iter().find(|k| k.kind == SUMMARY).unwrap().priced,
            "one unpriced summary makes the group unpriced, not averaged away"
        );
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
        record_turn_usage(&conn, &chat.id, "run-1", &round("flash", 10, 5, 0.25), None).unwrap();
        let report = spend_report(&conn, 0, 10).unwrap();
        assert_eq!(report.turns, 1);
        assert_eq!(report.total_usd, 0.25);
        // The `kind` and `priced` ALTERs ran on this file too: a row written
        // through the upgraded table is a priced turn, which is what the two
        // defaults say, and reading them back through `spend_report` is the guard
        // that the columns really are selectable here.
        assert_eq!(report.kinds.len(), 1);
        assert_eq!(report.kinds[0].kind, TURN);
        assert!(report.kinds[0].priced);
        assert!(report.models[0].priced);
        // The chat's own title comes from the join, which also proves `chats`
        // survived the batch on this file.
        assert_eq!(report.chats[0].title, "From before usage existed");

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
