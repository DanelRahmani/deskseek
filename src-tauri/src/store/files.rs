//! A folder of markdown files, one per named thing.
//!
//! Several stores in this app are the same shape: a design system, a saved
//! prompt — a *named* thing kept as one markdown document, listed out of a flat
//! folder, and found by an id derived from its name. There are only a few rules,
//! and one of them is a security rule (an id may never climb out of its folder),
//! so they live here rather than being copied into each store. A second copy of a
//! path guard is how path guards rot.
//!
//! The file format is the markdown one: a `# Name` title, then the body. Not
//! frontmatter, because a person opening the file should see a document, and only
//! the *first* heading is the name so a body may keep headings of its own.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A stored document: what one file in one of these folders holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Doc {
    /// The file's stem. Prefixed when the ids have to share a namespace.
    pub id: String,
    pub name: String,
    pub body: String,
}

/// The three things the folders disagree on, and everything else they share.
pub struct Store {
    /// Prepended to every id. Empty for a folder whose ids need no prefix.
    prefix: &'static str,
    /// What an error calls the thing, for the message a caller gets.
    noun: &'static str,
    /// The slug to fall back on when a name has nothing sluggable in it.
    fallback: &'static str,
}

impl Store {
    pub const fn new(prefix: &'static str, noun: &'static str, fallback: &'static str) -> Self {
        Self {
            prefix,
            noun,
            fallback,
        }
    }

    /// The id a name is stored under — the one place the name → id rule lives.
    pub fn id_for(&self, name: &str) -> String {
        format!(
            "{}{}",
            self.prefix,
            crate::tools::docs::slug_with(name, self.fallback)
        )
    }

    /// Whether an id is one of ours at all. The cheapest of the guards, and the
    /// one callers outside this module can usefully ask about.
    pub fn is_id(&self, id: &str) -> bool {
        id.starts_with(self.prefix) && !id.is_empty()
    }

    /// The file an id names, or `None` when the id could not be one of ours.
    ///
    /// This is the guard, and it is here so that no caller can forget it: a
    /// separator or a `..` is a path, not a name, and would walk out of the
    /// folder.
    pub(crate) fn file_for(&self, dir: &Path, id: &str) -> Option<PathBuf> {
        if !self.is_id(id) || id.contains(['/', '\\']) || id.contains("..") {
            return None;
        }
        Some(dir.join(format!("{id}.md")))
    }

    /// Every document in the folder, by name.
    ///
    /// The folder is flat — one file per document — so this is a single
    /// `read_dir`: no recursion, and no second place a document could hide.
    pub fn list(&self, dir: &Path) -> Vec<Doc> {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return out;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let Ok(raw) = std::fs::read_to_string(&path) else {
                continue;
            };
            out.push(parse(id, &raw));
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    pub fn read(&self, dir: &Path, id: &str) -> Option<Doc> {
        let path = self.file_for(dir, id)?;
        let raw = std::fs::read_to_string(path).ok()?;
        Some(parse(id, &raw))
    }

    /// Writes a document, overwriting one of the same name.
    ///
    /// Overwriting is the point rather than a risk: this is the authoring
    /// surface, so Save has to be able to save twice. The id comes from the name,
    /// so saving cannot land on some other document's file.
    pub fn write(&self, dir: &Path, name: &str, body: &str) -> Result<String, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let id = self.id_for(name);
        let path = self
            .file_for(dir, &id)
            .ok_or_else(|| format!("That is not a {} id.", self.noun))?;
        std::fs::write(path, compose(name, body)).map_err(|e| e.to_string())?;
        Ok(id)
    }

    pub fn remove(&self, dir: &Path, id: &str) -> Result<(), String> {
        let path = self
            .file_for(dir, id)
            .ok_or_else(|| format!("That is not a {} id.", self.noun))?;
        if !path.is_file() {
            return Err(format!("No {} called \"{id}\".", self.noun));
        }
        std::fs::remove_file(path).map_err(|e| e.to_string())
    }
}

/// The file as the app writes one: a title, then the body.
pub fn compose(name: &str, body: &str) -> String {
    // Collapsed to one line, because a newline in a name would let it smuggle in
    // a second heading and shadow the body's own structure.
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    format!("# {name}\n\n{}\n", body.trim())
}

/// The name and body of a stored document.
///
/// The first `# ` heading is the name, because that is what a markdown reader
/// calls the title. Only the *first* one counts, so a body with headings of its
/// own keeps them; a file with no heading falls back to its id, which is what
/// makes a hand-written file work.
pub fn parse(id: &str, raw: &str) -> Doc {
    let text = raw.trim_start_matches('\u{feff}').trim_start();
    if let Some(rest) = text.strip_prefix("# ") {
        let (heading, remainder) = match rest.find('\n') {
            Some(end) => (&rest[..end], &rest[end + 1..]),
            None => (rest, ""),
        };
        let name = heading.trim();
        if !name.is_empty() {
            return Doc {
                id: id.to_string(),
                name: name.to_string(),
                body: remainder.trim().to_string(),
            };
        }
    }
    Doc {
        id: id.to_string(),
        name: id.to_string(),
        body: text.trim().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A store with no prefix, which is what a folder with its own namespace
    /// looks like — and the case a prefix-based guard alone would get wrong.
    const BARE: Store = Store::new("", "prompt", "prompt");

    #[test]
    fn the_name_becomes_the_id_and_the_fallback_catches_the_rest() {
        assert_eq!(BARE.id_for("Review a diff"), "review-a-diff");
        assert_eq!(BARE.id_for("  Trim  me  "), "trim-me");
        // Nothing sluggable still has to produce an id rather than an empty path.
        assert_eq!(BARE.id_for("!!!"), "prompt");
        assert_eq!(BARE.id_for(""), "prompt");
    }

    #[test]
    fn an_empty_prefix_still_refuses_a_path() {
        let dir = Path::new("/prompts");
        // With no prefix to check, the separator and `..` rule is the *only*
        // guard left, which is exactly why it is not the prefix that protects us.
        for bad in ["../secret", "a/b", "a\\b", "..", ""] {
            assert!(BARE.file_for(dir, bad).is_none(), "{bad} must not resolve");
            assert!(BARE.read(dir, bad).is_none(), "{bad} must not read");
            assert!(BARE.remove(dir, bad).is_err(), "{bad} must not delete");
        }
        assert!(BARE.file_for(dir, "review-a-diff").is_some());
        assert!(!BARE.is_id(""));
    }

    #[test]
    fn a_name_with_a_newline_becomes_one_heading() {
        // Otherwise a name could smuggle in a second heading and shadow the
        // body's own structure.
        let raw = compose("Axiom\nHouse Style", "Body\n");
        assert!(raw.starts_with("# Axiom House Style\n"), "got {raw:?}");
        assert_eq!(parse("design-axiom-house-style", &raw).body, "Body");
    }

    #[test]
    fn only_the_first_heading_is_the_name() {
        // The body's own headings are the document's structure, not its title.
        let parsed = parse("design-x", "# House Style\n\n## Palette\n\ntoken | value\n");
        assert_eq!(parsed.name, "House Style");
        assert!(parsed.body.starts_with("## Palette"));
        assert!(parsed.body.contains("token | value"));
    }

    #[test]
    fn a_file_with_no_heading_falls_back_to_its_id() {
        // What makes a hand-written file work: no title, so the id is the name.
        let parsed = parse("design-hand-written", "Just some text.\n");
        assert_eq!(parsed.name, "design-hand-written");
        assert_eq!(parsed.body, "Just some text.");
    }

    #[test]
    fn a_document_survives_a_round_trip_through_its_file() {
        let root = std::env::temp_dir().join(format!("deskseek-store-{}", uuid::Uuid::new_v4()));
        let id = BARE.write(&root, "Review a diff", "Read the diff.\n\nThen look for:\n- bugs\n").unwrap();
        assert_eq!(id, "review-a-diff");
        assert!(root.join("review-a-diff.md").is_file(), "one file, named by id");

        let back = BARE.read(&root, &id).unwrap();
        assert_eq!(back.name, "Review a diff");
        assert!(back.body.starts_with("Read the diff."));
        assert!(back.body.contains("- bugs"), "the body's own list survives");

        // Saving twice revises rather than duplicating.
        BARE.write(&root, "Review a diff", "Shorter.\n").unwrap();
        assert_eq!(BARE.list(&root).len(), 1);
        assert_eq!(BARE.read(&root, &id).unwrap().body, "Shorter.");

        BARE.remove(&root, &id).unwrap();
        assert!(BARE.list(&root).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
