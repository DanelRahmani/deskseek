//! The quick-entry window: a second window over whatever the user is doing,
//! raised by a global chord.
//!
//! Exactly one rule lives here — turning the chord the user typed in Settings
//! into a shortcut the OS will register, and back into the one spelling the
//! plugin prints, so the handler can tell *which* shortcut fired without caring
//! how either side was typed. Everything else about the window is `lib.rs`;
//! everything else about the question is the ordinary commands the main window
//! already calls, so there is one run path rather than a second one.

use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

/// The window's label. Shared by the config that declares it, the show/hide
/// path, and the front end, which decides what to render from it.
pub const WINDOW: &str = "quick";

/// A combination that is unlikely to be taken and easy to change. Deliberately
/// not `Alt+Space`, which is the Windows system menu.
pub const DEFAULT_CHORD: &str = "Ctrl+Alt+Space";

/// The chord as a shortcut the OS can register.
pub fn parse(chord: &str) -> Result<Shortcut, String> {
    let trimmed = chord.trim();
    if trimmed.is_empty() {
        return Err(format!("A shortcut is needed — try {DEFAULT_CHORD}."));
    }
    trimmed.parse::<Shortcut>().map_err(|_| {
        format!("\"{chord}\" is not a shortcut I can register. Write it like {DEFAULT_CHORD}.")
    })
}

/// The chord in the plugin's own spelling, so the handler can compare what just
/// fired against what Settings holds without knowing who typed what.
pub fn label(chord: &str) -> String {
    parse(chord).map(|key| key.to_string()).unwrap_or_default()
}

/// Point the chord at a new combination.
///
/// The new one is registered **before** the old is dropped, so a chord another
/// application has already claimed costs the user nothing: the failure is
/// reported and the shortcut they had keeps working. Returns the canonical
/// label of what is now registered.
pub fn rebind(app: &AppHandle, old: &str, new: &str) -> Result<String, String> {
    let old_key = parse(old)?;
    let new_key = parse(new)?;
    if old_key == new_key {
        return Ok(new_key.to_string());
    }
    let shortcuts = app.global_shortcut();
    shortcuts.register(new_key).map_err(|_| {
        format!("{new} is already taken — another application owns it. The shortcut you had still works.")
    })?;
    let _ = shortcuts.unregister(old_key);
    Ok(new_key.to_string())
}

/// Show the quick window, or hide it when it is already up.
///
/// A toggle rather than a raise, because that is what every launcher does: the
/// chord is the user's way both in and out.
pub fn toggle(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(WINDOW)
        .ok_or("the quick-entry window is not open")?;

    if window.is_visible().unwrap_or(false) {
        window.hide().map_err(|e| e.to_string())?;
        return Ok(());
    }
    open(app)
}

/// Raise the window and put the cursor in it. The tray and the palette want this
/// rather than a toggle — they are outside the window, so it is never already up.
pub fn open(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(WINDOW)
        .ok_or("the quick-entry window is not open")?;

    place_on_cursor_monitor(app, &window);
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}

/// Centre the window on the monitor the *cursor* is on.
///
/// The honest approximation of "the monitor with the focused window": the chord
/// is pressed while looking at another application, so where the pointer is is
/// where the user is, and this needs no window enumeration to find out.
fn place_on_cursor_monitor(app: &AppHandle, window: &tauri::WebviewWindow) {
    let Ok(cursor) = app.cursor_position() else {
        return;
    };
    let Ok(Some(monitor)) = app.monitor_from_point(cursor.x, cursor.y) else {
        return;
    };
    let scale = monitor.scale_factor();
    let origin = monitor.position().to_logical::<f64>(scale);
    let screen = monitor.size().to_logical::<f64>(scale);
    let mine = window
        .outer_size()
        .map(|size| size.to_logical::<f64>(scale))
        .unwrap_or(tauri::LogicalSize::new(560.0, 132.0));
    let _ = window.set_position(tauri::LogicalPosition::new(
        origin.x + (screen.width - mine.width) / 2.0,
        origin.y + (screen.height - mine.height) / 2.0,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_chord_parses() {
        assert!(parse(DEFAULT_CHORD).is_ok());
    }

    #[test]
    fn a_chord_is_the_same_however_it_is_spelled() {
        // The handler compares the canonical label, so the spelling the user
        // types and the spelling the plugin prints have to agree — that is the
        // whole reason `label` exists rather than a string comparison.
        assert_eq!(label("Ctrl+Alt+Space"), label("control+alt+space"));
        assert_eq!(label("Ctrl+Alt+Space"), label("  ctrl + alt + space  "));
        assert!(!label(DEFAULT_CHORD).is_empty());
    }

    #[test]
    fn something_that_is_not_a_chord_is_refused_by_name() {
        let err = parse("Banana+Shift").unwrap_err();
        assert!(err.contains("Banana+Shift"), "the message should quote the input: {err}");
        assert!(parse("").is_err());
        // A modifier with no key is the mistake a person actually makes.
        assert!(parse("Ctrl+Alt").is_err());
    }

    #[test]
    fn an_unknown_chord_has_no_label_to_match_against() {
        // Empty rather than a panic: the handler compares against this on every
        // keypress, and a settings file a person hand-edited must not be able to
        // take the app down.
        assert_eq!(label("nonsense"), "");
    }
}
