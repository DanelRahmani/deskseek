//! A task that starts because something changed, rather than at a time.
//!
//! `schedule.rs` answers "when does this fire next" for a calendar. This answers
//! it for a *change*: it looks at what a trigger watches and compares what it
//! sees with what it saw last time.
//!
//! Two rules hold everything here together, and both are why this is one module
//! rather than four.
//!
//! The first is that **a trigger is not a second kind of task**. It is the same
//! row in `scheduled_tasks` with a different `kind`, so it keeps the same prompt,
//! the same chat-or-project scope, the same `fire_scheduled` run path, the same
//! run history and the same place in the panel. What changes is only what makes
//! it due — and the consequence is that `next_run_at` means "next *check*".
//!
//! The second is that **a poll is not a fire**. `last_state` holds the last
//! fingerprint we saw, and [`decide`] fires only when a fresh look differs from
//! it. A condition that is true is not the same as a condition that just *became*
//! true; without that distinction a folder that stays busy, or a page whose text
//! merely exists, would start a run every interval, forever. A `NULL` state means
//! "never looked": the first look stores the fingerprint without firing, which is
//! also why a trigger added while the app was shut starts at most one run when it
//! opens instead of replaying the whole time it was closed.
//!
//! The checks are deliberately a **poll** on the task's own interval rather than
//! OS watches — which is the one place this deviates from the plan's guess of
//! `fs_watch.rs`. A folder that writes fifty files inside one interval is then
//! one change and one run rather than fifty, there is no baseline to reconstruct
//! for the time the app spent closed, and a trigger's folder does not have to be
//! watched by the file tree's refcounted watcher, which belongs to the chats that
//! are open rather than to the tasks that are armed.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::time::UNIX_EPOCH;

use tauri::{AppHandle, Manager};

use crate::commands;
use crate::store::db;
use crate::tools::browse;
use crate::AppState;

/// The scheduler ticks every 30 seconds, so nothing can be noticed faster than
/// this however small an interval is set to — clamping says so out loud instead
/// of silently rounding up.
pub const MIN_INTERVAL_SECS: i64 = 30;
/// Five minutes. Long enough that a folder being watched is not read constantly,
/// short enough that "when a new file lands" still feels like it.
pub const DEFAULT_INTERVAL_SECS: i64 = 300;
/// At most this many runs an hour, however much the thing being watched flaps.
/// The cap *delays* rather than drops: a change it refuses to fire on is left
/// pending, so it fires as soon as the hour clears. It holds for a pushed event
/// too, or a chatty server would be a way round it.
pub const MAX_FIRES_PER_HOUR: i64 = 6;
/// For a trigger that is pushed rather than polled, so the tick can never find it
/// due.
pub const NEVER: i64 = i64::MAX;

/// One hour, in the units every timestamp here uses.
const HOUR: i64 = 3_600;
/// Most directory entries one fingerprint reads. A folder with a hundred
/// thousand files would otherwise be read on every tick.
const MAX_ENTRIES: usize = 2_000;

/// Which change a trigger waits for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A file appears in, or changes inside, a folder.
    File,
    /// A git ref resolves to a different commit.
    Git,
    /// A page's text changes, or comes to say something it did not before.
    Url,
    /// A server sends a notification. Pushed by the server, never polled.
    Mcp,
}

impl Kind {
    /// `None` for a cadence, and for anything unrecognised — so a task whose
    /// `kind` was hand-edited into nonsense keeps running on its calendar rather
    /// than silently never firing.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "file" => Some(Self::File),
            "git" => Some(Self::Git),
            "url" => Some(Self::Url),
            "mcp" => Some(Self::Mcp),
            _ => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Git => "git",
            Self::Url => "url",
            Self::Mcp => "mcp",
        }
    }

    /// Whether the tick has to look at it. An `mcp` trigger is woken by the
    /// server, so it is never due and the tick must leave it alone.
    pub fn polled(self) -> bool {
        !matches!(self, Self::Mcp)
    }

    /// What this kind watches, for the form's label and for an error message.
    pub fn target_label(self) -> &'static str {
        match self {
            Self::File => "folder",
            Self::Git => "repository",
            Self::Url => "URL",
            Self::Mcp => "server",
        }
    }
}

/// The watching half of a scheduled task: what it is waiting for.
///
/// Carried as one value through the create and update paths so a trigger's four
/// fields cannot be half-applied — a kind with no target, or a target left at
/// the interval the previously edited task happened to use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    /// `None` is a cadence: the calendar in `schedule.rs` decides when it runs.
    pub kind: Option<Kind>,
    /// Folder, repository, URL or server name — what is watched.
    pub target: String,
    /// Glob, ref name, the text to look for, or the notification method. Blank
    /// means "any file", "HEAD", "any change to the page", and "anything this
    /// server sends".
    pub filter: String,
    /// Seconds between checks. Ignored by an `mcp` trigger, which is pushed.
    pub interval_secs: i64,
}

impl Default for Spec {
    fn default() -> Self {
        Self {
            kind: None,
            target: String::new(),
            filter: String::new(),
            interval_secs: DEFAULT_INTERVAL_SECS,
        }
    }
}

impl Spec {
    pub fn new(kind: &str, target: &str, filter: &str, interval_secs: i64) -> Self {
        Self {
            kind: Kind::parse(kind),
            target: target.trim().to_string(),
            filter: filter.trim().to_string(),
            interval_secs,
        }
    }

    /// The stored name for what makes this due — `"cadence"` for the calendar,
    /// which is the one place that name is decided.
    pub fn kind_id(&self) -> &'static str {
        self.kind.map(|kind| kind.id()).unwrap_or("cadence")
    }

    /// Every trigger needs something to look at. The filter is optional: a blank
    /// one means "any file in this folder", "whatever HEAD points at", and "any
    /// change to the page".
    pub fn validate(&self) -> Result<(), String> {
        let Some(kind) = self.kind else {
            return Ok(());
        };
        if self.target.trim().is_empty() {
            return Err(format!(
                "A trigger needs something to watch: a {}.",
                kind.target_label()
            ));
        }
        Ok(())
    }

    /// Never faster than the tick that would have to notice it.
    pub fn interval(&self) -> i64 {
        self.interval_secs.max(MIN_INTERVAL_SECS)
    }

    /// When a trigger first looks, or `None` for a cadence — where the caller
    /// asks `schedule::next_run_at` for the calendar instead.
    ///
    /// A new trigger looks soon rather than at the next round hour: the user set
    /// it up to watch something now, and waiting until 09:00 to arm it would make
    /// an afternoon's edit look like it had not saved. An `mcp` trigger has no
    /// clock at all, so it is never due and only the server wakes it.
    pub fn first_check(&self, now: i64) -> Option<i64> {
        let kind = self.kind?;
        Some(if kind.polled() {
            now + self.interval()
        } else {
            NEVER
        })
    }
}

/// What a fresh fingerprint means against the one that was stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Nothing to compare against: store it and do not fire. The first look
    /// after a trigger is made, or after one is edited, is this one.
    Arm,
    /// Exactly what was there last time. Nothing happened.
    Same,
    /// Different from last time. Something happened.
    Fire,
}

/// The whole edge rule, as one function over two strings.
///
/// Pure, so "a value that stays true fires once and a value that flaps fires on
/// each transition" is pinned by a test rather than by waiting for a folder.
pub fn decide(previous: Option<&str>, current: &str) -> Decision {
    match previous {
        None => Decision::Arm,
        Some(last) if last == current => Decision::Same,
        Some(_) => Decision::Fire,
    }
}

/// What a folder looks like: every matching entry's name and modification time,
/// sorted. A new file moves it and a changed file moves it; a file that merely
/// sits there does not.
///
/// An empty filter matches every entry in the folder. The glob is the file tree's
/// own matcher rather than a second one, so `*.csv` means the same thing here as
/// it does in a search.
pub fn file_fingerprint(dir: &Path, filter: &str) -> Result<String, String> {
    let pattern = {
        let filter = filter.trim();
        if filter.is_empty() { "*" } else { filter }
    };
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;

    let mut seen: Vec<String> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
        let name = entry.file_name().to_string_lossy().to_string();
        if !crate::tools::walk::wildcard_match(pattern, &name) {
            continue;
        }
        // Seconds, because that is the resolution this needs: a file written
        // twice inside one second is one change, and the tick could not tell
        // them apart anyway.
        let modified = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|since| since.as_secs())
            .unwrap_or(0);
        seen.push(format!("{name}:{modified}"));
        if seen.len() >= MAX_ENTRIES {
            break;
        }
    }
    // `read_dir` promises no order, so the list is sorted before it is joined:
    // an unchanged folder has to produce an unchanged string.
    seen.sort();
    Ok(seen.join("\n"))
}

/// A git ref as it resolves right now — the commit the branch points at.
///
/// `git` is asked rather than `.git` being read, because the answer is not in one
/// file: a branch may be packed, HEAD may be detached, and a worktree keeps its
/// refs somewhere else again. The tool that already knows is the one to ask.
pub async fn git_fingerprint(repo: &Path, reference: &str) -> Result<String, String> {
    let reference = {
        let reference = reference.trim();
        if reference.is_empty() { "HEAD" } else { reference }
    };
    let output = tokio::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("rev-parse")
        .arg(reference)
        .output()
        .await
        .map_err(|e| format!("could not run git: {e}"))?;

    if !output.status.success() {
        let why = String::from_utf8_lossy(&output.stderr);
        let why = why.trim();
        return Err(format!(
            "git rev-parse {reference} failed{}",
            if why.is_empty() {
                String::new()
            } else {
                format!(": {why}")
            }
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// What a page's text means.
///
/// With a filter, *whether* the page says it — the predicate case, which fires on
/// the transition to present. Without one, a hash of the text, so any change at
/// all moves it — the "tell me when this changes" case. Two branches of one rule:
/// both are something a later look can differ from.
///
/// Pure, so the predicate is pinned by a test rather than by a server.
pub fn page_fingerprint(text: &str, filter: &str) -> String {
    let needle = filter.trim();
    if needle.is_empty() {
        return format!("{:016x}", hash(text));
    }
    // Case-insensitive, because a page's own capitalisation is not what anyone
    // means by "tell me when it says this".
    let found = text.to_lowercase().contains(&needle.to_lowercase());
    if found { "yes" } else { "no" }.to_string()
}

/// What a page says right now, fetched through the same `browse::fetch_url` the
/// `fetch_url` tool uses — so one place owns "how this app reads a page" and a
/// monitor sees what the model would have seen.
pub async fn url_fingerprint(
    http: &reqwest::Client,
    url: &str,
    filter: &str,
) -> Result<String, String> {
    Ok(page_fingerprint(&browse::fetch_url(http, url).await?, filter))
}

/// Not cryptographic and does not need to be: this only has to notice that two
/// pages differ, not resist anybody forging one that does not.
fn hash(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// Looks at what a trigger watches, right now.
async fn look(
    http: &reqwest::Client,
    kind: Kind,
    task: &db::ScheduledRow,
) -> Result<String, String> {
    let target = task.target.trim();
    if target.is_empty() {
        return Err(format!(
            "nothing is being watched — set a {}",
            kind.target_label()
        ));
    }
    match kind {
        Kind::File => file_fingerprint(Path::new(target), &task.filter),
        Kind::Git => git_fingerprint(Path::new(target), &task.filter).await,
        Kind::Url => url_fingerprint(http, target, &task.filter).await,
        // Unreachable: `tick` returns before this for a pushed trigger, and this
        // says so rather than pretending an MCP event has a fingerprint.
        Kind::Mcp => Err("an MCP event is pushed by the server, not checked".to_string()),
    }
}

/// One due trigger: look, compare, and maybe start a run.
///
/// Takes the whole app because the run path is the ordinary one —
/// `commands::fire_scheduled`, the same function the calendar uses — so a trigger
/// parks, notifies and is billed exactly like a cadence.
pub async fn tick(app: &AppHandle, state: &AppState, task: &db::ScheduledRow) {
    let Some(kind) = Kind::parse(&task.kind) else {
        return;
    };
    if !kind.polled() {
        return;
    }

    let now = db::now_ts();
    // Move the next check on *first*, exactly as the calendar path does: a check
    // that takes longer than the interval must not be started again by the tick
    // that follows it.
    let next = now + task.interval_secs.max(MIN_INTERVAL_SECS);
    touch(state, &task.id, next, now);

    let fingerprint = match look(&state.http, kind, task).await {
        Ok(fingerprint) => fingerprint,
        Err(err) => {
            // A trigger that cannot look has to say so, or a mistyped folder is a
            // task that silently never runs and looks like nothing is wrong.
            note(state, &task.id, &format!("could not check: {err}"));
            return;
        }
    };

    match decide(task.last_state.as_deref(), &fingerprint) {
        Decision::Arm => {
            remember(state, &task.id, &fingerprint, "armed — waiting for a change");
        }
        Decision::Same => note(state, &task.id, "no change"),
        Decision::Fire => {
            let recent = {
                let conn = state.db.lock().unwrap();
                db::fires_since(&conn, &task.id, now - HOUR).unwrap_or(0)
            };
            if recent >= MAX_FIRES_PER_HOUR {
                // The fingerprint is deliberately *not* stored, so the change is
                // still pending: it fires as soon as the hour clears rather than
                // being thrown away because it arrived during a burst.
                note(
                    state,
                    &task.id,
                    &format!("changed, but {MAX_FIRES_PER_HOUR} runs an hour is the cap — waiting"),
                );
                return;
            }
            remember(state, &task.id, &fingerprint, "changed — starting a run");
            if let Err(err) = commands::fire_scheduled(app, state, task).await {
                eprintln!("trigger {} could not start a run: {err}", task.id);
            }
        }
    }
}

/// Advances a trigger's next check.
fn touch(state: &AppState, id: &str, next_run_at: i64, checked_at: i64) {
    let conn = state.db.lock().unwrap();
    if let Err(err) = db::touch_scheduled(&conn, id, next_run_at, checked_at) {
        eprintln!("could not advance trigger {id}: {err}");
    }
}

/// The note on its own, for a look that did not move the fingerprint.
fn note(state: &AppState, id: &str, text: &str) {
    let conn = state.db.lock().unwrap();
    let _ = db::set_scheduled_note(&conn, id, text);
}

/// Stores a new fingerprint *and* says what happened, together: the state is the
/// thing the next tick compares against, so it may not lag a fire.
fn remember(state: &AppState, id: &str, fingerprint: &str, note: &str) {
    let conn = state.db.lock().unwrap();
    let _ = db::set_scheduled_state(&conn, id, fingerprint, note);
}

/// The tasks a server's notification should wake: the server's name has to match,
/// and so does the method — unless the filter is blank or `*`, which is how a task
/// says "anything this server sends".
///
/// Pure over the rows, so the matching rule is testable without a server, a
/// socket or a handshake.
pub fn matching_tasks<'a>(
    rows: &'a [db::ScheduledRow],
    server: &str,
    method: &str,
) -> Vec<&'a db::ScheduledRow> {
    rows.iter()
        .filter(|row| row.enabled)
        .filter(|row| Kind::parse(&row.kind) == Some(Kind::Mcp))
        .filter(|row| row.target.trim().eq_ignore_ascii_case(server.trim()))
        .filter(|row| {
            let filter = row.filter.trim();
            filter.is_empty() || filter == "*" || filter.eq_ignore_ascii_case(method.trim())
        })
        .collect()
}

/// A server pushed a notification: wake every task that asked for it.
///
/// The counterpart of `tick` for the one kind that is pushed rather than polled.
/// It reads the same table, honours the same cap and starts a run through the
/// same `fire_scheduled`, so a server-woken task parks, notifies and is billed
/// exactly like one that woke on a clock.
///
/// Nothing is buffered and nothing is replayed. A notification that arrives while
/// the app is closed is gone: there was no listener, and inventing a backlog would
/// mean a server able to queue runs inside DeskSeek — which is the archive's open
/// decision, taken here as "dropped".
pub async fn on_mcp_event(app: &AppHandle, server: &str, method: &str) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let now = db::now_ts();
    let woken: Vec<db::ScheduledRow> = {
        let conn = state.db.lock().unwrap();
        let rows = db::list_scheduled(&conn).unwrap_or_default();
        matching_tasks(&rows, server, method)
            .into_iter()
            .cloned()
            .collect()
    };

    for task in woken {
        let recent = {
            let conn = state.db.lock().unwrap();
            db::fires_since(&conn, &task.id, now - HOUR).unwrap_or(0)
        };
        if recent >= MAX_FIRES_PER_HOUR {
            note(
                &state,
                &task.id,
                &format!(
                    "{server} sent {method}, but {MAX_FIRES_PER_HOUR} runs an hour is the cap — waiting"
                ),
            );
            continue;
        }
        {
            let conn = state.db.lock().unwrap();
            // `NEVER` rather than a time: a pushed trigger has no next check, and
            // recording the run must not make it due.
            let _ = db::mark_scheduled_ran(&conn, &task.id, NEVER, now);
            let _ = db::set_scheduled_note(
                &conn,
                &task.id,
                &format!("{server} sent {method} — starting a run"),
            );
        }
        if let Err(err) = commands::fire_scheduled(app, &state, &task).await {
            eprintln!("trigger {} could not start a run: {err}", task.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_condition_that_stays_true_fires_once() {
        // The whole point of the edge rule. A first look arms; every later look
        // at the same value is nothing.
        assert_eq!(decide(None, "yes"), Decision::Arm);
        assert_eq!(decide(Some("yes"), "yes"), Decision::Same);
        assert_eq!(decide(Some("yes"), "yes"), Decision::Same);
    }

    #[test]
    fn a_condition_that_flaps_fires_on_each_transition() {
        assert_eq!(decide(Some("no"), "yes"), Decision::Fire);
        assert_eq!(decide(Some("yes"), "no"), Decision::Fire);
        // An empty fingerprint is still a fingerprint: a folder that becomes
        // empty is a change like any other.
        assert_eq!(decide(Some("yes"), ""), Decision::Fire);
        assert_eq!(decide(Some(""), ""), Decision::Same);
    }

    #[test]
    fn a_file_trigger_notices_a_new_file_and_not_a_still_one() {
        let dir = std::env::temp_dir().join(format!("deskseek-trigger-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch folder");
        std::fs::write(dir.join("one.csv"), "a").expect("a file");

        let first = file_fingerprint(&dir, "*.csv").expect("a fingerprint");
        assert_eq!(
            file_fingerprint(&dir, "*.csv").expect("a fingerprint"),
            first,
            "reading the same folder twice must not look like a change"
        );
        assert_eq!(decide(Some(&first), &first), Decision::Same);

        // A file the filter does not match is not this trigger's business.
        std::fs::write(dir.join("notes.txt"), "b").expect("a file");
        assert_eq!(
            file_fingerprint(&dir, "*.csv").expect("a fingerprint"),
            first,
            "a file outside the filter is not a change"
        );

        std::fs::write(dir.join("two.csv"), "c").expect("a file");
        let second = file_fingerprint(&dir, "*.csv").expect("a fingerprint");
        assert_ne!(second, first, "a new file is a change");
        assert_eq!(decide(Some(&first), &second), Decision::Fire);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_empty_filter_matches_every_name() {
        let dir = std::env::temp_dir().join(format!("deskseek-trigger-all-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch folder");
        std::fs::write(dir.join("anything.bin"), "a").expect("a file");

        let print = file_fingerprint(&dir, "   ").expect("a fingerprint");
        assert!(
            print.starts_with("anything.bin:"),
            "a blank filter watches the whole folder, got {print:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_that_is_not_there_is_an_error_and_not_an_empty_fingerprint() {
        // An empty fingerprint would read as "the folder is empty", which is a
        // different thing from "the folder is gone" — and the panel shows one of
        // them as a working trigger.
        let missing = std::env::temp_dir().join("deskseek-trigger-missing-hopefully");
        let _ = std::fs::remove_dir_all(&missing);
        assert!(file_fingerprint(&missing, "*").is_err());
    }

    #[test]
    fn a_trigger_needs_something_to_watch() {
        // Each kind is refused by the name of what it wants, so the sentence in
        // the panel says which field is missing rather than "invalid".
        for (kind, noun) in [("file", "folder"), ("git", "repository"), ("url", "URL")] {
            let err = Spec::new(kind, "  ", "", DEFAULT_INTERVAL_SECS)
                .validate()
                .unwrap_err();
            assert!(err.contains(noun), "{kind} should name a {noun}, got {err:?}");
        }

        // A cadence has no target to be missing.
        assert!(Spec::default().validate().is_ok());
        assert!(Spec::new("git", "C:\\src", "main", 60).validate().is_ok());
    }

    #[test]
    fn a_cadence_and_a_misspelt_kind_both_keep_the_calendar() {
        assert_eq!(Kind::parse("cadence"), None);
        assert_eq!(Kind::parse("every day"), None);
        assert_eq!(Kind::parse(" FILE "), Some(Kind::File));
        // An unparseable kind falls back to the calendar rather than to a trigger
        // that could never fire.
        assert!(Spec::new("frile", "C:\\x", "", 60).first_check(1_000).is_none());
    }

    #[test]
    fn a_trigger_looks_soon_and_never_faster_than_the_tick() {
        let spec = Spec::new("file", "C:\\x", "", 1);
        assert_eq!(
            spec.first_check(1_000),
            Some(1_000 + MIN_INTERVAL_SECS),
            "a one-second interval is clamped to the tick"
        );
        assert_eq!(
            Spec::new("file", "C:\\x", "", 900).first_check(1_000),
            Some(1_900)
        );
        assert!(Spec::default().first_check(1_000).is_none());
    }

    #[test]
    fn a_page_condition_fires_on_the_transition_it_describes() {
        // A filter is a predicate: the page says it or it does not, so a page
        // that keeps saying the same thing is Same and only becoming true is a
        // Fire.
        let says = |text: &str| page_fingerprint(text, "in stock");
        assert_eq!(says("Sold out and unlikely to return"), "no");
        assert_eq!(
            says("Now IN STOCK!"),
            "yes",
            "the page's own capitalisation is not the thing anyone means"
        );
        assert_eq!(says("In stock"), "yes");
        assert_eq!(
            decide(Some(&says("Sold out")), &says("In stock")),
            Decision::Fire,
            "becoming true is the fire"
        );
        assert_eq!(
            decide(Some(&says("In stock")), &says("In stock")),
            Decision::Same,
            "staying true is not"
        );
    }

    #[test]
    fn an_empty_page_filter_watches_the_whole_page() {
        // The other half of the same rule: no filter means the fingerprint is a
        // hash of the text, so any change at all moves it.
        let before = page_fingerprint("Price: 10", "");
        assert_eq!(
            page_fingerprint("Price: 10", "   "),
            before,
            "a blank filter and no filter are the same trigger"
        );
        let after = page_fingerprint("Price: 11", "");
        assert_ne!(after, before, "one character is a change");
        assert_eq!(decide(Some(&before), &after), Decision::Fire);
    }

    /// A row with only the trigger half filled in, which is all these tests read.
    fn row(kind: &str, target: &str, filter: &str) -> db::ScheduledRow {
        db::ScheduledRow {
            id: "t".to_string(),
            title: "T".to_string(),
            prompt: "do it".to_string(),
            chat_id: None,
            project_id: None,
            freq: "daily".to_string(),
            time: "09:00".to_string(),
            weekday: 0,
            nth: 1,
            enabled: true,
            next_run_at: NEVER,
            last_run_at: None,
            created_at: 0,
            kind: kind.to_string(),
            target: target.to_string(),
            filter: filter.to_string(),
            interval_secs: DEFAULT_INTERVAL_SECS,
            last_state: None,
            last_checked_at: None,
            last_note: String::new(),
        }
    }

    #[test]
    fn one_notification_matches_the_tasks_that_asked_for_it() {
        let rows = vec![
            row("mcp", "fs", "notifications/changed"),
            row("mcp", "fs", "*"),
            row("mcp", "github", "notifications/changed"),
            // A trigger of another kind that happens to name the same thing must
            // never be woken by an MCP event.
            row("file", "fs", "notifications/changed"),
        ];

        let hits = matching_tasks(&rows, "fs", "notifications/changed");
        assert_eq!(hits.len(), 2, "the named method and the wildcard both match");
        assert!(hits.iter().all(|woken| woken.target == "fs"));

        assert_eq!(
            matching_tasks(&rows, "fs", "notifications/other").len(),
            1,
            "only the wildcard wanted a method it did not name"
        );
        assert!(
            matching_tasks(&rows, "someone-else", "*").is_empty(),
            "a server the user did not subscribe to wakes nothing"
        );
    }

    #[test]
    fn a_disabled_trigger_is_not_woken() {
        let mut off = row("mcp", "fs", "ping");
        off.enabled = false;
        assert!(
            matching_tasks(&[off], "fs", "ping").is_empty(),
            "pausing a task has to stop it being woken, not just being polled"
        );
    }

    #[test]
    fn an_mcp_trigger_is_never_due_and_is_never_looked_at() {
        let pushed = Spec::new("mcp", "fs", "*", 60);
        assert_eq!(
            pushed.first_check(1_000),
            Some(NEVER),
            "a pushed trigger has no next check to be due at"
        );
        assert!(!Kind::Mcp.polled());
        // And the polled kinds still are, so "never due" is about this kind
        // rather than about the sentinel leaking into the others.
        assert_eq!(
            Spec::new("file", "C:\\x", "", 900).first_check(1_000),
            Some(1_900)
        );
    }
}
