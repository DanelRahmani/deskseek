//! Pinging the user: one place that decides whether an OS toast is wanted.
//!
//! A toast is for someone who is *not* looking at the app. When the window has
//! focus the answer is already on screen, and a popup over it is noise — so the
//! focus check lives here rather than in each caller, and the setting that turns
//! it off is one switch instead of a rule every call site has to remember.
//!
//! The in-app event is emitted either way: it costs nothing, it cannot be
//! suppressed by the OS, and a badge or a notice can react to it without asking
//! whether a popup was allowed.

use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::AppState;

/// A notice for the UI. `chatId` is null for an app-level ping.
pub const EVENT_NOTIFY: &str = "chat-notify";

/// Whether the main window is in front of the user.
fn is_focused(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .and_then(|w| w.is_focused().ok())
        .unwrap_or(false)
}

/// Tells the user something happened. Always emits [`EVENT_NOTIFY`]; raises an
/// OS toast only when notifications are switched on and the window is not in
/// front of them.
pub fn ping(app: &AppHandle, state: &AppState, chat_id: Option<&str>, title: &str, body: &str) {
    let _ = app.emit(
        EVENT_NOTIFY,
        json!({ "chatId": chat_id, "title": title, "body": body }),
    );

    let settings = state.settings.lock().unwrap().clone();
    if !settings.notifications {
        return;
    }
    if settings.notify_only_when_unfocused && is_focused(app) {
        return;
    }
    if let Err(err) = app.notification().builder().title(title).body(body).show() {
        // Focus assist, a policy, or a machine with no notification centre must
        // not break the run that wanted to say something.
        eprintln!("could not show a notification: {err}");
    }
}

/// A turn finished and the user was not necessarily watching. Does nothing when
/// the window has focus — `ping` makes that call.
pub fn turn_finished(app: &AppHandle, state: &AppState, chat_id: &str, failed: bool) {
    let title = {
        let conn = state.db.lock().unwrap();
        crate::store::db::get_chat(&conn, chat_id)
            .map(|chat| chat.title)
            .unwrap_or_else(|_| "DeskSeek".to_string())
    };
    if failed {
        ping(app, state, Some(chat_id), "A run stopped", &title);
    } else {
        ping(app, state, Some(chat_id), "Done", &title);
    }
}
