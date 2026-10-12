//! Design systems: a stored style reference the model applies when it builds
//! something.
//!
//! One markdown file per system, in a folder of their own: `<design_dir>/<id>.md`.
//!
//! They used to be skills with a `design-` prefix, living in the skills folder.
//! That worked — the per-chat toggle and the model reaching for one by name came
//! for free — but it meant a design system *was* a skill, so the catalogue the
//! model read and the list the Skills tab showed disagreed about the same data,
//! and every call site had to remember to check the prefix. A folder boundary
//! removes that whole class of bug: a design system is not a skill, so nothing
//! has to know that it is not one.
//!
//! The `design-` prefix stays on the id, because the per-chat switch reuses the
//! `chat_skills` table and the prefix is what keeps an id unambiguous in that
//! shared column.
//!
//! The store itself — the file format, the id from the name, and the guard that
//! stops an id walking out of the folder — is `store::files`, shared with the
//! saved-prompt folder. What is left here is what makes a design system a design
//! system: the prefix, and the draft read out of a project.

use std::path::{Path, PathBuf};

/// One markdown file per system, in a folder of their own.
const PREFIX: &str = "design-";

/// The store: the shared file rules, told what these are called and what their
/// ids look like.
pub const STORE: crate::store::files::Store =
    crate::store::files::Store::new(PREFIX, "design system", "system");

/// A stored style reference: what the model applies when it builds UI.
pub use crate::store::files::Doc as DesignSystem;

/// How much a draft reads. A design-system draft is a starting point, not a copy
/// of the repository.
const MAX_FILES: usize = 400;
/// How many dependencies are listed before the list stops being useful.
const MAX_DEPS: usize = 60;

pub fn is_design(id: &str) -> bool {
    STORE.is_id(id)
}

/// The id a new design system is written under.
pub fn id_for(name: &str) -> String {
    STORE.id_for(name)
}

/// Every stored system, by name.
pub fn list(dir: &Path) -> Vec<DesignSystem> {
    STORE.list(dir)
}

pub fn read(dir: &Path, id: &str) -> Option<DesignSystem> {
    STORE.read(dir, id)
}

/// Writes a system, overwriting one of the same name.
///
/// Overwriting is the point rather than a risk: this is the authoring surface,
/// so Save has to be able to save twice. The id is derived from the name, so
/// saving cannot land on some other system's file.
pub fn write(dir: &Path, name: &str, body: &str) -> Result<String, String> {
    STORE.write(dir, name, body)
}

pub fn remove(dir: &Path, id: &str) -> Result<(), String> {
    STORE.remove(dir, id)
}

fn skip(name: &str) -> bool {
    matches!(
        name,
        "node_modules"
            | "dist"
            | "build"
            | "out"
            | "target"
            | "vendor"
            | "coverage"
            | "venv"
            | ".venv"
            | ".git"
            | ".next"
            | ".turbo"
    )
}

/// The files worth reading, depth-first and bounded, skipping the folders that
/// are never source and hidden ones.
fn collect(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if out.len() >= MAX_FILES {
                return out;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path();
            if path.is_dir() {
                if !skip(&name) && !name.starts_with('.') {
                    stack.push(path);
                }
                continue;
            }
            let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let interesting = matches!(extension, "css" | "scss" | "sass" | "less")
                || name == "package.json"
                || name.starts_with("tailwind.config");
            if interesting {
                out.push(path);
            }
        }
    }
    out
}

/// The declaration at the end of a `;`-separated chunk, so a rule's own selector
/// does not have to be parsed.
fn declaration(chunk: &str) -> &str {
    chunk.rsplit(['{', '}']).next().unwrap_or(chunk).trim()
}

/// `--name: value` declarations, first-seen wins so the order is stable.
fn custom_properties(css: &str, out: &mut Vec<(String, String)>) {
    for chunk in css.split(';') {
        let Some(rest) = declaration(chunk).strip_prefix("--") else {
            continue;
        };
        let Some((name, value)) = rest.split_once(':') else {
            continue;
        };
        let name = format!("--{}", name.trim());
        let value = value.trim().to_string();
        if !value.is_empty() && !out.iter().any(|(n, _)| n == &name) {
            out.push((name, value));
        }
    }
}

fn font_families(css: &str, out: &mut Vec<String>) {
    for chunk in css.split(';') {
        let Some(rest) = declaration(chunk).strip_prefix("font-family") else {
            continue;
        };
        let Some(value) = rest.strip_prefix(':') else {
            continue;
        };
        let value = value.trim();
        if !value.is_empty() && !out.iter().any(|f| f == value) {
            out.push(value.to_string());
        }
    }
}

/// Every dependency this project declares, sorted. The whole list rather than a
/// curated one: which of them are "design" is a judgement the user can make and
/// this cannot.
fn notable_dependencies(package_json: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(package_json) else {
        return Vec::new();
    };
    let mut out: Vec<String> = Vec::new();
    for key in ["dependencies", "devDependencies"] {
        if let Some(map) = value.get(key).and_then(|v| v.as_object()) {
            out.extend(map.keys().cloned());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Draft a design-system reference from what a folder already does: the packages
/// it depends on, the custom properties its stylesheets define, and the font
/// stacks they use.
///
/// A starting point by design — it says so in the text. The point is to save the
/// blank page, not to guess a finished document.
pub fn draft_from_project(folder: &Path) -> String {
    let mut props: Vec<(String, String)> = Vec::new();
    let mut fonts: Vec<String> = Vec::new();
    let mut deps: Vec<String> = Vec::new();
    let mut stylesheets = 0usize;

    for path in collect(folder) {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name == "package.json" {
            deps = notable_dependencies(&text);
        } else if name.starts_with("tailwind.config") {
            if !deps.iter().any(|d| d == "tailwindcss") {
                deps.push("tailwindcss".to_string());
            }
        } else {
            stylesheets += 1;
            custom_properties(&text, &mut props);
            font_families(&text, &mut fonts);
        }
        if props.len() + fonts.len() > 300 {
            break;
        }
    }
    deps.sort();
    deps.dedup();

    let mut out = String::new();
    out.push_str(&format!(
        "> Drafted from `{}` — {stylesheets} stylesheet(s) read. Edit it: this is a starting \
         point, not a finished system.\n\n",
        folder.display()
    ));

    out.push_str("## Typography\n\n");
    if fonts.is_empty() {
        out.push_str("- No `font-family` stacks found; name the families this project uses.\n");
    } else {
        for family in fonts.iter().take(20) {
            out.push_str(&format!("- `{family}`\n"));
        }
    }

    out.push_str("\n## Palette\n\n");
    if props.is_empty() {
        out.push_str("- No CSS custom properties found; list the colours and their roles.\n");
    } else {
        out.push_str("| Token | Value |\n| --- | --- |\n");
        for (name, value) in props.iter().take(120) {
            out.push_str(&format!("| `{name}` | `{value}` |\n"));
        }
    }

    out.push_str("\n## Dependencies\n\n");
    if deps.is_empty() {
        out.push_str("- None found.\n");
    } else {
        for dependency in deps.iter().take(MAX_DEPS) {
            out.push_str(&format!("- `{dependency}`\n"));
        }
    }

    out.push_str(
        "\n## Rules\n\n\
- Spacing follows one scale; state its steps.\n\
- One accent colour, for the primary action and nothing else.\n\
- Body text meets 4.5:1 against its background.\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_prefixed_id_is_a_design_system() {
        assert!(is_design("design-axiom"));
        assert!(!is_design("concise"));
        assert_eq!(id_for("Axiom House Style"), "design-axiom-house-style");
        // A name with nothing sluggable in it still has to produce an id.
        assert!(is_design(&id_for("!!!")));
    }

    #[test]
    fn a_system_survives_a_round_trip() {
        let root = std::env::temp_dir().join(format!("deskseek-design-{}", uuid::Uuid::new_v4()));
        // A newline in a name would let it smuggle in a second heading, so the
        // name is collapsed before it becomes one.
        let id = write(&root, "Axiom\nHouse Style", "## Rules\n\n- One accent.\n").unwrap();
        assert_eq!(id, "design-axiom-house-style");
        let parsed = read(&root, &id).unwrap();
        assert_eq!(parsed.name, "Axiom House Style");
        assert_eq!(parsed.body, "## Rules\n\n- One accent.");
        assert_eq!(parsed.id, "design-axiom-house-style");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_store_is_one_file_per_system() {
        let root = std::env::temp_dir().join(format!("deskseek-design-{}", uuid::Uuid::new_v4()));
        let first = write(&root, "House Style", "# what\n- rules\n").unwrap();
        let second = write(&root, "Docs Theme", "- other\n").unwrap();
        assert_eq!(first, "design-house-style");
        assert_eq!(second, "design-docs-theme");

        // A flat folder of `.md`: one file each, and nothing else counted.
        std::fs::write(root.join("notes.txt"), "not a system").unwrap();
        let listed = list(&root);
        assert_eq!(listed.len(), 2, "{listed:?}");
        assert_eq!(listed[0].name, "Docs Theme", "listed by name");

        let read_back = read(&root, &first).unwrap();
        assert_eq!(read_back.name, "House Style");
        assert_eq!(read_back.body, "# what\n- rules");

        // Saving twice revises rather than duplicating — the authoring surface
        // has to be able to save twice.
        write(&root, "House Style", "- revised\n").unwrap();
        assert_eq!(list(&root).len(), 2);
        assert_eq!(read(&root, &first).unwrap().body, "- revised");

        remove(&root, &first).unwrap();
        assert_eq!(list(&root).len(), 1);
        assert!(read(&root, &first).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_id_that_could_walk_out_of_the_folder_is_refused() {
        let dir = Path::new("/design");
        // The prefix alone is not enough — these are still paths.
        for bad in ["../secret", "design-../secret", "design-a/b", "design-a\\b", "plain", ""] {
            assert!(STORE.file_for(dir, bad).is_none(), "{bad} must not resolve");
            assert!(read(dir, bad).is_none(), "{bad} must not read");
            assert!(remove(dir, bad).is_err(), "{bad} must not delete");
        }
        assert!(STORE.file_for(dir, "design-ok").is_some());
    }

    #[test]
    fn a_draft_reads_what_the_project_already_says() {
        let root = std::env::temp_dir().join(format!("deskseek-design-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("package.json"),
            r#"{"dependencies":{"react":"19"},"devDependencies":{"tailwindcss":"4"}}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("src/app.css"),
            ":root {\n  --accent: #0a7;\n  --radius: 8px;\n  font-family: Inter, sans-serif;\n}\n",
        )
        .unwrap();
        // Never read: the walk skips it, which is what keeps a draft cheap.
        std::fs::create_dir_all(root.join("node_modules/junk")).unwrap();
        std::fs::write(root.join("node_modules/junk/x.css"), "--nope: red;").unwrap();

        let draft = draft_from_project(&root);
        assert!(draft.contains("--accent"), "custom properties are collected");
        assert!(draft.contains("#0a7"), "their values are kept");
        assert!(draft.contains("Inter"), "font stacks are collected");
        assert!(draft.contains("tailwindcss"), "dependencies are listed");
        assert!(draft.contains("react"));
        assert!(!draft.contains("--nope"), "node_modules must not be read");
        assert!(draft.contains("Typography") && draft.contains("Palette"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_empty_folder_still_drafts_a_usable_skeleton() {
        let root = std::env::temp_dir().join(format!("deskseek-design-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let draft = draft_from_project(&root);
        // The headings are the point: they are what turns a blank page into
        // something to fill in.
        for heading in ["Typography", "Palette", "Dependencies", "Rules"] {
            assert!(draft.contains(heading), "missing {heading}");
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}
