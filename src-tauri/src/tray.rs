//! The system tray: the app's surface while its window is closed, and the way
//! back to it.
//!
//! The menu is a table rather than four hand-written items, because each row has
//! to agree with something outside this file — an id the window runs, or a
//! label a person reads. `src/lib/tray.check.ts` holds the forwarded ids to the
//! command palette's own list, so a rename on either side fails the gate instead
//! of leaving a menu item that does nothing.

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

/// The menu: id, label, and whether the **window** has to run it.
///
/// `true` forwards the id to the front end, which hands it to the same action
/// switch the command palette uses — so there is one implementation of "new
/// chat" rather than a second one that drifts. `false` means this file does it,
/// because it owns the window and the process.
pub const MENU: [(&str, &str, bool); 4] = [
    ("new-chat", "New chat", true),
    ("quick-entry", "Quick entry", true),
    ("show", "Show DeskSeek", false),
    ("quit", "Quit", false),
];

/// A menu item the window has to run. Named rather than inlined so the front
/// end's listener and this file's emitter are the same string.
pub const EVENT_TRAY: &str = "tray-action";

/// The tray's own id, so the close handler can ask whether there is one before
/// it relies on it.
pub const TRAY_ID: &str = "deskseek";

/// The main window's label — the one the close handler hides instead of closing.
pub const MAIN_WINDOW: &str = "main";

/// Build the tray. Failing is survivable: a machine without a tray (or with the
/// shell's tray disabled) simply keeps an ordinary window that closes.
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let menu = Menu::new(app)?;
    for (id, label, _) in MENU {
        let item = MenuItem::with_id(app, id, label, true, None::<&str>)?;
        menu.append(&item)?;
    }

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(
            app.default_window_icon()
                .expect("the bundle carries an icon or the app would not start")
                .clone(),
        )
        .tooltip("DeskSeek")
        .menu(&menu)
        // A left click is "show me", not "open the menu" — which is what every
        // other tray icon on this platform does.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| run(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// One chosen menu item.
fn run(app: &AppHandle, id: &str) {
    match MENU.iter().find(|(menu_id, _, _)| *menu_id == id) {
        Some((_, _, true)) => {
            let _ = app.emit(EVENT_TRAY, serde_json::json!({ "id": id }));
        }
        Some((_, _, false)) if id == "show" => show_main(app),
        // Anything else in this half of the table is Quit: the table is the
        // whole menu, so there is no third option for a typo to land on.
        Some(_) => app.exit(0),
        None => {}
    }
}

/// Bring the main window back, un-minimised and in front.
fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Whether closing the main window should hide it rather than quit.
///
/// Only when there is a tray to come back from. Without one, hiding the window
/// would leave a running process nobody can reach, which is worse than quitting.
pub fn closes_to_tray(app: &AppHandle) -> bool {
    app.tray_by_id(TRAY_ID).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tray_menu_is_the_four_things_the_window_can_do() {
        let ids: Vec<&str> = MENU.iter().map(|(id, _, _)| *id).collect();
        assert_eq!(ids.len(), 4);
        let mut unique = ids.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), 4, "two menu items share an id: {ids:?}");
        for (id, label, _) in MENU {
            assert!(!id.is_empty() && !label.is_empty(), "a blank menu item: {id:?}");
        }
        // Quit and Show are this file's; the other two are the window's. Pinned
        // because `run` reads the same flag to decide — a flipped bit here would
        // silently make "New chat" quit the app.
        let ours: Vec<&str> = MENU
            .iter()
            .filter(|(_, _, to_window)| !to_window)
            .map(|(id, _, _)| *id)
            .collect();
        assert_eq!(ours, vec!["show", "quit"]);
    }

    #[test]
    fn the_ids_the_window_runs_are_the_palette_s_own() {
        // The other half of `the_tray_menu_is_the_four_things_the_window_can_do`:
        // these are the ids the front end routes, and `src/lib/tray.check.ts`
        // checks them against the palette's list — this is the Rust side of that
        // agreement, asserted where a rename would break the build's own tests.
        let forwarded: Vec<&str> = MENU
            .iter()
            .filter(|(_, _, to_window)| *to_window)
            .map(|(id, _, _)| *id)
            .collect();
        assert_eq!(forwarded, vec!["new-chat", "quick-entry"]);
    }
}
