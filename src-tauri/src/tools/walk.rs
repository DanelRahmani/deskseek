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

/// Files under `root` whose **name** contains `needle`, case-insensitively.
///
/// The walk is the one above, so the same directories are skipped and the same
/// ceiling applies: a search can never be the thing that hangs the window. The
/// name without its extension matching outright beats a name that starts with
/// the needle, which beats one that merely contains it, and a shorter path beats
/// a longer one inside each rank — so looking for `mod` offers `mod.rs` before
/// `module_helper.rs`, and the shallow one before the same name ten folders
/// down.
pub fn find_named(root: &Path, needle: &str, cap: usize) -> Vec<PathBuf> {
    let needle = needle.to_lowercase();
    let mut hits: Vec<PathBuf> = walk_files(root)
        .into_iter()
        .filter(|path| {
            path.file_name()
                .map(|n| n.to_string_lossy().to_lowercase().contains(&needle))
                .unwrap_or(false)
        })
        .collect();
    hits.sort_by_key(|path| {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        // `mod` should find `mod.rs`, not rank it beside `module_helper.rs`:
        // the name a person types is the stem, and the extension is noise.
        let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(&name);
        let rank = if stem == needle {
            0
        } else if name.starts_with(&needle) {
            1
        } else {
            2
        };
        (rank, path.to_string_lossy().len())
    });
    hits.truncate(cap);
    hits
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

    /// The whole point of the search is reaching a file nobody expanded, so the
    /// depth is what is asserted — not that some names came back.
    #[test]
    fn find_named_reaches_a_file_three_folders_down() {
        let dir = std::env::temp_dir().join(format!("deskseek-find-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("a").join("b").join("c")).unwrap();
        std::fs::create_dir_all(dir.join("node_modules").join("x")).unwrap();
        std::fs::write(dir.join("a").join("b").join("c").join("buried.rs"), "x").unwrap();
        std::fs::write(dir.join("node_modules").join("x").join("buried.rs"), "x").unwrap();

        let hits = find_named(&dir, "buried", 50);
        assert_eq!(hits.len(), 1, "the skipped directory must stay skipped");
        assert!(hits[0].ends_with("c\\buried.rs") || hits[0].ends_with("c/buried.rs"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A needle that merely appears in a longer name must not outrank the file
    /// actually named after it, and the cap must trim the tail rather than the
    /// head of that ranking.
    #[test]
    fn find_named_ranks_an_exact_name_first_and_trims_the_tail() {
        let dir = std::env::temp_dir().join(format!("deskseek-rank-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("deep").join("down").join("here")).unwrap();
        std::fs::write(dir.join("mod.rs"), "x").unwrap();
        std::fs::write(dir.join("module_helper.rs"), "x").unwrap();
        std::fs::write(dir.join("deep").join("down").join("here").join("mod.rs"), "x").unwrap();
        let hits = find_named(&dir, "mod", 10);
        assert_eq!(hits.len(), 3, "every match is offered");
        assert_eq!(hits[0], dir.join("mod.rs"), "an exact name wins, and the shallow one first");
        assert_eq!(
            hits[1],
            dir.join("deep").join("down").join("here").join("mod.rs"),
            "then the exact name that is buried"
        );
        assert_eq!(hits[2], dir.join("module_helper.rs"), "a contains-match is last");

        let capped = find_named(&dir, "mod", 1);
        assert_eq!(capped.len(), 1, "the cap is applied");
        assert_eq!(
            capped[0],
            dir.join("mod.rs"),
            "the cap keeps the best hit, not the first one walked"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
