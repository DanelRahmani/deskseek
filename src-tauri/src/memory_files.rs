//! Categorised memory: one markdown file per part of the user's life.
//!
//! The profile is unchanged — `MEMORY.md`, kept short and injected into every
//! request, because identity and standing preferences are needed every time.
//! Everything else used to be a row in `memory_facts`; it is now a plain file
//! under `memory/`, one per category:
//!
//! ```text
//! memory/work.md
//! memory/education.md
//! memory/interests.md
//! ```
//!
//! The reason is the reason `MEMORY.md` is a file at all: the user can open it,
//! read it, fix a wrong line and delete a stale one. A memory store nobody can
//! browse is a memory store nobody trusts. Files are therefore the *source of
//! truth*, not a mirror of a database — the guarantee `MEMORY.md` already keeps,
//! extended to everything else.
//!
//! The cost model is unchanged, and that is deliberate. One file per category
//! does not mean one file *injected* per category: only the profile is always
//! sent, and the rest is looked up per turn by what the user just asked, under
//! the same character budget as before. Splitting the store must not turn a
//! bounded cost into an unbounded one.
//!
//! Retrieval here is a plain term-overlap score rather than bm25 over an FTS5
//! index. That is the price of the files being the truth: an index would have to
//! be invalidated whenever a line was edited by hand, and a store that silently
//! ignores your edits is worse than one that ranks a little less cleverly. At the
//! size a personal memory reaches — tens to a few hundred entries — overlap
//! ranking is not the weak link; the weak link was never being able to see it.

use std::io;
use std::path::{Path, PathBuf};

use chrono::Local;

/// Shortest word worth searching for; `is`, `the` and `a` are noise.
const MIN_TERM_CHARS: usize = 2;
/// Words taken from the user's message. A long paste should not become a query.
const MAX_TERMS: usize = 24;
/// Longest single entry considered, so one rambling line cannot eat the budget.
const MAX_ENTRY_CHARS: usize = 400;

/// A category a durable memory can be filed under.
///
/// The list is fixed rather than free-form on purpose: a model inventing a new
/// file per conversation would produce a directory of near-empty files, and the
/// point of categories is that the same subject always lands in the same place.
/// `other` is the floor for anything that does not fit, which is what makes
/// guessing safe.
pub struct Category {
    pub id: &'static str,
    /// Shown to the model in the guide and in the tool schema, so it can pick.
    pub about: &'static str,
}

pub const CATEGORIES: &[Category] = &[
    Category {
        id: "work",
        about: "the user's job, employer, clients, colleagues or business",
    },
    Category {
        id: "education",
        about: "their studies: degree, courses, exams, deadlines, teachers",
    },
    Category {
        id: "projects",
        about: "things they are building or running, and where those stand",
    },
    Category {
        id: "interests",
        about: "hobbies, sport, media and tastes outside work and study",
    },
    Category {
        id: "people",
        about: "family, friends and colleagues, and who they are to the user",
    },
    Category {
        id: "other",
        about: "anything durable that fits nowhere else — the default",
    },
];

/// The always-injected profile is a category too, but a special one: it is
/// `MEMORY.md` and it is never looked up, because it is always there.
pub const PROFILE: &str = "profile";

pub fn category_ids() -> Vec<&'static str> {
    let mut ids: Vec<&'static str> = CATEGORIES.iter().map(|c| c.id).collect();
    ids.push(PROFILE);
    ids
}

/// Whether `id` names a category this module will write to.
/// The category a name refers to, or `None` for anything the app does not know.
/// Case-insensitive, because "Education" and "education" are one category and a
/// difference in case must not create a second file.
///
/// This is the only way in: callers that just want "yes or no" ask whether it
/// returned `Some`, so there is one matching rule rather than a second one that
/// can disagree about case.
pub fn canonical(id: &str) -> Option<&'static str> {
    CATEGORIES
        .iter()
        .find(|c| c.id.eq_ignore_ascii_case(id.trim()))
        .map(|c| c.id)
}

/// The category a bare, unrecognised or missing value falls back to.
pub const FALLBACK: &str = "other";

/// The one-line description of a category, for the prompt guide.
pub fn about(id: &str) -> &'static str {
    CATEGORIES
        .iter()
        .find(|c| c.id == id)
        .map(|c| c.about)
        .unwrap_or("")
}

/// The file for one category. `project-<slug>` names a project file, which is how
/// a memory learned inside a folder stays inside that folder.
/// Every name the assistant may pass as a category, `profile` first. The tool
/// schema is built from this, so the list it offers cannot drift from the table
/// above — `profile` is included because it is a valid destination, not because it
/// is a category (`canonical` deliberately rejects it).
pub fn category_names() -> Vec<String> {
    std::iter::once(PROFILE.to_string())
        .chain(CATEGORIES.iter().map(|c| c.id.to_string()))
        .collect()
}

pub fn file_for(dir: &Path, category: &str) -> PathBuf {
    dir.join(format!("{}.md", safe_name(category)))
}

/// Categories are model-supplied and become file names, so nothing but letters,
/// digits and dashes may survive: `../work` must not escape the directory.
fn safe_name(category: &str) -> String {
    let cleaned: String = category
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let trimmed = cleaned.trim_matches('-');
    if trimmed.is_empty() {
        FALLBACK.to_string()
    } else {
        trimmed.to_string()
    }
}

/// One remembered line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// `YYYY-MM-DD`, or empty for a line written by hand without one.
    pub date: String,
    pub text: String,
}

impl Entry {
    fn line(&self, category: &str) -> String {
        if self.date.is_empty() {
            format!("- [{category}] {}", self.text)
        } else {
            format!("- [{category}] ({}) {}", self.date, self.text)
        }
    }
}

/// Lowercased, whitespace-collapsed, trailing punctuation removed — the dedup
/// key, unchanged from the tier-2 store so a migrated fact is not re-added.
pub fn normalise(text: &str) -> String {
    let lowered = text.to_lowercase();
    let mut out = String::with_capacity(lowered.len());
    let mut last_space = true;
    for ch in lowered.chars() {
        if ch.is_whitespace() {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
        } else {
            out.push(ch);
            last_space = false;
        }
    }
    out.trim()
        .trim_end_matches(['.', '!', '?', ',', ';', ':', ' '])
        .to_string()
}

/// Parses one file into entries. Anything that is not a dated or bare bullet is
/// left alone — headers, blank lines and hand-written prose are the user's.
pub fn parse(body: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    for line in body.lines() {
        let rest = line.trim().strip_prefix("- ").unwrap_or("").trim_start();
        // A `[category]` prefix is optional and only stripped when it actually
        // looks like one: `- see [1] for details` must keep its text.
        let rest = match (rest.starts_with('['), rest.find(']')) {
            (true, Some(end))
                if rest[1..end]
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-')
                    && end > 1 =>
            {
                rest[end + 1..].trim_start()
            }
            _ => rest,
        };
        if let Some(stripped) = rest.strip_prefix('(') {
            if let Some(end) = stripped.find(')') {
                let date = stripped[..end].trim().to_string();
                let text = stripped[end + 1..].trim().to_string();
                if !text.is_empty() {
                    out.push(Entry { date, text });
                }
                continue;
            }
        }
        let text = rest.trim().to_string();
        if !text.is_empty() {
            out.push(Entry {
                date: String::new(),
                text,
            });
        }
    }
    out
}

/// Reads one category file. A missing file is an empty list, not an error.
pub fn read(dir: &Path, category: &str) -> Vec<Entry> {
    std::fs::read_to_string(file_for(dir, category))
        .map(|body| parse(&body))
        .unwrap_or_default()
}

/// The header a category file gets when it is created.
fn header(category: &str) -> String {
    let title = {
        let mut chars = category.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => category.to_string(),
        }
    };
    let about = about(category);
    if about.is_empty() {
        format!("# {title}\n\nThings DeskSeek remembers about {category}.\n")
    } else {
        format!("# {title}\n\nThings DeskSeek remembers about {about}.\n")
    }
}

/// Appends entries to a category file, skipping anything already there.
///
/// Dedup is by normalised text, the same rule the database used, so a fact
/// proposed twice updates nothing instead of stacking a near-duplicate. Returns
/// how many lines were actually written.
pub fn append(dir: &Path, category: &str, entries: &[String]) -> io::Result<usize> {
    let wanted: Vec<String> = entries
        .iter()
        .map(|e| e.trim().to_string())
        .filter(|e| !e.is_empty())
        .collect();
    if wanted.is_empty() {
        return Ok(0);
    }
    let path = file_for(dir, category);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut body = std::fs::read_to_string(&path).unwrap_or_default();
    if body.trim().is_empty() {
        body = header(category);
    }
    if !body.ends_with('\n') {
        body.push('\n');
    }

    let existing: Vec<String> = parse(&body).iter().map(|e| normalise(&e.text)).collect();
    let today = Local::now().format("%Y-%m-%d").to_string();
    let mut added = 0;
    for text in wanted {
        let key = normalise(&text);
        if key.is_empty() || existing.contains(&key) {
            continue;
        }
        body.push_str(&format!("- ({today}) {text}\n"));
        added += 1;
    }
    if added > 0 {
        std::fs::write(&path, body)?;
    }
    Ok(added)
}

/// Removes the first entry whose text matches, so a line can be forgotten from
/// the panel without the user editing the file by hand. Case-insensitive on the
/// normalised form, which is what the panel shows.
pub fn remove(dir: &Path, category: &str, text: &str) -> io::Result<bool> {
    let path = file_for(dir, category);
    let Ok(body) = std::fs::read_to_string(&path) else {
        return Ok(false);
    };
    let wanted = normalise(text);
    let mut kept: Vec<String> = Vec::new();
    let mut removed = false;
    for line in body.lines() {
        let is_match = !removed
            && parse(&format!("{line}\n"))
                .first()
                .is_some_and(|e| normalise(&e.text) == wanted);
        if is_match {
            removed = true;
            continue;
        }
        kept.push(line.to_string());
    }
    if removed {
        let mut out = kept.join("\n");
        out.push('\n');
        std::fs::write(&path, out)?;
    }
    Ok(removed)
}

/// Every category that currently has a file, in the order `CATEGORIES` declares,
/// with anything else (project files) after it.
pub fn categories_in_use(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = CATEGORIES
        .iter()
        .map(|c| c.id.to_string())
        .filter(|id| file_for(dir, id).exists())
        .collect();
    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut extra: Vec<String> = entries
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.strip_suffix(".md").map(str::to_string)
            })
            .filter(|id| !CATEGORIES.iter().any(|c| c.id == id))
            .collect();
        extra.sort();
        out.extend(extra);
    }
    out
}

/// The project file a workspace owns, or None when the chat has no folder.
///
/// A memory learned inside a project stays inside it — the guarantee the scoped
/// fact store gave, kept by putting the scope in the file name.
pub fn project_category(workspace: Option<&Path>) -> Option<String> {
    let path = workspace?;
    let name = path.file_name()?.to_string_lossy().to_string();
    let slug = safe_name(&name);
    Some(format!("project-{slug}"))
}

/// Where a memory asked for with `requested` should actually be written, given
/// the chat it was learned in.
///
/// Only the profile is honoured in chat mode; everything else goes to the
/// project file when the chat has one, because "which folder was this learned in"
/// is a stronger signal than "which subject is it about" — the same rule the
/// scoped fact store used, so nothing changes for an existing user.
pub fn destination(requested: &str, workspace: Option<&Path>) -> String {
    if requested.trim().eq_ignore_ascii_case(PROFILE) {
        return PROFILE.to_string();
    }
    if let Some(project) = project_category(workspace) {
        return project;
    }
    canonical(requested).unwrap_or(FALLBACK).to_string()
}

/// The words worth searching for, deduped and capped.
fn terms(query: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in query.split(|c: char| !c.is_alphanumeric() && c != '_') {
        if word.chars().count() < MIN_TERM_CHARS {
            continue;
        }
        let word = word.to_lowercase();
        if !out.contains(&word) {
            out.push(word);
        }
        if out.len() == MAX_TERMS {
            break;
        }
    }
    out
}

/// Rank entries against the question, drop anything the profile already says, and
/// stop at the budget.
///
/// Pure, so the ordering and the cap can be pinned without a filesystem — the
/// failure mode here is silent (a memory that simply never surfaces), which is
/// exactly the kind of bug that needs a test rather than a look.
pub fn select(
    candidates: &[(String, Entry)],
    query: &str,
    profile: &str,
    budget_chars: usize,
) -> Option<String> {
    let terms = terms(query);
    if terms.is_empty() {
        return None;
    }
    // Against the whole profile body, not just its bullets: `MEMORY.md` is
    // usually written as `- (date) text` lines, but a hand-edited one is often
    // prose, and a repeated fact is a repeated fact either way.
    let already_said = normalise(profile);
    let lowered: Vec<String> = candidates
        .iter()
        .map(|(_, e)| e.text.to_lowercase())
        .collect();

    // Term weights: a word that appears in every candidate says nothing about
    // which one is meant, so it must not outvote the rare word that does. This is
    // an idf over the candidates in hand rather than a stopword list, which keeps
    // it language-agnostic — the user writes in both English and Dutch.
    let total = candidates.len() as f64;
    let weights: Vec<f64> = terms
        .iter()
        .map(|term| {
            let df = lowered.iter().filter(|text| text.contains(term)).count();
            if df == 0 {
                0.0
            } else {
                (1.0 + total / df as f64).ln()
            }
        })
        .collect();

    let mut scored: Vec<(f64, usize, &Entry, &str)> = Vec::new();
    for ((category, entry), haystack) in candidates.iter().zip(lowered.iter()) {
        if entry.text.chars().count() > MAX_ENTRY_CHARS {
            continue;
        }
        let key = normalise(&entry.text);
        if key.is_empty() || (!already_said.is_empty() && already_said.contains(&key)) {
            continue;
        }
        let mut score = 0.0;
        let mut hits = 0;
        for (index, term) in terms.iter().enumerate() {
            if haystack.contains(term) {
                hits += 1;
                score += weights[index];
            }
        }
        if hits > 0 {
            scored.push((score, hits, entry, category.as_str()));
        }
    }
    // Best score first; ties to more matching terms, then to the newer entry, so
    // the same question gives the same answer twice.
    scored.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| b.2.date.cmp(&a.2.date))
    });

    let mut lines: Vec<String> = Vec::new();
    let mut used = 0;
    for (_, _, entry, category) in scored {
        let line = entry.line(category);
        if used + line.chars().count() > budget_chars {
            break;
        }
        used += line.chars().count() + 1;
        lines.push(line);
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

/// Reads every category file plus the project file the chat owns, ready for
/// `select`.
pub fn candidates(dir: &Path, workspace: Option<&Path>) -> Vec<(String, Entry)> {
    let mut out: Vec<(String, Entry)> = Vec::new();
    for category in CATEGORIES.iter().map(|c| c.id) {
        for entry in read(dir, category) {
            out.push((category.to_string(), entry));
        }
    }
    if let Some(project) = project_category(workspace) {
        for entry in read(dir, &project) {
            out.push((project.clone(), entry));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("deskseek-memfiles-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn appending_creates_the_file_with_a_header_and_a_date() {
        let dir = tmp();
        assert_eq!(
            append(&dir, "education", &["Studies International Business.".to_string()]).unwrap(),
            1
        );
        let body = std::fs::read_to_string(file_for(&dir, "education")).unwrap();
        assert!(body.starts_with("# Education"), "{body}");
        assert!(body.contains("Studies International Business."));
        assert!(body.contains(") ")); // the date lives in parentheses
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_same_fact_twice_is_stored_once() {
        let dir = tmp();
        append(&dir, "work", &["Invoices go out on Fridays.".to_string()]).unwrap();
        // The second write differs only in case and trailing punctuation, which
        // is exactly what the old dedup key collapsed.
        append(&dir, "work", &["invoices go out on fridays".to_string()]).unwrap();
        assert_eq!(read(&dir, "work").len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_category_name_cannot_escape_the_directory() {
        let dir = tmp();
        let path = file_for(&dir, "../../evil");
        assert_eq!(path.parent(), Some(dir.as_path()), "{path:?}");
        // A name of pure punctuation still lands somewhere findable.
        assert!(file_for(&dir, "*").ends_with("other.md"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_hand_written_line_without_a_date_or_category_is_still_read() {
        let entries = parse("# Work\n\n- (2026-01-02) dated\n- just a note\n- see [1] for the rest\n");
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].date, "2026-01-02");
        assert_eq!(entries[0].text, "dated");
        assert_eq!(entries[1].date, "");
        assert_eq!(entries[1].text, "just a note");
        // A bracketed something that is not a category must not be eaten.
        assert_eq!(entries[2].text, "see [1] for the rest");
    }

    #[test]
    fn a_line_can_be_removed_without_touching_the_others() {
        let dir = tmp();
        append(&dir, "interests", &["Plays the guitar.".to_string(), "Hates coriander.".to_string()])
            .unwrap();
        assert!(!remove(&dir, "interests", "something else").unwrap());
        assert!(remove(&dir, "interests", "Plays the guitar").unwrap());
        let left = read(&dir, "interests");
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].text, "Hates coriander.");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_term_that_matches_everything_ranks_below_the_one_that_discriminates() {
        let candidates = vec![
            ("work".to_string(), Entry { date: "2026-01-01".into(), text: "June is busy.".into() }),
            ("work".to_string(), Entry { date: "2026-01-01".into(), text: "June holds the exam.".into() }),
            ("work".to_string(), Entry { date: "2026-01-01".into(), text: "June again.".into() }),
        ];
        // "june" is in all three and so says nothing; "exam" is in one, so that
        // line has to come first even though every line matched.
        let got = select(&candidates, "june exam", "", 4000).unwrap();
        assert!(got.lines().next().unwrap().contains("exam"), "{got}");
    }

    #[test]
    fn selection_ranks_by_how_many_terms_match() {
        let candidates = vec![
            ("education".to_string(), Entry { date: "2026-01-01".into(), text: "Takes an exam in June.".into() }),
            ("work".to_string(), Entry { date: "2026-01-01".into(), text: "June is a client deadline.".into() }),
            ("interests".to_string(), Entry { date: "2026-01-01".into(), text: "Reads science fiction.".into() }),
        ];
        let got = select(&candidates, "june exam", "", 4000).unwrap();
        // Both mention June; only one mentions the exam, so it comes first.
        assert!(got.starts_with("- [education]"), "{got}");
        assert!(!got.contains("science fiction"), "an unrelated entry must not be attached");
    }

    #[test]
    fn selection_never_repeats_what_the_profile_already_says() {
        let candidates = vec![(
            "other".to_string(),
            Entry { date: "2026-01-01".into(), text: "Writes in Dutch.".into() },
        )];
        assert!(select(&candidates, "which language should you write in", "Writes in Dutch.", 4000).is_none());
    }

    #[test]
    fn selection_respects_the_budget_and_a_question_with_no_terms() {
        let candidates = vec![(
            "work".to_string(),
            Entry { date: "2026-01-01".into(), text: "A very long memory about invoices indeed.".into() },
        )];
        assert!(select(&candidates, "invoices", "", 10).is_none(), "the budget is a hard stop");
        assert!(select(&candidates, "a", "", 4000).is_none(), "a one-letter question is not a query");
    }

    #[test]
    fn a_memory_learned_in_a_folder_stays_in_that_folder() {
        let dir = tmp();
        let project = Path::new("C:/work/acme");
        assert_eq!(destination("work", Some(project)), "project-acme");
        assert_eq!(destination(PROFILE, Some(project)), PROFILE, "the profile is never scoped");
        assert_eq!(destination("education", None), "education");
        assert_eq!(destination("nonsense", None), "other");

        append(&dir, "work", &["A global work note.".to_string()]).unwrap();
        append(&dir, &destination("work", Some(project)), &["An acme-only note.".to_string()]).unwrap();

        // Another folder sees the global note and not acme's.
        let elsewhere = candidates(&dir, Some(Path::new("C:/work/other")));
        assert!(elsewhere.iter().any(|(_, e)| e.text == "A global work note."));
        assert!(!elsewhere.iter().any(|(_, e)| e.text == "An acme-only note."));
        let here = candidates(&dir, Some(project));
        assert!(here.iter().any(|(_, e)| e.text == "An acme-only note."));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_category_is_matched_regardless_of_case_and_trimmed() {
        // The bug this guards: a case-sensitive match turned "Education" into the
        // fallback category, silently, with no error anywhere.
        assert_eq!(canonical("Education"), Some("education"));
        assert_eq!(canonical(" education "), Some("education"));
        assert_eq!(destination("Education", None), "education");
        assert_eq!(canonical("astrophysics"), None);
        assert_eq!(destination("astrophysics", None), FALLBACK);
    }

    #[test]
    fn a_target_outside_the_known_categories_lands_in_other() {
        let dir = tmp();
        // A model inventing a file per conversation would leave a directory of
        // near-empty files, so anything unrecognised is filed under `other`.
        append(&dir, &destination("crypto-tips", None), &["A note.".to_string()]).unwrap();
        assert!(file_for(&dir, "other").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
