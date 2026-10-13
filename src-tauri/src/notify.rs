//! Pinging the user: one place that decides whether an OS toast is wanted.
//!
//! A toast is for someone who is *not* looking at the app. When the window has
//! focus the answer is already on screen, and a popup over it is noise — so the
//! focus check lives here rather than in each caller, and the setting that turns
//! it off is one switch instead of a rule every call site has to remember.
//!
//! Quiet hours are the third reason to hold a popup back, and living here has the
//! same point: three settings now decide whether a toast is wanted, and a caller
//! that combined them itself would get the combination wrong somewhere.
//!
//! The in-app event is emitted either way: it costs nothing, it cannot be
//! suppressed by the OS, and a badge or a notice can react to it without asking
//! whether a popup was allowed. Quiet hours silence the toast, not the news.

use chrono::{Local, NaiveTime, Timelike};
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::store::settings::Settings;
use crate::AppState;

/// A notice for the UI. `chatId` is null for an app-level ping.
pub const EVENT_NOTIFY: &str = "chat-notify";

/// Whether the main window is in front of the user.
fn is_focused(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .and_then(|w| w.is_focused().ok())
        .unwrap_or(false)
}

/// "HH:MM" as minutes past local midnight, or `None` when it is not that.
///
/// `NaiveTime` does the parsing, exactly as the scheduler reads its own times, so
/// "9:00" and "09:00" are one spelling rather than two rules.
fn minute_of_day(value: &str) -> Option<u32> {
    let time = NaiveTime::parse_from_str(value.trim(), "%H:%M").ok()?;
    Some(time.hour() * 60 + time.minute())
}

/// Whether `now` (minutes past local midnight) is inside the quiet window.
///
/// The window may wrap midnight — 18:00 to 09:00 is the one people actually want
/// — which is why this is a function with a test rather than an inline
/// comparison. A window whose ends are equal is read as *no* window: "18:00 to
/// 18:00" would otherwise mean a silent day.
fn in_quiet_hours(from: &str, to: &str, now: u32) -> bool {
    let (Some(from), Some(to)) = (minute_of_day(from), minute_of_day(to)) else {
        return false;
    };
    if from == to {
        return false;
    }
    if from < to {
        (from..to).contains(&now)
    } else {
        now >= from || now < to
    }
}

/// Whether a popup is wanted, from the switches, the focus and the clock.
///
/// Pure, so the interaction between the three settings is a test rather than a
/// reading of control flow. None of them suppresses the in-app notice: `ping`
/// emits that before it asks.
pub fn wants_toast(settings: &Settings, focused: bool, now: u32) -> bool {
    if !settings.notifications {
        return false;
    }
    if settings.quiet_hours && in_quiet_hours(&settings.quiet_from, &settings.quiet_to, now) {
        return false;
    }
    if settings.notify_only_when_unfocused && focused {
        return false;
    }
    true
}

/// Tells the user something happened. Always emits [`EVENT_NOTIFY`]; raises an
/// OS toast only when the settings say one is wanted right now.
pub fn ping(app: &AppHandle, state: &AppState, chat_id: Option<&str>, title: &str, body: &str) {
    // Unconditional, and first: the notice is the part that must survive every
    // switch, so nothing below can skip it.
    let _ = app.emit(
        EVENT_NOTIFY,
        json!({ "chatId": chat_id, "title": title, "body": body }),
    );

    let settings = state.settings.lock().unwrap().clone();
    let now = Local::now();
    if !wants_toast(&settings, is_focused(app), now.hour() * 60 + now.minute()) {
        return;
    }
    if let Err(err) = app.notification().builder().title(title).body(body).show() {
        // Focus assist, a policy, or a machine with no notification centre must
        // not break the run that wanted to say something.
        eprintln!("could not show a notification: {err}");
    }
}

/// A nudge to get up, from the reminder ticker.
///
/// It goes through `ping` rather than raising a toast itself, so quiet hours and
/// the notification switch apply to it exactly as they do to a finished run.
pub fn reminder(app: &AppHandle, state: &AppState) {
    let minutes = state.settings.lock().unwrap().reminder_minutes;
    ping(
        app,
        state,
        None,
        "Time for a break",
        &format!("You have been at it for {minutes} minutes."),
    );
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

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hour: u32, minute: u32) -> u32 {
        hour * 60 + minute
    }

    #[test]
    fn a_quiet_window_holds_the_popup_back() {
        // The overnight window is the one people want, so the wrap is the case
        // that matters: 18:00 to 09:00 must cover both sides of midnight.
        assert!(in_quiet_hours("18:00", "09:00", at(18, 0)));
        assert!(in_quiet_hours("18:00", "09:00", at(23, 59)));
        assert!(in_quiet_hours("18:00", "09:00", at(0, 0)));
        assert!(in_quiet_hours("18:00", "09:00", at(8, 59)));
        assert!(!in_quiet_hours("18:00", "09:00", at(9, 0)), "the end is exclusive");
        assert!(!in_quiet_hours("18:00", "09:00", at(17, 59)));

        // A daytime window that does not wrap.
        assert!(in_quiet_hours("09:00", "17:00", at(12, 0)));
        assert!(!in_quiet_hours("09:00", "17:00", at(8, 59)));
        assert!(!in_quiet_hours("09:00", "17:00", at(17, 0)));

        // "9:00" is the same time as "09:00" — one parser, one spelling.
        assert!(in_quiet_hours("9:00", "17:00", at(10, 0)));

        // Degenerate and unparseable windows are read as no window at all, rather
        // than as a silent day or a panic.
        assert!(!in_quiet_hours("18:00", "18:00", at(18, 0)));
        assert!(!in_quiet_hours("", "", at(12, 0)));
        assert!(!in_quiet_hours("evening", "morning", at(12, 0)));
        assert!(!in_quiet_hours("25:00", "09:00", at(12, 0)));
    }

    #[test]
    fn the_three_switches_decide_whether_a_popup_is_wanted() {
        let mut s = Settings::default();
        // Defaults: notifications on, only when unfocused, no quiet hours.
        assert!(wants_toast(&s, false, at(12, 0)));
        assert!(!wants_toast(&s, true, at(12, 0)), "the window is in front of the user");

        // The master switch wins over everything.
        s.notifications = false;
        assert!(!wants_toast(&s, false, at(12, 0)));
        s.notifications = true;

        // Quiet hours then hold it back wherever the focus is, and stop holding
        // it back the moment the hour is outside the window.
        s.quiet_hours = true;
        s.quiet_from = "18:00".to_string();
        s.quiet_to = "09:00".to_string();
        assert!(!wants_toast(&s, false, at(22, 0)));
        assert!(wants_toast(&s, false, at(12, 0)));

        // Turning the window off is enough to get the popup back, without
        // touching the times that are stored.
        s.quiet_hours = false;
        assert!(wants_toast(&s, false, at(22, 0)));
    }
}
