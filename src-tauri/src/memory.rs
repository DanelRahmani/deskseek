//! `memory.md` — facts the assistant should remember across chats. Always read
//! at request time (not cached) so a manual edit takes effect immediately.

use std::io;
use std::path::Path;

use chrono::Local;

pub fn read(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .filter(|s| !s.trim().is_empty())
}

pub fn write(path: &Path, body: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, body)
}

/// Appends `- (YYYY-MM-DD) text` lines, creating the file with a header first.
pub fn append(path: &Path, entries: &[String]) -> io::Result<()> {
    if entries.is_empty() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut body = std::fs::read_to_string(path).unwrap_or_default();
    if body.trim().is_empty() {
        body = "# Memory\n\nFacts DeskSeek should remember across chats.\n".to_string();
    }
    if !body.ends_with('\n') {
        body.push('\n');
    }
    let today = Local::now().format("%Y-%m-%d").to_string();
    for entry in entries {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        body.push_str(&format!("- ({today}) {entry}\n"));
    }
    std::fs::write(path, body)
}

/// The system prompt the summariser is given. Kept here so it is versioned with
/// the code that parses its output.
pub const SUMMARY_SYSTEM_PROMPT: &str = "You maintain a long-term memory file for a user's \
assistant. Given a chat transcript, reply with ONLY a JSON object of the shape \
{\"summary\": string, \"memory\": string[]}.\n\
`summary` is one or two sentences describing what the chat was about.\n\
`memory` lists durable facts worth remembering in future chats: the user's \
preferences, names, ongoing projects, standing constraints. Prefer zero entries \
over weak ones. Each entry is a single self-contained sentence, no bullet \
prefix, no date. Never include secrets, API keys, passwords or private \
credentials. Return valid JSON only.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_creates_the_file_with_a_header_and_dated_entries() {
        let dir = std::env::temp_dir().join(format!("deskseek-mem-{}", uuid::Uuid::new_v4()));
        let path = dir.join("memory.md");
        append(&path, &["User is called Dan.".to_string(), "  ".to_string()]).unwrap();
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.starts_with("# Memory"));
        assert!(body.contains("User is called Dan."));
        // The blank entry was skipped.
        assert_eq!(body.matches("- (").count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn append_keeps_existing_content() {
        let dir = std::env::temp_dir().join(format!("deskseek-mem-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("memory.md");
        std::fs::write(&path, "# Memory\n\n- old fact\n").unwrap();
        append(&path, &["new fact".to_string()]).unwrap();
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.contains("old fact"));
        assert!(body.contains("new fact"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_file_reads_as_none() {
        let dir = std::env::temp_dir().join(format!("deskseek-mem-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("memory.md");
        std::fs::write(&path, "   \n").unwrap();
        assert!(read(&path).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
