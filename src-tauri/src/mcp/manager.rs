//! Starts MCP servers as child processes and exposes their tools to the model.
//!
//! Every tool is renamed `mcp__<server>__<tool>` because the model only sees a
//! flat list of function names, and because two servers may both offer a tool
//! called `search`.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use http::{HeaderName, HeaderValue};
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ContentBlock, ResourceContents, Tool as McpSpec,
};
use rmcp::service::RunningService;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::{which_command, StreamableHttpClientTransport, TokioChildProcess};
use rmcp::{RoleClient, ServiceExt};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter};
use tokio::process::ChildStderr;

use super::config::{self, McpConfig, McpServer};
use crate::api::types::Tool;
use crate::tools::{self, ToolClass};

/// Generous, because `npx -y <package>` may download on first use.
const START_TIMEOUT: Duration = Duration::from_secs(60);
const CALL_TIMEOUT: Duration = Duration::from_secs(120);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(3);

/// Lines kept per server. Enough to see what just happened, not a log file.
const LOG_CAP: usize = 200;
/// Logged arguments are truncated, so one giant call cannot fill the ring.
const LOG_ARG_CHARS: usize = 300;

pub const EVENT_LOG: &str = "mcp-log";
pub const EVENT_READY: &str = "mcp-ready";

type Client = RunningService<RoleClient, ()>;
type JsonObject = serde_json::Map<String, Value>;

/// A tool as the server describes it, before we rename it.
struct Discovered {
    server: String,
    remote: String,
    description: String,
    schema: JsonObject,
    read_only: bool,
}

/// A tool we actually offer the model.
struct Registered {
    server: String,
    remote: String,
    exposed: String,
    description: String,
    schema: JsonObject,
    read_only: bool,
}

impl Discovered {
    fn register(self, exposed: String) -> Registered {
        Registered {
            server: self.server,
            remote: self.remote,
            exposed,
            description: self.description,
            schema: self.schema,
            read_only: self.read_only,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatus {
    pub name: String,
    pub enabled: bool,
    pub running: bool,
    pub tool_count: usize,
    /// Why it is not running, when it is not.
    pub error: Option<String>,
}

/// One line in a server's log: a tool call we made, or something the child wrote
/// to stderr. Servers report failure there and nowhere else, so without this it
/// goes to a console nobody reads.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolLog {
    pub server: String,
    /// RFC3339 local time, which is also the display order within a server.
    pub at: String,
    /// `"call"` or `"stderr"`.
    pub kind: String,
    pub tool: String,
    /// The redacted arguments, calls only.
    pub args: String,
    /// A stderr line, or why the call failed.
    pub detail: String,
    pub ms: u64,
    /// Characters returned, calls only.
    pub size: usize,
    pub ok: bool,
}

/// Per-server ring buffers, plus the window to report through.
///
/// Interior mutability on purpose: a tool call runs under a read lock on the
/// manager, so logging must not force a write lock and serialise every server.
pub struct Logs {
    app: Option<AppHandle>,
    lines: Mutex<HashMap<String, VecDeque<ToolLog>>>,
}

impl Logs {
    fn new(app: Option<AppHandle>) -> Arc<Self> {
        Arc::new(Self {
            app,
            lines: Mutex::new(HashMap::new()),
        })
    }

    fn push(&self, entry: ToolLog) {
        let server = entry.server.clone();
        {
            let mut all = self.lines.lock().unwrap();
            let ring = all.entry(server).or_default();
            if ring.len() == LOG_CAP {
                ring.pop_front();
            }
            ring.push_back(entry.clone());
        }
        if let Some(app) = &self.app {
            let _ = app.emit(EVENT_LOG, entry);
        }
    }

    /// Oldest first, grouped by server — a log reads top to bottom.
    pub fn snapshot(&self) -> Vec<ToolLog> {
        let all = self.lines.lock().unwrap();
        let mut names: Vec<&String> = all.keys().collect();
        names.sort();
        names
            .into_iter()
            .flat_map(|name| all[name].iter().cloned())
            .collect()
    }

    /// Told when discovery finishes, so the UI can say what the model can
    /// actually reach before the first turn is sent.
    fn ready(&self, tool_count: usize, running: usize, failed: usize) {
        if let Some(app) = &self.app {
            let _ = app.emit(
                EVENT_READY,
                serde_json::json!({
                    "toolCount": tool_count,
                    "runningCount": running,
                    "failedCount": failed,
                }),
            );
        }
    }
}

pub struct McpManager {
    path: PathBuf,
    servers: Vec<McpServer>,
    clients: HashMap<String, Client>,
    registry: Vec<Registered>,
    errors: HashMap<String, String>,
    /// Set when `mcp.json` itself could not be read.
    config_error: Option<String>,
    logs: Arc<Logs>,
}

impl McpManager {
    pub fn new(path: PathBuf, app: Option<AppHandle>) -> Self {
        Self {
            path,
            servers: Vec::new(),
            clients: HashMap::new(),
            registry: Vec::new(),
            errors: HashMap::new(),
            config_error: None,
            logs: Logs::new(app),
        }
    }

    /// What has actually happened, oldest first, for the panel's Logs tab.
    pub fn tool_logs(&self) -> Vec<ToolLog> {
        self.logs.snapshot()
    }

    /// Tell the UI what discovery found. Fired after every (re)start, including
    /// the background one at launch, so the badge is right either way.
    fn announce(&self) {
        let status = self.status();
        let failed = status
            .iter()
            .filter(|server| server.enabled && !server.running)
            .count();
        self.logs
            .ready(self.registry.len(), self.clients.len(), failed);
    }

    /// Read the config and start whatever it enables. Called once at startup.
    pub async fn load(&mut self) {
        match config::load(&self.path) {
            Ok(cfg) => {
                self.servers = cfg.servers;
                self.config_error = None;
            }
            Err(err) => {
                self.servers.clear();
                self.config_error = Some(err);
            }
        }
        self.restart_all().await;
    }

    /// Re-read `mcp.json` from disk — the user may have edited it by hand.
    pub async fn reload(&mut self) {
        self.load().await;
    }

    /// Replace the server list and write it to disk, then restart everything.
    pub async fn replace_servers(&mut self, servers: Vec<McpServer>) -> Result<(), String> {
        let cfg = McpConfig {
            servers: servers.clone(),
        };
        config::save(&self.path, &cfg)?;
        self.servers = servers;
        self.config_error = None;
        self.restart_all().await;
        Ok(())
    }

    /// Restart one server, leaving the others alone.
    pub async fn restart_one(&mut self, name: &str) -> Result<(), String> {
        let Some(server) = self
            .servers
            .iter()
            .find(|server| server.name == name)
            .cloned()
        else {
            return Err(format!("no MCP server called '{name}'"));
        };

        self.remove_server(name).await;
        self.errors.remove(name);
        if !server.enabled {
            self.announce();
            return Ok(());
        }

        match start_server(server, self.logs.clone()).await {
            Ok((client, discovered)) => {
                // Names must stay unique against the servers still running.
                let mut used: HashSet<String> =
                    self.registry.iter().map(|t| t.exposed.clone()).collect();
                for tool in discovered {
                    let exposed = unique_name(&tool.server, &tool.remote, &mut used);
                    self.registry.push(tool.register(exposed));
                }
                self.clients.insert(name.to_string(), client);
                self.announce();
                Ok(())
            }
            Err(err) => {
                self.errors.insert(name.to_string(), err.clone());
                self.announce();
                Err(err)
            }
        }
    }

    pub async fn restart_all(&mut self) {
        self.stop_all().await;
        self.registry.clear();
        self.errors.clear();

        // An enabled server with nothing to start is recorded as an error rather
        // than dropped: "not running" with no reason reads as a bug, and the UI
        // uses this to tell "still starting" from "finished".
        let mut servers: Vec<McpServer> = Vec::new();
        for server in self.servers.iter().filter(|server| server.enabled) {
            if server.is_remote() || !server.command.trim().is_empty() {
                servers.push(server.clone());
            } else {
                self.errors
                    .insert(server.name.clone(), "neither a command nor a url".to_string());
            }
        }

        // Concurrently, because `npx` may be downloading several packages.
        let logs = self.logs.clone();
        let started = futures_util::future::join_all(servers.into_iter().map(|server| {
            let name = server.name.clone();
            let logs = logs.clone();
            async move { (name, start_server(server, logs).await) }
        }))
        .await;

        let mut used: HashSet<String> = HashSet::new();
        for (name, outcome) in started {
            match outcome {
                Ok((client, discovered)) => {
                    for tool in discovered {
                        let exposed = unique_name(&tool.server, &tool.remote, &mut used);
                        self.registry.push(tool.register(exposed));
                    }
                    self.clients.insert(name, client);
                }
                Err(err) => {
                    self.errors.insert(name, err);
                }
            }
        }
        self.announce();
    }

    /// Stop every server. Called on exit so no child outlives the app.
    pub async fn stop_all(&mut self) {
        let clients: Vec<Client> = self.clients.drain().map(|(_, client)| client).collect();
        // Concurrently: each close waits up to CLOSE_TIMEOUT for the child to die.
        futures_util::future::join_all(clients.into_iter().map(|mut client| async move {
            let _ = client.close_with_timeout(CLOSE_TIMEOUT).await;
        }))
        .await;
    }

    async fn remove_server(&mut self, name: &str) {
        if let Some(mut client) = self.clients.remove(name) {
            let _ = client.close_with_timeout(CLOSE_TIMEOUT).await;
        }
        self.registry.retain(|tool| tool.server != name);
    }

    pub fn status(&self) -> Vec<ServerStatus> {
        let mut out: Vec<ServerStatus> = self
            .servers
            .iter()
            .map(|server| ServerStatus {
                name: server.name.clone(),
                enabled: server.enabled,
                running: self.clients.contains_key(&server.name),
                tool_count: self
                    .registry
                    .iter()
                    .filter(|tool| tool.server == server.name)
                    .count(),
                error: self.errors.get(&server.name).cloned(),
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    pub fn serviced_count(&self) -> usize {
        self.clients.len()
    }

    /// The configured servers as written, for the editor UI.
    pub fn configured_servers(&self) -> Vec<McpServer> {
        self.servers.clone()
    }

    pub fn tool_count(&self) -> usize {
        self.registry.len()
    }

    pub fn config_error(&self) -> Option<String> {
        self.config_error.clone()
    }

    /// The tool specs to offer the model, each with the approval class it needs.
    pub fn tool_specs(&self) -> Vec<(Tool, ToolClass)> {
        self.registry
            .iter()
            .map(|tool| {
                let description = if tool.description.trim().is_empty() {
                    format!("Tool '{}' provided by the MCP server '{}'.", tool.remote, tool.server)
                } else {
                    format!("{} (from the MCP server '{}')", tool.description.trim(), tool.server)
                };
                (
                    Tool::function(
                        &tool.exposed,
                        &description,
                        Value::Object(tool.schema.clone()),
                    ),
                    self.class_of_tool(tool),
                )
            })
            .collect()
    }

    /// The approval class for a tool we offered, or `None` if it is not ours.
    pub fn class_of(&self, exposed: &str) -> Option<ToolClass> {
        self.registry
            .iter()
            .find(|tool| tool.exposed == exposed)
            .map(|tool| self.class_of_tool(tool))
    }

    /// MCP tools always ask for approval unless the user explicitly opted in to
    /// trusting this server's `readOnlyHint`.
    ///
    /// `rmcp`'s own guidance is that annotations come from an untrusted peer and
    /// must not drive tool-use decisions, so the default is to prompt and the
    /// shortcut has to be granted per server.
    fn class_of_tool(&self, tool: &Registered) -> ToolClass {
        let trusted = self
            .servers
            .iter()
            .find(|server| server.name == tool.server)
            .is_some_and(|server| server.trust_read_only);
        if trusted && tool.read_only {
            ToolClass::Read
        } else {
            ToolClass::Command
        }
    }

    /// Calls a tool by the name we exposed. `args` is the model's JSON object.
    pub async fn call(&self, exposed: &str, args: &Value) -> Result<String, String> {
        let tool = self
            .registry
            .iter()
            .find(|tool| tool.exposed == exposed)
            .ok_or_else(|| format!("'{exposed}' is not a tool from any running MCP server"))?;
        let client = self
            .clients
            .get(&tool.server)
            .ok_or_else(|| format!("the MCP server '{}' is not running", tool.server))?;

        let mut params = CallToolRequestParams::new(tool.remote.clone());
        if let Value::Object(map) = args {
            if !map.is_empty() {
                params = params.with_arguments(map.clone());
            }
        }

        let started = Instant::now();
        let outcome = match tokio::time::timeout(CALL_TIMEOUT, client.call_tool(params)).await {
            Err(_) => Err(format!(
                "the MCP server '{}' did not answer within {}s",
                tool.server,
                CALL_TIMEOUT.as_secs()
            )),
            Ok(Err(e)) => Err(format!("the MCP server '{}' failed: {e}", tool.server)),
            Ok(Ok(result)) => Ok(format_result(&result)),
        };

        self.logs.push(ToolLog {
            server: tool.server.clone(),
            at: now(),
            kind: "call".to_string(),
            tool: tool.remote.clone(),
            args: redact_args(args),
            detail: outcome.as_ref().err().cloned().unwrap_or_default(),
            ms: started.elapsed().as_millis() as u64,
            size: outcome.as_ref().map(String::len).unwrap_or(0),
            ok: outcome.is_ok(),
        });
        outcome
    }
}

async fn start_server(
    server: McpServer,
    logs: Arc<Logs>,
) -> Result<(Client, Vec<Discovered>), String> {
    let name = server.name.clone();
    let attempt = async {
        let client = connect(&server, &logs).await?;
        let specs = client
            .list_all_tools()
            .await
            .map_err(|e| format!("could not list its tools: {e}"))?;

        let discovered = specs
            .into_iter()
            .map(|spec| discovered_from(spec, &name))
            .collect();
        Ok::<_, String>((client, discovered))
    };

    match tokio::time::timeout(START_TIMEOUT, attempt).await {
        Ok(outcome) => outcome,
        Err(_) => Err(format!(
            "did not finish starting within {}s",
            START_TIMEOUT.as_secs()
        )),
    }
}

/// The transport half: stdio for a local server, streamable HTTP for a remote
/// one.
async fn connect(server: &McpServer, logs: &Arc<Logs>) -> Result<Client, String> {
    if server.is_remote() {
        return connect_http(server).await;
    }
    // `which_command` resolves Windows shims like `npx.cmd`, which
    // `tokio::process::Command` alone will not find.
    let mut command = which_command(&server.command)
        .map_err(|e| format!("'{}' was not found: {e}", server.command))?;
    command.args(&server.args);
    for (key, value) in &server.env {
        command.env(key, value);
    }

    // stderr is piped rather than inherited: it is where an MCP server says why
    // it fell over, and inheriting it puts that in a console nobody reads.
    let (transport, stderr) = TokioChildProcess::builder(command)
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not launch '{}': {e}", server.command))?;
    if let Some(stderr) = stderr {
        let logs = logs.clone();
        let name = server.name.clone();
        tokio::spawn(async move { read_stderr(stderr, name, logs).await });
    }

    ().serve(transport)
        .await
        .map_err(|e| format!("MCP handshake failed: {e}"))
}

/// A remote server is spoken to over HTTP, so there is no child to spawn and no
/// stderr to read: it explains itself in status codes instead.
///
/// Headers come from `mcp.json` and are validated by `http` rather than passed
/// through; rmcp rejects the reserved ones (Accept, Mcp-Session-Id,
/// Last-Event-Id) itself, so a typo surfaces as a clear start error rather than
/// a half-open session. The URL is used verbatim.
async fn connect_http(server: &McpServer) -> Result<Client, String> {
    let url = server.url.as_deref().unwrap_or_default().trim();
    let mut config = StreamableHttpClientTransportConfig::default();
    config.uri = url.into();
    for (name, value) in &server.headers {
        let header = HeaderName::from_bytes(name.trim().as_bytes())
            .map_err(|_| format!("'{name}' is not a valid HTTP header name"))?;
        let value = HeaderValue::from_str(value)
            .map_err(|_| format!("the value of '{name}' is not a valid HTTP header value"))?;
        config.custom_headers.insert(header, value);
    }
    ().serve(StreamableHttpClientTransport::from_config(config))
        .await
        .map_err(|e| format!("MCP handshake failed: {e}"))
}

async fn read_stderr(stderr: ChildStderr, server: String, logs: Arc<Logs>) {
    use tokio::io::{AsyncBufReadExt, BufReader};
    let mut lines = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let text: String = line.chars().take(LOG_ARG_CHARS).collect();
        logs.push(ToolLog {
            server: server.clone(),
            at: now(),
            kind: "stderr".to_string(),
            tool: String::new(),
            args: String::new(),
            detail: text,
            ms: 0,
            size: line.chars().count(),
            ok: true,
        });
    }
}

fn now() -> String {
    chrono::Local::now().to_rfc3339()
}

/// Arguments are shown in the log, so the ones that look like credentials are
/// replaced rather than printed. Deliberately blunt: a missed secret is worse
/// than a redacted word.
fn redact_args(args: &Value) -> String {
    let text = redact(args).to_string();
    if text.chars().count() > LOG_ARG_CHARS {
        let head: String = text.chars().take(LOG_ARG_CHARS).collect();
        format!("{head}…")
    } else {
        text
    }
}

fn redact(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| {
                    let value = if looks_secret(key) {
                        Value::String("***".to_string())
                    } else {
                        redact(value)
                    };
                    (key.clone(), value)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(redact).collect()),
        other => other.clone(),
    }
}

/// Whole words, not substrings: `accessToken` and `api_key` are secrets, but
/// `monkey` is not. camelCase has to be split too — `accesstoken` as one run of
/// alphanumerics matches nothing in the list.
fn looks_secret(key: &str) -> bool {
    let mut words = String::with_capacity(key.len() + 4);
    let mut previous: Option<char> = None;
    for ch in key.chars() {
        if let Some(prev) = previous {
            if ch.is_ascii_uppercase() && (prev.is_ascii_lowercase() || prev.is_ascii_digit()) {
                words.push(' ');
            }
        }
        words.push(ch);
        previous = Some(ch);
    }

    words
        .to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|part| {
            matches!(
                part,
                "token"
                    | "secret"
                    | "password"
                    | "passwd"
                    | "key"
                    | "apikey"
                    | "auth"
                    | "authorization"
                    | "credential"
                    | "credentials"
            )
        })
}

fn discovered_from(spec: McpSpec, server: &str) -> Discovered {
    Discovered {
        server: server.to_string(),
        remote: spec.name.to_string(),
        description: spec.description.map(|text| text.to_string()).unwrap_or_default(),
        schema: (*spec.input_schema).clone(),
        read_only: spec
            .annotations
            .as_ref()
            .and_then(|annotations| annotations.read_only_hint)
            .unwrap_or(false),
    }
}

/// Function names must be alphanumeric, `_` or `-`, so anything else becomes `_`.
fn slug(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

/// `mcp__<server>__<tool>`, kept unique and under the API's 64-character cap.
fn unique_name(server: &str, tool: &str, used: &mut HashSet<String>) -> String {
    let mut base = format!("mcp__{}__{}", slug(server), slug(tool));
    // Leaving room for a `_2` suffix; the string is ASCII by now.
    base.truncate(base.len().min(60));

    let mut candidate = base.clone();
    let mut suffix = 2;
    while !used.insert(candidate.clone()) {
        candidate = format!("{base}_{suffix}");
        suffix += 1;
    }
    candidate
}

/// One string per content block. Images and audio arrive as base64 the model
/// cannot read here, so they are described instead. ponytail: feed them into the
/// multimodal pipeline if a server that returns images turns out to matter.
fn format_blocks(blocks: &[ContentBlock]) -> Vec<String> {
    blocks
        .iter()
        .map(|block| match block {
            ContentBlock::Text(text) => text.text.clone(),
            ContentBlock::Image(image) => format!("[image returned, {}]", image.mime_type),
            ContentBlock::Audio(audio) => format!("[audio returned, {}]", audio.mime_type),
            ContentBlock::Resource(embedded) => match &embedded.resource {
                ResourceContents::TextResourceContents { uri, text, .. } => {
                    format!("[{uri}]\n{text}")
                }
                ResourceContents::BlobResourceContents { uri, .. } => {
                    format!("[binary resource: {uri}]")
                }
                _ => "[resource]".to_string(),
            },
            ContentBlock::ResourceLink(link) => format!("[resource: {}]", link.uri),
            _ => "[unsupported content]".to_string(),
        })
        .collect()
}

fn format_result(result: &CallToolResult) -> String {
    let mut parts = format_blocks(&result.content);

    // Some tools answer only with structured content.
    if parts.iter().all(|part| part.trim().is_empty()) {
        if let Some(structured) = &result.structured_content {
            if let Ok(text) = serde_json::to_string_pretty(structured) {
                parts.push(text);
            }
        }
    }

    let mut text = parts
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    if text.trim().is_empty() {
        text = "(the tool returned nothing)".to_string();
    }
    if result.is_error == Some(true) {
        text = format!("The tool reported an error:\n{text}");
    }
    tools::exec::truncate(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_strip_everything_the_api_rejects() {
        assert_eq!(slug("git-hub"), "git-hub");
        assert_eq!(slug("my server"), "my_server");
        assert_eq!(slug("@scope/pkg"), "_scope_pkg");
        // Non-ASCII becomes `_` but the ASCII letters between them stay.
        assert_eq!(slug("ünïcode"), "_n_code");
        assert!(slug("ünïcode").is_ascii());
    }

    #[test]
    fn exposed_names_are_namespaced_by_server() {
        let mut used = HashSet::new();
        assert_eq!(
            unique_name("github", "search", &mut used),
            "mcp__github__search"
        );
    }

    #[test]
    fn two_servers_offering_the_same_tool_do_not_collide() {
        let mut used = HashSet::new();
        let first = unique_name("fs", "read", &mut used);
        let second = unique_name("fs", "read", &mut used);
        assert_eq!(first, "mcp__fs__read");
        assert_eq!(second, "mcp__fs__read_2");
        assert_ne!(first, second);
    }

    #[test]
    fn a_very_long_tool_name_stays_within_the_api_limit() {
        let mut used = HashSet::new();
        let name = unique_name(&"s".repeat(80), &"t".repeat(80), &mut used);
        assert_eq!(name.len(), 60);
        // The suffix must not push a second long name over the limit either.
        let next = unique_name(&"s".repeat(80), &"t".repeat(80), &mut used);
        assert!(next.len() <= 64, "{next} is {} chars", next.len());
    }

    #[test]
    fn text_blocks_pass_through_and_media_is_described() {
        let blocks = vec![
            ContentBlock::text("first"),
            ContentBlock::image("AAAA", "image/png"),
            ContentBlock::text("second"),
        ];
        let parts = format_blocks(&blocks);
        assert_eq!(parts[0], "first");
        assert_eq!(parts[1], "[image returned, image/png]");
        assert_eq!(parts[2], "second");
    }

    #[test]
    fn an_unknown_tool_is_not_mistaken_for_a_builtin() {
        let manager = McpManager::new(PathBuf::from("mcp.json"), None);
        assert!(manager.class_of("read_file").is_none());
        assert!(manager.tool_specs().is_empty());
    }

    #[test]
    fn everything_prompts_unless_the_server_is_explicitly_trusted() {
        let mut manager = McpManager::new(PathBuf::from("mcp.json"), None);
        manager.servers = vec![McpServer {
            name: "fs".into(),
            command: "npx".into(),
            args: vec![],
            env: Default::default(),
            url: None,
            headers: Default::default(),
            enabled: true,
            trust_read_only: false,
        }];
        manager.registry = vec![Registered {
            server: "fs".into(),
            remote: "read".into(),
            exposed: "mcp__fs__read".into(),
            description: String::new(),
            schema: JsonObject::new(),
            read_only: true,
        }];

        // The server says read-only, but the user has not opted in.
        assert_eq!(manager.class_of("mcp__fs__read"), Some(ToolClass::Command));

        manager.servers[0].trust_read_only = true;
        assert_eq!(manager.class_of("mcp__fs__read"), Some(ToolClass::Read));

        // A tool the server does *not* mark read-only still prompts.
        manager.registry[0].read_only = false;
        assert_eq!(manager.class_of("mcp__fs__read"), Some(ToolClass::Command));
    }

    #[test]
    fn logged_arguments_keep_their_shape_but_lose_their_credentials() {
        let args = serde_json::json!({
            "query": "weather",
            "limit": 5,
            "accessToken": "sk-live-123",
            "api_key": "abc",
            "Authorization": "Bearer xyz",
            "nested": { "password": "hunter2", "keep": true },
            "list": ["fine", { "secret": "shh" }],
        });
        let text = redact_args(&args);
        assert!(text.contains("weather"), "{text}");
        assert!(text.contains("limit"));
        assert!(text.contains("nested"));
        assert!(!text.contains("shh"));
        for leak in ["sk-live-123", "abc", "Bearer xyz", "hunter2", "shh"] {
            assert!(!text.contains(leak), "{leak} leaked in {text}");
        }
        assert!(text.contains("***"));
    }

    #[test]
    fn a_word_that_merely_contains_a_secret_name_is_not_redacted() {
        assert!(!looks_secret("monkey"));
        assert!(!looks_secret("hockey_score"));
        assert!(!looks_secret("query"));
        assert!(looks_secret("apiKey"));
        assert!(looks_secret("X-Auth-Token"));
        assert!(looks_secret("DB_PASSWORD"));
    }

    #[test]
    fn long_arguments_are_truncated_rather_than_stored_whole() {
        let args = serde_json::json!({ "body": "x".repeat(LOG_ARG_CHARS * 2) });
        let text = redact_args(&args);
        assert!(text.chars().count() <= LOG_ARG_CHARS + 1, "{} chars", text.chars().count());
        assert!(text.ends_with('…'));
    }

    #[test]
    fn the_log_is_a_ring_not_an_unbounded_history() {
        let logs = Logs::new(None);
        for index in 0..LOG_CAP + 5 {
            logs.push(ToolLog {
                server: "fs".into(),
                at: now(),
                kind: "call".into(),
                tool: format!("tool-{index}"),
                args: String::new(),
                detail: String::new(),
                ms: 1,
                size: 0,
                ok: true,
            });
        }
        let lines = logs.snapshot();
        assert_eq!(lines.len(), LOG_CAP);
        // The oldest went, the newest stayed.
        assert_eq!(lines.first().unwrap().tool, "tool-5");
        assert_eq!(lines.last().unwrap().tool, format!("tool-{}", LOG_CAP + 4));
    }

    #[test]
    fn a_snapshot_groups_servers_and_reads_oldest_first() {
        let logs = Logs::new(None);
        for server in ["beta", "alpha"] {
            for step in 0..2 {
                logs.push(ToolLog {
                    server: server.into(),
                    at: now(),
                    kind: "stderr".into(),
                    tool: String::new(),
                    args: String::new(),
                    detail: format!("{server}-{step}"),
                    ms: 0,
                    size: 1,
                    ok: true,
                });
            }
        }
        let details: Vec<String> = logs.snapshot().into_iter().map(|line| line.detail).collect();
        assert_eq!(details, vec!["alpha-0", "alpha-1", "beta-0", "beta-1"]);
    }
}
