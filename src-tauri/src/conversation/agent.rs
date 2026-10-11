//! The agent loop. Streams a turn, runs whatever tools the model asks for —
//! asking the user first whenever a tool writes or executes — and repeats until
//! the model answers without calling a tool.

use std::path::{Path, PathBuf};

use serde_json::Value;
use tauri::{AppHandle, Emitter};

use crate::api::client::{Client, StreamEvent};
use crate::api::models;
use crate::api::types::{ChatRequest, StreamOptions, Thinking, Tool};
use crate::approvals::Decision;
use crate::conversation::assemble::{build_messages, system_with_memory, Turn};
use crate::store::{db, keychain, settings::Settings};
use crate::tools::{self, exec, ToolClass};
use crate::AppState;

pub const EVENT_DELTA: &str = "chat-delta";
pub const EVENT_TOOL: &str = "chat-tool";
pub const EVENT_APPROVAL: &str = "chat-approval";
pub const EVENT_DONE: &str = "chat-done";
pub const EVENT_ERROR: &str = "chat-error";

/// How many tool round-trips before we stop and hand back to the user. A cap is
/// essential: without it a confused model can loop forever on its own budget.
pub const MAX_TOOL_ROUNDS: usize = 12;

const CODING_RULES: &str = "\
You are in coding mode, working inside the folder above and nowhere else.

- Read before you write: use glob/grep/list_dir/read_file to understand the code first.
- Prefer edit_file over write_file for existing files. Keep changes small and focused.
- Every file write and every command is shown to the user for approval before it runs.
  Say in one line what you are about to do, then call the tool.
- Anything outside the workspace is refused by a sandbox; do not try.
- After changing code, run the project's tests or build where you can, and report the result.
- Report honestly. If a command fails, show the output instead of guessing.";

fn to_turn(m: &db::MessageRow) -> Turn {
    Turn {
        role: m.role.clone(),
        content: m.content.clone(),
        reasoning_content: m.reasoning_content.clone(),
        tool_calls: m
            .tool_calls_json
            .as_ref()
            .and_then(|j| serde_json::from_str(j).ok()),
        tool_call_id: m.tool_call_id.clone(),
        attachments: m
            .attachments_json
            .as_ref()
            .and_then(|j| serde_json::from_str(j).ok()),
    }
}

/// `AGENTS.md` if the repo has one, else DeskSeek's own file.
fn read_project_context(ws: &Path) -> Option<(String, String)> {
    for (label, path) in [
        ("AGENTS.md", ws.join("AGENTS.md")),
        (".deskseek/context.md", ws.join(".deskseek").join("context.md")),
    ] {
        if let Ok(body) = std::fs::read_to_string(&path) {
            if !body.trim().is_empty() {
                return Some((label.to_string(), body));
            }
        }
    }
    None
}

const DOCUMENT_RULES: &str = "\
You can read and write files in the folder above and nowhere else.

- Use list_dir and read_file to see what is already there before you write.
- When the user asks for a document, save it to a file with write_file rather than
  only printing it in the chat, and tell them the file name you used.
- Prefer edit_file over write_file when updating an existing document.
- Every write is shown to the user for approval before it happens; say in one line
  what you are about to do, then call the tool.";

fn build_system(
    settings: &Settings,
    state: &AppState,
    chat: &db::ChatRow,
    workspace: Option<&Path>,
) -> String {
    let coding = chat.kind == "coding";
    let mut base = settings.system_prompt.clone();

    if let Some(ws) = workspace {
        if coding {
            base.push_str(&format!("\n\n# Workspace\n\n{}\n\n{CODING_RULES}", ws.display()));
            if let Some((label, body)) = read_project_context(ws) {
                base.push_str(&format!("\n\n# Project context ({label})\n\n{body}"));
            }
        } else {
            base.push_str(&format!(
                "\n\n# Documents\n\nYour documents folder is:\n{}\n\n{DOCUMENT_RULES}",
                ws.display()
            ));
        }
    }

    // Instructions the user scoped to this section only.
    let scope = if coding { "coding" } else { "chat" };
    for instruction in settings
        .instructions
        .iter()
        .filter(|i| i.enabled && (i.scope == "both" || i.scope == scope))
    {
        base.push_str(&format!("\n\n# {}\n\n{}", instruction.name, instruction.body));
    }

    let enabled = {
        let conn = state.db.lock().unwrap();
        db::get_chat_skills(&conn, &chat.id).unwrap_or_default()
    };
    if !enabled.is_empty() {
        let available = crate::skills::discover(&state.paths.skills_dir());
        for skill in available.iter().filter(|s| enabled.contains(&s.id)) {
            base.push_str(&format!("\n\n# Skill: {}\n\n{}", skill.name, skill.body));
        }
    }

    let memory = crate::memory::read(&state.paths.memory_file());
    system_with_memory(&base, memory.as_deref())
}

/// Asks the user whether a tool may run, unless it is already covered by a
/// read-only class, a session grant, or a stored permission.
async fn approve(
    app: &AppHandle,
    state: &AppState,
    chat_id: &str,
    workspace: &Path,
    tool: &str,
    class: ToolClass,
    args: &Value,
) -> bool {
    let ws_key = workspace.to_string_lossy().to_string();

    if state.approvals.has_session_grant(&ws_key, tool) {
        return true;
    }
    {
        let conn = state.db.lock().unwrap();
        if db::is_tool_allowed(&conn, &ws_key, tool).unwrap_or(false) {
            return true;
        }
    }

    let request_id = uuid::Uuid::new_v4().to_string();
    let rx = state.approvals.register(&request_id);
    let _ = app.emit(
        EVENT_APPROVAL,
        serde_json::json!({
            "requestId": request_id,
            "chatId": chat_id,
            "tool": tool,
            "class": class,
            "args": args,
            "workspace": ws_key,
        }),
    );

    // A dropped sender means the app went away — deny rather than hang.
    let decision = rx.await.unwrap_or(Decision::Deny);
    match decision {
        Decision::Session => {
            state.approvals.grant_session(&ws_key, tool);
            true
        }
        Decision::Always => {
            let conn = state.db.lock().unwrap();
            let _ = db::allow_tool(&conn, &ws_key, tool);
            true
        }
        other => other.allows(),
    }
}

pub async fn run(app: AppHandle, state: &AppState, chat_id: &str) -> Result<(), String> {
    let api_key = keychain::get_api_key()?.ok_or("No API key set")?;
    let settings = state.settings.lock().unwrap().clone();
    let chat = {
        let conn = state.db.lock().unwrap();
        db::get_chat(&conn, chat_id).map_err(|e| e.to_string())?
    };

    let workspace: Option<PathBuf> = if chat.kind == "coding" {
        chat.workspace.as_deref().map(PathBuf::from)
    } else {
        // Chat mode works in a documents folder so it can create and read files.
        let configured = settings.documents_dir.trim();
        let dir = if configured.is_empty() {
            state.paths.documents_dir()
        } else {
            PathBuf::from(configured)
        };
        if let Err(err) = std::fs::create_dir_all(&dir) {
            eprintln!("could not create the documents folder: {err}");
        }
        Some(dir)
    };
    let tool_list: Vec<Tool> = tools::tools_for(&chat.kind)
        .into_iter()
        .map(|(tool, _)| tool)
        .collect();
    let tools_present = !tool_list.is_empty();

    let client = Client::new(state.http.clone(), api_key, settings.base_url.clone());
    let mut cost_total = 0.0f64;
    let mut last_usage = None;
    let mut last_message_id = String::new();
    let mut last_finish: Option<String> = None;

    for _round in 0..MAX_TOOL_ROUNDS {
        let history = {
            let conn = state.db.lock().unwrap();
            db::list_messages(&conn, chat_id).map_err(|e| e.to_string())?
        };
        let turns: Vec<Turn> = history.iter().map(to_turn).collect();
        let system = build_system(&settings, state, &chat, workspace.as_deref());
        // The user's turn is already in the stored history.
        let messages = build_messages(&system, &turns, None, tools_present);

        let request = ChatRequest {
            model: chat.model.clone(),
            messages,
            stream: true,
            stream_options: Some(StreamOptions {
                include_usage: true,
            }),
            thinking: Some(Thinking::enabled(chat.thinking)),
            reasoning_effort: if chat.thinking {
                Some(chat.effort.clone())
            } else {
                None
            },
            max_tokens: None,
            tools: if tools_present {
                Some(tool_list.clone())
            } else {
                None
            },
        };

        let emit_app = app.clone();
        let emit_chat = chat_id.to_string();
        let outcome = match client
            .stream_chat(&request, |event| {
                let (kind, text) = match event {
                    StreamEvent::Content(t) => ("content", t),
                    StreamEvent::Reasoning(t) => ("reasoning", t),
                };
                let _ = emit_app.emit(
                    EVENT_DELTA,
                    serde_json::json!({ "chatId": emit_chat, "kind": kind, "text": text }),
                );
            })
            .await
        {
            Ok(outcome) => outcome,
            Err(e) => {
                let message = e.to_string();
                let _ = app.emit(
                    EVENT_ERROR,
                    serde_json::json!({ "chatId": chat_id, "message": message }),
                );
                return Err(message);
            }
        };

        let peak = models::is_peak(chrono::Utc::now());
        if let Some(cost) = models::cost_usd(&chat.model, &outcome.usage, peak) {
            cost_total += cost;
        }
        last_usage = Some(outcome.usage.clone());
        last_finish = outcome.finish_reason.clone();

        let tool_calls_json = outcome
            .message
            .tool_calls
            .as_ref()
            .and_then(|c| serde_json::to_string(c).ok());

        {
            let conn = state.db.lock().unwrap();
            let row = db::insert_message(
                &conn,
                chat_id,
                "assistant",
                outcome.message.content.as_ref().and_then(|c| c.as_text()),
                outcome.message.reasoning_content.as_deref(),
                tool_calls_json.as_deref(),
                None,
                None,
            )
            .map_err(|e| e.to_string())?;
            db::touch_chat(&conn, chat_id).map_err(|e| e.to_string())?;
            last_message_id = row.id;
        }

        let calls = outcome.message.tool_calls.unwrap_or_default();
        if calls.is_empty() {
            break;
        }

        let Some(ws) = workspace.clone() else {
            break;
        };

        for call in calls {
            let name = call.function.name.clone();
            let args: Value = serde_json::from_str(&call.function.arguments).unwrap_or(Value::Null);
            let class = tools::class_of(&name);

            let _ = app.emit(
                EVENT_TOOL,
                serde_json::json!({
                    "chatId": chat_id, "phase": "requested",
                    "id": call.id, "tool": name, "class": class, "args": args
                }),
            );

            let allowed = class == ToolClass::Read
                || approve(&app, state, chat_id, &ws, &name, class, &args).await;

            let result = if allowed {
                exec::execute(&name, &args, &ws).await
            } else {
                Err("The user denied this action. Do not retry it — ask them what they would prefer.".to_string())
            };

            let (ok, text) = match &result {
                Ok(text) => (true, text.clone()),
                Err(err) => (false, err.clone()),
            };
            let _ = app.emit(
                EVENT_TOOL,
                serde_json::json!({
                    "chatId": chat_id, "phase": "finished",
                    "id": call.id, "tool": name, "ok": ok, "output": text
                }),
            );

            let content = match result {
                Ok(text) => text,
                Err(err) => format!("Error: {err}"),
            };
            {
                let conn = state.db.lock().unwrap();
                db::insert_message(&conn, chat_id, "tool", Some(&content), None, None, Some(&call.id), None)
                    .map_err(|e| e.to_string())?;
            }
        }
    }

    let _ = app.emit(
        EVENT_DONE,
        serde_json::json!({
            "chatId": chat_id,
            "messageId": last_message_id,
            "usage": last_usage,
            "costUsd": if cost_total > 0.0 { Some(cost_total) } else { None },
            "finishReason": last_finish,
        }),
    );

    Ok(())
}
