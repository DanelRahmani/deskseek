//! Reading the desktop: a text view of the Windows accessibility tree, so the
//! model can answer "what is on my screen" and read a dialog it cannot see.
//!
//! **Read-only, and Windows-only.** This module looks; it does not touch. There
//! is no synthetic input here on purpose — clicking and typing are the next
//! phase, and they bring the rails that acting needs (a global kill switch and a
//! per-run action budget) along with them.
//!
//! Two constraints shape the code:
//!
//! * `uiautomation::UIAutomation` and `UIElement` are **not `Send`** — they wrap
//!   COM pointers that may only be used on the thread that created them. So every
//!   call builds the client inside a `spawn_blocking` closure and drops it before
//!   returning: nothing COM-shaped is stored in `AppState`, and none of it is held
//!   across an `await`. That is also why the tree is read into plain `Row`s — the
//!   only thing to cross the thread boundary is owned data.
//! * An accessibility tree is unbounded in principle and enormous in practice, so
//!   every walk is capped by depth *and* node count, and off-screen nodes are
//!   skipped. A model that reads 5,000 nodes learns nothing and pays for all of
//!   them.

use serde_json::Value;

use super::exec::truncate;

/// How deep into a window the walk goes. Deeper is usually framework scaffolding.
const MAX_DEPTH: usize = 6;
/// How many rows the model is ever shown.
const MAX_ROWS: usize = 300;
/// Names are labels, not documents.
const MAX_NAME: usize = 80;

/// One line of the desktop tree, decoupled from COM so the formatting and the
/// caps can be checked without a screen attached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub depth: usize,
    /// The control type, e.g. `Button`. Language-independent, so the model sees
    /// the same word whatever Windows is set to.
    pub kind: String,
    pub name: String,
    pub id: String,
    pub enabled: bool,
}

/// UIA names routinely contain newlines and padding; a multi-line name would
/// break the indentation that carries the whole structure.
fn one_line(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(MAX_NAME).collect()
}

/// Does `name` (a window title) answer to `want`? Case-insensitive substring, so
/// the model can say "notepad" instead of guessing an exact title.
pub fn match_window(name: &str, want: &str) -> bool {
    let want = want.trim().to_lowercase();
    !want.is_empty() && name.to_lowercase().contains(&want)
}

/// The fallback when a named window is not open: say what *is*, rather than
/// leaving the model to guess again.
pub fn describe_windows(names: &[String]) -> String {
    if names.is_empty() {
        return "no top-level windows are open".to_string();
    }
    let mut out = String::from("Open windows:\n");
    for name in names {
        out.push_str("  ");
        out.push_str(&one_line(name));
        out.push('\n');
    }
    out
}

/// Renders the tree. The first row is the window itself, so the output reads as
/// one document rather than a header plus an unrelated list.
pub fn render_rows(rows: &[Row]) -> String {
    if rows.is_empty() {
        return "that window has no readable controls — it may be a game, a canvas, or drawn by \
                hand rather than built from standard controls"
            .to_string();
    }

    let mut out = String::new();
    for row in rows {
        out.push_str(&"  ".repeat(row.depth));
        out.push_str(&row.kind);
        let name = one_line(&row.name);
        if !name.is_empty() {
            out.push_str(&format!(" \"{name}\""));
        }
        // An id that only repeats the name is noise.
        if !row.id.is_empty() && row.id != row.name {
            out.push_str(&format!(" [id={}]", one_line(&row.id)));
        }
        if !row.enabled {
            out.push_str(" (disabled)");
        }
        out.push('\n');
    }
    if rows.len() >= MAX_ROWS {
        out.push_str(&format!(
            "… (stopped at {MAX_ROWS} controls; this window is big — name a different window to \
             narrow it down)\n"
        ));
    }
    truncate(out)
}

/// Runs one desktop tool. Windows-only; the stub keeps the tool list identical
/// on every platform so `tools_for` and `class_of` stay platform-independent.
#[cfg(windows)]
pub async fn execute(name: &str, args: &Value) -> Result<String, String> {
    match name {
        "ui_snapshot" => {
            let window = args
                .get("window")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            snapshot(window).await
        }
        other => Err(format!("unknown desktop tool '{other}'")),
    }
}

#[cfg(not(windows))]
pub async fn execute(name: &str, _args: &Value) -> Result<String, String> {
    Err(format!(
        "'{name}' reads the Windows desktop — it is not available on this platform"
    ))
}

#[cfg(windows)]
pub async fn snapshot(window: Option<String>) -> Result<String, String> {
    // Blocking, because COM belongs to the thread that made it. `spawn_blocking`
    // hands us a thread with no reactor, which is exactly right for synchronous
    // COM calls that can take tens of milliseconds.
    tokio::task::spawn_blocking(move || {
        let rows = com::read_tree(window.as_deref())?;
        Ok(render_rows(&rows))
    })
    .await
    .map_err(|e| format!("the desktop reader stopped unexpectedly: {e}"))?
}

#[cfg(windows)]
mod com {
    use super::{Row, describe_windows, match_window, MAX_DEPTH, MAX_ROWS};
    use uiautomation::core::{UIAutomation, UIElement, UITreeWalker};
    use uiautomation::types::ControlType;

    struct Window {
        name: String,
        element: UIElement,
        focused: bool,
    }

    /// One window's accessibility tree, flattened. Depth 0 is the window.
    pub fn read_tree(window: Option<&str>) -> Result<Vec<Row>, String> {
        let automation = UIAutomation::new().map_err(|e| {
            format!("could not reach the Windows accessibility layer: {e}")
        })?;
        let walker = automation
            .create_tree_walker()
            .map_err(|e| format!("could not walk the desktop: {e}"))?;

        let windows = top_level_windows(&automation);
        let names: Vec<String> = windows.iter().map(|w| w.name.clone()).collect();

        let target = match window {
            Some(want) => windows
                .iter()
                .find(|w| match_window(&w.name, want))
                .ok_or_else(|| {
                    format!("no open window matches \"{want}\".\n\n{}", describe_windows(&names))
                })?,
            // The focused window is the one the user is looking at.
            None => windows
                .iter()
                .find(|w| w.focused)
                .or_else(|| windows.iter().find(|w| !w.name.trim().is_empty()))
                .ok_or_else(|| "no top-level window is open".to_string())?,
        };

        let mut rows = Vec::new();
        walk(&walker, &target.element, 0, &mut rows);
        if rows.is_empty() {
            return Err(format!("nothing readable was found in \"{}\"", target.name));
        }
        Ok(rows)
    }

    /// Top-level windows, in the order the accessibility layer reports them.
    /// Depth 2 rather than 1 so a modal dialog (the interesting case) is included
    /// alongside the window that owns it.
    fn top_level_windows(automation: &UIAutomation) -> Vec<Window> {
        let matcher = automation
            .create_matcher()
            .depth(2)
            .timeout(0)
            .control_type(ControlType::Window);
        let Ok(elements) = matcher.find_all() else {
            return Vec::new();
        };
        elements
            .into_iter()
            .map(|element| Window {
                name: element.get_name().unwrap_or_default(),
                focused: element.has_keyboard_focus().unwrap_or(false),
                element,
            })
            .collect()
    }

    /// Depth-first, capped. An element the user cannot see is not part of the
    /// screen they are describing, so off-screen branches are skipped whole.
    fn walk(walker: &UITreeWalker, element: &UIElement, depth: usize, rows: &mut Vec<Row>) {
        if rows.len() >= MAX_ROWS || element.is_offscreen().unwrap_or(false) {
            return;
        }
        rows.push(row_of(element, depth));
        if rows.len() >= MAX_ROWS || depth >= MAX_DEPTH {
            return;
        }

        // A missing child or sibling simply ends that branch — the tree changes
        // under us constantly, and a vanished node is not an error.
        let mut child = walker.get_first_child(element);
        while let Ok(node) = child {
            walk(walker, &node, depth + 1, rows);
            if rows.len() >= MAX_ROWS {
                return;
            }
            child = walker.get_next_sibling(&node);
        }
    }

    fn row_of(element: &UIElement, depth: usize) -> Row {
        Row {
            depth,
            kind: element
                .get_control_type()
                .map(|kind| format!("{kind:?}"))
                .unwrap_or_else(|_| "Element".to_string()),
            name: element.get_name().unwrap_or_default(),
            id: element.get_automation_id().unwrap_or_default(),
            // An unreadable enabled flag is not a reason to call it disabled.
            enabled: element.is_enabled().unwrap_or(true),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(depth: usize, kind: &str, name: &str) -> Row {
        Row {
            depth,
            kind: kind.to_string(),
            name: name.to_string(),
            id: String::new(),
            enabled: true,
        }
    }

    #[test]
    fn structure_becomes_indentation() {
        let rows = [
            row(0, "Window", "Untitled - Notepad"),
            row(1, "Pane", ""),
            row(2, "Document", "Text Editor"),
        ];
        let text = render_rows(&rows);
        assert_eq!(
            text.lines().collect::<Vec<_>>(),
            vec![
                "Window \"Untitled - Notepad\"",
                "  Pane",
                "    Document \"Text Editor\"",
            ],
            "got: {text}"
        );
    }

    #[test]
    fn a_multiline_name_cannot_break_the_indentation() {
        let rows = [row(0, "Window", "line one\n   line two")];
        let text = render_rows(&rows);
        assert_eq!(text.lines().count(), 1, "got: {text}");
        assert!(text.contains("\"line one line two\""), "got: {text}");
    }

    #[test]
    fn an_id_that_only_repeats_the_name_is_left_out() {
        let mut duplicated = row(0, "Button", "Save");
        duplicated.id = "Save".to_string();
        assert_eq!(render_rows(&[duplicated]), "Button \"Save\"\n");

        let mut distinct = row(0, "Button", "Save");
        distinct.id = "btnSave".to_string();
        assert_eq!(render_rows(&[distinct]), "Button \"Save\" [id=btnSave]\n");
    }

    #[test]
    fn a_disabled_control_is_marked_so_the_model_does_not_try_it() {
        let mut disabled = row(0, "Button", "Send");
        disabled.enabled = false;
        assert!(render_rows(&[disabled]).contains("(disabled)"));
    }

    #[test]
    fn an_empty_tree_says_so_instead_of_rendering_nothing() {
        let text = render_rows(&[]);
        assert!(text.contains("no readable controls"), "got: {text}");
    }

    #[test]
    fn a_huge_window_is_capped_and_admits_it() {
        let rows: Vec<Row> = (0..MAX_ROWS).map(|i| row(1, "Text", &format!("row {i}"))).collect();
        let text = render_rows(&rows);
        assert!(text.contains("stopped at"), "got the tail of: {}", &text[text.len() - 200..]);
    }

    #[test]
    fn an_overlong_name_is_cut_not_wrapped() {
        let rows = [row(0, "Text", &"x".repeat(MAX_NAME * 3))];
        let text = render_rows(&rows);
        assert!(text.len() < MAX_NAME * 3, "got {} chars", text.len());
    }

    #[test]
    fn window_matching_is_case_insensitive_and_forgiving() {
        assert!(match_window("Untitled - Notepad", "notepad"));
        assert!(match_window("Untitled - Notepad", "Untitled"));
        assert!(match_window("Visual Studio Code", "  studio code "));
        assert!(!match_window("Untitled - Notepad", "excel"));
        // An empty request must not match the first window it meets.
        assert!(!match_window("Untitled - Notepad", "   "));
    }

    #[test]
    fn a_missed_window_lists_what_is_actually_open() {
        let text = describe_windows(&["Notepad".to_string(), "Settings".to_string()]);
        assert!(text.contains("Notepad") && text.contains("Settings"), "got: {text}");
        assert!(describe_windows(&[]).contains("no top-level windows"));
    }

    /// The COM plumbing, against whatever desktop this machine has. It **skips**
    /// (rather than failing) when there is no interactive session to read, so the
    /// suite still runs on a build agent — the same bargain the live browser test
    /// makes. Nothing is asserted about *which* window is focused: that is the
    /// runner's business, not this test's.
    #[cfg(windows)]
    #[tokio::test]
    async fn it_reads_a_real_desktop_end_to_end() {
        let Ok(text) = snapshot(None).await else {
            eprintln!("no readable desktop in this session — skipping the live desktop test");
            return;
        };
        assert!(!text.trim().is_empty(), "a successful read must produce something");
        // The first line is the window itself, so it must not be indented.
        let first = text.lines().next().unwrap_or_default();
        assert!(!first.starts_with(' '), "the window line must sit at the margin: {first}");
        assert!(first.split_once(' ').is_some(), "the window line names a control type: {first}");
        // Visible under `--nocapture`, so a human can see what the model would.
        eprintln!(
            "read {} rows; first three:\n{}",
            text.lines().count(),
            text.lines().take(3).collect::<Vec<_>>().join("\n")
        );
    }
}
