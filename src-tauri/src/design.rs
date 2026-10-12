//! Design systems: a stored style reference the model applies when it builds
//! something.
//!
//! Stored as a skill with a `design-` id, deliberately. The app already has one
//! mechanism for "a piece of text that joins the prompt when it is switched on
//! for this chat" — storage, the per-chat toggle, the model reaching for it by
//! name, and deletion all come for free, and there is nothing here to keep in
//! step with them. The prefix is what keeps the two lists apart in the UI; this
//! module is the one definition of it.

use std::path::{Path, PathBuf};

/// Design systems live in the same folder as skills; the id is what tells them
/// apart.
pub const PREFIX: &str = "design-";

/// How much a draft reads. A design-system draft is a starting point, not a copy
/// of the repository.
const MAX_FILES: usize = 400;
/// How many dependencies are listed before the list stops being useful.
const MAX_DEPS: usize = 60;

pub fn is_design(id: &str) -> bool {
    id.starts_with(PREFIX)
}

/// The skill id a new design system is written under.
pub fn id_for(name: &str) -> String {
    format!("{PREFIX}{}", crate::tools::docs::slug_with(name, "system"))
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
    fn only_a_prefixed_skill_is_a_design_system() {
        assert!(is_design("design-axiom"));
        assert!(!is_design("concise"));
        assert_eq!(id_for("Axiom House Style"), "design-axiom-house-style");
        // A name with nothing sluggable in it still has to produce an id.
        assert!(is_design(&id_for("!!!")));
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
