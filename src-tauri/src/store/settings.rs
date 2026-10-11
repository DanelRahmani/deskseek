//! User settings, stored as JSON next to the database. Never contains the API
//! key — that lives in the OS keychain.

use std::path::Path;

use serde::{Deserialize, Serialize};

pub const DEFAULT_BASE_URL: &str = "https://api.deepseek.com";
pub const DEFAULT_SYSTEM_PROMPT: &str = "You are DeskSeek, a helpful, concise assistant.";

/// A named instruction block that is injected only in matching contexts, so
/// "when you write a document, use this type" does not pollute every chat.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instruction {
    pub id: String,
    pub name: String,
    /// `chat` | `coding` | `both`
    pub scope: String,
    pub body: String,
    pub enabled: bool,
}

fn default_instructions() -> Vec<Instruction> {
    vec![
        Instruction {
            id: "documents".to_string(),
            name: "Document design".to_string(),
            scope: "chat".to_string(),
            body: "When you write or edit a document, follow this house style:\n\
- Body text at 11pt in a serif face; headings in a sans-serif face at 1.2x the body size.\n\
- One blank line between paragraphs, never indentation.\n\
- Write documents as Markdown unless asked for another format.\n\
- Offer to save the result to a file rather than only printing it in the chat."
                .to_string(),
            enabled: true,
        },
        Instruction {
            id: "coding-style".to_string(),
            name: "Coding style".to_string(),
            scope: "coding".to_string(),
            body: "Match the conventions already in the repository — read a neighbouring file \
before inventing a new pattern. Prefer the smallest change that works, and leave one runnable \
check behind for non-trivial logic."
                .to_string(),
            enabled: true,
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub base_url: String,
    pub chat_model: String,
    pub coding_model: String,
    pub system_prompt: String,
    pub thinking: bool,
    pub effort: String,
    /// A theme id from `THEMES` in src/lib/theme.ts. An unknown id resolves to
    /// the first theme rather than failing, so this is free to grow.
    #[serde(default = "default_theme")]
    pub theme: String,
    /// `system` follows the OS; `dark` and `light` override it.
    #[serde(default = "default_theme_mode")]
    pub theme_mode: String,
    /// Accent colour as `#rrggbb`, replacing the theme's own. Empty keeps the
    /// theme's, which is the only value guaranteed to clear the contrast floors.
    #[serde(default)]
    pub accent: String,
    /// UI font family. Empty uses the built-in stack.
    #[serde(default)]
    pub font_ui: String,
    /// Code font family. Empty uses the built-in stack.
    #[serde(default)]
    pub font_code: String,
    /// Base UI font size in px. 0 uses the default.
    #[serde(default)]
    pub font_size: u32,
    /// Suppress transitions and animations.
    #[serde(default)]
    pub reduce_motion: bool,
    /// Folder chat-mode document tools may touch. Empty means the default
    /// `documents` folder inside the app data directory.
    #[serde(default)]
    pub documents_dir: String,
    #[serde(default = "default_instructions")]
    pub instructions: Vec<Instruction>,
    /// Master switch for fetch_url / browser_read / web_search.
    #[serde(default = "default_true")]
    pub browsing_enabled: bool,
    /// Base URL of a SearXNG instance that allows `format=json`. Empty disables
    /// web search but keeps the two fetch tools working.
    #[serde(default)]
    pub searxng_url: String,
    /// Optional explicit path to a Chromium-family browser. Empty means
    /// auto-detect (Edge, then Chrome, then Chromium on PATH).
    #[serde(default)]
    pub browser_path: String,
    /// Permission mode new chats start in: `manual` | `auto` | `skip`.
    #[serde(default = "default_permission_mode")]
    pub permission_mode: String,
    /// Stop a run once it has spent this many dollars. 0 disables the limit.
    /// Checked between rounds, because a loop of rounds is where a bill runs
    /// away — see `conversation::agent::run`.
    #[serde(default)]
    pub cost_limit_usd: f64,
    /// Where shell commands run: `auto` (the strongest rung available) |
    /// `host` | `docker` | `wsl`. Only the first two of those are isolation.
    #[serde(default = "default_sandbox")]
    pub sandbox: String,
    /// Image the Docker rung runs commands in. It must already carry whatever
    /// toolchain the workspace needs — `alpine` is small and starts instantly,
    /// but it has no node, no cargo and no python.
    #[serde(default = "default_sandbox_image")]
    pub sandbox_image: String,
    /// Master switch for reading the desktop (`ui_snapshot`). Off by default:
    /// the accessibility tree is whatever is on screen, and what is on screen
    /// may be private.
    #[serde(default)]
    pub computer_use: bool,
    /// Set once the first-run setup screen has been finished or dismissed.
    #[serde(default)]
    pub setup_done: bool,
}

fn default_permission_mode() -> String {
    "manual".to_string()
}

fn default_sandbox() -> String {
    "auto".to_string()
}

fn default_sandbox_image() -> String {
    "alpine:latest".to_string()
}

fn default_theme() -> String {
    "slate".to_string()
}

fn default_theme_mode() -> String {
    "system".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_string(),
            chat_model: "deepseek-flash".to_string(),
            coding_model: "deepseek-v4-pro".to_string(),
            system_prompt: DEFAULT_SYSTEM_PROMPT.to_string(),
            thinking: true,
            effort: "high".to_string(),
            theme: default_theme(),
            theme_mode: default_theme_mode(),
            accent: String::new(),
            font_ui: String::new(),
            font_code: String::new(),
            font_size: 0,
            reduce_motion: false,
            documents_dir: String::new(),
            instructions: default_instructions(),
            browsing_enabled: true,
            searxng_url: String::new(),
            browser_path: String::new(),
            permission_mode: default_permission_mode(),
            cost_limit_usd: 0.0,
            sandbox: default_sandbox(),
            sandbox_image: default_sandbox_image(),
            computer_use: false,
            setup_done: false,
        }
    }
}

/// Loads settings, falling back to defaults when the file is missing or
/// unreadable — a corrupt config should never stop the app starting.
pub fn load(path: &Path) -> Settings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, settings: &Settings) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(settings).expect("settings are serializable");
    std::fs::write(path, json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_yields_defaults() {
        let s = load(Path::new("/definitely/not/here/settings.json"));
        assert_eq!(s.base_url, DEFAULT_BASE_URL);
        assert_eq!(s.chat_model, "deepseek-flash");
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = std::env::temp_dir().join(format!("deskseek-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        let mut s = Settings::default();
        s.chat_model = "deepseek-v4-pro".to_string();
        s.thinking = false;
        save(&path, &s).unwrap();
        let back = load(&path);
        assert_eq!(back.chat_model, "deepseek-v4-pro");
        assert!(!back.thinking);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_file_falls_back_to_defaults() {
        let dir = std::env::temp_dir().join(format!("deskseek-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(load(&path).effort, "high");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
