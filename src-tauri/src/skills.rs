//! Lightweight "skills": markdown files with optional YAML-ish frontmatter.
//!
//! Layout: `<skills_dir>/<id>/SKILL.md`. A skill's body is injected into the
//! system prompt when the skill is enabled for a chat. No code execution — a
//! skill is a reusable instruction, which keeps the trust surface tiny.

use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    /// Directory name — the stable identifier.
    pub id: String,
    pub name: String,
    pub description: String,
    pub body: String,
}

/// Splits `---\nkey: value\n---\nbody` into (frontmatter, body).
fn split_frontmatter(raw: &str) -> (Option<&str>, &str) {
    let trimmed = raw.trim_start_matches('\u{feff}');
    if !trimmed.starts_with("---") {
        return (None, raw);
    }
    let after = &trimmed[3..];
    match after.find("\n---") {
        Some(end) => {
            let fm = &after[..end];
            let rest = &after[end + 4..];
            (Some(fm), rest.trim_start_matches(['\r', '\n']))
        }
        None => (None, raw),
    }
}

fn frontmatter_field(frontmatter: &str, key: &str) -> Option<String> {
    for line in frontmatter.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(key) {
            if let Some(value) = rest.strip_prefix(':') {
                let value = value.trim().trim_matches('"').trim_matches('\'');
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

pub fn parse_skill(id: &str, raw: &str) -> Skill {
    let (frontmatter, body) = split_frontmatter(raw);
    let name = frontmatter
        .and_then(|fm| frontmatter_field(fm, "name"))
        .unwrap_or_else(|| id.to_string());
    let description = frontmatter
        .and_then(|fm| frontmatter_field(fm, "description"))
        .unwrap_or_default();
    Skill {
        id: id.to_string(),
        name,
        description,
        body: body.trim().to_string(),
    }
}

/// Scans `<root>/<id>/SKILL.md` for every skill directory.
pub fn discover(root: &Path) -> Vec<Skill> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let id = entry.file_name().to_string_lossy().to_string();
        let file = entry.path().join("SKILL.md");
        let Ok(raw) = std::fs::read_to_string(&file) else {
            continue;
        };
        out.push(parse_skill(&id, &raw));
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// The directory id a skill file should be installed under.
///
/// Prefers the frontmatter `name`; falls back to the file's own stem, except
/// that a file literally called `SKILL.md` is named after its folder instead —
/// "skill" is not an id. Slugged so it is safe as a directory name.
pub fn id_for(source: &Path, raw: &str) -> String {
    let (frontmatter, _) = split_frontmatter(raw);
    let from_name = frontmatter.and_then(|fm| frontmatter_field(fm, "name"));
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let candidate = match from_name.filter(|n| !n.trim().is_empty()) {
        Some(name) => name,
        None if stem.eq_ignore_ascii_case("skill") => source
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string(),
        None => stem,
    };
    crate::tools::docs::slug_with(&candidate, "skill")
}

const EXAMPLES: &[(&str, &str)] = &[
    (
        "concise",
        "---\nname: Concise\ndescription: Answer in as few words as possible, no preamble.\n---\nBe terse. Skip pleasantries, restatement of the question, and closing offers of\nfurther help. Prefer a short list over a paragraph. Never pad.\n",
    ),
    (
        "explain",
        "---\nname: Explain step by step\ndescription: Walk through reasoning for a beginner.\n---\nAssume the reader is new to the topic. Define jargon the first time it appears.\nWork through the reasoning in order rather than jumping to the conclusion, and\nfinish with one concrete example.\n",
    ),
    (
        "code-reviewer",
        "---\nname: Code reviewer\ndescription: Critique code for correctness, edge cases and clarity.\n---\nWhen the user shares code, review it for: correctness bugs first, then unhandled\nedge cases, then clarity. Quote the offending line and give the fix. Do not\nrewrite the whole file unless asked; do not praise filler.\n",
    ),
];

/// Writes the bundled example skills, but only the first time.
///
/// If the skills directory already exists the user has been here, and a skill
/// they *deleted* must not come back on the next launch — which is why this
/// checks the directory, not each file. Nothing the user edited is rewritten.
pub fn bundle_examples(root: &Path) -> io::Result<()> {
    if root.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(root)?;
    for (id, content) in EXAMPLES {
        let dir = root.join(id);
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("SKILL.md"), content)?;
    }
    Ok(())
}

/// The id a skill that arrived as *text* should live under: the frontmatter
/// `name`, else its first markdown heading, else a plain `skill`.
///
/// There is no file to borrow a name from — pasting is the point — so the text
/// has to name itself. Slugged like every other id, because this becomes a
/// directory name.
pub fn id_from_text(raw: &str) -> String {
    let (frontmatter, body) = split_frontmatter(raw);
    let from_name = frontmatter.and_then(|fm| frontmatter_field(fm, "name"));
    let from_heading = body
        .lines()
        .find_map(|line| line.trim().strip_prefix("# "))
        .map(str::to_string);
    let candidate = from_name
        .filter(|name| !name.trim().is_empty())
        .or(from_heading)
        .unwrap_or_default();
    crate::tools::docs::slug_with(&candidate, "skill")
}

/// Writes the bundled examples that are missing, returning the ids it added.
///
/// The deliberate opposite of `bundle_examples`, which runs once and never
/// again: this one is the user asking, so it works in a directory that already
/// exists — and still refuses to overwrite a skill, because an edited example
/// is the user's now.
pub fn write_missing_examples(root: &Path) -> io::Result<Vec<String>> {
    std::fs::create_dir_all(root)?;
    let mut added = Vec::new();
    for (id, content) in EXAMPLES {
        let dir = root.join(id);
        if dir.join("SKILL.md").exists() {
            continue;
        }
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("SKILL.md"), content)?;
        added.push((*id).to_string());
    }
    Ok(added)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pasted_skill_names_itself_from_the_frontmatter_then_the_heading() {
        // Pasting has no file to borrow a name from, so the text has to say what
        // it is — and whatever it says has to survive being a directory name.
        assert_eq!(
            id_from_text("---\nname: Code Reviewer\n---\nBody."),
            "code-reviewer"
        );
        assert_eq!(id_from_text("# Trip planner\n\nSteps."), "trip-planner");
        // Nothing usable still lands somewhere safe rather than at an empty id.
        assert_eq!(id_from_text("just some notes"), "skill");
        assert_eq!(id_from_text("---\nname: ../evil\n---\nx"), "evil");
    }

    #[test]
    fn the_examples_can_be_put_back_without_clobbering_an_edit() {
        let dir = std::env::temp_dir().join(format!("deskseek-skills-{}", uuid::Uuid::new_v4()));
        bundle_examples(&dir).unwrap();
        std::fs::remove_dir_all(dir.join("explain")).unwrap();
        std::fs::write(dir.join("concise").join("SKILL.md"), "---\nname: Mine\n---\nEdited.").unwrap();

        // Only the missing one comes back, and the edited one is left alone.
        assert_eq!(write_missing_examples(&dir).unwrap(), vec!["explain".to_string()]);
        let skills = discover(&dir);
        assert_eq!(skills.len(), 3);
        let concise = skills.iter().find(|s| s.id == "concise").unwrap();
        assert_eq!(concise.name, "Mine");
        // Asking again has nothing left to do.
        assert!(write_missing_examples(&dir).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_frontmatter_fields_and_body() {
        let raw = "---\nname: Code reviewer\ndescription: Critique code.\n---\nDo the thing.\n";
        let s = parse_skill("code-reviewer", raw);
        assert_eq!(s.name, "Code reviewer");
        assert_eq!(s.description, "Critique code.");
        assert_eq!(s.body, "Do the thing.");
    }

    #[test]
    fn a_file_without_frontmatter_still_loads() {
        let s = parse_skill("plain", "Just instructions.");
        assert_eq!(s.name, "plain");
        assert_eq!(s.description, "");
        assert_eq!(s.body, "Just instructions.");
    }

    #[test]
    fn quoted_values_are_unwrapped() {
        let s = parse_skill("q", "---\nname: \"Quoted\"\n---\nbody");
        assert_eq!(s.name, "Quoted");
    }

    #[test]
    fn bundled_examples_are_discoverable_and_do_not_clobber_edits() {
        let dir = std::env::temp_dir().join(format!("deskseek-skills-{}", uuid::Uuid::new_v4()));
        bundle_examples(&dir).unwrap();
        let skills = discover(&dir);
        assert_eq!(skills.len(), 3);

        // An edit survives a second bundling pass.
        let edited = dir.join("concise").join("SKILL.md");
        std::fs::write(&edited, "---\nname: Mine\n---\nEdited.").unwrap();
        bundle_examples(&dir).unwrap();
        let concise = discover(&dir).into_iter().find(|s| s.id == "concise").unwrap();
        assert_eq!(concise.name, "Mine");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_deleted_example_is_not_resurrected_on_the_next_launch() {
        let dir = std::env::temp_dir().join(format!("deskseek-skills-{}", uuid::Uuid::new_v4()));
        bundle_examples(&dir).unwrap();

        std::fs::remove_dir_all(dir.join("explain")).unwrap();
        bundle_examples(&dir).unwrap();

        let ids: Vec<String> = discover(&dir).into_iter().map(|s| s.id).collect();
        assert!(!ids.contains(&"explain".to_string()), "got: {ids:?}");
        assert!(ids.contains(&"concise".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_skill_id_comes_from_the_name_then_the_file() {
        // Frontmatter wins.
        assert_eq!(id_for(Path::new("anything.md"), "---\nname: Code Reviewer\n---\nbody"), "code-reviewer");
        // Otherwise the file's own stem.
        assert_eq!(id_for(Path::new("My Helper.md"), "no frontmatter"), "my-helper");
        // A file literally called SKILL.md is named after its folder.
        assert_eq!(
            id_for(Path::new("/tmp/tidy-notes/SKILL.md"), "no frontmatter"),
            "tidy-notes"
        );
        // A `SKILL.md` with no folder to borrow a name from still gets a usable
        // id rather than an empty one.
        assert_eq!(id_for(Path::new("SKILL.md"), "no frontmatter"), "skill");
    }
}
