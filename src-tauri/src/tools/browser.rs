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

use std::collections::VecDeque;
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
use super::{MODEL_MAX_SIDE, ToolImage, ToolOutput};

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

/// How much of the console is kept. A page that logs in a loop would otherwise
/// grow this for ever, and a log is only ever read from the end.
const MAX_CONSOLE: usize = 200;
/// How many requests are kept. A network log is unbounded by nature — a page
/// makes thousands — which is the whole reason this is a ring buffer and not a
/// list.
const MAX_NETWORK: usize = 300;

/// One line the page logged, as CDP's `Log` domain describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ConsoleEntry {
    /// `error` | `warning` | `info` | `verbose`.
    level: String,
    /// `javascript` | `network` | `deprecation` | … — where it came from.
    source: String,
    text: String,
}

/// One request the page made, joined with whatever came back for it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct NetEntry {
    id: String,
    method: String,
    url: String,
    /// CDP's resource type: `XHR`, `Document`, `Script`, `Image`…
    kind: String,
    /// The status, once a response arrived. `None` while it is in flight.
    status: Option<i64>,
    /// Set when the request failed outright — a DNS failure, a refused
    /// connection, a cancelled navigation.
    failed: Option<String>,
}

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
    /// The debugging port, kept so the page list can be read over HTTP — the one
    /// thing the WebSocket cannot tell us, because it is attached to one page.
    port: u16,
    /// Which of those pages we are driving. Compared by id rather than by URL,
    /// because two tabs can legitimately be on the same address.
    target_id: String,
    /// Where we are driving, and what the page logged and fetched while we were.
    /// Bounded ring buffers rather than lists: both grow without limit by nature.
    console: VecDeque<ConsoleEntry>,
    network: VecDeque<NetEntry>,
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
        let page = first_page(port).await?;
        let ws = attach(&page.ws_url).await?;

        let mut session = Self {
            child,
            ws,
            next_id: 1,
            port,
            target_id: page.id,
            console: VecDeque::new(),
            network: VecDeque::new(),
        };
        // A failure here costs the console and the network views and nothing
        // else, so it is not worth failing a session over: the page tools work
        // either way.
        session.watch_diagnostics().await;
        Ok(session)
    }

    /// Ask for the two event streams this session collects.
    ///
    /// `Log` covers console output *and* the browser's own complaints — a failed
    /// request, a deprecation, a refused resource — and `Network` covers requests.
    /// `Runtime.consoleAPICalled` deliberately stays off: it duplicates what
    /// `Log` already reports, and a log with every line twice is worse than one
    /// with every line once.
    async fn watch_diagnostics(&mut self) {
        let _ = self.call("Log.enable", json!({})).await;
        let _ = self.call("Network.enable", json!({})).await;
    }

    /// Stop driving one page and start driving another.
    ///
    /// The tab list, the console and the network log are dropped with the old
    /// connection on purpose: they describe a *page*, and a tab switch that kept
    /// them would attribute one page's errors to another.
    async fn switch_to(&mut self, ws_url: &str, target_id: &str) -> Result<(), String> {
        let ws = attach(ws_url).await?;
        let mut previous = std::mem::replace(&mut self.ws, ws);
        let _ = previous.close(None).await;
        self.target_id = target_id.to_string();
        self.console.clear();
        self.network.clear();
        self.watch_diagnostics().await;
        Ok(())
    }

    async fn stop(mut self) {
        let _ = self.ws.close(None).await;
        let _ = self.child.kill().await;
    }

    /// One CDP command. Anything that arrives without a matching `id` is an
    /// event rather than an answer, and is filed into the console or network
    /// buffer on its way past; the caller's mutex guarantees only one command is
    /// ever in flight.
    async fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let payload = json!({ "id": id, "method": method, "params": params }).to_string();

        self.ws
            .send(Message::Text(payload.into()))
            .await
            .map_err(|e| format!("the browser connection was lost: {e}"))?;

        let ws = &mut self.ws;
        // Borrowed apart from `ws` so the pump can keep them while it owns the
        // socket: everything that arrives without our `id` on it is an event, and
        // these two buffers are what the events are for.
        let console = &mut self.console;
        let network = &mut self.network;
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
                    if let Some(method) = value.get("method").and_then(Value::as_str) {
                        collect(method, value.get("params").unwrap_or(&Value::Null), console, network);
                    }
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

    /// A picture of the page.
    ///
    /// One capture serves both readers: the same JPEG is shown in the chat and
    /// handed to the model, which is why the shot is asked for at a size a model
    /// can read rather than at the window size — CDP scales it, so no image library
    /// is needed for the halving and the bytes are a fraction of a full frame.
    ///
    /// The text says the same thing either way, because whether a picture is
    /// actually sent is a property of the endpoint and is decided later: a model
    /// that cannot see reads the page with `browser_extract` or `browser_eval`.
    async fn screenshot(
        &mut self,
        app: &AppHandle,
        chat_id: &str,
        call_id: &str,
    ) -> Result<ToolOutput, String> {
        // The page's own size, so the scale can be worked out. A failure here is
        // not worth losing the screenshot over: the full-size frame is a perfectly
        // good picture, just a bigger one.
        let metrics = self.call("Page.getLayoutMetrics", json!({})).await.ok();
        let mut params = json!({ "format": "jpeg", "quality": 70 });
        let mut note = "the page".to_string();
        if let Some((width, height)) = metrics.as_ref().and_then(viewport_of) {
            let scale = model_scale(width, height);
            if scale < 1.0 {
                params["clip"] =
                    json!({ "x": 0, "y": 0, "width": width, "height": height, "scale": scale });
                params["captureBeyondViewport"] = json!(false);
            }
            note = format!(
                "the page at {}×{}px",
                (width * scale).round() as u32,
                (height * scale).round() as u32
            );
        }
        let result = self.call("Page.captureScreenshot", params).await?;
        let data = result
            .get("data")
            .and_then(Value::as_str)
            .ok_or("the browser returned no image")?;
        let data_url = format!("data:image/jpeg;base64,{data}");
        let _ = app.emit(
            EVENT_SHOT,
            json!({ "chatId": chat_id, "callId": call_id, "dataUrl": data_url }),
        );
        let text = format!(
            "Photographed {note}. The picture is in the chat. If you cannot see images, read the \
             page with `browser_extract`, or ask a narrower question with `browser_eval`."
        );
        Ok(ToolOutput::text(text).with_image(ToolImage {
            data_url,
            note,
        }))
    }
}

/// The page's own size, from `Page.getLayoutMetrics`.
///
/// The visual viewport first, because that is what the user is looking at — a
/// page zoomed in reports a layout viewport several screens wide, and capturing
/// all of it would be a picture of a page nobody is on.
fn viewport_of(metrics: &Value) -> Option<(f64, f64)> {
    for key in ["cssVisualViewport", "visualViewport", "layoutViewport"] {
        let view = &metrics[key];
        // A key CDP did not send is skipped rather than ending the search: an
        // older browser sends only the layout viewport, and a `?` here would abort
        // on the first absent key and lose a size that is right there.
        let (Some(width), Some(height)) =
            (view["clientWidth"].as_f64(), view["clientHeight"].as_f64())
        else {
            continue;
        };
        if width > 0.0 && height > 0.0 {
            return Some((width, height));
        }
    }
    None
}

/// The scale to ask CDP for, so the picture fits `MODEL_MAX_SIDE`.
///
/// A fraction rather than the desktop's whole-number factor, because the browser
/// resamples properly and a clip can be scaled by any amount. Never above 1: a
/// small window is not worth enlarging, and a picture blown up invents detail the
/// model would then read.
fn model_scale(width: f64, height: f64) -> f64 {
    let longest = width.max(height);
    if longest <= MODEL_MAX_SIDE as f64 {
        1.0
    } else {
        (MODEL_MAX_SIDE as f64 / longest).clamp(0.1, 1.0)
    }
}

#[cfg(test)]
mod picture_tests {
    use super::*;

    #[test]
    fn a_page_is_scaled_only_when_it_is_too_big_for_a_model() {
        assert_eq!(model_scale(1280.0, 720.0), 1.0);
        assert_eq!(model_scale(800.0, 600.0), 1.0);
        // Half of 2560 is 1280, which is the whole point of the ceiling.
        assert_eq!(model_scale(2560.0, 1440.0), 0.5);
        assert!(model_scale(5120.0, 2880.0) < 0.3);
        // Never above one: a small page is not worth enlarging, because a picture
        // blown up invents detail the model would then read.
        assert!(model_scale(320.0, 200.0) <= 1.0);
    }

    #[test]
    fn the_page_size_is_read_out_of_the_metrics_cdp_sends() {
        // The visual viewport wins: a page zoomed in reports a layout viewport
        // several screens wide, and capturing all of it would be a picture of a
        // page nobody is looking at.
        let metrics = json!({
            "layoutViewport": { "clientWidth": 4000, "clientHeight": 3000 },
            "cssVisualViewport": { "clientWidth": 1280, "clientHeight": 720 }
        });
        assert_eq!(viewport_of(&metrics), Some((1280.0, 720.0)));

        // Only a layout viewport is still an answer.
        let older = json!({ "layoutViewport": { "clientWidth": 900, "clientHeight": 600 } });
        assert_eq!(viewport_of(&older), Some((900.0, 600.0)));

        // Nothing usable is not an answer, which leaves the full-size frame.
        assert_eq!(viewport_of(&json!({})), None);
        assert_eq!(
            viewport_of(&json!({ "cssVisualViewport": { "clientWidth": 0, "clientHeight": 0 } })),
            None
        );
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

/// One page the browser has open, as its own `/json/list` describes it.
#[derive(Debug, Clone, Deserialize)]
struct Target {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(rename = "webSocketDebuggerUrl", default)]
    ws_url: String,
}

/// A client for the debugging *port*.
///
/// Localhost must never be reached through the user's proxy, which is the whole
/// reason this is built rather than using `state.http`.
fn loopback() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())
}

/// Every page the browser has open, in the order Chrome lists them.
async fn list_pages(port: u16) -> Result<Vec<Target>, String> {
    let http = loopback()?;
    let response = http
        .get(format!("http://127.0.0.1:{port}/json/list"))
        .send()
        .await
        .map_err(|e| format!("could not ask the browser for its pages: {e}"))?;
    let targets = response
        .json::<Vec<Target>>()
        .await
        .map_err(|e| format!("the browser's page list could not be read: {e}"))?;
    Ok(targets
        .into_iter()
        .filter(|target| target.kind == "page" && !target.ws_url.is_empty())
        .collect())
}

/// The first page target. It may not exist for a moment after launch, so this
/// retries rather than failing.
async fn first_page(port: u16) -> Result<Target, String> {
    for _ in 0..20 {
        if let Ok(pages) = list_pages(port).await {
            if let Some(page) = pages.into_iter().next() {
                return Ok(page);
            }
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    Err("the browser opened no page to drive".to_string())
}

/// The tab a `window.open` just made.
///
/// Polled rather than reported: `window.open` returns as soon as the target
/// exists in the browser's own bookkeeping, and the page list is the only place
/// that says so.
async fn wait_for_new_tab(port: u16, before: &[String]) -> Result<Target, String> {
    for _ in 0..20 {
        if let Ok(pages) = list_pages(port).await {
            if let Some(page) = pages.into_iter().find(|page| !before.contains(&page.id)) {
                return Ok(page);
            }
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    Err("the new tab never appeared in the browser's page list".to_string())
}

/// Close one tab, over the same debugging port.
async fn close_target(port: u16, id: &str) -> Result<(), String> {
    loopback()?
        .get(format!("http://127.0.0.1:{port}/json/close/{id}"))
        .send()
        .await
        .map_err(|e| format!("could not close that tab: {e}"))?;
    Ok(())
}

/// The debugger socket for one page target.
async fn attach(
    ws_url: &str,
) -> Result<WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>, String> {
    let (ws, _) = tokio_tungstenite::connect_async(ws_url)
        .await
        .map_err(|e| format!("could not attach to the browser: {e}"))?;
    Ok(ws)
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

/// File one CDP event into the buffer it belongs in.
///
/// A free function because the pump already owns the socket: this takes the two
/// buffers it needs and nothing else.
fn collect(
    method: &str,
    params: &Value,
    console: &mut VecDeque<ConsoleEntry>,
    network: &mut VecDeque<NetEntry>,
) {
    match method {
        "Log.entryAdded" => record_console(console, params),
        "Network.requestWillBeSent" => record_request(network, params),
        "Network.responseReceived" => record_response(network, params),
        "Network.loadingFailed" => record_failure(network, params),
        _ => {}
    }
}

/// Keep the newest `cap` and drop the rest.
fn keep_newest<T>(buffer: &mut VecDeque<T>, cap: usize) {
    while buffer.len() > cap {
        buffer.pop_front();
    }
}

fn field_or(value: &Value, key: &str, fallback: &str) -> String {
    let got = field(value, key);
    if got.is_empty() {
        fallback.to_string()
    } else {
        got
    }
}

/// One `Log.entryAdded`.
///
/// Nothing here is trusted: this runs inside the pump every other browser call
/// depends on, so a shape it does not recognise is dropped rather than allowed
/// to take a session down.
fn record_console(buffer: &mut VecDeque<ConsoleEntry>, params: &Value) {
    let entry = params.get("entry").unwrap_or(&Value::Null);
    let text = entry.get("text").and_then(Value::as_str).unwrap_or("").trim();
    // `Log` fills `text` for every entry it reports — a console message, a failed
    // request, a deprecation. A line with none is a shape we do not know.
    if text.is_empty() {
        return;
    }
    buffer.push_back(ConsoleEntry {
        level: field_or(entry, "level", "log"),
        source: field_or(entry, "source", "javascript"),
        text: text.to_string(),
    });
    keep_newest(buffer, MAX_CONSOLE);
}

/// One `Network.requestWillBeSent`.
fn record_request(buffer: &mut VecDeque<NetEntry>, params: &Value) {
    let id = field(params, "requestId");
    if id.is_empty() {
        return;
    }
    let request = params.get("request").unwrap_or(&Value::Null);
    buffer.push_back(NetEntry {
        id,
        method: field_or(request, "method", "GET"),
        url: field(request, "url"),
        kind: field_or(params, "type", "Other"),
        status: None,
        failed: None,
    });
    keep_newest(buffer, MAX_NETWORK);
}

/// The response for a request already recorded.
///
/// A response whose request is not in the buffer is dropped rather than appended:
/// it happened before `Network.enable`, and a line with no method and no address
/// is not a request anybody can act on.
fn record_response(buffer: &mut VecDeque<NetEntry>, params: &Value) {
    let id = field(params, "requestId");
    let response = params.get("response").unwrap_or(&Value::Null);
    let status = response.get("status").and_then(Value::as_i64);
    let kind = field(params, "type");
    for entry in buffer.iter_mut().rev() {
        if entry.id == id {
            entry.status = status;
            if !kind.is_empty() {
                entry.kind = kind;
            }
            return;
        }
    }
}

/// `Network.loadingFailed` — a request that never came back.
fn record_failure(buffer: &mut VecDeque<NetEntry>, params: &Value) {
    let id = field(params, "requestId");
    let reason = field_or(params, "errorText", "the request failed");
    for entry in buffer.iter_mut().rev() {
        if entry.id == id {
            entry.failed = Some(reason);
            return;
        }
    }
}

/// The console, oldest first, with a note when the start has been dropped.
fn render_console(console: &VecDeque<ConsoleEntry>) -> String {
    if console.is_empty() {
        return "The page has logged nothing since it was opened — and anything it logged before \
                that is gone, because the log starts when DeskSeek attaches. Open the page again \
                to watch it from the beginning."
            .to_string();
    }
    let mut out = String::new();
    if console.len() >= MAX_CONSOLE {
        out.push_str(&format!("(the oldest lines have been dropped; these are the last {MAX_CONSOLE})\n"));
    }
    for entry in console {
        out.push_str(&format!("[{}] {}: {}\n", entry.level, entry.source, entry.text));
    }
    truncate(out)
}

/// The requests, oldest first, each with whatever came back for it.
fn render_network(network: &VecDeque<NetEntry>) -> String {
    if network.is_empty() {
        return "The page has made no requests since it was opened.".to_string();
    }
    let mut out = String::new();
    if network.len() >= MAX_NETWORK {
        out.push_str(&format!(
            "(the oldest requests have been dropped; these are the last {MAX_NETWORK})\n"
        ));
    }
    for entry in network {
        let outcome = match (&entry.failed, entry.status) {
            (Some(reason), _) => format!("FAILED ({reason})"),
            (None, Some(status)) => status.to_string(),
            (None, None) => "pending".to_string(),
        };
        out.push_str(&format!(
            "{}  {} {}  [{}]\n",
            outcome, entry.method, entry.url, entry.kind
        ));
    }
    truncate(out)
}

/// The open tabs, with the one being driven marked.
fn render_tabs(pages: &[Target], current: &str) -> String {
    if pages.is_empty() {
        return "The browser has no page open.".to_string();
    }
    let mut out = String::new();
    for (index, page) in pages.iter().enumerate() {
        let here = if page.id == current { "→" } else { " " };
        let title = page.title.trim();
        let title = if title.is_empty() { "(untitled)" } else { title };
        out.push_str(&format!("{here} {index}: {title}\n     {}\n", page.url.trim()));
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
) -> Result<ToolOutput, String> {
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
) -> Result<ToolOutput, String> {
    // The one tool that gives back a picture rather than a page of text.
    if name == "browser_screenshot" {
        return session.screenshot(app, chat_id, call_id).await;
    }
    // Every other arm answers with words and is left exactly as it was: wrapping
    // the match is a smaller change than teaching fifteen arms a richer type.
    let result: Result<String, String> = match name {
        "browser_open" => {
            let url = browse::normalise_url(require(args, "url")?);
            let new_tab = args.get("new_tab").and_then(Value::as_bool).unwrap_or(false);
            if new_tab {
                // `window.open` rather than `Target.createTarget`: the latter is a
                // browser-level command and this connection is attached to a
                // page, while a popup is something the page itself can do.
                let before: Vec<String> =
                    list_pages(session.port).await?.into_iter().map(|page| page.id).collect();
                let expression = format!(
                    "window.open({}, \"_blank\") !== null",
                    args_literal(&Value::String(url.clone()))
                );
                session.evaluate(&expression).await?;
                let opened = wait_for_new_tab(session.port, &before).await?;
                session.switch_to(&opened.ws_url, &opened.id).await?;
            } else {
                session.navigate(&url, cancel).await?;
            }
            session.wait_ready(cancel).await?;
            Ok(render_page(&session.read_page().await?))
        }
        "browser_tabs" => {
            let pages = list_pages(session.port).await?;
            Ok(render_tabs(&pages, &session.target_id))
        }
        "browser_tab" => {
            let action = args
                .get("action")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("switch");
            let index = args
                .get("index")
                .and_then(|value| match value {
                    // A model that quotes the number is not making a different
                    // request, so both spellings are read rather than one being
                    // silently treated as tab 0.
                    Value::String(text) => text.trim().parse::<u64>().ok(),
                    other => other.as_u64(),
                })
                .ok_or("browser_tab needs the tab's `index`, exactly as browser_tabs listed it")?
                as usize;
            let pages = list_pages(session.port).await?;
            let page = pages.get(index).cloned().ok_or_else(|| {
                format!("there is no tab {index} — browser_tabs lists what is open")
            })?;
            let title = if page.title.trim().is_empty() {
                page.url.clone()
            } else {
                page.title.clone()
            };

            if action == "close" {
                close_target(session.port, &page.id).await?;
                if page.id == session.target_id {
                    // We were driving that one, so the socket is now talking to
                    // nothing. Move to whatever is left rather than leaving the
                    // session attached to a closed page.
                    match list_pages(session.port).await?.into_iter().next() {
                        Some(next) => session.switch_to(&next.ws_url, &next.id).await?,
                        None => {
                            return Err(
                                "that was the last tab, so there is no page left to drive — use \
                                 browser_open to open one"
                                    .to_string(),
                            );
                        }
                    }
                }
                return Ok(format!("Closed \"{title}\".").into());
            }

            if page.id == session.target_id {
                return Ok(format!("Already driving \"{title}\".").into());
            }
            session.switch_to(&page.ws_url, &page.id).await?;
            session.wait_ready(cancel).await?;
            Ok(format!(
                "Switched to \"{title}\".\n\n{}",
                render_page(&session.read_page().await?)
            )
            .into())
        }
        "browser_console" => Ok(render_console(&session.console)),
        "browser_network" => Ok(render_network(&session.network)),
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
                )
                .into());
            }
            Ok(typed)
        }
        other => Err(format!("unknown browser tool '{other}'")),
    };
    result.map(Into::into)
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
    fn a_console_entry_keeps_its_level_and_text() {
        let mut console = VecDeque::new();
        record_console(
            &mut console,
            &json!({
                "entry": { "level": "error", "source": "network", "text": "Failed to load /api/x" }
            }),
        );
        record_console(
            &mut console,
            &json!({ "entry": { "text": "hello" } }),
        );
        // A shape with no text is not a line: it would render as an empty one.
        record_console(&mut console, &json!({ "entry": { "level": "error" } }));
        record_console(&mut console, &json!({}));

        assert_eq!(console.len(), 2);
        let rendered = render_console(&console);
        assert!(rendered.contains("[error] network: Failed to load /api/x"), "got: {rendered}");
        // A missing level and source fall back rather than rendering as blanks.
        assert!(rendered.contains("[log] javascript: hello"), "got: {rendered}");
    }

    #[test]
    fn a_request_and_its_response_become_one_line() {
        let mut network = VecDeque::new();
        record_request(
            &mut network,
            &json!({
                "requestId": "42",
                "type": "XHR",
                "request": { "method": "POST", "url": "https://api.test/v1/items" }
            }),
        );
        // In flight: no status yet, and it says so rather than looking like a 0.
        assert!(render_network(&network).contains("pending"), "got: {}", render_network(&network));

        record_response(
            &mut network,
            &json!({ "requestId": "42", "type": "XHR", "response": { "status": 404 } }),
        );
        assert_eq!(network.len(), 1, "the response must join the request, not add a row");
        let rendered = render_network(&network);
        assert!(rendered.contains("404  POST https://api.test/v1/items  [XHR]"), "got: {rendered}");

        // A response for a request we never saw is dropped rather than appended
        // as a row with no method and no address.
        record_response(&mut network, &json!({ "requestId": "nope", "response": { "status": 200 } }));
        assert_eq!(network.len(), 1);

        // A failure replaces the status, because "it never came back" is the fact.
        record_failure(&mut network, &json!({ "requestId": "42", "errorText": "net::ERR_FAILED" }));
        let rendered = render_network(&network);
        assert!(rendered.contains("FAILED (net::ERR_FAILED)"), "got: {rendered}");
    }

    #[test]
    fn the_buffers_keep_the_newest_and_drop_the_oldest() {
        let mut network = VecDeque::new();
        for n in 0..MAX_NETWORK + 5 {
            record_request(
                &mut network,
                &json!({
                    "requestId": format!("r{n}"),
                    "type": "XHR",
                    "request": { "method": "GET", "url": format!("https://api.test/{n}") }
                }),
            );
        }
        assert_eq!(network.len(), MAX_NETWORK, "an unbounded network log is the whole problem");
        // The five oldest are the ones that went.
        assert_eq!(network.front().map(|entry| entry.id.as_str()), Some("r5"));
        assert!(render_network(&network).contains("oldest requests have been dropped"));
    }

    #[test]
    fn a_page_list_marks_the_one_being_driven() {
        let pages = vec![
            Target {
                kind: "page".to_string(),
                id: "A".to_string(),
                title: "Notepad".to_string(),
                url: "https://a.test/".to_string(),
                ws_url: "ws://127.0.0.1:9/devtools/page/A".to_string(),
            },
            Target {
                kind: "page".to_string(),
                id: "B".to_string(),
                title: "  ".to_string(),
                url: "https://b.test/".to_string(),
                ws_url: "ws://127.0.0.1:9/devtools/page/B".to_string(),
            },
        ];
        let rendered = render_tabs(&pages, "B");
        let marked: Vec<&str> = rendered
            .lines()
            .filter(|line| line.starts_with('→'))
            .collect();
        assert_eq!(marked.len(), 1, "exactly one tab is the current one: {rendered}");
        assert!(marked[0].contains("1: (untitled)"), "got: {rendered}");
        assert!(rendered.contains("0: Notepad"), "got: {rendered}");
        // With nothing open there is no list to misread.
        assert!(render_tabs(&[], "A").contains("no page open"));
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
