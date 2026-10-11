//! A small filesystem walker and glob matcher, so `glob` and `grep` need no
//! extra dependency. Handles `*`, `?` and `**`.

use std::path::{Path, PathBuf};

/// Directories never worth walking in a coding context.
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".venv",
    "venv",
    "__pycache__",
    ".next",
    ".idea",
];

const MAX_FILES: usize = 5_000;

pub fn should_skip(dir_name: &str) -> bool {
    SKIP_DIRS.contains(&dir_name)
}

/// Every file under `root`, skipping noise directories, capped at `MAX_FILES`.
pub fn walk_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(root, &mut out);
    out
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    if out.len() >= MAX_FILES {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if out.len() >= MAX_FILES {
            return;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            if !should_skip(&name) {
                walk(&entry.path(), out);
            }
        } else if kind.is_file() {
            out.push(entry.path());
        }
    }
}

/// Glob match supporting `*` (no `/`), `?` (no `/`) and `**` (anything).
/// A leading `**/` also matches zero directories.
pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    matches_from(&p, &t)
}

fn matches_from(p: &[char], t: &[char]) -> bool {
    // `**/` may match nothing at all.
    if p.len() >= 3 && p[0] == '*' && p[1] == '*' && p[2] == '/' && matches_from(&p[3..], t) {
        return true;
    }
    if p.is_empty() {
        return t.is_empty();
    }
    match p[0] {
        '*' => {
            let double = p.len() > 1 && p[1] == '*';
            let rest = if double { &p[2..] } else { &p[1..] };
            let mut i = 0;
            loop {
                if matches_from(rest, &t[i..]) {
                    return true;
                }
                if i == t.len() {
                    return false;
                }
                if !double && t[i] == '/' {
                    return false;
                }
                i += 1;
            }
        }
        '?' => {
            if t.is_empty() || t[0] == '/' {
                false
            } else {
                matches_from(&p[1..], &t[1..])
            }
        }
        c => {
            if t.is_empty() || t[0] != c {
                false
            } else {
                matches_from(&p[1..], &t[1..])
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_does_not_cross_a_slash() {
        assert!(wildcard_match("*.rs", "main.rs"));
        assert!(wildcard_match("src/*.rs", "src/main.rs"));
        assert!(!wildcard_match("src/*.rs", "src/a/b.rs"));
    }

    #[test]
    fn double_star_crosses_slashes() {
        assert!(wildcard_match("**/*.rs", "a/b/c.rs"));
        assert!(wildcard_match("**/*.rs", "c.rs"), "leading **/ matches zero dirs");
        assert!(wildcard_match("src/**", "src/a/b.rs"));
    }

    #[test]
    fn question_mark_is_a_single_character() {
        assert!(wildcard_match("a?c", "abc"));
        assert!(!wildcard_match("a?c", "ac"));
    }

    #[test]
    fn walk_finds_files_and_skips_noise_directories() {
        let dir = std::env::temp_dir().join(format!("deskseek-walk-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::create_dir_all(dir.join("node_modules").join("x")).unwrap();
        std::fs::write(dir.join("src").join("a.rs"), "a").unwrap();
        std::fs::write(dir.join("node_modules").join("x").join("b.js"), "b").unwrap();
        let files = walk_files(&dir);
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("a.rs"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
