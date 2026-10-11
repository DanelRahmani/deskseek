//! The user's desktop: reading it as text, and — carefully — acting on it.
//!
//! Windows-only. Two halves that are deliberately not equal in power:
//!
//! * **Reading** (`ui_snapshot`) walks the accessibility tree and is harmless.
//! * **Acting** (`ui_click`, `ui_type`) really presses things and really types.
//!   That is a different risk class, so it is fenced four ways: the [`Gate`] (a
//!   hotkey that stops everything and stays stopped until the user says
//!   otherwise), a per-run action budget in `agent::run`, a minimum gap between
//!   actions, and an approval prompt for every single call.
//!
//! Two constraints shape the code:
//!
//! * `uiautomation::UIAutomation` and `UIElement` are **not `Send`** — they wrap
//!   COM pointers that may only be used on the thread that created them. So every
//!   call builds the client inside a `spawn_blocking` closure and drops it before
//!   returning: nothing COM-shaped is stored in `AppState`, and none of it is held
//!   across an `await`. That is also why the tree is read into plain `Row`s and
//!   the request into a plain [`Query`] — the only things to cross the thread
//!   boundary are owned data.
//! * An accessibility tree is unbounded in principle and enormous in practice, so
//!   every walk is capped by depth *and* node count, and off-screen nodes are
//!   skipped. A model that reads 5,000 nodes learns nothing and pays for all of
//!   them.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::exec::truncate;

/// Emitted whenever computer control is stopped or resumed, so the chat can say so.
pub const EVENT_COMPUTER: &str = "computer-state";

/// How deep into a window the walk goes. Deeper is usually framework scaffolding.
const MAX_DEPTH: usize = 6;
/// How many rows the model is ever shown.
const MAX_ROWS: usize = 300;
/// Names are labels, not documents.
const MAX_NAME: usize = 80;
/// How far down a search looks for the control the model asked for.
const SEARCH_DEPTH: u32 = 12;
/// The most controls offered in a "nothing matched" list.
const MAX_CANDIDATES: usize = 30;
/// Acting on a desktop is not free: hold at least this long between actions, so
/// the screen has a chance to catch up and a runaway loop is visible.
const MIN_ACTION_GAP: Duration = Duration::from_millis(250);

/// The kill switch, and the pacing that keeps a loop honest.
///
/// `paused` is process-wide rather than per chat: the hotkey exists to stop
/// *everything*, and the chat you cannot see is the one you most want stopped.
#[derive(Default)]
pub struct Gate {
    paused: AtomicBool,
    last_action: Mutex<Option<Instant>>,
}

impl Gate {
    /// Stops computer control. True only when it was not already stopped, so the
    /// caller can tell "I just stopped this" from "it was already off".
    pub fn pause(&self) -> bool {
        !self.paused.swap(true, Ordering::SeqCst)
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    /// Refuses while stopped, and otherwise holds the caller back until the gap
    /// since the last action has passed. Runs on a blocking thread, so sleeping
    /// here is right rather than rude.
    fn wait_turn(&self) -> Result<(), String> {
        if self.is_paused() {
            return Err(
                "Computer control is stopped (Ctrl+Alt+Esc). Tell the user what you were about to \
                 do; they can start it again from the banner in the chat."
                    .to_string(),
            );
        }
        let mut last = self.last_action.lock().unwrap();
        if let Some(previous) = *last {
            let elapsed = previous.elapsed();
            if elapsed < MIN_ACTION_GAP {
                std::thread::sleep(MIN_ACTION_GAP - elapsed);
            }
        }
        *last = Some(Instant::now());
        Ok(())
    }
}

/// Tools that change something on the desktop, as opposed to looking at it.
/// These are what the per-run budget counts.
pub fn is_action(name: &str) -> bool {
    matches!(name, "ui_click" | "ui_type")
}

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

/// What the model asked to act on. Parsed from JSON into plain owned data so it
/// can cross into the blocking thread; `kind` stays a string because the
/// `ControlType` it maps to is not ours to send.
#[derive(Debug, Default, Clone)]
pub struct Query {
    pub name: Option<String>,
    pub id: Option<String>,
    pub kind: Option<String>,
    pub window: Option<String>,
}

fn text_arg(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

impl Query {
    fn from_args(args: &Value) -> Self {
        Self {
            name: text_arg(args, "name"),
            id: text_arg(args, "id"),
            kind: text_arg(args, "type"),
            window: text_arg(args, "window"),
        }
    }

    /// A control has to be named or identified. Matching on the *type* alone
    /// would press the first button in the window, which is never what was meant.
    fn is_specific(&self) -> bool {
        self.name.is_some() || self.id.is_some()
    }
}

/// Runs one desktop tool.
#[cfg(windows)]
pub async fn execute(name: &str, args: &Value, gate: &Gate) -> Result<String, String> {
    match name {
        "ui_snapshot" => snapshot(Query::from_args(args)).await,
        "ui_click" => {
            let query = Query::from_args(args);
            if !query.is_specific() {
                return Err(
                    "ui_click needs either the control's `name` or its `id` — the type alone does \
                     not say which one you mean"
                        .to_string(),
                );
            }
            gate.wait_turn()?;
            click(query).await
        }
        "ui_type" => {
            let query = Query::from_args(args);
            if !query.is_specific() {
                return Err(
                    "ui_type needs either the field's `name` or its `id` to know where the text \
                     goes"
                        .to_string(),
                );
            }
            let text = args.get("text").and_then(Value::as_str).unwrap_or("").to_string();
            let clear = args.get("clear").and_then(Value::as_bool).unwrap_or(false);
            let submit = args.get("submit").and_then(Value::as_bool).unwrap_or(false);
            gate.wait_turn()?;
            type_text(query, text, clear, submit).await
        }
        other => Err(format!("unknown desktop tool '{other}'")),
    }
}

#[cfg(not(windows))]
pub async fn execute(name: &str, _args: &Value, _gate: &Gate) -> Result<String, String> {
    Err(format!(
        "'{name}' drives the Windows desktop — it is not available on this platform"
    ))
}

/// Blocking, because COM belongs to the thread that made it. `spawn_blocking`
/// hands us a thread with no reactor, which is exactly right for synchronous COM
/// calls that can take tens of milliseconds.
#[cfg(windows)]
async fn run_blocking<T, F>(work: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| format!("the desktop reader stopped unexpectedly: {e}"))?
}

#[cfg(windows)]
pub async fn snapshot(query: Query) -> Result<String, String> {
    run_blocking(move || Ok(render_rows(&com::read_tree(query.window.as_deref(), MAX_ROWS)?))).await
}

#[cfg(windows)]
pub async fn click(query: Query) -> Result<String, String> {
    run_blocking(move || com::click(&query)).await
}

#[cfg(windows)]
pub async fn type_text(
    query: Query,
    text: String,
    clear: bool,
    submit: bool,
) -> Result<String, String> {
    run_blocking(move || com::type_text(&query, &text, clear, submit)).await
}

#[cfg(windows)]
mod com {
    use super::{
        MAX_CANDIDATES, MAX_DEPTH, Query, Row, SEARCH_DEPTH, describe_windows, match_window,
        one_line, render_rows,
    };
    use uiautomation::core::{UIAutomation, UIElement, UITreeWalker};
    use uiautomation::patterns::UIInvokePattern;
    use uiautomation::types::ControlType;

    /// How much of the window is shown back after an action. Smaller than a full
    /// snapshot on purpose: the model just acted, so it wants confirmation, not
    /// the whole tree again.
    const FOLLOW_ROWS: usize = 60;

    struct Window {
        name: String,
        element: UIElement,
        focused: bool,
    }

    /// One window's accessibility tree, flattened. Depth 0 is the window.
    pub fn read_tree(window: Option<&str>, max_rows: usize) -> Result<Vec<Row>, String> {
        let automation = open()?;
        let walker = automation
            .create_tree_walker()
            .map_err(|e| format!("could not walk the desktop: {e}"))?;
        let target = target_window(&automation, window)?;
        Ok(rows_from(&walker, &target.element, max_rows))
    }

    fn open() -> Result<UIAutomation, String> {
        UIAutomation::new()
            .map_err(|e| format!("could not reach the Windows accessibility layer: {e}"))
    }

    /// The window to work in: the one the model named, or the one with focus.
    ///
    /// Depth 2 rather than 1 so a modal dialog — the interesting case — is
    /// included alongside the window that owns it.
    fn target_window(automation: &UIAutomation, window: Option<&str>) -> Result<Window, String> {
        let matcher = automation
            .create_matcher()
            .depth(2)
            .timeout(0)
            .control_type(ControlType::Window);
        let mut windows: Vec<Window> = matcher
            .find_all()
            .unwrap_or_default()
            .into_iter()
            .map(|element| Window {
                name: element.get_name().unwrap_or_default(),
                focused: element.has_keyboard_focus().unwrap_or(false),
                element,
            })
            .collect();
        let names: Vec<String> = windows.iter().map(|w| w.name.clone()).collect();

        let found = match window {
            Some(want) => windows.iter().position(|w| match_window(&w.name, want)),
            None => windows
                .iter()
                .position(|w| w.focused)
                .or_else(|| windows.iter().position(|w| !w.name.trim().is_empty())),
        };
        let at = found.ok_or_else(|| match window {
            Some(want) => format!("no open window matches \"{want}\".\n\n{}", describe_windows(&names)),
            None => "no top-level window is open".to_string(),
        })?;
        Ok(windows.swap_remove(at))
    }

    /// Depth-first, capped. An element the user cannot see is not part of the
    /// screen they are describing, so off-screen branches are skipped whole.
    fn rows_from(walker: &UITreeWalker, root: &UIElement, max_rows: usize) -> Vec<Row> {
        let mut rows = Vec::new();
        walk(walker, root, 0, max_rows, &mut rows);
        rows
    }

    fn walk(
        walker: &UITreeWalker,
        element: &UIElement,
        depth: usize,
        max_rows: usize,
        rows: &mut Vec<Row>,
    ) {
        if rows.len() >= max_rows || element.is_offscreen().unwrap_or(false) {
            return;
        }
        rows.push(row_of(element, depth));
        if rows.len() >= max_rows || depth >= MAX_DEPTH {
            return;
        }

        // A missing child or sibling simply ends that branch — the tree changes
        // under us constantly, and a vanished node is not an error.
        let mut child = walker.get_first_child(element);
        while let Ok(node) = child {
            walk(walker, &node, depth + 1, max_rows, rows);
            if rows.len() >= max_rows {
                return;
            }
            child = walker.get_next_sibling(&node);
        }
    }

    fn row_of(element: &UIElement, depth: usize) -> Row {
        Row {
            depth,
            kind: kind_of(element),
            name: element.get_name().unwrap_or_default(),
            id: element.get_automation_id().unwrap_or_default(),
            // An unreadable enabled flag is not a reason to call it disabled.
            enabled: element.is_enabled().unwrap_or(true),
        }
    }

    fn kind_of(element: &UIElement) -> String {
        element
            .get_control_type()
            .map(|kind| format!("{kind:?}"))
            .unwrap_or_else(|_| "Element".to_string())
    }

    /// One source of truth for the `type` argument, so the accepted names and the
    /// error that lists them can never drift apart.
    const KINDS: &[(&str, ControlType)] = &[
        ("button", ControlType::Button),
        ("checkbox", ControlType::CheckBox),
        ("combobox", ControlType::ComboBox),
        ("edit", ControlType::Edit),
        ("hyperlink", ControlType::Hyperlink),
        ("listitem", ControlType::ListItem),
        ("menuitem", ControlType::MenuItem),
        ("radiobutton", ControlType::RadioButton),
        ("tabitem", ControlType::TabItem),
        ("text", ControlType::Text),
        ("treeitem", ControlType::TreeItem),
    ];

    fn resolve_kind(want: &str) -> Result<Option<ControlType>, String> {
        let key = want.trim().to_lowercase();
        if key.is_empty() {
            return Ok(None);
        }
        KINDS
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, kind)| Some(*kind))
            .ok_or_else(|| {
                let accepted: Vec<&str> = KINDS.iter().map(|(name, _)| *name).collect();
                format!("'{want}' is not a control type. Use one of: {}", accepted.join(", "))
            })
    }

    /// A human label for a control, in the same shape the snapshot uses.
    fn label(element: &UIElement) -> String {
        let kind = kind_of(element);
        let name = one_line(&element.get_name().unwrap_or_default());
        if name.is_empty() {
            kind
        } else {
            format!("{kind} \"{name}\"")
        }
    }

    /// Every enabled, on-screen control that matches the query.
    fn matches(
        automation: &UIAutomation,
        root: &UIElement,
        query: &Query,
    ) -> Result<Vec<UIElement>, String> {
        let mut matcher = automation.create_matcher().from_ref(root).depth(SEARCH_DEPTH).timeout(0);
        if let Some(name) = &query.name {
            matcher = matcher.contains_name(name.clone());
        }
        if let Some(kind) = resolve_kind(query.kind.as_deref().unwrap_or(""))? {
            matcher = matcher.control_type(kind);
        }
        if let Some(id) = &query.id {
            // `UIMatcher` has no id filter, so this is a predicate. UIA ids are
            // optional, so an element without one simply does not match.
            let want = id.clone();
            matcher = matcher.filter_fn(Box::new(move |e: &UIElement| {
                Ok(e.get_automation_id().map(|got| got == want).unwrap_or(false))
            }));
        }
        let found = matcher.find_all().map_err(|e| {
            format!("could not search that window: {e} — it may have closed while you were looking")
        })?;
        Ok(found
            .into_iter()
            .filter(|e| e.is_enabled().unwrap_or(true) && !e.is_offscreen().unwrap_or(false))
            .collect())
    }

    /// What the model could have meant: the buttons in the window, so a miss
    /// costs one turn instead of a guessing game.
    fn describe_controls(automation: &UIAutomation, root: &UIElement) -> String {
        let buttons = automation
            .create_matcher()
            .from_ref(root)
            .depth(SEARCH_DEPTH)
            .timeout(0)
            .control_type(ControlType::Button)
            .find_all()
            .unwrap_or_default();

        let mut names: Vec<String> = buttons
            .into_iter()
            .filter(|e| e.is_enabled().unwrap_or(true) && !e.is_offscreen().unwrap_or(false))
            .map(|e| label(&e))
            .collect();
        names.sort();
        names.dedup();
        names.truncate(MAX_CANDIDATES);

        if names.is_empty() {
            return "Nothing matched, and this window reports no buttons at all. Its controls are \
                    probably drawn by the app itself rather than built from standard controls, so \
                    this one may have to be done by hand — say so instead of clicking at random."
                .to_string();
        }
        let mut out = String::from("Buttons in this window:\n");
        for name in names {
            out.push_str("  ");
            out.push_str(&name);
            out.push('\n');
        }
        out
    }

    /// The search plus the "here is what you could have meant" fallback.
    fn locate(
        automation: &UIAutomation,
        window: &Window,
        query: &Query,
    ) -> Result<(UIElement, usize), String> {
        let found = matches(automation, &window.element, query)?;
        let count = found.len();
        found.into_iter().next().map(|first| (first, count)).ok_or_else(|| {
            format!(
                "nothing in \"{}\" matched.\n\n{}",
                one_line(&window.name),
                describe_controls(automation, &window.element)
            )
        })
    }

    /// Presses a control. The Invoke pattern is tried first: it is a direct
    /// request to the control itself, so the mouse never moves and a window that
    /// happens to be in the way cannot be hit instead.
    fn activate(element: &UIElement) -> Result<String, String> {
        if let Ok(invoke) = element.get_pattern::<UIInvokePattern>() {
            if invoke.invoke().is_ok() {
                return Ok("Pressed".to_string());
            }
        }
        element
            .click()
            .map_err(|e| format!("that control would not press or click: {e}"))?;
        Ok("Clicked".to_string())
    }

    /// Say so when the match was not unique: the model may have meant the other
    /// one, and it should hear that from the result rather than from a surprise.
    fn ambiguity(count: usize) -> String {
        match count {
            1 => String::new(),
            n => format!(" — {n} controls matched and the first was used"),
        }
    }

    fn follow_up(automation: &UIAutomation, window: &Window) -> String {
        match automation.create_tree_walker() {
            Ok(walker) => render_rows(&rows_from(&walker, &window.element, FOLLOW_ROWS)),
            Err(_) => String::new(),
        }
    }

    pub fn click(query: &Query) -> Result<String, String> {
        let automation = open()?;
        let window = target_window(&automation, query.window.as_deref())?;
        let (element, count) = locate(&automation, &window, query)?;
        let described = label(&element);
        let how = activate(&element)?;
        Ok(format!(
            "{how} {described} in \"{}\"{}.\n\n{}",
            one_line(&window.name),
            ambiguity(count),
            follow_up(&automation, &window)
        ))
    }

    pub fn type_text(query: &Query, text: &str, clear: bool, submit: bool) -> Result<String, String> {
        let automation = open()?;
        let window = target_window(&automation, query.window.as_deref())?;
        let (field, count) = locate(&automation, &window, query)?;
        let described = label(&field);

        // Focus first: both typing calls go to whatever holds focus, so a field
        // that never took it would silently receive nothing.
        let _ = field.set_focus();
        if clear {
            // Select-all then delete, rather than assuming the field is empty.
            let _ = field.send_keys("{ctrl}a", 0);
            let _ = field.send_keys("{delete}", 0);
        }
        field
            .send_text(text, 0)
            .map_err(|e| format!("could not type into {described}: {e}"))?;
        if submit {
            field
                .send_keys("{enter}", 0)
                .map_err(|e| format!("the text went in, but Enter could not be pressed: {e}"))?;
        }

        let action = match (clear, submit) {
            (false, false) => "Typed into",
            (true, false) => "Replaced the text in",
            (false, true) => "Typed into and submitted",
            (true, true) => "Replaced the text in and submitted",
        };
        Ok(format!(
            "{action} {described} in \"{}\"{}.\n\n{}",
            one_line(&window.name),
            ambiguity(count),
            follow_up(&automation, &window)
        ))
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

    #[test]
    fn only_the_acting_tools_count_as_actions() {
        assert!(is_action("ui_click"));
        assert!(is_action("ui_type"));
        // Looking is not acting, and must not spend the run's budget.
        assert!(!is_action("ui_snapshot"));
        assert!(!is_action("screen_capture"));
        assert!(!is_action("browser_open"));
    }

    #[test]
    fn a_paused_gate_refuses_and_stays_refused_until_it_is_resumed() {
        let gate = Gate::default();
        assert!(!gate.is_paused());
        assert!(gate.wait_turn().is_ok(), "an open gate lets work through");

        // The first pause is the one that happened; a second is not news.
        assert!(gate.pause(), "the first press stops it");
        assert!(!gate.pause(), "a second press finds it already stopped");
        let error = gate.wait_turn().unwrap_err();
        assert!(error.contains("Ctrl+Alt+Esc"), "the reason names the way out: {error}");

        gate.resume();
        assert!(!gate.is_paused());
        assert!(gate.wait_turn().is_ok(), "resuming really reopens it");
    }

    #[test]
    fn actions_are_spaced_so_a_loop_cannot_run_away() {
        let gate = Gate::default();
        assert!(gate.wait_turn().is_ok(), "the first action is not held back");
        let started = Instant::now();
        assert!(gate.wait_turn().is_ok());
        assert!(
            started.elapsed() >= MIN_ACTION_GAP,
            "the second action was not held back: {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn acting_must_name_something_specific() {
        let vague = Query { kind: Some("button".into()), ..Default::default() };
        assert!(!vague.is_specific(), "a type alone does not identify a control");
        assert!(Query { name: Some("Save".into()), ..Default::default() }.is_specific());
        assert!(Query { id: Some("btnSave".into()), ..Default::default() }.is_specific());
    }

    #[test]
    fn query_arguments_are_read_from_the_json_the_model_sends() {
        let query = Query::from_args(&serde_json::json!({
            "name": "  Save  ",
            "type": "Button",
            "window": "notepad",
            "id": "",
        }));
        assert_eq!(query.name.as_deref(), Some("Save"));
        assert_eq!(query.kind.as_deref(), Some("Button"));
        assert_eq!(query.window.as_deref(), Some("notepad"));
        // An empty id is the same as no id — the model should not have to omit a
        // key it is not using.
        assert_eq!(query.id, None);
    }

    /// The COM plumbing, against whatever desktop this machine has. It **skips**
    /// (rather than failing) when there is no interactive session to read, so the
    /// suite still runs on a build agent — the same bargain the live browser test
    /// makes. Nothing is asserted about *which* window is focused: that is the
    /// runner's business, not this test's.
    #[cfg(windows)]
    #[tokio::test]
    async fn it_reads_a_real_desktop_end_to_end() {
        let Ok(text) = snapshot(Query::default()).await else {
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
