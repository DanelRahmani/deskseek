//! Tauri commands: the whole surface the frontend talks to.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use chrono::Local;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::api::provider::Model;
use crate::api::types::Attachment;
use crate::approvals::Decision;
use crate::backup;
use crate::questions::Answer;
use crate::conversation::{agent, assemble, summarize};
use crate::mcp::{McpServer, ServerStatus, ToolLog};
use crate::memory;
use crate::notify;
use crate::schedule;
use crate::searxng;
use crate::skills;
use crate::store::{
    db, keychain,
    settings::{Instruction, Settings},
};
use crate::tools::{browse, desktop, exec, guard::{Egress, PermissionMode}, sandbox, sandbox_exec, walk};
use crate::triggers;
use crate::AppState;

fn title_from(text: &str) -> String {
    let line = text.lines().next().unwrap_or("").trim();
    let mut title: String = line.chars().take(48).collect();
    if line.chars().count() > 48 {
        title.push('…');
    }
    if title.is_empty() {
        "New chat".to_string()
    } else {
        title
    }
}

// ---------------------------------------------------------------- settings

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<Settings, String> {
    // The quick-entry chord is the one setting that is a *registration* rather
    // than a value, so changing it has to succeed before anything is written —
    // otherwise the file would claim a chord the OS never accepted. A chord
    // another application owns is refused by name and the old one still works.
    let previous = state.settings.lock().unwrap().quick_chord.clone();
    if settings.quick_chord != previous {
        crate::quick::rebind(&app, &previous, &settings.quick_chord)?;
    }
    if let Err(err) = crate::store::settings::save(&state.paths.settings_file(), &settings) {
        // Put the registration back the way it was: the file is unchanged, so
        // the running app should match it.
        if settings.quick_chord != previous {
            let _ = crate::quick::rebind(&app, &settings.quick_chord, &previous);
        }
        return Err(err.to_string());
    }
    *state.settings.lock().unwrap() = settings.clone();
    Ok(settings)
}

/// Raise the quick-entry window. The chord does this from inside the app; the
/// tray and the command palette need a way in that is not a keyboard shortcut.
#[tauri::command]
pub fn show_quick_entry(app: AppHandle) -> Result<(), String> {
    crate::quick::open(&app)
}

/// What the Appshot produced: the picture and the words, both staged on disk and
/// returned with a thumbnail so the window that asked can show what it got.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Appshot {
    /// The staged PNG. It only reaches the chat when the message is sent, through
    /// the same `attach_path` a dropped file uses.
    pub path: String,
    /// The window's readable text, as its own file, for the same reason.
    pub text_path: String,
    /// A thumbnail for the quick window, which has a path and no way to render
    /// one.
    pub data_url: String,
    pub width: u32,
    pub height: u32,
    pub title: String,
}

/// Capture the window that was in front when the quick window was raised.
///
/// The *remembered* window, not the focused one: pressing this button means our
/// own window has the focus, which is exactly the window nobody wants a picture
/// of.
#[tauri::command]
pub fn capture_focused_window(state: State<'_, AppState>) -> Result<Appshot, String> {
    let shot = desktop::capture_focused()?;
    let dir = state.paths.shots_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not use {}: {e}", dir.display()))?;
    let path = dir.join("appshot.png");
    let text_path = dir.join("appshot.txt");
    std::fs::write(&path, &shot.png).map_err(|e| e.to_string())?;
    std::fs::write(&text_path, &shot.text).map_err(|e| e.to_string())?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&shot.png);
    Ok(Appshot {
        path: path.display().to_string(),
        text_path: text_path.display().to_string(),
        data_url: format!("data:image/png;base64,{encoded}"),
        width: shot.width,
        height: shot.height,
        title: shot.title,
    })
}

// ---------------------------------------------------------------- backup

/// What a backup is worth: `src/backup.rs` owns the list and the archive. The
/// API key is deliberately absent — it lives in the OS keychain, which is not a
/// file this app can hand out, and a backup is a file that gets mailed around.
///
/// Write the whole profile to one zip in the documents folder.
///
/// The database is checkpointed first, so the single file the archive holds is
/// complete: in WAL mode the newest commits are still in `deskseek.db-wal` until
/// something flushes them, and those commits are exactly what a backup is for.
#[tauri::command]
pub fn backup_profile(state: State<'_, AppState>) -> Result<String, String> {
    let dir = documents_dir_path(&state);
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not use {}: {e}", dir.display()))?;
    {
        let conn = state.db.lock().unwrap();
        db::checkpoint(&conn);
    }
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let path = crate::tools::docs::free_path(&dir, &format!("deskseek-backup-{stamp}"), "zip");
    backup::write(state.paths.root(), &path)?;
    Ok(path.display().to_string())
}

/// Put a whole profile back over this one.
///
/// All-or-nothing in the same spirit as before: the archive is unpacked into a
/// scratch folder first, so a file that is not a backup — or is a truncated one —
/// leaves the live profile untouched rather than half-replaced.
///
/// The caller reloads the window afterwards: every open chat, transcript and
/// stored shell in the UI belongs to the database that has just been replaced.
#[tauri::command]
pub async fn restore_profile(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let root = state.paths.root().to_path_buf();
    let scratch = backup::scratch_dir(&root);
    std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
    let result = restore_into(&state, &root, &scratch, &path).await;
    // The scratch copy is a full profile; leaving it behind would double the
    // disk cost of a restore for no reason.
    let _ = std::fs::remove_dir_all(&scratch);
    result
}

async fn restore_into(
    state: &AppState,
    root: &Path,
    scratch: &Path,
    path: &str,
) -> Result<(), String> {
    backup::extract(Path::new(path), scratch)?;

    let restored_dir = scratch.join(backup::DB_FILE);
    if restored_dir.exists() {
        // Under the lock for the whole swap: between parking the connection and
        // reopening it, the handle must not be handed to anyone else.
        let mut conn = state.db.lock().unwrap();
        db::swap_file(&mut conn, &state.paths.db_file(), &restored_dir)?;
    }
    backup::replace_items(scratch, root)?;

    // The settings and the server list just changed underneath the running app.
    let settings = crate::store::settings::load(&state.paths.settings_file());
    *state.settings.lock().unwrap() = settings;
    state.mcp.write().await.reload().await;
    Ok(())
}

// ------------------------------------------------------------------- key

/// Whether the default endpoint could be called at all.
///
/// An endpoint that needs no key is ready; one that needs a key is ready only
/// once it is stored. The same check the client makes, said early so the error
/// names the setting rather than the request.
fn ready_to_send(state: &AppState) -> Result<(), String> {
    let settings = state.settings.lock().unwrap();
    let endpoint = settings
        .endpoint()
        .ok_or("No endpoint is configured — add one in Settings")?;
    if endpoint.has_key() && !keychain::has_key(&endpoint.key_ref) {
        return Err(format!("No API key set for {}", endpoint.label));
    }
    Ok(())
}

#[tauri::command]
pub fn has_api_key(state: State<'_, AppState>) -> bool {
    ready_to_send(&state).is_ok()
}

#[tauri::command]
pub fn masked_api_key(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let settings = state.settings.lock().unwrap();
    let Some(endpoint) = settings.endpoint() else {
        return Ok(None);
    };
    Ok(keychain::get_key(&endpoint.key_ref)?.map(|key| keychain::mask(&key)))
}

#[tauri::command]
pub fn set_api_key(state: State<'_, AppState>, key: String) -> Result<(), String> {
    if key.trim().is_empty() {
        return Err("API key is empty".to_string());
    }
    let settings = state.settings.lock().unwrap();
    let endpoint = settings
        .endpoint()
        .ok_or("No endpoint is configured — add one in Settings")?;
    // The key never travels through the window, so it is stored against the
    // endpoint's own keychain account: two endpoints do not share one secret.
    keychain::set_key(&endpoint.key_ref, key.trim())
}

#[tauri::command]
pub fn clear_api_key(state: State<'_, AppState>) -> Result<(), String> {
    let settings = state.settings.lock().unwrap();
    let Some(endpoint) = settings.endpoint() else {
        return Ok(());
    };
    keychain::delete_key(&endpoint.key_ref)
}

#[tauri::command]
pub async fn test_connection(state: State<'_, AppState>) -> Result<(), String> {
    let settings = state.settings.lock().unwrap().clone();
    crate::conversation::client(&state, &settings)?
        .check_auth()
        .await
        .map_err(|e| e.to_string())
}

/// Checks one endpoint by id — including one that is not the default.
///
/// The key comes from the keychain rather than from the window, so an endpoint
/// being tested has to be saved first for its key to be found. That is the whole
/// reason the settings dialog saves before it tests.
#[tauri::command]
pub async fn test_endpoint(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let settings = state.settings.lock().unwrap().clone();
    let endpoint = settings
        .endpoints
        .iter()
        .find(|endpoint| endpoint.id == id)
        .ok_or("that endpoint is not saved yet")?
        .clone();
    endpoint.validate()?;
    let key = keychain::get_key(&endpoint.key_ref)?;
    crate::api::client::Client::new(state.http.clone(), endpoint, key)
        .check_auth()
        .await
        .map_err(|e| e.to_string())
}

/// The models the default endpoint offers, for the pickers.
///
/// Read from the settings rather than shipped as a constant: the list belongs to
/// the endpoint, and a second endpoint's models are not this app's business to
/// guess at.
#[tauri::command]
pub fn list_models(state: State<'_, AppState>) -> Vec<Model> {
    state
        .settings
        .lock()
        .unwrap()
        .endpoint()
        .map(|endpoint| endpoint.models.clone())
        .unwrap_or_default()
}

// ----------------------------------------------------------------- chats

#[tauri::command]
pub fn list_chats(state: State<'_, AppState>, kind: String) -> Result<Vec<db::ChatRow>, String> {
    let conn = state.db.lock().unwrap();
    db::list_chats(&conn, &kind).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_chat(
    state: State<'_, AppState>,
    kind: String,
    workspace: Option<String>,
) -> Result<db::ChatRow, String> {
    let settings = state.settings.lock().unwrap().clone();
    let model = settings.model_for(&kind);
    let chat = {
        let conn = state.db.lock().unwrap();
        db::create_chat(
            &conn,
            &kind,
            workspace.as_deref(),
            &model,
            settings.thinking,
            &settings.effort,
            &settings.permission_mode,
        )
        .map_err(|e| e.to_string())?
    };
    seed_default_skills(&state, &chat.id);
    Ok(chat)
}

/// A new chat starts with every skill whose file says `default: on` already
/// switched on.
///
/// The flag lives in the skill's own SKILL.md, so the user's standing choices
/// are made once, in the folder that is already the store for skills, and no
/// chat has to be set up by hand. It is not free: the bodies of these skills
/// become part of every request in that chat, which is precisely what marking
/// one asks for. Everything else costs a single line of catalogue in the prompt
/// and is fetched with `use_skill` only when it is actually wanted.
fn seed_default_skills(state: &AppState, chat_id: &str) {
    let defaults: Vec<String> = skills::discover(&state.paths.skills_dir())
        .into_iter()
        .filter(|skill| skill.default_on)
        .map(|skill| skill.id)
        .collect();
    if defaults.is_empty() {
        return;
    }
    let conn = state.db.lock().unwrap();
    if let Err(err) = db::set_chat_skills(&conn, chat_id, &defaults) {
        eprintln!("could not switch this chat's default skills on: {err}");
    }
}

#[tauri::command]
pub fn get_messages(state: State<'_, AppState>, chat_id: String) -> Result<Vec<db::MessageRow>, String> {
    let conn = state.db.lock().unwrap();
    db::list_messages(&conn, &chat_id).map_err(|e| e.to_string())
}

/// One page of search results, with whether there is another one behind it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessagePage {
    pub hits: Vec<db::MessageHit>,
    /// True when at least one more page exists, so the UI knows to offer it.
    pub more: bool,
}

/// Search every conversation by what was said in it, not just by its title.
///
/// The results are ranked by relevance, so a common word puts its most useful
/// hits first — but a common word also has more of them than fit on a screen, so
/// the list pages. One row over the page size is asked for and thrown away: its
/// presence *is* the answer to "is there more", which costs nothing where a
/// `count(*)` over the match set would.
#[tauri::command]
pub fn search_messages(
    state: State<'_, AppState>,
    query: String,
    offset: Option<i64>,
) -> Result<MessagePage, String> {
    const PAGE: i64 = 40;
    let conn = state.db.lock().unwrap();
    let mut hits = db::search_messages(&conn, &query, offset.unwrap_or(0), PAGE + 1)
        .map_err(|e| e.to_string())?;
    let more = hits.len() as i64 > PAGE;
    hits.truncate(PAGE as usize);
    Ok(MessagePage { hits, more })
}

/// Undo the file changes of the most recent agent run in this chat, and report
/// which files were put back. An empty list means there was nothing to undo.
#[tauri::command]
pub fn rewind_last_run(state: State<'_, AppState>, chat_id: String) -> Result<Vec<String>, String> {
    let conn = state.db.lock().unwrap();
    db::rewind_last_run(&conn, &chat_id)
}

/// Re-run the conversation from its last user turn.
///
/// `text` is edited wording when the caller has some; None re-runs the turn as
/// it stands, which is a regenerate. Everything after that turn is dropped
/// first, so the model cannot see the answer it is being asked to replace.
#[tauri::command]
pub async fn resend_last_turn(
    app: AppHandle,
    state: State<'_, AppState>,
    chat_id: String,
    text: Option<String>,
) -> Result<(), String> {
    ready_to_send(&state)?;
    {
        let conn = state.db.lock().unwrap();
        let turn = db::last_user_message(&conn, &chat_id)
            .map_err(|e| e.to_string())?
            .ok_or("There is nothing to re-send.")?;
        if let Some(text) = text.as_deref() {
            if text.trim().is_empty() {
                return Err("A message cannot be emptied — delete the chat instead.".to_string());
            }
            db::set_message_content(&conn, &turn.id, text).map_err(|e| e.to_string())?;
        }
        // Moved to the discarded pile rather than dropped, so the answer being
        // replaced can be brought back without a second run.
        db::discard_tail(&conn, &chat_id, turn.seq)?;
    }

    // This replaces a turn, so it registers a run exactly as send_message does:
    // one live run per chat, and it cancels any turn already in flight.
    run_turn(app, &state, &chat_id, false, None).await
}

/// Puts back the answer a resend replaced, and keeps the one it replaces.
///
/// No model call and no re-run: the two answers are already recorded, so the
/// button is a swap. `after_seq` is the seq of the user turn the answer belonged
/// to, which is what the transcript has in hand when it draws the button.
#[tauri::command]
pub fn restore_discarded_tail(
    state: State<'_, AppState>,
    chat_id: String,
    after_seq: i64,
) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::restore_tail(&conn, &chat_id, after_seq)
}

/// Copies a conversation: its transcript, and for a chat-mode one its folder of
/// files too.
///
/// A coding chat's folder belongs to the project it works in, and duplicating a
/// conversation is not a reason to copy a client's working tree — so there the
/// copy shares the workspace, exactly as any other chat in that project does.
///
/// The copy starts with no spend; `db::duplicate_chat` says why.
#[tauri::command]
pub fn duplicate_chat(state: State<'_, AppState>, chat_id: String) -> Result<db::ChatRow, String> {
    let source = {
        let conn = state.db.lock().unwrap();
        db::get_chat(&conn, &chat_id).map_err(|e| e.to_string())?
    };
    let copy = {
        let conn = state.db.lock().unwrap();
        db::duplicate_chat(&conn, &chat_id)
            .map_err(|e| e.to_string())?
            .ok_or("That conversation is gone.")?
    };
    if source.workspace.is_none() {
        // A chat with nothing in its folder yet is fine, not an error.
        if let (Ok(from), Ok(to)) = (chat_dir(&state, &chat_id), chat_dir(&state, &copy.id)) {
            let _ = copy_files(&from, &to);
        }
    }
    Ok(copy)
}

/// Copies a chat folder into another one, subfolders and all.
///
/// A chat folder is what the conversation produced or was handed, so this
/// mirrors whatever is actually there rather than guessing at names. Files are
/// copied, never moved: the original has to stay exactly as it was.
fn copy_files(from: &Path, to: &Path) -> Result<usize, String> {
    let mut copied = 0;
    let mut stack = vec![(from.to_path_buf(), to.to_path_buf())];
    while let Some((src, dst)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&src) else {
            continue;
        };
        std::fs::create_dir_all(&dst)
            .map_err(|e| format!("could not make {}: {e}", dst.display()))?;
        for entry in entries.flatten() {
            let target = dst.join(entry.file_name());
            if entry.path().is_dir() {
                stack.push((entry.path(), target));
                continue;
            }
            std::fs::copy(entry.path(), &target)
                .map_err(|e| format!("could not copy {}: {e}", entry.path().display()))?;
            copied += 1;
        }
    }
    Ok(copied)
}

#[tauri::command]
pub fn rename_chat(state: State<'_, AppState>, chat_id: String, title: String) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::rename_chat(&conn, &chat_id, &title).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_chat(state: State<'_, AppState>, chat_id: String) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::delete_chat(&conn, &chat_id).map_err(|e| e.to_string())
}

/// Moves a chat to the sidebar's archive, or back out of it. The opposite of
/// `delete_chat`: nothing is removed, so it is safe to offer on a row.
#[tauri::command]
pub fn set_chat_archived(
    state: State<'_, AppState>,
    chat_id: String,
    archived: bool,
) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::set_chat_archived(&conn, &chat_id, archived).map_err(|e| e.to_string())
}

/// Switches a chat's "does not feed memory" on or off.
///
/// With it on, nothing said in the chat is written to memory and its prompt
/// carries none — the whole guarantee is `memory::enabled`, asked from both the
/// prompt assembly and `save_memory`.
#[tauri::command]
pub fn set_chat_incognito(
    state: State<'_, AppState>,
    chat_id: String,
    incognito: bool,
) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::set_chat_incognito(&conn, &chat_id, incognito).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_chat_model(
    state: State<'_, AppState>,
    chat_id: String,
    model: String,
    thinking: bool,
    effort: String,
) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::set_chat_model(&conn, &chat_id, &model, thinking, &effort).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_chat_permission_mode(
    state: State<'_, AppState>,
    chat_id: String,
    mode: String,
) -> Result<(), String> {
    let mode = PermissionMode::parse(&mode).as_str();
    let conn = state.db.lock().unwrap();
    db::set_chat_permission_mode(&conn, &chat_id, mode).map_err(|e| e.to_string())
}

/// What this chat is allowed to send out. Parsed rather than stored raw, so the
/// database only ever holds a policy the run loop can act on.
#[tauri::command]
pub fn set_chat_egress(
    state: State<'_, AppState>,
    chat_id: String,
    egress: String,
) -> Result<(), String> {
    let egress = Egress::parse(&egress).as_str();
    let conn = state.db.lock().unwrap();
    db::set_chat_egress(&conn, &chat_id, egress).map_err(|e| e.to_string())
}

/// How much rope this chat has. An unrecognised value becomes standard rather
/// than being stored, so the database only ever holds a mode this app can act on.
#[tauri::command]
pub fn set_chat_mode(
    state: State<'_, AppState>,
    chat_id: String,
    mode: String,
) -> Result<(), String> {
    let normalised = crate::tools::AgentMode::parse(&mode);
    let conn = state.db.lock().unwrap();
    db::set_chat_mode(&conn, &chat_id, normalised.id()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_chat_workspace(
    state: State<'_, AppState>,
    chat_id: String,
    workspace: Option<String>,
) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::set_chat_workspace(&conn, &chat_id, workspace.as_deref()).map_err(|e| e.to_string())
}

// ----------------------------------------------------------------- memory

#[tauri::command]
pub fn get_memory(state: State<'_, AppState>) -> Option<String> {
    memory::read(&state.paths.memory_file())
}

#[tauri::command]
pub fn set_memory(state: State<'_, AppState>, body: String) -> Result<(), String> {
    memory::write(&state.paths.memory_file(), &body).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn memory_path(state: State<'_, AppState>) -> String {
    state.paths.memory_file().display().to_string()
}

/// Adds an entry by hand, from the memory panel.
///
/// Routed through the same `destination` rule `save_memory` uses, so the panel
/// cannot write somewhere the assistant would not. There is no workspace here: a
/// memory typed into the panel is not tied to one project. Returns how many entries
/// were newly added rather than already present.
#[tauri::command]
pub fn append_memory(
    state: State<'_, AppState>,
    category: String,
    entries: Vec<String>,
) -> Result<usize, String> {
    let destination = crate::memory_files::destination(&category, None);
    if destination == crate::memory_files::PROFILE {
        return memory::append(&state.paths.memory_file(), &entries)
            .map(|()| entries.len())
            .map_err(|e| e.to_string());
    }
    crate::memory_files::append(&state.paths.memory_dir(), &destination, &entries)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_proposals(
    state: State<'_, AppState>,
    chat_id: String,
    status: Option<String>,
) -> Result<Vec<db::MemoryProposalRow>, String> {
    let conn = state.db.lock().unwrap();
    db::list_proposals(&conn, &chat_id, status.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn accept_proposals(state: State<'_, AppState>, ids: Vec<String>) -> Result<usize, String> {
    let pending: Vec<db::MemoryProposalRow> = {
        let conn = state.db.lock().unwrap();
        db::proposals_by_ids(&conn, &ids)
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|r| r.status == "pending")
            .collect()
    };

    // A `profile` entry belongs in MEMORY.md, which is injected on every request;
    // everything else goes to its category file — or, inside a project, to that
    // project's file. Because of that difference an accepted batch is split rather
    // than appended in one place.
    let (profile, filed): (Vec<_>, Vec<_>) = pending.iter().partition(|r| r.target == "profile");

    if !profile.is_empty() {
        let texts: Vec<String> = profile.iter().map(|r| r.text.clone()).collect();
        memory::append(&state.paths.memory_file(), &texts).map_err(|e| e.to_string())?;
    }

    if !filed.is_empty() {
        let conn = state.db.lock().unwrap();
        for row in &filed {
            // Scoped to the project the memory was learned in, so a note about one
            // client does not surface in another one's chat. Every row is looked up
            // separately: one accepted batch can span chats.
            let workspace = db::get_chat(&conn, &row.chat_id)
                .ok()
                .and_then(|chat| chat.workspace)
                .map(std::path::PathBuf::from);
            let category = crate::memory_files::destination(&row.target, workspace.as_deref());
            crate::memory_files::append(
                &state.paths.memory_dir(),
                &category,
                std::slice::from_ref(&row.text),
            )
            .map_err(|e| e.to_string())?;
        }
    }

    {
        let conn = state.db.lock().unwrap();
        db::set_proposal_status(&conn, &ids, "accepted").map_err(|e| e.to_string())?;
    }
    Ok(pending.len())
}

#[tauri::command]
pub fn reject_proposals(state: State<'_, AppState>, ids: Vec<String>) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::set_proposal_status(&conn, &ids, "rejected").map_err(|e| e.to_string())
}

/// The memory store as the panel shows it: every category, whether or not it has a
/// file yet, plus anything a project folder has added. Read from disk rather than
/// cached, so a file the user edited by hand shows up as edited.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEntryView {
    pub date: String,
    pub text: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryCategoryView {
    pub category: String,
    pub about: String,
    pub entries: Vec<MemoryEntryView>,
}

#[tauri::command]
pub fn memory_overview(state: State<'_, AppState>) -> Result<Vec<MemoryCategoryView>, String> {
    let dir = state.paths.memory_dir();
    let mut ids: Vec<String> = crate::memory_files::category_ids()
        .into_iter()
        .map(str::to_string)
        .collect();
    for extra in crate::memory_files::categories_in_use(&dir) {
        if !ids.contains(&extra) {
            ids.push(extra);
        }
    }
    Ok(ids
        .into_iter()
        .map(|category| {
            let about = crate::memory_files::about(&category);
            MemoryCategoryView {
                entries: crate::memory_files::read(&dir, &category)
                    .into_iter()
                    .map(|e| MemoryEntryView {
                        date: e.date,
                        text: e.text,
                    })
                    .collect(),
                about: if about.is_empty() {
                    // Only a project file can be unknown to the category table.
                    "remembered while working in this project".to_string()
                } else {
                    about.to_string()
                },
                category,
            }
        })
        .collect())
}

/// What an imported memory file did: how many notes were newly written (ones
/// already present are not counted), and the categories anything landed in.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryImport {
    pub added: usize,
    pub categories: Vec<String>,
}

/// Import an exported memory file from another assistant.
///
/// The file is a markdown document of sections: a heading names a category, the
/// lines under it are that category's notes. A heading this build does not know
/// falls back to `other` rather than inventing a file — see
/// `memory_files::parse_export`. Additive by construction: every line goes through
/// the same `append` the assistant uses, which skips anything already there, so
/// importing the same file twice changes nothing and nothing already written is
/// overwritten.
#[tauri::command]
pub fn import_memory(state: State<'_, AppState>, path: String) -> Result<MemoryImport, String> {
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    let dir = state.paths.memory_dir();
    let mut report = MemoryImport {
        added: 0,
        categories: Vec::new(),
    };
    for (category, lines) in crate::memory_files::parse_export(&raw) {
        let added = crate::memory_files::append(&dir, &category, &lines)
            .map_err(|e| e.to_string())?;
        if added > 0 {
            report.added += added;
            if !report.categories.contains(&category) {
                report.categories.push(category);
            }
        }
    }
    Ok(report)
}

/// Forgets one entry, identified by the text the user can see. Only that entry
/// goes: the file is rewritten line by line, so everything else in it survives.
#[tauri::command]
pub fn delete_memory_entry(
    state: State<'_, AppState>,
    category: String,
    text: String,
) -> Result<bool, String> {
    crate::memory_files::remove(&state.paths.memory_dir(), &category, &text)
        .map_err(|e| e.to_string())
}

/// Returns `summary\u{1f}<proposal count>\u{1f}<topic>\u{1f}<chatlog path>`.
#[tauri::command]
pub async fn summarize_chat(state: State<'_, AppState>, chat_id: String) -> Result<String, String> {
    let (summary, created, chatlog) = summarize::summarize_chat(&state, &chat_id, true).await?;
    let (topic, path) = match chatlog {
        Some(joined) => {
            let mut parts = joined.splitn(2, '\u{1f}');
            (
                parts.next().unwrap_or_default().to_string(),
                parts.next().unwrap_or_default().to_string(),
            )
        }
        None => (String::new(), String::new()),
    };
    Ok(format!("{summary}\u{1f}{created}\u{1f}{topic}\u{1f}{path}"))
}

/// Replaces the conversation with a summary of itself, so a long chat keeps
/// working without hitting the context limit.
#[tauri::command]
pub async fn compact_chat(state: State<'_, AppState>, chat_id: String) -> Result<String, String> {
    let (summary, _, _) = summarize::summarize_chat(&state, &chat_id, false).await?;
    {
        let conn = state.db.lock().unwrap();
        summarize::fold(&conn, &chat_id, &summary).map_err(|e| e.to_string())?;
    }
    Ok(summary)
}

#[tauri::command]
pub fn documents_dir(state: State<'_, AppState>) -> String {
    documents_dir_path(&state).display().to_string()
}

/// The folder generated documents land in: the configured one, else the default.
/// Shared so the export cannot pick a different folder than the document tools.
fn documents_dir_path(state: &AppState) -> PathBuf {
    let configured = state.settings.lock().unwrap().documents_dir.clone();
    if configured.trim().is_empty() {
        state.paths.documents_dir()
    } else {
        PathBuf::from(configured)
    }
}

/// The folder a chat's own files live in — the one the document tools write to
/// and the file panel shows, so an export lands where the user is looking rather
/// than in a shared pile.
fn chat_dir(state: &AppState, chat_id: &str) -> Result<PathBuf, String> {
    let chat = {
        let conn = state.db.lock().unwrap();
        db::get_chat(&conn, chat_id).map_err(|e| e.to_string())?
    };
    let dir = agent::workspace_for(state, &chat).ok_or("This chat has no folder yet.")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not use {}: {e}", dir.display()))?;
    Ok(dir)
}

/// Write a chat to a Markdown file in the chat's own folder, so it can be read,
/// printed or mailed. A second export of the same chat becomes `name-2` rather
/// than overwriting the first. Returns the path written.
#[tauri::command]
pub fn export_chat(state: State<'_, AppState>, chat_id: String) -> Result<String, String> {
    let (chat, messages) = {
        let conn = state.db.lock().unwrap();
        let chat = db::get_chat(&conn, &chat_id).map_err(|e| e.to_string())?;
        let messages = db::list_messages(&conn, &chat_id).map_err(|e| e.to_string())?;
        (chat, messages)
    };
    let dir = chat_dir(&state, &chat_id)?;
    let path = crate::tools::docs::free_path(&dir, &crate::tools::docs::slug(&chat.title), "md");
    std::fs::write(&path, chat_markdown(&chat, &messages)).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

/// The whole conversation as JSON, so it can be carried to another machine.
///
/// Versioned from the first release of the format: a file that outlives the code
/// that wrote it is the entire point, so it says which shape it is in and the
/// importer refuses what it does not know rather than guessing.
///
/// Attachments are referenced, not embedded. A conversation's images and
/// documents live in its folder as files; a JSON that swallowed them would be a
/// container, and the honest thing is to say the copy is the transcript only.
#[derive(Serialize, Deserialize)]
struct ChatBundle {
    version: u32,
    chat: db::ChatRow,
    messages: Vec<db::MessageRow>,
}

const BUNDLE_VERSION: u32 = 1;

#[tauri::command]
pub fn export_chat_json(state: State<'_, AppState>, chat_id: String) -> Result<String, String> {
    let bundle = {
        let conn = state.db.lock().unwrap();
        ChatBundle {
            version: BUNDLE_VERSION,
            chat: db::get_chat(&conn, &chat_id).map_err(|e| e.to_string())?,
            messages: db::list_messages(&conn, &chat_id).map_err(|e| e.to_string())?,
        }
    };
    let json = serde_json::to_string_pretty(&bundle).map_err(|e| e.to_string())?;
    let dir = chat_dir(&state, &chat_id)?;
    let path = crate::tools::docs::free_path(&dir, &crate::tools::docs::slug(&bundle.chat.title), "chat.json");
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

/// Brings a bundle back as a new conversation. The original is untouched, and
/// the import is additive: nothing is overwritten, so a half-typed file cannot
/// cost the user a chat.
#[tauri::command]
pub fn import_chat_json(state: State<'_, AppState>, path: String) -> Result<db::ChatRow, String> {
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("could not read {path}: {e}"))?;
    let bundle: ChatBundle = serde_json::from_str(&raw)
        .map_err(|e| format!("that is not a DeskSeek conversation: {e}"))?;
    if bundle.version > BUNDLE_VERSION {
        return Err(format!(
            "That file was written by a newer DeskSeek (format {}); this one understands {}.",
            bundle.version, BUNDLE_VERSION
        ));
    }
    let conn = state.db.lock().unwrap();
    let chat = db::create_chat(
        &conn,
        "chat",
        None,
        &bundle.chat.model,
        bundle.chat.thinking,
        &bundle.chat.effort,
        &bundle.chat.permission_mode,
    )
    .map_err(|e| e.to_string())?;
    db::rename_chat(&conn, &chat.id, &bundle.chat.title).map_err(|e| e.to_string())?;
    db::set_chat_mode(&conn, &chat.id, &bundle.chat.agent_mode).map_err(|e| e.to_string())?;
    for m in &bundle.messages {
        db::insert_message(
            &conn,
            &chat.id,
            &m.role,
            m.content.as_deref(),
            m.reasoning_content.as_deref(),
            m.tool_calls_json.as_deref(),
            m.tool_call_id.as_deref(),
            m.attachments_json.as_deref(),
        )
        .map_err(|e| e.to_string())?;
    }
    db::get_chat(&conn, &chat.id).map_err(|e| e.to_string())
}

/// Write a chat to a PDF in the chat's own folder.
///
/// Deliberately routed through the same `doc_create` tool the model uses rather
/// than printing from a second implementation: one Markdown reader, one HTML
/// template and one browser invocation to keep in step. The consequence is the
/// same as for the tool — it needs a findable Chromium-family browser, and says
/// so plainly when there is none instead of writing nothing.
#[tauri::command]
pub async fn export_chat_pdf(
    state: State<'_, AppState>,
    chat_id: String,
) -> Result<String, String> {
    let (chat, messages) = {
        let conn = state.db.lock().unwrap();
        let chat = db::get_chat(&conn, &chat_id).map_err(|e| e.to_string())?;
        let messages = db::list_messages(&conn, &chat_id).map_err(|e| e.to_string())?;
        (chat, messages)
    };
    let dir = chat_dir(&state, &chat_id)?;
    let browser = state.settings.lock().unwrap().browser_path.clone();
    let args = serde_json::json!({
        "name": chat.title,
        "format": "pdf",
        "content": chat_markdown(&chat, &messages),
    });
    crate::tools::docs::execute("doc_create", &args, &dir, &browser).await
}

/// Render a conversation as Markdown.
///
/// Only what a reader would want: the user's words and the assistant's written
/// replies. Tool rows are the machinery behind a reply — their output can be
/// enormous and is not the conversation — and a turn that produced only a tool
/// call has nothing to say on its own, so both are left out.
fn chat_markdown(chat: &db::ChatRow, messages: &[db::MessageRow]) -> String {
    let mut out = format!("# {}\n\n", chat.title);
    if let Some(workspace) = &chat.workspace {
        out.push_str(&format!("_Working in `{workspace}`._\n\n"));
    }
    for message in messages {
        let heading = match message.role.as_str() {
            "user" => "## You",
            "assistant" => "## DeskSeek",
            _ => continue,
        };
        let Some(body) = message
            .content
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
        else {
            continue;
        };
        out.push_str(&format!("{heading}\n\n{body}\n\n"));
        if let Some(json) = message.attachments_json.as_deref() {
            if let Ok(items) = serde_json::from_str::<Vec<Attachment>>(json) {
                let names: Vec<&str> = items.iter().map(|a| a.name.as_str()).collect();
                if !names.is_empty() {
                    out.push_str(&format!("_(attached: {})_\n\n", names.join(", ")));
                }
            }
        }
    }
    out
}

#[tauri::command]
pub fn chatlog_dir(state: State<'_, AppState>) -> String {
    state.paths.chatlog_dir().display().to_string()
}

// ----------------------------------------------------------------- context

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextBreakdown {
    pub parts: Vec<agent::PromptPart>,
    /// Pictures riding along with the history. The API charges for these by
    /// picture rather than by the characters of their data URL, so they are
    /// counted instead of measured — calling a 200 KB base64 string 55k tokens
    /// would be worse than saying nothing about it.
    pub images: usize,
}

/// What the next request to this chat would be made of.
///
/// Every number comes from the code the run itself uses — the system prompt from
/// `agent::system_prompt`, the tool list from `agent::tools_for_chat`, the turns
/// from the rows the run would read — so the panel describes the request that is
/// really about to be sent rather than an approximation of it.
///
/// Characters, not tokens. The app's one token estimate is characters ÷ 3.6 in
/// the frontend, beside the meter; a second estimate here would disagree with the
/// number sitting next to this panel.
#[tauri::command]
pub async fn context_breakdown(
    state: State<'_, AppState>,
    chat_id: String,
) -> Result<ContextBreakdown, String> {
    let settings = state.settings.lock().unwrap().clone();
    let chat = {
        let conn = state.db.lock().unwrap();
        db::get_chat(&conn, &chat_id).map_err(|e| e.to_string())?
    };
    let history = {
        let conn = state.db.lock().unwrap();
        db::list_messages(&conn, &chat_id).map_err(|e| e.to_string())?
    };
    let turns: Vec<assemble::Turn> = history.iter().map(agent::to_turn).collect();
    // The newest user turn is what the memory files are queried with, and what this
    // turn is about — the same rule the run uses.
    let ask = turns
        .iter()
        .rev()
        .find(|turn| turn.role == "user")
        .and_then(|turn| turn.content.as_deref());

    let tools = agent::tools_for_chat(&state, &settings, &chat).await;
    let plan = sandbox_exec::plan_for(
        sandbox_exec::Mode::parse(&settings.sandbox),
        &state.sandbox.read().await.clone(),
        &settings.sandbox_image,
    );
    let workspace = agent::workspace_for(&state, &chat);
    let (_, mut parts) = agent::system_prompt(
        &settings,
        &state,
        &chat,
        workspace.as_deref(),
        tools.mcp,
        ask,
        &plan,
    );

    // The schemas go over the wire as JSON, so the JSON is what is measured.
    if let Ok(json) = serde_json::to_string(&tools.list) {
        parts.push(agent::part(
            format!("Tool schemas ({})", tools.list.len()),
            "tools",
            json.chars().count(),
        ));
    }

    // By role, because a conversation is not one thing: reasoning is only echoed
    // back when the request carries tools, and tool results can dwarf both sides.
    let (mut said, mut replied, mut reasoned, mut tool_out, mut images) = (0, 0, 0, 0, 0);
    for turn in &turns {
        images += turn
            .attachments
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .filter(|a| a.is_image())
            .count();
        // A text attachment is sent as part of the message, so it is counted
        // as the user's own words rather than quietly left out of the meter.
        let attached: usize = turn
            .attachments
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|a| a.text.chars().count())
            .sum();
        let chars = turn.content.as_deref().unwrap_or("").chars().count() + attached;
        match turn.role.as_str() {
            "user" => said += chars,
            "assistant" => {
                replied += chars;
                reasoned += turn.reasoning_content.as_deref().unwrap_or("").chars().count();
            }
            "tool" => tool_out += chars,
            _ => {}
        }
    }
    for (label, chars) in [
        ("Your messages", said),
        ("Replies", replied),
        ("Reasoning", reasoned),
        ("Tool results", tool_out),
    ] {
        if chars > 0 {
            parts.push(agent::part(label, "conversation", chars));
        }
    }

    Ok(ContextBreakdown { parts, images })
}

/// Open a generated document in whatever application owns its type.
///
/// Restricted to the extensions the document tools produce: the path comes back
/// from a tool result, but "open any path the webview asks for" is a bigger door
/// than this feature needs. The opener free function is used rather than the
/// plugin command on purpose — it is the one that does not need a scope entry.
#[tauri::command]
pub fn open_document(path: String) -> Result<(), String> {
    const OPENABLE: [&str; 5] = ["md", "docx", "xlsx", "pptx", "pdf"];
    let file = std::path::Path::new(&path);
    let ext = file
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !OPENABLE.contains(&ext.as_str()) {
        return Err(format!("DeskSeek will not open a .{ext} file."));
    }
    if !file.is_file() {
        return Err(format!("{path} is not there any more."));
    }
    tauri_plugin_opener::open_path(file, None::<&str>).map_err(|e| e.to_string())
}

// --------------------------------------------------------------- artifacts

#[tauri::command]
pub fn list_artifacts(
    state: State<'_, AppState>,
    chat_id: String,
) -> Result<Vec<db::ArtifactRow>, String> {
    let conn = state.db.lock().unwrap();
    db::list_artifacts(&conn, &chat_id).map_err(|e| e.to_string())
}

/// One artifact plus one version's content. `version` of `None` means current.
#[tauri::command]
pub fn get_artifact(
    state: State<'_, AppState>,
    artifact_id: String,
    version: Option<i64>,
) -> Result<db::ArtifactDetail, String> {
    let conn = state.db.lock().unwrap();
    db::get_artifact_detail(&conn, &artifact_id, version).map_err(|e| e.to_string())
}

/// An edit made in the panel. It appends a version — nothing is overwritten —
/// and hands back the whole detail so the version list is refreshed in the same
/// round trip.
#[tauri::command]
pub fn save_artifact_version(
    state: State<'_, AppState>,
    artifact_id: String,
    content: String,
) -> Result<db::ArtifactDetail, String> {
    if content.trim().is_empty() {
        return Err("An artifact cannot be saved empty.".to_string());
    }
    let limit = crate::tools::MAX_ARTIFACT_CHARS;
    if content.chars().count() > limit {
        return Err(format!(
            "That is longer than the {limit} character limit for one artifact."
        ));
    }
    let conn = state.db.lock().unwrap();
    db::add_artifact_version(&conn, &artifact_id, &content, "user").map_err(|e| e.to_string())?;
    db::get_artifact_detail(&conn, &artifact_id, None).map_err(|e| e.to_string())
}

/// Rewrites one selection of an artifact and answers with the replacement text.
///
/// A single non-streaming model call rather than a turn: the user pointed at a
/// sentence, and the panel patches the answer back in at the offsets they
/// selected. The *patch* and the *save* stay on the frontend and in
/// `save_artifact_version` — this only gets the new words.
#[tauri::command]
pub async fn revise_selection(
    state: State<'_, AppState>,
    chat_id: String,
    before: String,
    selection: String,
    after: String,
    instruction: String,
) -> Result<String, String> {
    agent::revise_selection(&state, &chat_id, &before, &selection, &after, &instruction).await
}

#[tauri::command]
pub fn get_summary(state: State<'_, AppState>, chat_id: String) -> Result<Option<String>, String> {
    let conn = state.db.lock().unwrap();
    db::get_summary(&conn, &chat_id).map_err(|e| e.to_string())
}

// ----------------------------------------------------------------- skills

#[derive(Serialize)]
pub struct SkillView {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    /// `default: on` — a new chat starts with it switched on. Not the same
    /// question as `enabled`, which is about *this* chat.
    pub default_on: bool,
}

#[tauri::command]
pub fn list_skills(state: State<'_, AppState>, chat_id: Option<String>) -> Vec<SkillView> {
    let enabled: Vec<String> = match chat_id {
        Some(id) => {
            let conn = state.db.lock().unwrap();
            db::get_chat_skills(&conn, &id).unwrap_or_default()
        }
        None => Vec::new(),
    };
    skills::discover(&state.paths.skills_dir())
        .into_iter()
        .map(|s| SkillView {
            enabled: enabled.contains(&s.id),
            default_on: s.default_on,
            id: s.id,
            name: s.name,
            description: s.description,
        })
        .collect()
}

/// Marks a skill as one a new chat should start with — or stops it being one.
///
/// This is not a per-chat switch: it is the skill's own file, so it applies to
/// chats that do not exist yet and leaves the current one exactly as it is.
#[tauri::command]
pub fn set_skill_default(
    state: State<'_, AppState>,
    id: String,
    default_on: bool,
) -> Result<(), String> {
    if id.is_empty() || id.contains(['/', '\\']) || id.contains("..") {
        return Err("That is not a skill id.".to_string());
    }
    if !state.paths.skills_dir().join(&id).is_dir() {
        return Err(format!("No skill called \"{id}\"."));
    }
    skills::set_default(&state.paths.skills_dir(), &id, default_on).map_err(|e| e.to_string())
}

/// The list to store when one store's list is saved: what the caller sent, plus
/// the ids belonging to the *other* store that were already there.
///
/// Skills and design systems share the `chat_skills` table, and each surface
/// knows only its own list — the Skills tab must never show a design system. So
/// without this, saving one list would switch the other off as a side effect of
/// an unrelated click. The rule lives here, in the one function both callers go
/// through, rather than in each caller where it can be forgotten.
fn keep_other_store(
    current: &[String],
    incoming: &[String],
    mine: impl Fn(&str) -> bool,
) -> Vec<String> {
    let mut next: Vec<String> = incoming.iter().filter(|id| mine(id)).cloned().collect();
    next.extend(current.iter().filter(|id| !mine(id)).cloned());
    next
}

#[tauri::command]
pub fn set_chat_skills(
    state: State<'_, AppState>,
    chat_id: String,
    skill_ids: Vec<String>,
) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    let current = db::get_chat_skills(&conn, &chat_id).unwrap_or_default();
    let next = keep_other_store(&current, &skill_ids, |id| !crate::design::is_design(id));
    db::set_chat_skills(&conn, &chat_id, &next).map_err(|e| e.to_string())
}

/// The same, for the design systems switched on in a chat.
///
/// A separate command rather than one shared "set the enabled ids": each side
/// then speaks only for its own store, and neither has to know the other's list
/// to avoid destroying it.
#[tauri::command]
pub fn set_chat_designs(
    state: State<'_, AppState>,
    chat_id: String,
    design_ids: Vec<String>,
) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    let current = db::get_chat_skills(&conn, &chat_id).unwrap_or_default();
    let next = keep_other_store(&current, &design_ids, crate::design::is_design);
    db::set_chat_skills(&conn, &chat_id, &next).map_err(|e| e.to_string())
}

/// Where skills live, so the panel can show the real path and open it.
#[tauri::command]
pub fn skills_dir(state: State<'_, AppState>) -> String {
    state.paths.skills_dir().display().to_string()
}

/// Writes a skill body to `skills/<id>/SKILL.md`, refusing an id that is taken.
///
/// An id that already exists is refused rather than overwritten: installing
/// twice should not quietly replace a skill the user may have edited. Shared by
/// both install routes so they cannot drift on that rule.
fn write_skill(root: &std::path::Path, id: &str, raw: &str) -> Result<String, String> {
    let dir = root.join(id);
    if dir.exists() {
        return Err(format!("A skill called \"{id}\" is already installed."));
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("SKILL.md"), raw).map_err(|e| e.to_string())?;
    Ok(id.to_string())
}

/// Install a skill from a markdown file the user picked.
///
/// The file is copied to `skills/<id>/SKILL.md`, where the id comes from the
/// file's frontmatter `name` or, failing that, its own name.
#[tauri::command]
pub fn install_skill(state: State<'_, AppState>, path: String) -> Result<String, String> {
    let source = std::path::Path::new(&path);
    if !source.is_file() {
        return Err(format!("{path} is not a file."));
    }
    let raw = std::fs::read_to_string(source).map_err(|e| e.to_string())?;
    let id = skills::id_for(source, &raw);
    write_skill(&state.paths.skills_dir(), &id, &raw)
}

/// Install a skill pasted into the panel instead of picked from a file.
///
/// The shortcut that matters: a skill usually arrives as text — out of a chat, a
/// gist, a repository page — and saving it to disk first only to pick it back up
/// is the part people do not finish. The pasted text names itself.
#[tauri::command]
pub fn install_skill_text(state: State<'_, AppState>, text: String) -> Result<String, String> {
    if text.trim().is_empty() {
        return Err("There is nothing to install.".to_string());
    }
    let id = skills::id_from_text(&text);
    write_skill(&state.paths.skills_dir(), &id, &text)
}

/// Put back the bundled examples the user does not have. Returns the ids added,
/// which is empty when there was nothing missing.
#[tauri::command]
pub fn install_examples(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    skills::write_missing_examples(&state.paths.skills_dir()).map_err(|e| e.to_string())
}

/// Remove a skill directory. The id must be a plain directory name — a value
/// with a separator or `..` would let a bad string delete something else.
#[tauri::command]
pub fn delete_skill(state: State<'_, AppState>, id: String) -> Result<(), String> {
    if id.is_empty() || id.contains(['/', '\\']) || id.contains("..") {
        return Err("That is not a skill id.".to_string());
    }
    let dir = state.paths.skills_dir().join(&id);
    if !dir.is_dir() {
        return Err(format!("No skill called \"{id}\"."));
    }
    std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())
}

/// Import skills from a folder the user picked — either one skill directory, or
/// a folder holding several. A skill whose name is already installed is skipped
/// rather than replaced, and the report names both.
#[tauri::command]
pub fn import_skills(state: State<'_, AppState>, path: String) -> Result<skills::SkillImport, String> {
    let folder = std::path::Path::new(&path);
    if !folder.is_dir() {
        return Err(format!("{path} is not a folder."));
    }
    skills::import_folder(folder, &state.paths.skills_dir())
}

/// Write every skill to a folder the user picked, one `<id>/SKILL.md` each — the
/// shape Claude, Gemini and this app all read — so a library can leave here.
#[tauri::command]
pub fn export_skills(state: State<'_, AppState>, path: String) -> Result<Vec<String>, String> {
    skills::export_all(std::path::Path::new(&path), &state.paths.skills_dir())
}

// ---------------------------------------------------------------- projects

#[tauri::command]
pub fn list_projects(state: State<'_, AppState>) -> Result<Vec<db::ProjectRow>, String> {
    let conn = state.db.lock().unwrap();
    db::list_projects(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_project(
    state: State<'_, AppState>,
    name: String,
    folder: Option<String>,
) -> Result<db::ProjectRow, String> {
    let conn = state.db.lock().unwrap();
    db::create_project(&conn, &name, folder.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn rename_project(state: State<'_, AppState>, id: String, name: String) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::rename_project(&conn, &id, &name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_project_folder(
    state: State<'_, AppState>,
    id: String,
    folder: Option<String>,
) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::set_project_folder(&conn, &id, folder.as_deref()).map_err(|e| e.to_string())
}

/// Deletes the container, never the chats inside it — they are only unassigned.
#[tauri::command]
pub fn delete_project(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::delete_project(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_chat_project(
    state: State<'_, AppState>,
    chat_id: String,
    project_id: Option<String>,
) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::set_chat_project(&conn, &chat_id, project_id.as_deref()).map_err(|e| e.to_string())
}

/// Replaces a project's rules. Every chat in the project inherits them on its
/// next turn, so nothing has to be copied into the chats themselves.
#[tauri::command]
pub fn set_project_instructions(
    state: State<'_, AppState>,
    id: String,
    instructions: Vec<Instruction>,
) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::set_project_instructions(&conn, &id, &instructions).map_err(|e| e.to_string())
}

/// Spend per project, all-time. One query for every project the panel lists.
#[tauri::command]
pub fn project_spend(state: State<'_, AppState>) -> Result<Vec<db::ProjectSpend>, String> {
    let conn = state.db.lock().unwrap();
    db::project_spend(&conn).map_err(|e| e.to_string())
}

/// The memory file a project owns, or `None` when it has no folder.
///
/// The one place this is worked out: `project_category` in `memory_files.rs`
/// decides the file name, and the read and both writes below all come through
/// here. A project with no folder has no shared file — each of its chats keeps
/// its own — so "no folder" and "no memory" are the same answer, never a
/// fallback that would show another project's notes.
fn project_memory_category(
    state: &AppState,
    project_id: &str,
) -> Result<Option<String>, String> {
    let folder = {
        let conn = state.db.lock().unwrap();
        db::get_project(&conn, project_id)
            .map_err(|e| e.to_string())?
            .and_then(|p| p.folder)
    };
    Ok(folder
        .as_deref()
        .and_then(|f| crate::memory_files::project_category(Some(std::path::Path::new(f)))))
}

/// What the assistant has remembered about a project.
#[tauri::command]
pub fn project_memory(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<MemoryEntryView>, String> {
    let Some(category) = project_memory_category(&state, &project_id)? else {
        return Ok(Vec::new());
    };
    Ok(crate::memory_files::read(&state.paths.memory_dir(), &category)
        .into_iter()
        .map(|e| MemoryEntryView {
            date: e.date,
            text: e.text,
        })
        .collect())
}

/// Adds one entry to the project's own memory file, from the panel that shows it.
///
/// The panel is the writer here, so forgetting something no longer means leaving
/// the project and opening *Settings → Memory*.
#[tauri::command]
pub fn append_project_memory(
    state: State<'_, AppState>,
    project_id: String,
    text: String,
) -> Result<usize, String> {
    let category = project_memory_category(&state, &project_id)?
        .ok_or_else(|| "This project has no folder, so it keeps no shared memory.".to_string())?;
    crate::memory_files::append(&state.paths.memory_dir(), &category, &[text])
        .map_err(|e| e.to_string())
}

/// Forgets one entry from the project's own memory file. Only that line goes.
#[tauri::command]
pub fn delete_project_memory(
    state: State<'_, AppState>,
    project_id: String,
    text: String,
) -> Result<bool, String> {
    let category = project_memory_category(&state, &project_id)?
        .ok_or_else(|| "This project has no folder, so it keeps no shared memory.".to_string())?;
    crate::memory_files::remove(&state.paths.memory_dir(), &category, &text)
        .map_err(|e| e.to_string())
}

/// Every artifact in every chat — what the gallery lists.
#[tauri::command]
pub fn list_all_artifacts(state: State<'_, AppState>) -> Result<Vec<db::ArtifactRow>, String> {
    let conn = state.db.lock().unwrap();
    db::list_all_artifacts(&conn, 200).map_err(|e| e.to_string())
}

// ----------------------------------------------------------- design systems

/// One design system as the hub lists it: enough for a row and its switch.
///
/// Deliberately not a `SkillView`. A design system has no `default_on` — it is
/// opted into a chat deliberately — and no description of its own, and giving it
/// both to reuse a shape would be inventing two facts to save a struct.
#[derive(Serialize)]
pub struct DesignView {
    pub id: String,
    pub name: String,
    /// Switched on for the chat that asked, if one did.
    pub enabled: bool,
}

#[tauri::command]
pub fn list_design_systems(state: State<'_, AppState>, chat_id: Option<String>) -> Vec<DesignView> {
    let enabled: Vec<String> = match chat_id {
        Some(id) => {
            let conn = state.db.lock().unwrap();
            db::get_chat_skills(&conn, &id).unwrap_or_default()
        }
        None => Vec::new(),
    };
    crate::design::list(&state.paths.design_dir())
        .into_iter()
        .map(|s| DesignView {
            enabled: enabled.contains(&s.id),
            id: s.id,
            name: s.name,
        })
        .collect()
}

/// Writes a design system from the hub's form, overwriting one of that name.
/// Overwriting is the point here rather than a risk: this is the authoring
/// surface, so Save has to be able to save twice.
#[tauri::command]
pub fn save_design_system(
    state: State<'_, AppState>,
    name: String,
    body: String,
) -> Result<String, String> {
    if name.trim().is_empty() {
        return Err("A design system needs a name.".to_string());
    }
    if body.trim().is_empty() {
        return Err("There is nothing to save yet.".to_string());
    }
    crate::design::write(&state.paths.design_dir(), &name, &body)
}

/// Draft one from a folder that already has a look, then store it.
#[tauri::command]
pub fn design_system_from_project(
    state: State<'_, AppState>,
    folder: String,
) -> Result<String, String> {
    let path = std::path::Path::new(&folder);
    if !path.is_dir() {
        return Err(format!("{folder} is not a folder."));
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("design system");
    let draft = crate::design::draft_from_project(path);
    crate::design::write(&state.paths.design_dir(), name, &draft)
}

/// Removes one design system's file. The id guard is `design::file_for`'s, not
/// repeated here — one place to get the rule right.
#[tauri::command]
pub fn delete_design_system(state: State<'_, AppState>, id: String) -> Result<(), String> {
    crate::design::remove(&state.paths.design_dir(), &id)
}

/// Where design systems live, so the hub can show the real path.
#[tauri::command]
pub fn design_dir(state: State<'_, AppState>) -> String {
    state.paths.design_dir().display().to_string()
}

// ---------------------------------------------------------------- prompts

/// A saved prompt as the UI needs it: the id to insert by, and the text.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptView {
    pub id: String,
    pub name: String,
    /// The whole prompt, because the composer's job is to put it in the box.
    pub body: String,
}

/// Every saved prompt, by name.
///
/// The body comes along because there is no separate "open one" step: picking a
/// prompt from the menu is the only thing that ever happens to it, and a second
/// round trip to fill the box would be a round trip for nothing.
#[tauri::command]
pub fn list_prompts(state: State<'_, AppState>) -> Vec<PromptView> {
    crate::prompts::list(&state.paths.prompts_dir())
        .into_iter()
        .map(|p| PromptView {
            id: p.id,
            name: p.name,
            body: p.body,
        })
        .collect()
}

/// Writes a prompt from the hub's form, overwriting one of that name.
#[tauri::command]
pub fn save_prompt(state: State<'_, AppState>, name: String, body: String) -> Result<String, String> {
    if name.trim().is_empty() {
        return Err("A prompt needs a name.".to_string());
    }
    if body.trim().is_empty() {
        return Err("There is nothing to save yet.".to_string());
    }
    crate::prompts::write(&state.paths.prompts_dir(), &name, &body)
}

/// Removes one prompt's file. The id guard is the store's, not repeated here.
#[tauri::command]
pub fn delete_prompt(state: State<'_, AppState>, id: String) -> Result<(), String> {
    crate::prompts::remove(&state.paths.prompts_dir(), &id)
}

/// Where saved prompts live, so the hub can show the real path.
#[tauri::command]
pub fn prompts_dir(state: State<'_, AppState>) -> String {
    state.paths.prompts_dir().display().to_string()
}

/// Import a folder of another assistant's prompt files. A prompt file is a
/// markdown document whose first heading is its name, so one lifted from
/// anywhere needs no edit. A name already saved is skipped, never overwritten.
#[tauri::command]
pub fn import_prompts(
    state: State<'_, AppState>,
    path: String,
) -> Result<crate::prompts::PromptImport, String> {
    let folder = std::path::Path::new(&path);
    if !folder.is_dir() {
        return Err(format!("{path} is not a folder."));
    }
    crate::prompts::import_folder(folder, &state.paths.prompts_dir())
}

// ------------------------------------------------------------ permissions

#[tauri::command]
pub fn respond_approval(
    state: State<'_, AppState>,
    request_id: String,
    decision: String,
) -> Result<bool, String> {
    let answered = state.approvals.respond(&request_id, Decision::parse(&decision));
    // Answering it here is still answering it: the Scheduled panel's listing must
    // not outlive the park, wherever the click came from.
    let conn = state.db.lock().unwrap();
    let _ = db::forget_parked(&conn, &request_id);
    Ok(answered)
}

/// Resolve a question the model asked mid-run, which resumes the parked run.
#[tauri::command]
pub fn respond_question(
    state: State<'_, AppState>,
    request_id: String,
    answer: Answer,
) -> Result<bool, String> {
    let answered = state.questions.respond(&request_id, answer);
    let conn = state.db.lock().unwrap();
    let _ = db::forget_parked(&conn, &request_id);
    Ok(answered)
}

/// Everything a scheduled run is parked on, so it can be answered from the
/// Scheduled panel rather than in a chat nobody has open.
#[tauri::command]
pub fn list_parked(state: State<'_, AppState>) -> Result<Vec<db::ParkedRow>, String> {
    let conn = state.db.lock().unwrap();
    db::list_parked(&conn).map_err(|e| e.to_string())
}

/// Clears a park a crash left behind.
///
/// Such a row is marked `lost` on the next launch because the run it belonged to
/// died with the process — there is nothing left to resume — so this only takes
/// the notice off the list.
#[tauri::command]
pub fn dismiss_parked(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::forget_parked(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_tool_permissions(
    state: State<'_, AppState>,
    workspace: String,
) -> Result<Vec<String>, String> {
    let conn = state.db.lock().unwrap();
    db::list_allowed_tools(&conn, &workspace).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn revoke_tool_permission(
    state: State<'_, AppState>,
    workspace: String,
    tool: String,
    target: String,
) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::revoke_tool(&conn, &workspace, &tool, &target).map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolGrant {
    pub workspace: String,
    pub tool: String,
    /// What the grant covers — a site, a program, a folder, an app. Empty means
    /// the whole tool, which is what a grant meant before it could name one.
    pub target: String,
}

/// Every stored "always allow" grant: which tool, in which folder, covering what.
/// The other decisions last only as long as the app is running; these are the
/// ones that survive a restart, so they are the ones worth showing.
#[tauri::command]
pub fn tool_grants(state: State<'_, AppState>) -> Result<Vec<ToolGrant>, String> {
    let conn = state.db.lock().unwrap();
    let rows = db::all_allowed_tools(&conn).map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|(workspace, tool, target)| ToolGrant {
            workspace,
            tool,
            target,
        })
        .collect())
}

/// How many decisions the panel lists. A cap for the same reason the spend
/// window has one: the argument comes from the webview, and a list of everything
/// ever decided is not a panel.
const RECENT_DECISIONS_LIMIT: usize = 100;

/// The last decisions the app made on the user's behalf, newest first.
///
/// `tool_grants` answers "what is switched on now"; this answers "what happened",
/// which is the half that leaves no other trace — a denial and a guard refusal
/// both produce no tool result.
#[tauri::command]
pub fn recent_decisions(state: State<'_, AppState>) -> Result<Vec<db::ApprovalRecord>, String> {
    let conn = state.db.lock().unwrap();
    db::recent_decisions(&conn, RECENT_DECISIONS_LIMIT).map_err(|e| e.to_string())
}

// ------------------------------------------------------------------- spend
/// The longest window the panel may ask for. A cap rather than trust, because
/// the argument arrives from the webview: an unbounded one would let a single
/// call ask for every row ever written and hand the answer back as JSON.
const MAX_SPEND_DAYS: u32 = 365;
const DEFAULT_SPEND_DAYS: u32 = 30;
/// The dearest conversations to name. The panel is a glance, and a list of every
/// chat ever used is not one.
const SPEND_CHAT_LIMIT: usize = 10;

/// The start of the window a `days` argument means, capped.
///
/// Shared rather than spelled out per command, so the spend panel, the recap and
/// the export cannot end up looking at three different windows — a third copy of
/// the same two lines is exactly how that happens.
fn spend_window(days: Option<u32>) -> i64 {
    let days = days.unwrap_or(DEFAULT_SPEND_DAYS).clamp(1, MAX_SPEND_DAYS);
    db::now_ts() - i64::from(days) * 86_400
}

/// What the last `days` of turns cost, grouped by day, model and chat.
///
/// One command, one round trip: the panel opens and is complete.
#[tauri::command]
pub fn spend_report(
    state: State<'_, AppState>,
    days: Option<u32>,
) -> Result<db::SpendReport, String> {
    let since = spend_window(days);
    let conn = state.db.lock().unwrap();
    db::spend_report(&conn, since, SPEND_CHAT_LIMIT).map_err(|e| e.to_string())
}

/// The shape of the same window: how the turns fell across days and hours, and
/// which conversations they were.
///
/// Takes the same `days` argument as `spend_report` and applies the same cap, so
/// the two panels cannot be looking at two different windows.
#[tauri::command]
pub fn recap(state: State<'_, AppState>, days: Option<u32>) -> Result<db::Recap, String> {
    let since = spend_window(days);
    let conn = state.db.lock().unwrap();
    db::recap(&conn, since, SPEND_CHAT_LIMIT).map_err(|e| e.to_string())
}

/// The decision log and the bill, written to one file in the documents folder.
///
/// Returns the path, as the backup does, so the notice can name the file. The
/// window is the spend panel's own, and the decisions are the ones the settings
/// panel lists: the file says what the screens say, by calling the same reads.
///
/// The arguments of a call are not in it. An approval can cover a command
/// carrying a credential, which is why the log holds a digest in the first place
/// — and a file that gets mailed around is the last place to undo that.
#[tauri::command]
pub fn export_audit(state: State<'_, AppState>, days: Option<u32>) -> Result<String, String> {
    let export = {
        let conn = state.db.lock().unwrap();
        db::audit(&conn, spend_window(days), RECENT_DECISIONS_LIMIT, SPEND_CHAT_LIMIT)
            .map_err(|e| e.to_string())?
    };
    let json = serde_json::to_string_pretty(&export).map_err(|e| e.to_string())?;
    let dir = documents_dir_path(&state);
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not use {}: {e}", dir.display()))?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let path = crate::tools::docs::free_path(&dir, &format!("deskseek-audit-{stamp}"), "json");
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

// --------------------------------------------------------------- maintenance

/// What a sweep did, and how big the database is now.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Maintenance {
    /// Rows nothing can reach any more, across both tables.
    pub pruned: usize,
    pub bytes: u64,
}

/// Sweep the tables that grow for ever, and report the size.
///
/// Cheap in the common case: a launch-day prune is a handful of rows, and the
/// `VACUUM` that actually rewrites the file only runs when the prune was large
/// enough to be worth it (`db::vacuum_if_worth_it`).
#[tauri::command]
pub fn maintain_db(state: State<'_, AppState>) -> Result<Maintenance, String> {
    let conn = state.db.lock().unwrap();
    let report = db::prune(&conn).map_err(|e| e.to_string())?;
    db::vacuum_if_worth_it(&conn, report.total());
    Ok(Maintenance {
        pruned: report.total(),
        bytes: db::size_bytes(&conn),
    })
}

// --------------------------------------------------------------- messaging

/// Runs one turn, keeping the live registry and the durable marker in step.
///
/// `runs.rs` is in memory, so it is the stop button's fact and nothing more —
/// a process killed mid-turn leaves no trace of it. `in_flight_runs` is that
/// trace: written before the run and cleared after it, however it ended
/// (finished, stopped, errored). A kill never reaches the clear, and the row it
/// leaves behind is what the next launch reports as an interrupted turn, so the
/// two must be written together rather than in two places that could drift.
async fn run_turn(
    app: AppHandle,
    state: &AppState,
    chat_id: &str,
    unattended: bool,
    task_id: Option<&str>,
) -> Result<(), String> {
    let run_id = uuid::Uuid::new_v4().to_string();
    let (live, cancel) = state.runs.begin(chat_id);
    {
        let conn = state.db.lock().unwrap();
        let _ = db::mark_run_started(&conn, chat_id, &run_id);
    }
    let result = agent::run(app, state, chat_id, &run_id, cancel, unattended, task_id).await;
    state.runs.finish(chat_id, live);
    {
        let conn = state.db.lock().unwrap();
        let _ = db::mark_run_ended(&conn, chat_id);
    }
    result
}

#[tauri::command]
pub async fn send_message(
    app: AppHandle,
    state: State<'_, AppState>,
    chat_id: String,
    text: String,
    attachments: Option<Vec<Attachment>>,
) -> Result<(), String> {
    start_turn(app, &state, &chat_id, &text, attachments.unwrap_or_default(), false, None).await
}

/// The body of `send_message`, callable outside a command context.
///
/// The scheduler runs a turn through exactly this path rather than a second one
/// of its own, so a task that fired at 09:00 gets the same project scope, the
/// same mode and the same tool gating as a message the user typed — and a bug in
/// one cannot make the other behave differently.
///
/// `unattended` says the user did not press send. It changes nothing about what
/// the run may do; it only makes a prompt the run parks on raise a notification,
/// so a scheduled task cannot sit in silence waiting to be noticed.
///
/// `task_id` is the scheduled task this turn belongs to, or `None` for anything a
/// person started — including *Run now* from the panel, which is still the task's
/// run and is tagged as one.
pub async fn start_turn(
    app: AppHandle,
    state: &AppState,
    chat_id: &str,
    text: &str,
    attachments: Vec<Attachment>,
    unattended: bool,
    task_id: Option<&str>,
) -> Result<(), String> {
    ready_to_send(state)?;

    let chat = {
        let conn = state.db.lock().unwrap();
        db::get_chat(&conn, chat_id).map_err(|e| e.to_string())?
    };
    if chat.kind == "coding" && chat.workspace.is_none() {
        return Err("Pick a folder for this coding task first.".to_string());
    }

    if text.trim().is_empty() && attachments.is_empty() {
        return Err("Nothing to send.".to_string());
    }
    let attachments_json = if attachments.is_empty() {
        None
    } else {
        Some(serde_json::to_string(&attachments).map_err(|e| e.to_string())?)
    };

    {
        let conn = state.db.lock().unwrap();
        db::insert_message(
            &conn,
            chat_id,
            "user",
            Some(text),
            None,
            None,
            None,
            attachments_json.as_deref(),
        )
        .map_err(|e| e.to_string())?;
        if chat.title == "New chat" {
            let title = if text.trim().is_empty() {
                format!("{} attachment(s)", attachments.len())
            } else {
                title_from(text)
            };
            db::rename_chat(&conn, chat_id, &title).map_err(|e| e.to_string())?;
        }
    }

    // One live run per chat: registering cancels any turn already in flight.
    let result = run_turn(app.clone(), state, chat_id, unattended, task_id).await;
    // Tell someone who is not looking. `notify` decides whether that is a toast
    // at all; this only reports what happened.
    notify::turn_finished(&app, state, chat_id, result.is_err());
    result
}

/// Stops the live run for a chat. False when nothing was running.
#[tauri::command]
pub fn stop_chat(state: State<'_, AppState>, chat_id: String) -> bool {
    state.runs.cancel(&chat_id)
}

/// Chat ids with a run in flight — how a background tab knows it is still going.
#[tauri::command]
pub fn running_chats(state: State<'_, AppState>) -> Vec<String> {
    state.runs.running()
}

/// Turns a previous session left in flight, newest first.
///
/// Read once at startup, which is the only moment it means anything: while a run
/// is live its marker is present on purpose, so a later read would report the
/// run the user is already watching.
#[tauri::command]
pub fn interrupted_runs(state: State<'_, AppState>) -> Result<Vec<db::InterruptedRun>, String> {
    let conn = state.db.lock().unwrap();
    db::interrupted_runs(&conn).map_err(|e| e.to_string())
}

/// Stop treating a chat as interrupted, once the user has dealt with it.
#[tauri::command]
pub fn dismiss_interrupted(state: State<'_, AppState>, chat_id: String) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::mark_run_ended(&conn, &chat_id).map_err(|e| e.to_string())
}

// --------------------------------------------------------------- scheduling

/// A scheduled task fired. The UI cannot see the app's clock, so it is told.
pub const EVENT_SCHEDULED_FIRED: &str = "scheduled-fired";

/// "HH:MM" as stored, normalised through the parsed time so `9:00` and `09:00`
/// cannot both end up in the database.
fn clock(time: chrono::NaiveTime) -> String {
    time.format("%H:%M").to_string()
}

#[tauri::command]
pub fn list_scheduled(state: State<'_, AppState>) -> Result<Vec<db::ScheduledRow>, String> {
    let conn = state.db.lock().unwrap();
    db::list_scheduled(&conn).map_err(|e| e.to_string())
}

/// Creates a scheduled task from the three fields that describe a schedule,
/// computing its first run here.
///
/// The one create path: the form's command and the model's `schedule_task` tool
/// both come through it, so a task made either way gets the same normalising
/// (`Schedule::parse`) and the same clock. A second copy of this would be a
/// second answer to "when does it first run". `spec` is the watching half — a
/// cadence unless the caller says otherwise — and it is what decides whether
/// "first run" means a time or the first look. `model` is the model this task
/// runs on, or blank for the chat's own.
#[allow(clippy::too_many_arguments)]
pub fn create_scheduled_task(
    state: &AppState,
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
) -> Result<db::ScheduledRow, String> {
    if prompt.trim().is_empty() {
        return Err("A scheduled task needs something to do.".to_string());
    }
    spec.validate()?;
    let sched = schedule::Schedule::parse(freq, time, weekday, nth);
    let next = match spec.first_check(db::now_ts()) {
        Some(next) => next,
        None => schedule::next_run_at(Local::now(), &sched),
    };
    let conn = state.db.lock().unwrap();
    db::create_scheduled(
        &conn,
        title,
        prompt.trim(),
        chat_id,
        project_id,
        sched.freq.id(),
        &clock(sched.time),
        sched.weekday as i64,
        sched.nth as i64,
        spec,
        model,
        next,
    )
    .map_err(|e| e.to_string())
}

/// Adds a task, computing its first run here. The form sends the three fields
/// that describe a schedule rather than a timestamp, so "daily at 09:00" is one
/// description and not two that can disagree.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn create_scheduled(
    state: State<'_, AppState>,
    title: String,
    prompt: String,
    chat_id: Option<String>,
    project_id: Option<String>,
    freq: String,
    time: String,
    weekday: i64,
    nth: i64,
    kind: String,
    target: String,
    filter: String,
    interval_secs: i64,
    model: Option<String>,
) -> Result<db::ScheduledRow, String> {
    let spec = triggers::Spec::new(&kind, &target, &filter, interval_secs);
    create_scheduled_task(
        &state,
        &title,
        &prompt,
        chat_id.as_deref(),
        project_id.as_deref(),
        &freq,
        &time,
        weekday,
        nth,
        &spec,
        model.as_deref().unwrap_or(""),
    )
}

/// Rewrites a task. A changed time moves its next run: otherwise an edit made
/// ten minutes before the old time would appear to do nothing for a day. A
/// trigger's "next run" is its next look on its own interval, and an edit
/// always re-arms it — see `db::update_scheduled`.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn update_scheduled(
    state: State<'_, AppState>,
    id: String,
    title: String,
    prompt: String,
    chat_id: Option<String>,
    project_id: Option<String>,
    freq: String,
    time: String,
    weekday: i64,
    nth: i64,
    enabled: bool,
    kind: String,
    target: String,
    filter: String,
    interval_secs: i64,
    model: Option<String>,
) -> Result<(), String> {
    if prompt.trim().is_empty() {
        return Err("A scheduled task needs something to do.".to_string());
    }
    let spec = triggers::Spec::new(&kind, &target, &filter, interval_secs);
    spec.validate()?;
    let sched = schedule::Schedule::parse(&freq, &time, weekday, nth);
    let next = match spec.first_check(db::now_ts()) {
        Some(next) => next,
        None => schedule::next_run_at(Local::now(), &sched),
    };
    let conn = state.db.lock().unwrap();
    db::update_scheduled(
        &conn,
        &id,
        &title,
        prompt.trim(),
        chat_id.as_deref(),
        project_id.as_deref(),
        sched.freq.id(),
        &clock(sched.time),
        sched.weekday as i64,
        sched.nth as i64,
        enabled,
        &spec,
        model.as_deref().unwrap_or(""),
        next,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_scheduled(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::delete_scheduled(&conn, &id).map_err(|e| e.to_string())
}

/// A scheduled task's recent runs, newest first.
///
/// Read from `turn_usage`, the same rows `spend_report` sums, so what the panel
/// lists for a task and what the spend view reports over the same window are two
/// readings of one number. Capped, so the panel cannot ask for the whole history.
#[tauri::command]
pub fn task_runs(
    state: State<'_, AppState>,
    task_id: String,
    limit: Option<usize>,
) -> Result<Vec<db::TaskRunRow>, String> {
    let conn = state.db.lock().unwrap();
    db::task_runs(&conn, &task_id, limit.unwrap_or(20).min(100)).map_err(|e| e.to_string())
}

/// Runs a task now, without moving when it next runs on its own.
#[tauri::command]
pub async fn run_scheduled_now(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<String, String> {
    let task = {
        let conn = state.db.lock().unwrap();
        db::list_scheduled(&conn)
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|t| t.id == id)
            .ok_or("That task is gone.")?
    };
    fire_scheduled(&app, &state, &task).await
}

/// Runs one task: in its own chat, or in a fresh one, then the prompt through
/// the ordinary turn path.
///
/// The caller advances `next_run_at` *before* calling this, so a run longer than
/// the tick interval is never started a second time.
pub async fn fire_scheduled(
    app: &AppHandle,
    state: &AppState,
    task: &db::ScheduledRow,
) -> Result<String, String> {
    let settings = state.settings.lock().unwrap().clone();
    // What a fresh chat for this run is created on: the task's pin when it has
    // one, so the conversation the user later opens is on the same model that
    // ran it, and the standing default otherwise.
    let fresh_model = if task.model.trim().is_empty() {
        settings.model_for("chat")
    } else {
        task.model.trim().to_string()
    };
    // Whether this run got a chat of its own. A chat the task already owns keeps
    // the skills it has; only a brand-new one takes the standing defaults.
    let (chat_id, fresh) = {
        let conn = state.db.lock().unwrap();
        let existing = task
            .chat_id
            .as_deref()
            .and_then(|id| db::get_chat(&conn, id).ok());
        match existing {
            Some(chat) => {
                // Nobody is watching an unattended run, so nothing may park on an
                // approval prompt — it would sit there until the user came back.
                // Auto is the honest mode for a run the user scheduled.
                let _ = db::set_chat_permission_mode(&conn, &chat.id, "auto");
                (chat.id, false)
            }
            None => {
                // A fresh chat per run, so a daily task does not become one
                // conversation that grows forever.
                let folder = task.project_id.as_deref().and_then(|id| {
                    db::list_projects(&conn)
                        .ok()?
                        .into_iter()
                        .find(|p| p.id == id)?
                        .folder
                });
                let chat = db::create_chat(
                    &conn,
                    "chat",
                    folder.as_deref(),
                    &fresh_model,
                    settings.thinking,
                    &settings.effort,
                    "auto",
                )
                .map_err(|e| e.to_string())?;
                db::rename_chat(&conn, &chat.id, &task.title).map_err(|e| e.to_string())?;
                if let Some(project) = task.project_id.as_deref() {
                    db::set_chat_project(&conn, &chat.id, Some(project))
                        .map_err(|e| e.to_string())?;
                }
                (chat.id, true)
            }
        }
    };
    // After the lock is released: the seeding helper takes it itself.
    if fresh {
        seed_default_skills(state, &chat_id);
    }

    let _ = app.emit(
        EVENT_SCHEDULED_FIRED,
        serde_json::json!({ "taskId": task.id, "chatId": chat_id }),
    );
    // No toast here: `start_turn` reports when the run *finishes*, which is the
    // moment the user asked to hear about. The event above is what says it
    // started, for the UI that is already open.
    start_turn(app.clone(), state, &chat_id, &task.prompt, Vec::new(), true, Some(&task.id)).await?;
    Ok(chat_id)
}

/// Whether computer control is currently stopped. Read on startup so the banner
/// is right even if the hotkey was used before the window was focused.
#[tauri::command]
pub fn computer_state(state: State<'_, AppState>) -> bool {
    state.computer.is_paused()
}

/// Starts or stops computer control from the UI. Emits the same event the kill
/// switch does, so the banner and the truth never disagree.
#[tauri::command]
pub fn set_computer_paused(app: AppHandle, state: State<'_, AppState>, paused: bool) -> bool {
    if paused {
        state.computer.pause();
    } else {
        state.computer.resume();
    }
    let paused = state.computer.is_paused();
    let _ = app.emit(
        desktop::EVENT_COMPUTER,
        serde_json::json!({
            "paused": paused,
            "reason": if paused { Some("Computer control is stopped.") } else { None },
        }),
    );
    paused
}

// ----------------------------------------------------------------- setup

/// What the first-run screen needs: which of the pieces it recommends are
/// already on this machine, and whether the configured search actually works.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvProbe {
    pub has_api_key: bool,
    /// `docker --version` — the CLI is on PATH. Client-only, so it answers even
    /// when the engine is stopped.
    pub docker_cli: bool,
    /// `docker info` — the engine actually answers. False for the very common
    /// case of Docker Desktop installed and simply not started.
    pub docker_running: bool,
    /// What `docker --version` printed, when the CLI answered at all.
    pub docker_version: String,
    /// Docker Desktop's launcher is where it installs, so the engine can be
    /// started from the screen rather than only reported as stopped.
    pub docker_desktop: bool,
    /// WSL2 can run the same jobs where Docker is not installed.
    pub wsl: bool,
    /// The distribution commands would run in, when one is installed.
    pub wsl_distro: String,
    /// Where `run_command` will actually run, and why — the resolved rung, not
    /// the raw capability, because that is the thing the user can change.
    pub sandbox_rung: String,
    pub sandbox_note: Option<String>,
    /// "Edge" / "Chrome" / "Chromium" — whichever browser_read will drive.
    pub browser_name: String,
    /// The executable it will drive, so the setting can be checked by eye.
    pub browser_path: String,
    pub searxng_url: String,
    pub searxng_ok: bool,
}

/// A tool's version output, trimmed to the one line a screen can show.
fn first_line(raw: &str) -> String {
    raw.lines().next().unwrap_or("").trim().chars().take(120).collect()
}

/// "Installed" and "running" are different questions, and Docker answers them
/// with different commands: `docker --version` is a client-only string, while
/// `docker info` fails with exit 1 when the daemon's named pipe is not there.
/// Asking the second one to detect the first reported a working Docker Desktop
/// as "not found", so both are asked separately.
///
/// Probes are slow-tolerant but bounded: a machine without Docker should say so
/// in a second, not hang the screen.
#[tauri::command]
pub async fn probe_environment(state: State<'_, AppState>) -> Result<EnvProbe, String> {
    let settings = state.settings.lock().unwrap().clone();

    // One probe, shared with the run loop: the answer is cached on the state so
    // a run never has to ask Docker anything before it can start. `capture`
    // rather than `probe` because the version string is worth having too, and
    // it answers "is the CLI there" just as well: a program that cannot be run
    // at all is the only thing that gives None.
    let (caps, docker) = tokio::join!(
        sandbox_exec::detect(),
        exec::capture("docker", &["--version"], 6),
    );
    *state.sandbox.write().await = caps.clone();

    // Pressing Re-check here is exactly how a stopped engine becomes a running
    // one, so the resolved rung is recomputed from the fresh answer.
    let plan = sandbox_exec::plan_for(
        sandbox_exec::Mode::parse(&settings.sandbox),
        &caps,
        &settings.sandbox_image,
    );

    // Found the same way browser_read finds it, so the screen names the browser
    // that will really be driven rather than making its own second guess. The
    // version is deliberately not probed: `chrome.exe --version` opens a window
    // on Windows instead of printing, and opening browsers is not something
    // that should happen because somebody looked at their settings.
    let browser = browse::find_browser(&settings.browser_path);

    let searxng_ok = browse::searxng_ready(&state.http, &settings.searxng_url).await;

    Ok(EnvProbe {
        has_api_key: ready_to_send(&state).is_ok(),
        docker_cli: docker.is_some(),
        docker_running: caps.docker_running,
        docker_version: docker.as_deref().map(first_line).unwrap_or_default(),
        docker_desktop: sandbox_exec::docker_desktop().is_some(),
        wsl: caps.wsl_distro.is_some(),
        wsl_distro: caps.wsl_distro.clone().unwrap_or_default(),
        sandbox_rung: plan.rung.as_str().to_string(),
        sandbox_note: plan.note,
        browser_name: browser.as_deref().map(browse::browser_name).unwrap_or_default(),
        browser_path: browser
            .as_deref()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        searxng_url: settings.searxng_url,
        searxng_ok,
    })
}

/// Start Docker Desktop, best effort.
///
/// The engine answers tens of seconds after the launcher returns, so this only
/// starts it and says so — Re-check is what waits for the answer, and claiming
/// the engine is up here would be a lie the very next command would expose.
#[tauri::command]
pub fn start_docker() -> Result<(), String> {
    let launcher = sandbox_exec::docker_desktop().ok_or(
        "Docker Desktop is not in the folder it usually installs to — start it from the Start menu.",
    )?;
    std::process::Command::new(launcher)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Stand a local SearXNG instance up, and point `web_search` at it.
///
/// `searxng.rs` owns everything about the container; this owns the app's side of
/// it — where the config lives, and that the setting follows *only* once the
/// instance actually answers. Saving the URL first would leave a screen reading
/// "SearXNG answering" over an instance that never came up.
///
/// Returns the URL rather than a probe: the caller re-probes with
/// `probe_environment`, which is what the screen already trusts, so a button that
/// worked looks exactly like someone pasting the URL in by hand.
#[tauri::command]
pub async fn install_searxng(state: State<'_, AppState>) -> Result<String, String> {
    let url = searxng::ensure_running(&state.paths.searxng_dir(), &state.http).await?;

    let mut settings = state.settings.lock().unwrap().clone();
    settings.searxng_url = url.clone();
    crate::store::settings::save(&state.paths.settings_file(), &settings)
        .map_err(|e| e.to_string())?;
    *state.settings.lock().unwrap() = settings;

    Ok(url)
}

/// Show a file or folder in the system file manager.
///
/// Same reasoning as `open_document`: the opener free function is the one that
/// needs no scope entry in the capability file, and revealing is the read-only
/// cousin of opening. The path is checked first so a stale one is reported
/// rather than silently doing nothing.
#[tauri::command]
pub fn reveal_path(path: String) -> Result<(), String> {
    let target = std::path::Path::new(&path);
    if !target.exists() {
        return Err(format!("{path} is not there any more."));
    }
    tauri_plugin_opener::reveal_item_in_dir(target).map_err(|e| e.to_string())
}

// ------------------------------------------------------------- attachments

/// How large a picture may be before the attachment is refused. It is sent
/// inline, so a huge one would be a stored row nobody can use.
const MAX_IMAGE_BYTES: u64 = 25 * 1024 * 1024;
/// Text beyond this is left in the file rather than copied into the message.
const MAX_TEXT_BYTES: u64 = 200 * 1024;
/// How much of a *document* is pasted into the message.
///
/// ponytail: an attachment rides in the chat's history, so a long document would
/// be paid for on every later turn. Past this the file stays in the chat's own
/// folder and the model is told to read it when it needs it. The ceiling is a
/// document worth having up front that is too long to paste; the upgrade is to
/// make the threshold a setting beside the other context controls.
const MAX_INLINE_DOC_CHARS: usize = 20_000;

fn file_name_of(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .to_string()
}

/// A path in `dir` that is not taken.
///
/// Deliberately not `docs::free_path`: that one always builds `stem.ext`, and a
/// file the user drops may have no extension at all.
fn free_path_for(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let path = Path::new(name);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = path.extension().and_then(|e| e.to_str());
    for n in 2..1000 {
        let candidate = match ext {
            Some(ext) => dir.join(format!("{stem}-{n}.{ext}")),
            None => dir.join(format!("{stem}-{n}")),
        };
        if !candidate.exists() {
            return candidate;
        }
    }
    // A thousand collisions is not a real situation; the timestamp is a floor.
    dir.join(format!("{stem}-{}", db::now_ts()))
}

/// Take a file the user just added into the chat's own folder.
///
/// This is what makes a chat really own its documents: after a drop the file is
/// in the folder the file panel shows, `read_file` can open it, and a later
/// document is created beside it. A name that is already taken keeps both files
/// — `report.docx` becomes `report-2.docx` — because quietly replacing one would
/// be the only unrecoverable thing this could do.
fn adopt_file(state: &AppState, chat_id: &str, source: &Path) -> Result<(PathBuf, String), String> {
    let dir = chat_dir(state, chat_id)?;
    // Already in the folder: leave it where it is rather than making a copy of it
    // beside itself.
    if let (Ok(from), Ok(here)) = (std::fs::canonicalize(source), std::fs::canonicalize(&dir)) {
        if from.parent() == Some(here.as_path()) {
            let name = file_name_of(&from);
            return Ok((from, name));
        }
    }
    let name = file_name_of(source);
    let target = free_path_for(&dir, &name);
    std::fs::copy(source, &target)
        .map_err(|e| format!("could not copy {name} into this chat's folder: {e}"))?;
    let stored = file_name_of(&target);
    Ok((target, stored))
}

/// The same, for bytes: a file chosen with the picker or pasted into the composer
/// is a `File`, not a path, and the webview cannot write to disk itself.
fn save_bytes(
    state: &AppState,
    chat_id: &str,
    name: &str,
    bytes: &[u8],
) -> Result<(PathBuf, String), String> {
    let dir = chat_dir(state, chat_id)?;
    // The name arrives from the webview, so it is reduced to a single component
    // before it is used as one: `file_name()` of `../../x` is `x`, and the write
    // cannot leave the chat's folder.
    let name = Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    let target = free_path_for(&dir, name);
    std::fs::write(&target, bytes)
        .map_err(|e| format!("could not save {name} in this chat's folder: {e}"))?;
    let stored = file_name_of(&target);
    Ok((target, stored))
}

/// The text of a document, when there is one and it is short enough to paste.
///
/// `None` means "do not paste anything": a scan with no text in it, a file that
/// could not be read, or a document whose text is too long to ride along with
/// every later turn in the chat.
fn inlined_document(name: &str, path: &Path, size: u64) -> Option<String> {
    if !crate::tools::extract::is_extractable(name) || size > crate::tools::extract::MAX_DOC_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let text = crate::tools::extract::text_of(name, &bytes).ok()?;
    (text.chars().count() <= MAX_INLINE_DOC_CHARS).then_some(text)
}

/// What the model is given of a file that has just been added.
///
/// A document it can read is pasted in when it is short enough to be worth it,
/// and otherwise named so it can be read on demand — the file is in the chat's
/// folder either way, so nothing is lost by waiting until something needs it.
fn attached_text(path: &Path, mime: &str, size: u64) -> String {
    // An image is the one attachment the model is *shown* rather than told: the
    // caller already holds the bytes to downscale, so there is nothing to
    // describe and no `read_file` to suggest — for a PNG that would answer
    // "binary file" anyway.
    if mime.starts_with("image/") {
        return String::new();
    }
    let name = file_name_of(path);
    if crate::tools::extract::is_extractable(&name) {
        if let Some(text) = inlined_document(&name, path, size) {
            return text;
        }
        // One sentence that is true whether the text was too long or there was
        // none to lift out, so nothing here has to guess which it was.
        return format!(
            "{name} is in this chat's folder; its text is not pasted in here. Call \
             read_file(\"{name}\") when you need it — and if that comes back with nothing, the \
             document's words are a picture rather than characters."
        );
    }
    if is_text_mime(mime) && size <= MAX_TEXT_BYTES {
        if let Ok(bytes) = std::fs::read(path) {
            return String::from_utf8_lossy(&bytes).into_owned();
        }
    }
    format!("{name} is in this chat's folder. read_file(\"{name}\") will open it.")
}

/// Add a dropped file to the chat.
///
/// A native drop hands the webview a *path*, not a `File`, and the webview
/// cannot read the disk itself — so the file is handled here. Every file is
/// copied into the chat's own folder, and what the model can read of it is
/// returned with it. A picture is *also* returned as a `data:` URL, because an
/// image is the one thing the model can be shown rather than told (the frontend
/// downscales it before it is sent).
#[tauri::command]
pub fn attach_path(
    state: State<'_, AppState>,
    chat_id: Option<String>,
    path: String,
) -> Result<Attachment, String> {
    let file = Path::new(&path);
    if !file.is_file() {
        return Err(format!("{path} is not a file."));
    }
    let name = file_name_of(file);
    let size = std::fs::metadata(file).map_err(|e| e.to_string())?.len();
    let mime = mime_for(&name);

    if mime.starts_with("image/") {
        if size > MAX_IMAGE_BYTES {
            return Err(format!("{name} is larger than 25 MB."));
        }
        // A picture is filed like every other attachment — the chat owns what the
        // user put in it — so a screenshot in a conversation is in that
        // conversation's folder rather than only in a message row.
        let stored = match chat_id.as_deref() {
            Some(id) => adopt_file(&state, id, file)?.1,
            None => name.clone(),
        };
        let bytes = std::fs::read(file).map_err(|e| e.to_string())?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
        return Ok(Attachment {
            data_url: format!("data:{mime};base64,{encoded}"),
            text: String::new(),
            name: stored,
            mime,
        });
    }

    // Dropped before the first conversation exists: there is no folder for it to
    // join yet, so the text is still lifted out (that needs nothing but the file)
    // and nothing claims it is filed anywhere. `send` creates the chat from the
    // draft, and a file added after that is filed properly.
    let Some(chat_id) = chat_id else {
        let text = inlined_document(&name, file, size).unwrap_or_default();
        return Ok(Attachment {
            name,
            mime,
            data_url: String::new(),
            text,
        });
    };

    let (stored_path, stored_name) = adopt_file(&state, &chat_id, file)?;
    let text = attached_text(&stored_path, &mime, size);
    Ok(Attachment {
        name: stored_name,
        mime,
        data_url: String::new(),
        text,
    })
}

/// The picker's route to the same place a drop lands.
///
/// Its bytes are sent here because the webview has a `File` and no path, and it
/// is written into the chat's folder exactly as a dropped one is — one rule,
/// whichever way the file arrived.
#[tauri::command]
pub fn attach_bytes(
    state: State<'_, AppState>,
    chat_id: Option<String>,
    name: String,
    data_url: String,
) -> Result<Attachment, String> {
    let bytes = decode_data_url(&data_url).ok_or("That file could not be read.")?;
    let mime = mime_for(&name);
    // No chat yet: keep the rule the composer already had — a text file carries
    // its own text, anything else travels as a name.
    let Some(chat_id) = chat_id else {
        let text = if is_text_mime(&mime) && bytes.len() as u64 <= MAX_TEXT_BYTES {
            String::from_utf8_lossy(&bytes).into_owned()
        } else {
            String::new()
        };
        return Ok(Attachment {
            name,
            mime,
            data_url: String::new(),
            text,
        });
    };
    let (stored_path, stored_name) = save_bytes(&state, &chat_id, &name, &bytes)?;
    let text = attached_text(&stored_path, &mime, bytes.len() as u64);
    Ok(Attachment {
        name: stored_name,
        mime,
        data_url: String::new(),
        text,
    })
}

/// The bytes a `data:` URL carries.
fn decode_data_url(data_url: &str) -> Option<Vec<u8>> {
    let comma = data_url.find(',')?;
    let (meta, payload) = data_url.split_at(comma + 1);
    if !meta.contains(";base64") {
        return None;
    }
    base64::engine::general_purpose::STANDARD.decode(payload).ok()
}

/// Whether a MIME type is plain text the model can be handed directly.
fn is_text_mime(mime: &str) -> bool {
    mime.starts_with("text/") || mime == "application/json" || mime == "application/xml"
}

/// A best-effort MIME type from a file name.
///
/// Only the `image/*` cases matter: `Attachment::is_image` is what decides
/// whether an attachment becomes a multimodal content part, so getting those
/// right is what lets a dropped screenshot actually reach the model.
fn mime_for(name: &str) -> String {
    let ext = std::path::Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "json" => "application/json",
        "csv" => "text/csv",
        "html" | "htm" => "text/html",
        "txt" | "md" | "log" => "text/plain",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        _ => "application/octet-stream",
    }
    .to_string()
}

// ------------------------------------------------------------------- files

/// The folder a chat owns, canonicalized so two chats naming the same folder
/// end up sharing one watch.
fn chat_root(state: &AppState, chat_id: &str) -> Result<PathBuf, String> {
    let chat = {
        let conn = state.db.lock().unwrap();
        db::get_chat(&conn, chat_id).map_err(|e| e.to_string())?
    };
    let root = agent::workspace_for(state, &chat).ok_or("This chat has no folder yet.")?;
    Ok(std::fs::canonicalize(&root).unwrap_or(root))
}

/// Where this chat's own files live, so the header can show the user the folder
/// instead of only promising that one exists.
#[tauri::command]
pub fn chat_folder(state: State<'_, AppState>, chat_id: String) -> Result<String, String> {
    Ok(chat_dir(&state, &chat_id)?.display().to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeEntry {
    pub name: String,
    /// Workspace-relative, with `/` separators.
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeListing {
    /// Absolute path of the folder being listed.
    pub root: String,
    /// The folder listed, relative to the root — empty at the top.
    pub path: String,
    pub entries: Vec<TreeEntry>,
}

/// One level of the tree. Lazy on purpose: a folder holding ten thousand files
/// should not be read just to draw the row that says it exists.
#[tauri::command]
pub fn list_tree(
    state: State<'_, AppState>,
    chat_id: String,
    path: Option<String>,
) -> Result<TreeListing, String> {
    const MAX_ENTRIES: usize = 2_000;
    let root = chat_root(&state, &chat_id)?;
    let rel = path.unwrap_or_default();
    let dir = sandbox::resolve(&root, if rel.is_empty() { "." } else { &rel })?;

    let mut entries = Vec::new();
    for entry in std::fs::read_dir(&dir)
        .map_err(|e| format!("cannot read '{rel}': {e}"))?
        .flatten()
    {
        if entries.len() >= MAX_ENTRIES {
            break;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() && walk::should_skip(&name) {
            continue;
        }
        entries.push(TreeEntry {
            path: sandbox::display(&root, &entry.path()),
            is_dir: kind.is_dir(),
            size: if kind.is_file() {
                entry.metadata().map(|m| m.len()).unwrap_or(0)
            } else {
                0
            },
            name,
        });
    }
    // Folders first, then case-insensitive by name — how every editor does it.
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(TreeListing {
        root: root.to_string_lossy().to_string(),
        path: rel,
        entries,
    })
}

/// Find files anywhere under the chat's folder by **name**.
///
/// The tree is lazy, so a box that only filtered what had been opened could not
/// find a file nobody had expanded — it looked like a search and was not one.
/// This reads the folder instead, skipping the same noise directories the walker
/// skips and capped the same way, so a name is found three levels down without
/// opening anything. Contents are still `grep`'s job, and the model's.
#[tauri::command]
pub fn find_in_tree(
    state: State<'_, AppState>,
    chat_id: String,
    query: String,
) -> Result<Vec<TreeEntry>, String> {
    const MAX_HITS: usize = 200;
    let needle = query.trim().to_lowercase();
    // One letter would match most of a large tree and is never what was meant.
    if needle.chars().count() < 2 {
        return Ok(Vec::new());
    }
    let root = chat_root(&state, &chat_id)?;
    Ok(walk::find_named(&root, &needle, MAX_HITS)
        .into_iter()
        .filter_map(|path| {
            let meta = std::fs::metadata(&path).ok()?;
            Some(TreeEntry {
                name: path.file_name()?.to_string_lossy().to_string(),
                path: sandbox::display(&root, &path),
                is_dir: false,
                size: meta.len(),
            })
        })
        .collect())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePreview {
    pub path: String,
    pub size: u64,
    pub text: Option<String>,
    /// Why there is no text: binary, or over the preview cap.
    pub note: Option<String>,
}
/// A read-only peek at a file, so the user can watch the agent's work without
/// interrupting it.
#[tauri::command]
pub fn read_preview(
    state: State<'_, AppState>,
    chat_id: String,
    path: String,
) -> Result<FilePreview, String> {
    const MAX_PREVIEW_BYTES: u64 = 512 * 1024;
    let root = chat_root(&state, &chat_id)?;
    let full = sandbox::resolve(&root, &path)?;
    let meta = std::fs::metadata(&full).map_err(|e| format!("cannot open '{path}': {e}"))?;
    if meta.is_dir() {
        return Err(format!("'{path}' is a folder"));
    }

    let size = meta.len();
    // A Word, Excel, PowerPoint or PDF file is not text, and "binary file" is a
    // useless thing to show someone who just clicked their own report. The panel
    // shows what is in the document instead, through the same reader the model
    // uses — so what the user sees and what the model sees cannot disagree.
    if crate::tools::extract::is_extractable(&path) {
        let limit = crate::tools::extract::MAX_DOC_BYTES;
        if size > limit {
            return Ok(FilePreview {
                path,
                size,
                text: None,
                note: Some(format!("{} MB — too large to read", size / (1024 * 1024))),
            });
        }
        let bytes = std::fs::read(&full).map_err(|e| format!("cannot open '{path}': {e}"))?;
        return Ok(match crate::tools::extract::text_of(&path, &bytes) {
            Ok(text) => FilePreview {
                path,
                size,
                text: Some(text),
                note: Some("text lifted out of the document".to_string()),
            },
            Err(note) => FilePreview {
                path,
                size,
                text: None,
                note: Some(note),
            },
        });
    }
    if size > MAX_PREVIEW_BYTES {
        return Ok(FilePreview {
            path,
            size,
            text: None,
            note: Some(format!("{} KB — too large to preview", size / 1024)),
        });
    }
    match std::fs::read_to_string(&full) {
        Ok(text) => Ok(FilePreview {
            path,
            size,
            text: Some(text),
            note: None,
        }),
        // Not every byte sequence is text; say so rather than guess an encoding.
        Err(_) => Ok(FilePreview {
            path,
            size,
            text: None,
            note: Some("binary file".to_string()),
        }),
    }
}

/// Start watching this chat's folder so the tree keeps itself up to date.
#[tauri::command]
pub fn watch_workspace(state: State<'_, AppState>, chat_id: String) -> Result<String, String> {
    let root = chat_root(&state, &chat_id)?;
    state.files.watch(&root)?;
    Ok(root.to_string_lossy().to_string())
}

#[tauri::command]
pub fn unwatch_workspace(state: State<'_, AppState>, chat_id: String) -> Result<(), String> {
    if let Ok(root) = chat_root(&state, &chat_id) {
        state.files.unwatch(&root);
    }
    Ok(())
}

// ------------------------------------------------------------------- MCP

/// Everything the MCP panel needs in one round trip.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpStatus {
    pub servers: Vec<ServerStatus>,
    pub config_error: Option<String>,
    pub tool_count: usize,
    pub running_count: usize,
    pub config_path: String,
}

async fn mcp_snapshot(state: &AppState) -> McpStatus {
    let mcp = state.mcp.read().await;
    McpStatus {
        servers: mcp.status(),
        config_error: mcp.config_error(),
        tool_count: mcp.tool_count(),
        running_count: mcp.serviced_count(),
        config_path: state.paths.mcp_file().to_string_lossy().to_string(),
    }
}

#[tauri::command]
pub async fn mcp_status(state: State<'_, AppState>) -> Result<McpStatus, String> {
    Ok(mcp_snapshot(&state).await)
}

#[tauri::command]
pub async fn mcp_config_path(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.paths.mcp_file().to_string_lossy().to_string())
}

#[tauri::command]
pub async fn mcp_servers(state: State<'_, AppState>) -> Result<Vec<McpServer>, String> {
    Ok(state.mcp.read().await.configured_servers())
}

#[tauri::command]
pub async fn mcp_save_servers(
    state: State<'_, AppState>,
    servers: Vec<McpServer>,
) -> Result<McpStatus, String> {
    state.mcp.write().await.replace_servers(servers).await?;
    Ok(mcp_snapshot(&state).await)
}

/// Re-read `mcp.json` in case the user edited it by hand.
#[tauri::command]
pub async fn mcp_reload(state: State<'_, AppState>) -> Result<McpStatus, String> {
    state.mcp.write().await.reload().await;
    Ok(mcp_snapshot(&state).await)
}

/// What the running servers have actually been doing, oldest first. A snapshot
/// only: new lines arrive on the `mcp-log` event while the panel is open.
#[tauri::command]
pub async fn mcp_logs(state: State<'_, AppState>) -> Result<Vec<ToolLog>, String> {
    Ok(state.mcp.read().await.tool_logs())
}

/// Restart one server. A failure is reported through the status, not as an
/// error, so the panel can show *which* server is unhappy.
#[tauri::command]
pub async fn mcp_restart(
    state: State<'_, AppState>,
    name: String,
) -> Result<McpStatus, String> {
    let _ = state.mcp.write().await.restart_one(&name).await;
    Ok(mcp_snapshot(&state).await)
}

#[cfg(test)]
mod tests {
    use super::{attached_text, chat_markdown, keep_other_store, title_from};
    use crate::store::db;
    use std::path::Path;

    fn chat() -> db::ChatRow {
        db::ChatRow {
            id: "c1".to_string(),
            title: "Roadmap".to_string(),
            kind: "chat".to_string(),
            workspace: None,
            folder: None,
            model: "deepseek-flash".to_string(),
            thinking: true,
            effort: "high".to_string(),
            permission_mode: "manual".to_string(),
            cost_usd: 0.0,
            agent_mode: "standard".to_string(),
            archived: false,
            incognito: false,
            egress: "all".to_string(),
            project_id: None,
            created_at: 0,
            updated_at: 0,
        }
    }

    fn message(role: &str, content: Option<&str>, attachments: Option<&str>) -> db::MessageRow {
        db::MessageRow {
            id: "m".to_string(),
            chat_id: "c1".to_string(),
            seq: 0,
            role: role.to_string(),
            content: content.map(str::to_string),
            reasoning_content: None,
            tool_calls_json: None,
            tool_call_id: None,
            attachments_json: attachments.map(str::to_string),
            created_at: 0,
            discarded: false,
        }
    }

    /// The two stores share `chat_skills`, and each surface knows only its own
    /// list — so saving one must not switch the other off.
    #[test]
    fn saving_one_list_never_switches_the_other_store_off() {
        let current = vec!["concise".to_string(), "design-house-style".to_string()];

        // The Skills tab saves its list: a skill it dropped goes, and the design
        // system it never showed stays.
        let after_skills = keep_other_store(&current, &["terse".to_string()], |id| {
            !crate::design::is_design(id)
        });
        assert_eq!(
            after_skills,
            vec!["terse".to_string(), "design-house-style".to_string()]
        );

        // And the other way round.
        let after_designs =
            keep_other_store(&current, &["design-docs".to_string()], crate::design::is_design);
        assert_eq!(
            after_designs,
            vec!["design-docs".to_string(), "concise".to_string()]
        );

        // A design id sent to the skills list is not that list's to switch on,
        // so it is dropped rather than honoured.
        let smuggled =
            keep_other_store(&[], &["design-x".to_string()], |id| !crate::design::is_design(id));
        assert!(smuggled.is_empty(), "{smuggled:?}");
    }

    /// An image is shown to the model, not described to it.
    ///
    /// The file still reaches the chat's folder — that half is the frontend
    /// rule in `filing.ts` — but nothing about it belongs in the text the model
    /// receives, and a sentence telling it to `read_file` a picture it has just
    /// been handed would be a lie as well as noise.
    #[test]
    fn an_image_is_never_described_as_a_file_to_open() {
        assert_eq!(
            attached_text(Path::new("shot.png"), "image/png", 10),
            "",
            "an image is filed and shown, never named for reading"
        );
        let told = attached_text(Path::new("mystery.bin"), "application/octet-stream", 10);
        assert!(
            told.contains("is in this chat's folder"),
            "an opaque file is still named for the model: {told}"
        );
    }

    #[test]
    fn an_export_keeps_the_speakers_and_drops_the_machinery() {
        let messages = vec![
            message(
                "user",
                Some("Plan the launch"),
                Some(r#"[{"name":"brief.pdf","mime":"application/pdf","data_url":"data:,"}]"#),
            ),
            message("user", Some("   "), None), // blank: nothing to say
            message("assistant", None, None),   // tool-only turn: nothing to say
            message("tool", Some("noisy output"), None), // machinery, not the chat
            message("assistant", Some("Here is the plan."), None),
        ];
        let md = chat_markdown(&chat(), &messages);

        assert!(md.starts_with("# Roadmap"));
        assert!(md.contains("## You\n\nPlan the launch"));
        assert!(md.contains("_(attached: brief.pdf)_"));
        assert!(md.contains("## DeskSeek\n\nHere is the plan."));
        assert!(
            !md.contains("noisy output"),
            "tool output is machinery, not the conversation"
        );
        // The blank user row was dropped, so exactly one "## You" survives.
        assert_eq!(md.matches("## You").count(), 1);
    }

    #[test]
    fn title_comes_from_the_first_line() {
        assert_eq!(title_from("Hello there\nsecond"), "Hello there");
    }

    #[test]
    fn long_titles_are_truncated_with_an_ellipsis() {
        let t = title_from(&"x".repeat(100));
        assert!(t.ends_with('…'));
        assert_eq!(t.chars().count(), 49);
    }

    #[test]
    fn empty_input_keeps_the_default_title() {
        assert_eq!(title_from("   "), "New chat");
    }
}
