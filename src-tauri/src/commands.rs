//! Tauri commands: the whole surface the frontend talks to.

use tauri::{AppHandle, Emitter, State};

use crate::api::client::{Client, StreamEvent};
use crate::api::models::{self, ModelInfo};
use crate::api::types::{ChatRequest, StreamOptions, Thinking};
use crate::conversation::assemble::{build_messages, system_with_memory, Turn};
use crate::store::{db, keychain, settings::Settings};
use crate::AppState;

const EVENT_DELTA: &str = "chat-delta";
const EVENT_DONE: &str = "chat-done";
const EVENT_ERROR: &str = "chat-error";

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
    }
}

fn read_memory(state: &AppState) -> Option<String> {
    std::fs::read_to_string(state.paths.memory_file())
        .ok()
        .filter(|s| !s.trim().is_empty())
}

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
    let client = Client::new(state.http.clone(), key, base_url);
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

// --------------------------------------------------------------- messaging

#[tauri::command]
pub async fn send_message(
    app: AppHandle,
    state: State<'_, AppState>,
    chat_id: String,
    text: String,
) -> Result<(), String> {
    let api_key = keychain::get_api_key()?.ok_or("No API key set")?;
    let settings = state.settings.lock().unwrap().clone();

    let chat = {
        let conn = state.db.lock().unwrap();
        db::get_chat(&conn, &chat_id).map_err(|e| e.to_string())?
    };

    // History is read *before* the new user message is stored, so it is not
    // appended twice by `build_messages`.
    let history = {
        let conn = state.db.lock().unwrap();
        db::list_messages(&conn, &chat_id).map_err(|e| e.to_string())?
    };
    let turns: Vec<Turn> = history.iter().map(to_turn).collect();

    {
        let conn = state.db.lock().unwrap();
        db::insert_message(&conn, &chat_id, "user", Some(&text), None, None, None)
            .map_err(|e| e.to_string())?;
        if chat.title == "New chat" {
            db::rename_chat(&conn, &chat_id, &title_from(&text)).map_err(|e| e.to_string())?;
        }
    }

    let system = system_with_memory(&settings.system_prompt, read_memory(&state).as_deref());
    let messages = build_messages(&system, &turns, &text, false);

    let request = ChatRequest {
        model: chat.model.clone(),
        messages,
        stream: true,
        stream_options: Some(StreamOptions { include_usage: true }),
        thinking: Some(Thinking::enabled(chat.thinking)),
        reasoning_effort: if chat.thinking { Some(chat.effort.clone()) } else { None },
        max_tokens: None,
        tools: None,
    };

    let client = Client::new(state.http.clone(), api_key, settings.base_url.clone());

    let emit_app = app.clone();
    let emit_chat = chat_id.clone();
    let outcome = client
        .stream_chat(&request, |event| {
            let (kind, delta) = match event {
                StreamEvent::Content(t) => ("content", t),
                StreamEvent::Reasoning(t) => ("reasoning", t),
            };
            let _ = emit_app.emit(
                EVENT_DELTA,
                serde_json::json!({ "chatId": emit_chat, "kind": kind, "text": delta }),
            );
        })
        .await;

    let outcome = match outcome {
        Ok(o) => o,
        Err(e) => {
            let _ = app.emit(
                EVENT_ERROR,
                serde_json::json!({ "chatId": chat_id, "message": e.to_string() }),
            );
            return Err(e.to_string());
        }
    };

    let tool_calls_json = outcome
        .message
        .tool_calls
        .as_ref()
        .and_then(|c| serde_json::to_string(c).ok());

    let stored = {
        let conn = state.db.lock().unwrap();
        let row = db::insert_message(
            &conn,
            &chat_id,
            "assistant",
            outcome.message.content.as_deref(),
            outcome.message.reasoning_content.as_deref(),
            tool_calls_json.as_deref(),
            None,
        )
        .map_err(|e| e.to_string())?;
        db::touch_chat(&conn, &chat_id).map_err(|e| e.to_string())?;
        row
    };

    let peak = models::is_peak(chrono::Utc::now());
    let cost = models::cost_usd(&chat.model, &outcome.usage, peak);

    let _ = app.emit(
        EVENT_DONE,
        serde_json::json!({
            "chatId": chat_id,
            "messageId": stored.id,
            "usage": outcome.usage,
            "costUsd": cost,
            "finishReason": outcome.finish_reason,
        }),
    );

    Ok(())
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
