//! Tauri commands: the whole surface the frontend talks to.

use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::api::models::{self, ModelInfo};
use crate::api::types::Attachment;
use crate::approvals::Decision;
use crate::conversation::{agent, summarize};
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
    /// WSL2 can run the same jobs where Docker is not installed.
    pub wsl: bool,
    /// Where `run_command` will actually run, and why — the resolved rung, not
    /// the raw capability, because that is the thing the user can change.
    pub sandbox_rung: String,
    pub sandbox_note: Option<String>,
    pub searxng_url: String,
    pub searxng_ok: bool,
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
    // a run never has to ask Docker anything before it can start.
    let (caps, docker_cli) = tokio::join!(
        sandbox_exec::detect(),
        exec::probe("docker", &["--version"], 6),
    );
    *state.sandbox.write().await = caps.clone();

    // Pressing Re-check here is exactly how a stopped engine becomes a running
    // one, so the resolved rung is recomputed from the fresh answer.
    let plan = sandbox_exec::plan_for(
        sandbox_exec::Mode::parse(&settings.sandbox),
        &caps,
        &settings.sandbox_image,
    );

    let searxng_ok = browse::searxng_ready(&state.http, &settings.searxng_url).await;

    Ok(EnvProbe {
        has_api_key: keychain::has_api_key(),
        docker_cli,
        docker_running: caps.docker_running,
        wsl: caps.wsl_distro.is_some(),
        sandbox_rung: plan.rung.as_str().to_string(),
        sandbox_note: plan.note,
        searxng_url: settings.searxng_url,
        searxng_ok,
    })
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
