//! User settings, stored as JSON next to the database. Never contains the API
//! key — that lives in the OS keychain.

use std::path::Path;

use serde::{Deserialize, Serialize};

pub const DEFAULT_BASE_URL: &str = "https://api.deepseek.com";
pub const DEFAULT_SYSTEM_PROMPT: &str = "You are DeskSeek, a helpful, concise assistant.";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub base_url: String,
    pub chat_model: String,
    pub coding_model: String,
    pub system_prompt: String,
    pub thinking: bool,
    pub effort: String,
    pub theme: String,
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
            theme: "system".to_string(),
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
