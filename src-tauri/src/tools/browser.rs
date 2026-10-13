//! Browser use: driving a real Chromium-family browser over the Chrome DevTools
//! Protocol so the model can click, type and read pages that need JavaScript.
//!
//! Two deliberate choices keep this small and safe:
//!
//! * **No browser is shipped and no CDP crate is pulled in.** `chromiumoxide`
//!   would be a second implementation of the same protocol; the protocol itself
//!   is just JSON over a WebSocket, so this module speaks it directly and reuses
//!   the Edge/Chrome detection that `browse.rs` already has.
//! * **A dedicated profile, never the user's session.** DeskSeek launches the
//!   browser with its own `--user-data-dir` under the app's data folder and an
//!   auto-assigned debugging port. It cannot see the user's tabs, cookies or
//!   logins, and it cannot be reached from outside the machine.
//!
//! Clicks and typing go through `Runtime.evaluate` rather than CDP input events:
//! driving the DOM is both simpler and more reliable than synthesising mouse
//! coordinates, and it works identically in headless mode.

use std::path::Path;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tokio::process::{Child, Command};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use futures_util::{SinkExt, StreamExt};

use crate::runs::Cancel;
use crate::AppState;

use super::browse;
use super::exec::truncate;

/// Emitted when a screenshot is taken, so the chat can show the picture. It
/// travels on its own event because the tool result the model sees stays text.
pub const EVENT_SHOT: &str = "chat-shot";

/// How much of a page's text is handed to the model.
const MAX_PAGE_CHARS: usize = 12_000;
/// A single CDP round trip. Generous: navigating a heavy page can be slow.
const CALL_TIMEOUT_SECS: u64 = 15;
/// How long a navigation may take before we read whatever is there.
const READY_TIMEOUT_SECS: u64 = 20;
const VIEWPORT: &str = "1280,900";

/// The longest-lived piece of browser state: one lazily-started session, shared
/// by every chat. A second concurrent browser would fight over the profile
/// directory, so the mutex is load-bearing, not just for convenience.
#[derive(Default)]
pub struct Browser {
    session: tokio::sync::Mutex<Option<Session>>,
}

impl Browser {
    pub async fn stop(&self) {
        if let Some(session) = self.session.lock().await.take() {
            session.stop().await;
        }
    }

    /// The URL the browser is on, when it has a page open at all.
    ///
    /// Asked only by the approval path, which has to know *where* an action will
    /// land before it can offer a grant covering exactly that place — the action
    /// tools take a selector, not a place, so the page is the only thing that
    /// says. `try_lock` on purpose: a browser busy with another call means no
    /// answer rather than a prompt that waits for it, and no answer simply means
    /// the prompt offers the whole tool instead of a site.
    pub async fn current_url(&self) -> Option<String> {
        let mut guard = self.session.try_lock().ok()?;
        let session = guard.as_mut()?;
        match session.evaluate("location.href").await {
            Ok(Value::String(url)) => Some(url),
            _ => None,
        }
    }
}

struct Session {
    child: Child,
    ws: WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    next_id: i64,
}

impl Session {
    /// Launches the browser and attaches to its first page target.
    async fn start(browser_path: &str, profile: &Path) -> Result<Self, String> {
        let exe = browse::find_browser(browser_path).ok_or_else(|| {
            "no Chromium-family browser found — install Chrome or Edge, or set the path in \
             Settings → Browsing"
                .to_string()
        })?;
        std::fs::create_dir_all(profile).map_err(|e| format!("cannot create the browser profile: {e}"))?;

        // A port file left behind by a crashed run would be read as this run's
        // port, so it is cleared first.
        let port_file = profile.join("DevToolsActivePort");
        let _ = std::fs::remove_file(&port_file);

        let mut cmd = Command::new(&exe);
        cmd.arg("--headless=new")
            .arg("--disable-gpu")
            .arg("--disable-extensions")
            .arg("--mute-audio")
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg("--disable-background-networking")
            .arg(format!("--user-data-dir={}", profile.display()))
            .arg(format!("--window-size={VIEWPORT}"))
            // Port 0 lets the browser pick a free one; it writes the choice to
            // DevToolsActivePort rather than us racing for a port ourselves.
            .arg("--remote-debugging-port=0")
            .arg("about:blank")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let child = cmd
            .spawn()
            .map_err(|e| format!("could not start {}: {e}", exe.display()))?;

        let port = wait_for_port(&port_file).await?;
        let target = first_page(port).await?;
        let (ws, _) = tokio_tungstenite::connect_async(target.as_str())
            .await
            .map_err(|e| format!("could not attach to the browser: {e}"))?;

        Ok(Self {
            child,
            ws,
            next_id: 1,
        })
    }

    async fn stop(mut self) {
        let _ = self.ws.close(None).await;
        let _ = self.child.kill().await;
    }

    /// One CDP command. Events (messages with no matching `id`) are skipped;
    /// the caller's mutex guarantees only one command is ever in flight.
    async fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let payload = json!({ "id": id, "method": method, "params": params }).to_string();

        self.ws
            .send(Message::Text(payload.into()))
            .await
            .map_err(|e| format!("the browser connection was lost: {e}"))?;

        let ws = &mut self.ws;
        let pump = async move {
            while let Some(frame) = ws.next().await {
                let frame = match frame {
                    Ok(frame) => frame,
                    Err(e) => return Err(format!("the browser connection was lost: {e}")),
                };
                let text = match frame {
                    Message::Text(text) => text.as_str().to_string(),
                    Message::Binary(bytes) => String::from_utf8_lossy(&bytes).to_string(),
                    Message::Close(_) => return Err("the browser closed the connection".to_string()),
                    _ => continue,
                };
                let Ok(value) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                if value.get("id").and_then(Value::as_i64) != Some(id) {
                    continue;
                }
                if let Some(error) = value.get("error") {
                    let message = error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown error");
                    return Err(format!("{method} failed: {message}"));
                }
                return Ok(value.get("result").cloned().unwrap_or(Value::Null));
            }
            Err("the browser connection ended".to_string())
        };

        match tokio::time::timeout(Duration::from_secs(CALL_TIMEOUT_SECS), pump).await {
            Ok(result) => result,
            Err(_) => Err(format!("{method} timed out after {CALL_TIMEOUT_SECS}s")),
        }
    }

    /// Runs JavaScript in the page and returns the value, JSON-decoded.
    async fn evaluate(&mut self, expression: &str) -> Result<Value, String> {
        let result = self
            .call(
                "Runtime.evaluate",
                json!({
                    "expression": expression,
                    "returnByValue": true,
                    "awaitPromise": true,
                    "userGesture": true,
                }),
            )
            .await?;

        if let Some(details) = result.get("exceptionDetails") {
            let text = details
                .get("exception")
                .and_then(|e| e.get("description"))
                .and_then(Value::as_str)
                .or_else(|| details.get("text").and_then(Value::as_str))
                .unwrap_or("the page raised an error");
            return Err(text.to_string());
        }

        let remote = result.get("result").cloned().unwrap_or(Value::Null);
        if let Some(value) = remote.get("value") {
            return Ok(value.clone());
        }
        // A DOM node or a function cannot be serialised; CDP describes it
        // instead of failing, and the description is the useful part.
        Ok(Value::String(
            remote
                .get("description")
                .or_else(|| remote.get("type"))
                .and_then(Value::as_str)
                .unwrap_or("(no value)")
                .to_string(),
        ))
    }

    async fn navigate(&mut self, url: &str, cancel: &Cancel) -> Result<(), String> {
        let result = self.call("Page.navigate", json!({ "url": url })).await?;
        if let Some(error) = result.get("errorText").and_then(Value::as_str) {
            return Err(format!("could not load {url}: {error}"));
        }
        self.wait_ready(cancel).await
    }

    /// Waits for the page to finish loading, then gives client-side rendering a
    /// moment to settle. Giving up is not an error: a page that never reaches
    /// `complete` (a live feed, an open socket) is still readable.
    async fn wait_ready(&mut self, cancel: &Cancel) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(READY_TIMEOUT_SECS);
        while Instant::now() < deadline {
            if cancel.is_cancelled() {
                return Err("stopped by the user".to_string());
            }
            // Mid-navigation the execution context is destroyed and this errors;
            // that just means "not ready yet".
            if let Ok(Value::String(state)) = self.evaluate("document.readyState").await {
                if state == "complete" {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
        Ok(())
    }

    async fn read_page(&mut self) -> Result<Page, String> {
        let value = self.evaluate(PAGE_JS).await?;
        page_from_value(&value)
    }

    /// The one tool that produces something the user sees rather than something
    /// the model reads.
    async fn screenshot(&mut self, app: &AppHandle, chat_id: &str, call_id: &str) -> Result<String, String> {
        let result = self
            .call(
                "Page.captureScreenshot",
                json!({ "format": "jpeg", "quality": 70 }),
            )
            .await?;
        let data = result
            .get("data")
            .and_then(Value::as_str)
            .ok_or("the browser returned no image")?;
        let data_url = format!("data:image/jpeg;base64,{data}");
        let _ = app.emit(
            EVENT_SHOT,
            json!({ "chatId": chat_id, "callId": call_id, "dataUrl": data_url }),
        );
        Ok(
            "Screenshot captured and shown to the user in the chat. You cannot see it — use \
             browser_extract or browser_eval to read what is on the page."
                .to_string(),
        )
    }
}

/// Waits for Chrome to write the port it chose.
async fn wait_for_port(port_file: &Path) -> Result<u16, String> {
    for _ in 0..120 {
        if let Ok(body) = std::fs::read_to_string(port_file) {
            if let Some(port) = body
                .lines()
                .next()
                .and_then(|line| line.trim().parse::<u16>().ok())
                .filter(|port| *port > 0)
            {
                return Ok(port);
            }
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    Err(
        "the browser started but never reported a debugging port — it may have been blocked by \
         policy or a security tool"
            .to_string(),
    )
}

#[derive(Deserialize)]
struct Target {
    #[serde(rename = "type")]
    kind: String,
    #[serde(rename = "webSocketDebuggerUrl", default)]
    ws_url: String,
}

/// The page target's debugger socket. The tab may not exist for a moment after
/// launch, so this retries rather than failing.
async fn first_page(port: u16) -> Result<String, String> {
    // Localhost must never be reached through the user's proxy.
    let http = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;
    let list_url = format!("http://127.0.0.1:{port}/json/list");

    for _ in 0..20 {
        if let Ok(response) = http.get(&list_url).send().await {
            if let Ok(targets) = response.json::<Vec<Target>>().await {
                if let Some(target) = targets
                    .into_iter()
                    .find(|t| t.kind == "page" && !t.ws_url.is_empty())
                {
                    return Ok(target.ws_url);
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    Err("the browser opened no page to drive".to_string())
}

/// What a page reports about itself.
#[derive(Debug, Clone, Default, Deserialize)]
struct Page {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    text: String,
}

fn page_from_value(value: &Value) -> Result<Page, String> {
    serde_json::from_value(value.clone())
        .map_err(|_| "the page did not report a title, url and text".to_string())
}

/// Title, address and readable text — the model's eyes on the page.
fn render_page(page: &Page) -> String {
    let mut out = String::new();
    let title = page.title.trim();
    if !title.is_empty() {
        out.push_str(title);
        out.push('\n');
    }
    out.push_str(page.url.trim());
    out.push_str("\n\n");

    let text = page.text.trim();
    if text.is_empty() {
        out.push_str(
            "(no readable text — the page may be a canvas, an image, or still building itself)",
        );
    } else {
        out.push_str(&text.chars().take(MAX_PAGE_CHARS).collect::<String>());
        if text.chars().count() > MAX_PAGE_CHARS {
            out.push_str("\n\n(page text truncated)");
        }
    }
    truncate(out)
}

/// The value of a `browser_eval`, in the most readable shape available.
fn render_eval(value: &Value) -> String {
    match value {
        Value::String(text) => truncate(text.clone()),
        Value::Null => "null".to_string(),
        other => truncate(serde_json::to_string_pretty(other).unwrap_or_else(|_| other.to_string())),
    }
}

fn args_literal(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string())
}

const PAGE_JS: &str = r#"(() => ({
  title: document.title || "",
  url: location.href,
  text: document.body ? (document.body.innerText || "") : ""
}))()"#;

// `r##` because the script itself contains `"#` (a CSS id selector).
const CLICK_JS: &str = r##"(() => {
  const a = __ARGS__;
  const el = document.querySelector(a.selector);
  if (!el) {
    const candidates = Array.from(document.querySelectorAll("a,button,input,select,textarea,[role=button],[onclick]"))
      .slice(0, 40)
      .map((e) => e.tagName.toLowerCase()
        + (e.id ? "#" + e.id : "")
        + (e.getAttribute("name") ? "[name=" + e.getAttribute("name") + "]" : "")
        + " \"" + String(e.innerText || e.value || e.getAttribute("aria-label") || "").trim().slice(0, 60) + "\"");
    return { ok: false, error: "no element matches " + a.selector, candidates };
  }
  el.scrollIntoView({ block: "center" });
  el.click();
  return { ok: true, tag: el.tagName.toLowerCase(), text: String(el.innerText || el.value || "").trim().slice(0, 120) };
})()"##;

const TYPE_JS: &str = r#"(() => {
  const a = __ARGS__;
  const el = document.querySelector(a.selector);
  if (!el) {
    return { ok: false, error: "no element matches " + a.selector };
  }
  el.scrollIntoView({ block: "center" });
  el.focus();
  const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
  const setter = Object.getOwnPropertyDescriptor(proto, "value");
  if (setter && setter.set) { setter.set.call(el, a.value); } else { el.value = a.value; }
  el.dispatchEvent(new Event("input", { bubbles: true }));
  el.dispatchEvent(new Event("change", { bubbles: true }));
  let submitted = false;
  if (a.submit && el.form) {
    if (el.form.requestSubmit) { el.form.requestSubmit(); } else { el.form.submit(); }
    submitted = true;
  }
  return { ok: true, value: el.value, submitted };
})()"#;

fn click_script(selector: &str) -> String {
    CLICK_JS.replace("__ARGS__", &args_literal(&json!({ "selector": selector })))
}

fn type_script(selector: &str, value: &str, submit: bool) -> String {
    TYPE_JS.replace(
        "__ARGS__",
        &args_literal(&json!({ "selector": selector, "value": value, "submit": submit })),
    )
}

/// Turns a `{ok:false, ...}` result from the click/type scripts into the text
/// the model gets back. A miss names the elements that *are* on the page, which
/// saves the model a round trip guessing at selectors.
fn miss_reason(value: &Value, action: &str) -> Option<String> {
    match value.get("ok").and_then(Value::as_bool) {
        Some(true) => None,
        Some(false) => {
            let mut message = value
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("that did not work")
                .to_string();
            if let Some(candidates) = value.get("candidates").and_then(Value::as_array) {
                if !candidates.is_empty() {
                    message.push_str("\n\nElements on this page:\n");
                    for candidate in candidates.iter().filter_map(Value::as_str) {
                        message.push_str("  ");
                        message.push_str(candidate);
                        message.push('\n');
                    }
                }
            }
            Some(message)
        }
        None => Some(format!("{action} returned an unexpected result")),
    }
}

fn field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn click_outcome(value: &Value, selector: &str) -> Result<String, String> {
    if let Some(reason) = miss_reason(value, "the click") {
        return Err(reason);
    }
    let text = field(value, "text");
    let tag = field(value, "tag");
    Ok(if text.is_empty() {
        format!("Clicked {selector} ({tag}).")
    } else {
        format!("Clicked {selector} ({tag}) — \"{text}\".")
    })
}

fn type_outcome(value: &Value, selector: &str) -> Result<String, String> {
    if let Some(reason) = miss_reason(value, "typing") {
        return Err(reason);
    }
    Ok(format!(
        "Typed into {selector}; it now contains \"{}\".",
        field(value, "value")
    ))
}

fn require<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("{key} is required"))
}

/// Runs one browser tool against the shared session.
pub async fn execute(
    app: &AppHandle,
    state: &AppState,
    chat_id: &str,
    call_id: &str,
    name: &str,
    args: &Value,
    cancel: &Cancel,
) -> Result<String, String> {
    let settings = state.settings.lock().unwrap().clone();
    if !settings.browsing_enabled {
        return Err("browser use is switched off in Settings → Browsing".to_string());
    }

    let mut session = state.browser.session.lock().await;
    if session.is_none() {
        *session = Some(Session::start(&settings.browser_path, &state.paths.browser_dir()).await?);
    }
    let live = session.as_mut().expect("started above");

    let outcome = dispatch(live, app, chat_id, call_id, name, args, cancel).await;

    // A session that errored may be half-dead (the tab closed, the browser
    // crashed). Dropping it means the next call starts a clean one instead of
    // failing forever.
    if outcome.is_err() {
        if let Some(dead) = session.take() {
            dead.stop().await;
        }
    }
    outcome
}

#[allow(clippy::too_many_arguments)]
async fn dispatch(
    session: &mut Session,
    app: &AppHandle,
    chat_id: &str,
    call_id: &str,
    name: &str,
    args: &Value,
    cancel: &Cancel,
) -> Result<String, String> {
    match name {
        "browser_open" => {
            let url = browse::normalise_url(require(args, "url")?);
            session.navigate(&url, cancel).await?;
            Ok(render_page(&session.read_page().await?))
        }
        "browser_extract" => Ok(render_page(&session.read_page().await?)),
        "browser_eval" => {
            let expression = require(args, "expression")?;
            Ok(render_eval(&session.evaluate(expression).await?))
        }
        "browser_click" => {
            let selector = require(args, "selector")?;
            let result = session.evaluate(&click_script(selector)).await?;
            let clicked = click_outcome(&result, selector)?;
            session.wait_ready(cancel).await?;
            Ok(format!(
                "{clicked}\n\n{}",
                render_page(&session.read_page().await?)
            ))
        }
        "browser_type" => {
            let selector = require(args, "selector")?;
            let text = args.get("text").and_then(Value::as_str).unwrap_or("");
            let submit = args.get("submit").and_then(Value::as_bool).unwrap_or(false);
            let result = session.evaluate(&type_script(selector, text, submit)).await?;
            let typed = type_outcome(&result, selector)?;
            if submit {
                session.wait_ready(cancel).await?;
                return Ok(format!(
                    "{typed} The form was submitted.\n\n{}",
                    render_page(&session.read_page().await?)
                ));
            }
            Ok(typed)
        }
        "browser_screenshot" => session.screenshot(app, chat_id, call_id).await,
        other => Err(format!("unknown browser tool '{other}'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_reports_its_title_address_and_text() {
        let page = page_from_value(&json!({
            "title": "Example",
            "url": "https://example.com/",
            "text": "  hello  "
        }))
        .unwrap();
        let rendered = render_page(&page);
        assert!(rendered.starts_with("Example\nhttps://example.com/"));
        assert!(rendered.trim_end().ends_with("hello"));
    }

    #[test]
    fn a_page_with_no_text_says_so_rather_than_returning_nothing() {
        let page = page_from_value(&json!({ "title": "Canvas", "url": "https://x.test/", "text": "   " }))
            .unwrap();
        let rendered = render_page(&page);
        assert!(rendered.contains("no readable text"), "got: {rendered}");
    }

    #[test]
    fn a_shapeless_evaluate_result_is_an_error_not_a_blank_page() {
        // The scripts always return an object; anything else means the
        // expression ran somewhere unexpected.
        assert!(page_from_value(&json!("just a string")).is_err());
        assert!(page_from_value(&Value::Null).is_err());
        // Missing fields are tolerated — a page mid-load has no title.
        assert!(page_from_value(&json!({})).is_ok());
    }

    #[test]
    fn every_shape_of_evaluate_result_is_renderable() {
        assert_eq!(render_eval(&json!("plain")), "plain");
        assert_eq!(render_eval(&Value::Null), "null");
        assert!(render_eval(&json!({"a": 1})).contains("\"a\""));
        assert!(render_eval(&json!([1, 2])).contains('1'));
    }

    #[test]
    fn a_missing_element_names_what_is_actually_on_the_page() {
        let result = json!({
            "ok": false,
            "error": "no element matches #buy",
            "candidates": ["button#buy-now \"Buy\"", "a \"Terms\""]
        });
        let error = click_outcome(&result, "#buy").unwrap_err();
        assert!(error.contains("no element matches #buy"));
        assert!(error.contains("button#buy-now"), "got: {error}");
    }

    #[test]
    fn a_click_and_a_type_report_what_they_hit() {
        let clicked = json!({ "ok": true, "tag": "button", "text": "Buy now" });
        let summary = click_outcome(&clicked, "#buy").unwrap();
        assert!(summary.contains("button") && summary.contains("Buy now"), "got: {summary}");

        let typed = json!({ "ok": true, "value": "hello", "submitted": false });
        let summary = type_outcome(&typed, "#q").unwrap();
        assert!(summary.contains("hello"), "got: {summary}");

        // An unexpected shape must not read as success.
        assert!(click_outcome(&json!({}), "#x").is_err());
    }

    #[test]
    fn a_selector_cannot_break_out_of_its_script() {
        // A model-invented selector is untrusted text; it must land inside a
        // JSON string literal, never as raw JavaScript.
        let hostile = "a\"); fetch('https://evil.test'); (\"";
        let script = click_script(hostile);

        let literal = args_literal(&json!({ "selector": hostile }));
        assert!(script.contains(&literal), "the selector must be embedded as JSON");
        assert!(
            !script.contains(&format!("\"{hostile}\"")),
            "the raw selector must never appear"
        );
        // The payload still round-trips, so the script really does see it.
        let parsed: Value = serde_json::from_str(&literal).unwrap();
        assert_eq!(parsed["selector"], hostile);
    }

    #[test]
    fn typing_escapes_text_the_same_way_and_carries_the_submit_flag() {
        let hostile = "line\none\"}; alert(1);//";
        let script = type_script("#q", hostile, true);
        let literal = args_literal(&json!({
            "selector": "#q", "value": hostile, "submit": true
        }));
        assert!(script.contains(&literal), "the text must be embedded as JSON");
        // The *raw* text must never appear: its newline and quote would close the
        // string literal and the rest would run as JavaScript.
        assert!(
            !script.contains(&format!("\"{hostile}\"")),
            "the raw text must never appear"
        );
        let parsed: Value = serde_json::from_str(&literal).unwrap();
        assert_eq!(parsed["value"], hostile);
        assert_eq!(parsed["submit"], true);
        // The placeholder is gone, so nothing is left to substitute.
        assert!(!script.contains("__ARGS__"));
    }

    #[test]
    fn required_arguments_are_trimmed_and_emptiness_is_refused() {
        assert_eq!(require(&json!({"url": "  x.test  "}), "url").unwrap(), "x.test");
        assert!(require(&json!({"url": "   "}), "url").is_err());
        assert!(require(&json!({}), "url").is_err());
        assert!(require(&json!({"url": 7}), "url").is_err());
    }

    #[test]
    fn a_long_page_is_cut_to_the_model_budget() {
        let page = Page {
            title: "T".into(),
            url: "https://x.test/".into(),
            text: "a".repeat(MAX_PAGE_CHARS * 2),
        };
        let rendered = render_page(&page);
        assert!(rendered.contains("(page text truncated)"));
        assert!(rendered.len() < MAX_PAGE_CHARS * 2);
    }

    /// The protocol plumbing, against whatever browser this machine has. It
    /// **skips** rather than fails where none is installed, so the suite still
    /// runs on a machine (or CI) without Edge or Chrome. Stays offline: the
    /// page is built with `innerHTML` on `about:blank`, never a real URL.
    #[tokio::test]
    async fn it_drives_a_real_browser_end_to_end() {
        if browse::find_browser("").is_none() {
            eprintln!("no Chromium-family browser installed — skipping the live browser test");
            return;
        }
        let profile = std::env::temp_dir().join(format!("deskseek-browser-{}", uuid::Uuid::new_v4()));
        let cancel = Cancel::new();

        let mut session = match Session::start("", &profile).await {
            Ok(session) => session,
            Err(err) => panic!("could not start the browser: {err}"),
        };

        session.navigate("about:blank", &cancel).await.unwrap();
        session
            .evaluate(
                "document.body.innerHTML = '<h1>Desk</h1><button id=\"go\">Go</button><input id=\"q\">'; true",
            )
            .await
            .unwrap();

        // Reading the page finds what the script put there.
        let page = session.read_page().await.unwrap();
        assert!(page.text.contains("Desk"), "got: {}", page.text);
        assert_eq!(page.url, "about:blank");

        // A click reports what it hit; a miss lists the controls that exist.
        let hit = session.evaluate(&click_script("#go")).await.unwrap();
        assert_eq!(hit["ok"], true, "got: {hit}");
        assert_eq!(hit["tag"], "button");
        let miss = session.evaluate(&click_script("#nope")).await.unwrap();
        assert_eq!(miss["ok"], false);
        assert!(
            miss.get("candidates").and_then(Value::as_array).is_some_and(|c| !c.is_empty()),
            "a miss must list what is on the page: {miss}"
        );

        // Typing goes through the native setter, so quoted text survives and
        // frameworks that watch `value` see the change.
        let typed = session
            .evaluate(&type_script("#q", "hello \"there\"", false))
            .await
            .unwrap();
        assert_eq!(typed["ok"], true, "got: {typed}");
        assert_eq!(typed["value"], "hello \"there\"");

        // And a screenshot really comes back as image data.
        let shot = session
            .call("Page.captureScreenshot", json!({ "format": "jpeg", "quality": 70 }))
            .await
            .unwrap();
        assert!(
            shot.get("data").and_then(Value::as_str).is_some_and(|d| d.len() > 100),
            "no image data came back"
        );

        session.stop().await;
        // Best effort: Chrome can still hold a lock for a moment after exiting.
        let _ = std::fs::remove_dir_all(&profile);
    }
}
