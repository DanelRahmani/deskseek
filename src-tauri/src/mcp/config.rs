//! Where MCP servers are configured: a single `mcp.json` in the app data dir.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// One MCP server the user has configured.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServer {
    /// Shown in the UI and used as the middle segment of every tool name.
    pub name: String,
    /// The executable to launch: `npx`, `uvx`, or an absolute path.
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Extra environment variables for the child process, e.g. API tokens.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Run this server's tools without asking when the server marks them
    /// `readOnlyHint`. Off by default — see `manager::class_of_exposed`.
    #[serde(default)]
    pub trust_read_only: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpConfig {
    #[serde(default)]
    pub servers: Vec<McpServer>,
}

/// Reads `mcp.json`. A missing file is an empty config, but a *broken* file is
/// an error the user needs to see — silently starting no servers would look
/// like DeskSeek ignoring their configuration.
pub fn load(path: &Path) -> Result<McpConfig, String> {
    if !path.exists() {
        return Ok(McpConfig::default());
    }
    let body = std::fs::read_to_string(path)
        .map_err(|e| format!("could not read {}: {e}", path.display()))?;
    if body.trim().is_empty() {
        return Ok(McpConfig::default());
    }
    serde_json::from_str(&body)
        .map_err(|e| format!("{} is not valid JSON: {e}", path.display()))
}

pub fn save(path: &Path, config: &McpConfig) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let body = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    std::fs::write(path, body).map_err(|e| format!("could not write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_an_empty_config() {
        let dir = std::env::temp_dir().join("deskseek-mcp-missing");
        let _ = std::fs::remove_dir_all(&dir);
        let config = load(&dir.join("mcp.json")).unwrap();
        assert!(config.servers.is_empty());
    }

    #[test]
    fn enabled_defaults_to_true_but_trust_is_opt_in() {
        let body = r#"{"servers":[{"name":"files","command":"npx"}]}"#;
        let config: McpConfig = serde_json::from_str(body).unwrap();
        assert!(config.servers[0].enabled);
        assert!(!config.servers[0].trust_read_only);
    }

    /// Every config written before HTTP existed omits the new keys, so the
    /// defaults have to keep meaning "a stdio child process".
    #[test]
    fn a_server_without_a_command_is_not_runnable() {
        let body = r#"{"servers":[{"name":"files","command":"npx"}]}"#;
        let server = &serde_json::from_str::<McpConfig>(body).unwrap().servers[0];
        assert_eq!(server.command, "npx");
        assert!(server.args.is_empty());
        assert!(server.env.is_empty());
    }

    #[test]
    fn a_broken_file_reports_the_path_rather_than_vanishing() {
        let dir = std::env::temp_dir().join("deskseek-mcp-broken");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mcp.json");
        std::fs::write(&path, "{ not json").unwrap();
        let err = load(&path).unwrap_err();
        assert!(err.contains("mcp.json"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_round_trip_preserves_every_field() {
        let dir = std::env::temp_dir().join("deskseek-mcp-roundtrip");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("mcp.json");
        let config = McpConfig {
            servers: vec![McpServer {
                name: "git".into(),
                command: "uvx".into(),
                args: vec!["mcp-server-git".into()],
                env: BTreeMap::from([("TOKEN".to_string(), "abc".to_string())]),
                enabled: false,
                trust_read_only: true,
            }],
        };
        save(&path, &config).unwrap();
        let back = load(&path).unwrap();
        assert_eq!(back.servers.len(), 1);
        assert_eq!(back.servers[0].command, "uvx");
        assert_eq!(back.servers[0].args, vec!["mcp-server-git".to_string()]);
        assert_eq!(back.servers[0].env["TOKEN"], "abc");
        assert!(!back.servers[0].enabled);
        assert!(back.servers[0].trust_read_only);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
