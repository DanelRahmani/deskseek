//! Tauri commands: the whole surface the frontend talks to.

use std::path::PathBuf;

use base64::Engine as _;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::api::models::{self, ModelInfo};
use crate::api::types::Attachment;
use crate::approvals::Decision;
use crate::conversation::{agent, assemble, summarize};
use crate::mcp::{McpServer, ServerStatus, ToolLog};
use crate::memory;
use crate::skills;
use crate::store::{db, keychain, settings::Settings};
use crate::tools::{browse, desktop, exec, guard::PermissionMode, sandbox, sandbox_exec, walk};
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
pub fn save_settings(state: State<'_, AppState>, settings: Settings) -> Result<Settings, String> {
    crate::store::settings::save(&state.paths.settings_file(), &settings).map_err(|e| e.to_string())?;
    *state.settings.lock().unwrap() = settings.clone();
    Ok(settings)
}

// ---------------------------------------------------------------- backup

/// The API key is deliberately absent: it lives in the OS keychain, which is not
/// a file this app can hand out, and a backup is a file that gets mailed around.
/// Everything else the app would miss if the disk died is here.
const BACKUP_VERSION: u32 = 1;

/// Write settings and the MCP server list to one JSON file in the documents
/// folder. The MCP file is copied as opaque JSON rather than re-serialised: this
/// does not need to understand it, only to move it.
#[tauri::command]
pub fn backup_config(state: State<'_, AppState>) -> Result<String, String> {
    let dir = documents_dir_path(&state);
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not use {}: {e}", dir.display()))?;
    let settings = state.settings.lock().unwrap().clone();
    let mcp = std::fs::read_to_string(state.paths.mcp_file())
        .ok()
        .and_then(|body| serde_json::from_str::<serde_json::Value>(&body).ok())
        .unwrap_or(serde_json::Value::Null);
    let bundle = serde_json::json!({
        "version": BACKUP_VERSION,
        "settings": settings,
        "mcp": mcp,
    });
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let path = crate::tools::docs::free_path(&dir, &format!("deskseek-backup-{stamp}"), "json");
    let body = serde_json::to_string_pretty(&bundle).map_err(|e| e.to_string())?;
    std::fs::write(&path, body).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

/// Put a backup back. Deliberately all-or-nothing: the settings are parsed before
/// anything is written, so a file that is not a backup leaves the app untouched
/// rather than half-restored.
#[tauri::command]
pub async fn restore_config(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let body = std::fs::read_to_string(&path).map_err(|e| format!("could not read {path}: {e}"))?;
    let bundle: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| format!("{path} is not a DeskSeek backup: {e}"))?;
    let settings: Settings = serde_json::from_value(
        bundle
            .get("settings")
            .cloned()
            .ok_or("this backup has no settings in it")?,
    )
    .map_err(|e| format!("the settings in {path} could not be read: {e}"))?;

    crate::store::settings::save(&state.paths.settings_file(), &settings)
        .map_err(|e| e.to_string())?;
    *state.settings.lock().unwrap() = settings;

    if let Some(mcp) = bundle.get("mcp").filter(|value| !value.is_null()) {
        let pretty = serde_json::to_string_pretty(mcp).map_err(|e| e.to_string())?;
        std::fs::write(state.paths.mcp_file(), pretty).map_err(|e| e.to_string())?;
    }
    state.mcp.write().await.reload().await;
    Ok(())
}

// ------------------------------------------------------------------- key

#[tauri::command]
pub fn has_api_key() -> bool {
    keychain::has_api_key()
}

#[tauri::command]
pub fn masked_api_key() -> Result<Option<String>, String> {
    Ok(keychain::get_api_key()?.map(|k| keychain::mask(&k)))
}

#[tauri::command]
pub fn set_api_key(key: String) -> Result<(), String> {
    if key.trim().is_empty() {
        return Err("API key is empty".to_string());
    }
    keychain::set_api_key(key.trim())
}

#[tauri::command]
pub fn clear_api_key() -> Result<(), String> {
    keychain::delete_api_key()
}

#[tauri::command]
pub async fn test_connection(state: State<'_, AppState>) -> Result<(), String> {
    let key = keychain::get_api_key()?.ok_or("No API key set")?;
    let base_url = state.settings.lock().unwrap().base_url.clone();
    let client = crate::api::client::Client::new(state.http.clone(), key, base_url);
    client.check_auth().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_models() -> Vec<ModelInfo> {
    models::MODELS.to_vec()
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
    let model = if kind == "coding" {
        settings.coding_model.clone()
    } else {
        settings.chat_model.clone()
    };
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
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_messages(state: State<'_, AppState>, chat_id: String) -> Result<Vec<db::MessageRow>, String> {
    let conn = state.db.lock().unwrap();
    db::list_messages(&conn, &chat_id).map_err(|e| e.to_string())
}

/// Search every conversation by what was said in it, not just by its title.
#[tauri::command]
pub fn search_messages(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<db::MessageHit>, String> {
    const HITS: i64 = 40;
    let conn = state.db.lock().unwrap();
    db::search_messages(&conn, &query, HITS).map_err(|e| e.to_string())
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
    keychain::get_api_key()?.ok_or("No API key set")?;
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
        db::delete_messages_after(&conn, &chat_id, turn.seq).map_err(|e| e.to_string())?;
    }

    // This replaces a turn, so it registers a run exactly as send_message does:
    // one live run per chat, and it cancels any turn already in flight.
    let (run_id, cancel) = state.runs.begin(&chat_id);
    let result = agent::run(app, &state, &chat_id, cancel).await;
    state.runs.finish(&chat_id, run_id);
    result
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
        db::delete_messages(&conn, &chat_id).map_err(|e| e.to_string())?;
        let body = format!(
            "Context compacted. Summary of the conversation so far:\n\n{summary}\n\nContinue from here."
        );
        db::insert_message(&conn, &chat_id, "user", Some(&body), None, None, None, None)
            .map_err(|e| e.to_string())?;
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

/// Write a chat to a Markdown file in the documents folder, so it can be read,
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
    let dir = documents_dir_path(&state);
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not use {}: {e}", dir.display()))?;
    let path = crate::tools::docs::free_path(&dir, &crate::tools::docs::slug(&chat.title), "md");
    std::fs::write(&path, chat_markdown(&chat, &messages)).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

/// Write a chat to a PDF in the documents folder.
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
    let dir = documents_dir_path(&state);
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not use {}: {e}", dir.display()))?;
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
            id: s.id,
            name: s.name,
            description: s.description,
        })
        .collect()
}

#[tauri::command]
pub fn set_chat_skills(
    state: State<'_, AppState>,
    chat_id: String,
    skill_ids: Vec<String>,
) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::set_chat_skills(&conn, &chat_id, &skill_ids).map_err(|e| e.to_string())
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

// ------------------------------------------------------------ permissions

#[tauri::command]
pub fn respond_approval(
    state: State<'_, AppState>,
    request_id: String,
    decision: String,
) -> Result<bool, String> {
    Ok(state.approvals.respond(&request_id, Decision::parse(&decision)))
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
) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::revoke_tool(&conn, &workspace, &tool).map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolGrant {
    pub workspace: String,
    pub tool: String,
}

/// Every stored "always allow" grant: which tool, in which folder. The other
/// decisions last only as long as the app is running; these are the ones that
/// survive a restart, and until now nothing showed them.
#[tauri::command]
pub fn tool_grants(state: State<'_, AppState>) -> Result<Vec<ToolGrant>, String> {
    let conn = state.db.lock().unwrap();
    let rows = db::all_allowed_tools(&conn).map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|(workspace, tool)| ToolGrant { workspace, tool })
        .collect())
}

// --------------------------------------------------------------- messaging

#[tauri::command]
pub async fn send_message(
    app: AppHandle,
    state: State<'_, AppState>,
    chat_id: String,
    text: String,
    attachments: Option<Vec<Attachment>>,
) -> Result<(), String> {
    keychain::get_api_key()?.ok_or("No API key set")?;

    let chat = {
        let conn = state.db.lock().unwrap();
        db::get_chat(&conn, &chat_id).map_err(|e| e.to_string())?
    };
    if chat.kind == "coding" && chat.workspace.is_none() {
        return Err("Pick a folder for this coding task first.".to_string());
    }

    let attachments = attachments.unwrap_or_default();
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
            &chat_id,
            "user",
            Some(&text),
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
                title_from(&text)
            };
            db::rename_chat(&conn, &chat_id, &title).map_err(|e| e.to_string())?;
        }
    }

    // One live run per chat: registering cancels any turn already in flight.
    let (run_id, cancel) = state.runs.begin(&chat_id);
    let result = agent::run(app, &state, &chat_id, cancel).await;
    state.runs.finish(&chat_id, run_id);
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
        has_api_key: keychain::has_api_key(),
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

/// Read a dropped file into an attachment.
///
/// A native drop hands the webview a *path*, not a `File`, and the webview
/// cannot read the disk itself — so the file is read here. What comes back
/// depends on what the model can actually use: an image is returned as a
/// `data:` URL (the frontend downscales it), a text file as its own text, and
/// anything else as a name and a MIME type with no payload at all. The model is
/// text-only, so the bytes of a PDF would be a row that is stored and never
/// sent.
#[tauri::command]
pub fn attach_path(path: String) -> Result<Attachment, String> {
    const MAX_IMAGE_BYTES: u64 = 25 * 1024 * 1024;
    const MAX_TEXT_BYTES: u64 = 200 * 1024;
    let file = std::path::Path::new(&path);
    if !file.is_file() {
        return Err(format!("{path} is not a file."));
    }
    let name = file
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .to_string();
    let size = std::fs::metadata(file).map_err(|e| e.to_string())?.len();
    let mime = mime_for(&name);

    if mime.starts_with("image/") {
        if size > MAX_IMAGE_BYTES {
            return Err(format!("{name} is larger than 25 MB."));
        }
        let bytes = std::fs::read(file).map_err(|e| e.to_string())?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
        return Ok(Attachment {
            data_url: format!("data:{mime};base64,{encoded}"),
            text: String::new(),
            name,
            mime,
        });
    }

    let text = if is_text_mime(&mime) && size <= MAX_TEXT_BYTES {
        let bytes = std::fs::read(file).map_err(|e| e.to_string())?;
        String::from_utf8_lossy(&bytes).into_owned()
    } else {
        // Nothing the text-only model can read: keep the label, drop the bytes.
        String::new()
    };
    Ok(Attachment {
        name,
        mime,
        data_url: String::new(),
        text,
    })
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
    use super::{chat_markdown, title_from};
    use crate::store::db;

    fn chat() -> db::ChatRow {
        db::ChatRow {
            id: "c1".to_string(),
            title: "Roadmap".to_string(),
            kind: "chat".to_string(),
            workspace: None,
            model: "deepseek-flash".to_string(),
            thinking: true,
            effort: "high".to_string(),
            permission_mode: "manual".to_string(),
            cost_usd: 0.0,
            agent_mode: "standard".to_string(),
            archived: false,
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
        }
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
