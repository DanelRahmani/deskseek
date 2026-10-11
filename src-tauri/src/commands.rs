//! Tauri commands: the whole surface the frontend talks to.

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::api::models::{self, ModelInfo};
use crate::api::types::Attachment;
use crate::approvals::Decision;
use crate::conversation::{agent, summarize};
use crate::mcp::{McpServer, ServerStatus};
use crate::memory;
use crate::skills;
use crate::store::{db, keychain, settings::Settings};
use crate::tools::guard::PermissionMode;
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

#[tauri::command]
pub fn append_memory(state: State<'_, AppState>, entries: Vec<String>) -> Result<(), String> {
    memory::append(&state.paths.memory_file(), &entries).map_err(|e| e.to_string())
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
    let rows = {
        let conn = state.db.lock().unwrap();
        db::proposals_by_ids(&conn, &ids).map_err(|e| e.to_string())?
    };
    let texts: Vec<String> = rows
        .iter()
        .filter(|r| r.status == "pending")
        .map(|r| r.text.clone())
        .collect();

    memory::append(&state.paths.memory_file(), &texts).map_err(|e| e.to_string())?;

    {
        let conn = state.db.lock().unwrap();
        db::set_proposal_status(&conn, &ids, "accepted").map_err(|e| e.to_string())?;
    }
    Ok(texts.len())
}

#[tauri::command]
pub fn reject_proposals(state: State<'_, AppState>, ids: Vec<String>) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::set_proposal_status(&conn, &ids, "rejected").map_err(|e| e.to_string())
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
    let configured = state.settings.lock().unwrap().documents_dir.clone();
    let dir = if configured.trim().is_empty() {
        state.paths.documents_dir()
    } else {
        std::path::PathBuf::from(configured)
    };
    dir.display().to_string()
}

#[tauri::command]
pub fn chatlog_dir(state: State<'_, AppState>) -> String {
    state.paths.chatlog_dir().display().to_string()
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
    use super::title_from;

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
