//! Lightweight "skills": markdown files with optional YAML-ish frontmatter.
//!
//! Layout: `<skills_dir>/<id>/SKILL.md`. A skill's body is injected into the
//! system prompt when the skill is enabled for a chat. No code execution — a
//! skill is a reusable instruction, which keeps the trust surface tiny.

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    /// Directory name — the stable identifier.
    pub id: String,
    pub name: String,
    pub description: String,
    pub body: String,
    /// `default: on` in the frontmatter: whether a *new* chat should start with
    /// this skill already switched on. The user's standing choices belong in the
    /// file, next to the skill they are about — this app has one store for these
    /// and it is the skills folder.
    pub default_on: bool,
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
    // Deliberately off unless the file says otherwise: a skill the user added but
    // has not thought about yet must not start changing every new chat.
    let default_on = frontmatter
        .and_then(|fm| frontmatter_field(fm, "default"))
        .is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "on" | "true" | "yes"));
    Skill {
        id: id.to_string(),
        name,
        description,
        body: body.trim().to_string(),
        default_on,
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

/// One line, whitespace collapsed. A name or description is a single line in
/// frontmatter, so a newline in one would let it smuggle in a second field.
fn one_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A SKILL.md as the app writes one: frontmatter the parser can read back, then
/// the instructions.
///
/// Composed here rather than in the tool so the format has exactly one
/// definition — `parse_skill(compose(..))` is the test for that. `default_on` is
/// a parameter rather than a constant because the two callers that *rewrite* a
/// skill (revising one by talking, saving a design system twice) must carry the
/// flag through instead of silently clearing it.
pub fn compose(name: &str, description: &str, default_on: bool, body: &str) -> String {
    let name = one_line(name);
    let description = one_line(description);
    let flag = if default_on { "on" } else { "off" };
    let mut head = format!("---\nname: {name}\n");
    if !description.is_empty() {
        head.push_str(&format!("description: {description}\n"));
    }
    head.push_str(&format!("default: {flag}\n---\n{}\n", body.trim()));
    head
}

/// Flips a skill's `default:` flag in its own file.
///
/// Rewritten line by line rather than recomposed from the parsed parts: a skill
/// file is the user's text, and a toggle must not quietly drop frontmatter the
/// app does not read. Only the block above the closing `---` changes; the body is
/// passed through byte for byte.
pub fn set_default(root: &Path, id: &str, on: bool) -> io::Result<()> {
    let file = root.join(id).join("SKILL.md");
    let raw = std::fs::read_to_string(&file)?;
    let flag = format!("default: {}", if on { "on" } else { "off" });

    let (frontmatter, body) = split_frontmatter(&raw);
    let rewritten = match frontmatter {
        Some(frontmatter) => {
            let mut kept: Vec<&str> = frontmatter
                .trim()
                .lines()
                .filter(|line| !line.trim_start().starts_with("default"))
                .collect();
            kept.push(flag.as_str());
            format!("---\n{}\n---\n{body}", kept.join("\n"))
        }
        // No frontmatter to edit: give the file one, and keep every byte of what
        // was there as the body. The byte-order mark, if the file arrived with
        // one, belongs at the top of the file — not stranded inside the body.
        None => format!("---\n{flag}\n---\n\n{}", raw.trim_start_matches('\u{feff}')),
    };
    std::fs::write(&file, rewritten)
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

// ------------------------------------------------------------ import / export
//
// `skills/<id>/SKILL.md` is not a DeskSeek format — it is the open one, the
// layout Anthropic publishes as Agent Skills and Google builds its own skills
// on. So the way in and the way out are both a byte copy of the *folder*, not a
// re-write through `compose`: a skill that arrived with frontmatter this build
// does not read, or with a reference file beside its instructions, has to come
// back out exactly as it went in. That is the whole interop promise.

/// The outcome of importing a folder: what landed, and what was left alone
/// because a skill of that name is already here. Two lists rather than a count,
/// so the panel can say which.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillImport {
    pub imported: Vec<String>,
    pub skipped: Vec<String>,
}

/// Copies a directory tree, files byte for byte.
///
/// ponytail: a symlink inside a skill folder is followed by `fs::copy`, so it
/// would copy the linked file's bytes rather than the link. That is a
/// deliberate ceiling — the user picked this folder themselves, and a skill that
/// carries a symlink out of its own tree is not a case worth code until it
/// happens.
fn copy_dir(from: &Path, to: &Path) -> io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// The skill directories inside a folder the user pointed at.
///
/// A folder that *is* a skill — it holds a `SKILL.md` — is one skill; otherwise
/// each subdirectory that holds one is a skill. Anything else is a stray file
/// and is skipped, because a `.txt` beside a skill is not a skill.
pub fn skill_dirs(folder: &Path) -> Vec<PathBuf> {
    if folder.join("SKILL.md").is_file() {
        return vec![folder.to_path_buf()];
    }
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(folder) else {
        return out;
    };
    for entry in entries.flatten() {
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false)
            && entry.path().join("SKILL.md").is_file()
        {
            out.push(entry.path());
        }
    }
    out.sort();
    out
}

/// Import every skill in a folder into the store.
///
/// The folder name is the id, because that is the open standard's rule and the
/// only reason an export reproduces the same folder. It is checked with the one
/// path guard this repo has (`store::files::safe_component`), not a second copy
/// of it. A name already installed is *skipped* rather than overwritten:
/// importing twice must never replace a skill the user may have edited, and the
/// caller gets `skipped` so it can say so.
pub fn import_folder(folder: &Path, root: &Path) -> Result<SkillImport, String> {
    let mut out = SkillImport::default();
    for source in skill_dirs(folder) {
        let name = source
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        if !crate::store::files::safe_component(&name) {
            return Err(format!("{name:?} is not a usable skill name."));
        }
        let dir = root.join(&name);
        if dir.exists() {
            out.skipped.push(name);
            continue;
        }
        std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
        copy_dir(&source, &dir).map_err(|e| e.to_string())?;
        out.imported.push(name);
    }
    Ok(out)
}

/// Copy every skill in the store into `dest`, one `<id>/SKILL.md` each — the
/// folder shape Claude, Gemini and this app all read.
pub fn export_all(dest: &Path, root: &Path) -> Result<Vec<String>, String> {
    // Exporting into the store would read and write the same folders at once.
    if dest == root {
        return Err("Pick a folder other than the skills folder.".to_string());
    }
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for skill in discover(root) {
        copy_dir(&root.join(&skill.id), &dest.join(&skill.id))
            .map_err(|e| format!("{}: {e}", skill.id))?;
        out.push(skill.id);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_composed_skill_reads_back_as_itself() {
        let raw = compose("Terse answers", "Keep it short.", false, "Be brief.\nNo filler.");
        let skill = parse_skill("terse-answers", &raw);
        assert_eq!(skill.name, "Terse answers");
        assert_eq!(skill.description, "Keep it short.");
        assert_eq!(skill.body, "Be brief.\nNo filler.");
        assert!(!skill.default_on);

        // A skill with no description is still a skill.
        let bare = compose("Bare", "", false, "Do the thing.");
        assert_eq!(parse_skill("bare", &bare).description, "");
        assert_eq!(parse_skill("bare", &bare).body, "Do the thing.");

        // A name carrying a newline must not become a second frontmatter field.
        let sneaky = compose("ok\ndescription: evil", "real", false, "body");
        assert_eq!(parse_skill("x", &sneaky).description, "real");
    }

    #[test]
    fn the_default_flag_round_trips_through_a_file() {
        let raw = compose("House style", "How to write.", true, "Use short sentences.");
        assert!(parse_skill("house-style", &raw).default_on);

        // Rewriting a skill has to carry the flag: a chat revising a standing
        // preference by talking must not silently turn it off.
        let rewritten = compose("House style", "How to write.", true, "Use very short sentences.");
        assert!(parse_skill("house-style", &rewritten).default_on);

        // A file that never mentions the flag is off, not a parse error.
        assert!(!parse_skill("x", "---\nname: X\n---\nBody.").default_on);
        assert!(!parse_skill("x", "Body only.").default_on);
        // And only the spellings that mean yes are yes, in any case.
        for yes in ["on", "TRUE", "Yes"] {
            let raw = format!("---\nname: X\ndefault: {yes}\n---\nBody.");
            assert!(parse_skill("x", &raw).default_on, "{yes} should mean on");
        }
        for no in ["off", "false", "maybe", ""] {
            let raw = format!("---\nname: X\ndefault: {no}\n---\nBody.");
            assert!(!parse_skill("x", &raw).default_on, "{no:?} should mean off");
        }
    }

    #[test]
    fn toggling_the_default_flag_leaves_the_rest_of_the_file_alone() {
        let dir = std::env::temp_dir().join(format!("deskseek-skill-{}", uuid::Uuid::new_v4()));
        let root = dir.join("skills");
        std::fs::create_dir_all(root.join("house-style")).unwrap();
        // Frontmatter the app does not read, a body with markdown in it, and a
        // line in the body starting with "default" to catch a sloppy filter.
        std::fs::write(
            root.join("house-style").join("SKILL.md"),
            "---\nname: House style\ntags: writing, tone\n---\nUse short sentences.\n\ndefault: this is prose, not a field.\n",
        )
        .unwrap();

        set_default(&root, "house-style", true).unwrap();
        let raw = std::fs::read_to_string(root.join("house-style").join("SKILL.md")).unwrap();
        assert!(parse_skill("house-style", &raw).default_on);
        assert!(raw.contains("tags: writing, tone"), "unknown frontmatter kept: {raw}");
        assert!(
            raw.contains("default: this is prose, not a field."),
            "the body is not frontmatter: {raw}"
        );
        assert_eq!(
            parse_skill("house-style", &raw).body,
            "Use short sentences.\n\ndefault: this is prose, not a field."
        );

        // Flipping it back changes the flag and nothing else.
        set_default(&root, "house-style", false).unwrap();
        let raw = std::fs::read_to_string(root.join("house-style").join("SKILL.md")).unwrap();
        assert!(!parse_skill("house-style", &raw).default_on);
        assert!(raw.contains("tags: writing, tone"));
        assert_eq!(raw.matches("default:").count(), 2, "one field, one prose line: {raw}");

        // A file with no frontmatter at all gains one without losing its text.
        std::fs::create_dir_all(root.join("bare")).unwrap();
        std::fs::write(root.join("bare").join("SKILL.md"), "# Bare\n\nJust a body.\n").unwrap();
        set_default(&root, "bare", true).unwrap();
        let bare = parse_skill(
            "bare",
            &std::fs::read_to_string(root.join("bare").join("SKILL.md")).unwrap(),
        );
        assert!(bare.default_on);
        assert_eq!(bare.body, "# Bare\n\nJust a body.");
        let _ = std::fs::remove_dir_all(&dir);
    }

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

    #[test]
    fn a_skill_folder_round_trips_through_export_and_import_unchanged() {
        let base = std::env::temp_dir().join(format!("deskseek-interop-{}", uuid::Uuid::new_v4()));
        let arrived = base.join("from-claude").join("concise");
        std::fs::create_dir_all(arrived.join("references")).unwrap();
        // Frontmatter this build does not read — `license`, `allowed-tools` —
        // and a file beside SKILL.md: both have to survive a trip through the
        // store byte for byte, which is the interop promise.
        let body = "---\nname: Concise\nlicense: MIT\nallowed-tools: Read, Grep\n---\nBe terse.\n";
        std::fs::write(arrived.join("SKILL.md"), body).unwrap();
        std::fs::write(arrived.join("references").join("style.md"), "no filler\n").unwrap();

        let store = base.join("skills");
        let report = import_folder(&base.join("from-claude"), &store).unwrap();
        assert_eq!(report.imported, vec!["concise".to_string()]);
        assert_eq!(
            std::fs::read_to_string(store.join("concise").join("SKILL.md")).unwrap(),
            body,
            "unknown frontmatter is preserved verbatim"
        );
        assert_eq!(
            std::fs::read_to_string(store.join("concise").join("references").join("style.md"))
                .unwrap(),
            "no filler\n",
            "a file beside the instructions comes with it"
        );
        // And the store reads it as a normal skill.
        let parsed = discover(&store).into_iter().find(|s| s.id == "concise").unwrap();
        assert_eq!(parsed.name, "Concise");

        // Back out again, into the folder shape another tool reads unchanged.
        let out = base.join("exported");
        assert_eq!(export_all(&out, &store).unwrap(), vec!["concise".to_string()]);
        assert_eq!(std::fs::read_to_string(out.join("concise").join("SKILL.md")).unwrap(), body);
        assert!(out.join("concise").join("references").join("style.md").is_file());

        // Importing the same folder twice is a skip, not an overwrite.
        let again = import_folder(&base.join("from-claude"), &store).unwrap();
        assert!(again.imported.is_empty());
        assert_eq!(again.skipped, vec!["concise".to_string()]);
        assert_eq!(
            std::fs::read_to_string(store.join("concise").join("SKILL.md")).unwrap(),
            body,
            "the second import left the first one's bytes alone"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_stray_file_beside_a_skill_is_not_a_skill() {
        let base = std::env::temp_dir().join(format!("deskseek-stray-{}", uuid::Uuid::new_v4()));
        let folder = base.join("dropped");
        std::fs::create_dir_all(folder.join("real")).unwrap();
        std::fs::write(folder.join("notes.txt"), "just notes").unwrap();
        std::fs::write(folder.join("README.md"), "# not a skill").unwrap();
        std::fs::write(folder.join("real").join("SKILL.md"), "---\nname: Real\n---\nBody.").unwrap();

        let store = base.join("skills");
        let report = import_folder(&folder, &store).unwrap();
        assert_eq!(report.imported, vec!["real".to_string()]);
        assert!(!store.join("notes.txt").exists());
        assert!(!store.join("README.md").exists());
        assert_eq!(discover(&store).len(), 1);

        // A folder holding only stray files imports nothing at all.
        let only_stray = base.join("only-stray");
        std::fs::create_dir_all(&only_stray).unwrap();
        std::fs::write(only_stray.join("notes.txt"), "x").unwrap();
        assert!(import_folder(&only_stray, &store).unwrap().imported.is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn the_importer_uses_the_one_path_guard() {
        // The guard lives in store/files.rs; the importer asks it rather than
        // keeping a second copy. A folder named with `..` is what it refuses —
        // and a bare store is exactly the case a prefix check would get wrong.
        for bad in ["../secret", "a/b", "a\\b", "..", ""] {
            assert!(!crate::store::files::safe_component(bad), "{bad} is not a component");
        }
        assert!(crate::store::files::safe_component("concise"));

        let base = std::env::temp_dir().join(format!("deskseek-guard-{}", uuid::Uuid::new_v4()));
        let weird = base.join("a..b");
        std::fs::create_dir_all(&weird).unwrap();
        std::fs::write(weird.join("SKILL.md"), "body").unwrap();
        assert!(
            import_folder(&weird, &base.join("skills")).is_err(),
            "a name carrying `..` must not be imported"
        );
        // Exporting the store into itself is refused, not an infinite copy.
        assert!(export_all(&base, &base).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }
}
