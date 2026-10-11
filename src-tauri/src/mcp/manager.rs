//! Starts MCP servers as child processes and exposes their tools to the model.
//!
//! Every tool is renamed `mcp__<server>__<tool>` because the model only sees a
//! flat list of function names, and because two servers may both offer a tool
//! called `search`.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use rmcp::model::{
    CallToolRequestParams, CallToolResult, ContentBlock, ResourceContents, Tool as McpSpec,
};
use rmcp::service::RunningService;
use rmcp::transport::{which_command, TokioChildProcess};
use rmcp::{RoleClient, ServiceExt};
use serde::Serialize;
use serde_json::Value;

use super::config::{self, McpConfig, McpServer};
use crate::api::types::Tool;
use crate::tools::{self, ToolClass};

/// Generous, because `npx -y <package>` may download on first use.
const START_TIMEOUT: Duration = Duration::from_secs(60);
const CALL_TIMEOUT: Duration = Duration::from_secs(120);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(3);

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

pub struct McpManager {
    path: PathBuf,
    servers: Vec<McpServer>,
    clients: HashMap<String, Client>,
    registry: Vec<Registered>,
    errors: HashMap<String, String>,
    /// Set when `mcp.json` itself could not be read.
    config_error: Option<String>,
}

impl McpManager {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            servers: Vec::new(),
            clients: HashMap::new(),
            registry: Vec::new(),
            errors: HashMap::new(),
            config_error: None,
        }
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
            return Ok(());
        }

        match start_server(server).await {
            Ok((client, discovered)) => {
                // Names must stay unique against the servers still running.
                let mut used: HashSet<String> =
                    self.registry.iter().map(|t| t.exposed.clone()).collect();
                for tool in discovered {
                    let exposed = unique_name(&tool.server, &tool.remote, &mut used);
                    self.registry.push(tool.register(exposed));
                }
                self.clients.insert(name.to_string(), client);
                Ok(())
            }
            Err(err) => {
                self.errors.insert(name.to_string(), err.clone());
                Err(err)
            }
        }
    }

    pub async fn restart_all(&mut self) {
        self.stop_all().await;
        self.registry.clear();
        self.errors.clear();

        let servers: Vec<McpServer> = self
            .servers
            .iter()
            .filter(|server| server.enabled && !server.command.trim().is_empty())
            .cloned()
            .collect();

        // Concurrently, because `npx` may be downloading several packages.
        let started = futures_util::future::join_all(servers.into_iter().map(|server| async move {
            let name = server.name.clone();
            (name, start_server(server).await)
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

        let result = tokio::time::timeout(CALL_TIMEOUT, client.call_tool(params))
            .await
            .map_err(|_| {
                format!(
                    "the MCP server '{}' did not answer within {}s",
                    tool.server,
                    CALL_TIMEOUT.as_secs()
                )
            })?
            .map_err(|e| format!("the MCP server '{}' failed: {e}", tool.server))?;

        Ok(format_result(&result))
    }
}

async fn start_server(server: McpServer) -> Result<(Client, Vec<Discovered>), String> {
    let name = server.name.clone();
    let attempt = async {
        // `which_command` resolves Windows shims like `npx.cmd`, which
        // `tokio::process::Command` alone will not find.
        let mut command = which_command(&server.command)
            .map_err(|e| format!("'{}' was not found: {e}", server.command))?;
        command.args(&server.args);
        for (key, value) in &server.env {
            command.env(key, value);
        }

        let transport = TokioChildProcess::new(command)
            .map_err(|e| format!("could not launch '{}': {e}", server.command))?;
        let client = ()
            .serve(transport)
            .await
            .map_err(|e| format!("MCP handshake failed: {e}"))?;
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
        let manager = McpManager::new(PathBuf::from("mcp.json"));
        assert!(manager.class_of("read_file").is_none());
        assert!(manager.tool_specs().is_empty());
    }

    #[test]
    fn everything_prompts_unless_the_server_is_explicitly_trusted() {
        let mut manager = McpManager::new(PathBuf::from("mcp.json"));
        manager.servers = vec![McpServer {
            name: "fs".into(),
            command: "npx".into(),
            args: vec![],
            env: Default::default(),
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
}
