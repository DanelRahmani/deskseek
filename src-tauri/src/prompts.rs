//! Saved prompts: a request that gets typed again and again, kept as a file.
//!
//! One markdown file per prompt, in a folder of their own —
//! `<prompts_dir>/<slug>.md` — with the first `# ` heading as the name, exactly
//! like a design system (see `store::files` for the rules themselves).
//!
//! Why not a skill: a skill is a *set of rules* the model follows, and it costs
//! catalogue space in every request. A prompt is a *one-shot text* the user
//! wants in the box, and the model never sees the list — so the two are on
//! opposite sides of the same line, and sharing a folder would blur the only
//! distinction that matters.
//!
//! Hand-editable on purpose. The app is a convenience over the folder, not the
//! only way in.

use std::path::Path;

/// A saved prompt: a name, and the text it stands for.
pub use crate::store::files::Doc as Prompt;

/// Ids are bare slugs. Nothing shares this namespace, so there is no prefix to
/// keep one out of something else's way — and the guard in `store::files` does
/// not depend on the prefix, which is the point.
pub const STORE: crate::store::files::Store =
    crate::store::files::Store::new("", "prompt", "prompt");

/// Every saved prompt, by name.
///
/// The body comes with it, because picking one from the composer's menu is the
/// only thing that ever happens to a prompt — and a second round trip to fetch
/// the text would be a round trip for nothing.
pub fn list(dir: &Path) -> Vec<Prompt> {
    STORE.list(dir)
}

/// Writes a prompt, overwriting one of the same name — the authoring surface has
/// to be able to save twice.
pub fn write(dir: &Path, name: &str, body: &str) -> Result<String, String> {
    STORE.write(dir, name, body)
}

pub fn remove(dir: &Path, id: &str) -> Result<(), String> {
    STORE.remove(dir, id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prompt_is_a_bare_slug_and_a_round_trip() {
        let root = std::env::temp_dir().join(format!("deskseek-prompt-{}", uuid::Uuid::new_v4()));
        // No prefix anywhere in the id: nothing shares this namespace.
        assert_eq!(STORE.id_for("Review a diff"), "review-a-diff");
        assert_eq!(STORE.id_for("Explain this error"), "explain-this-error");

        let id = write(&root, "Review a diff", "Read the diff, then list what is wrong.\n").unwrap();
        assert_eq!(id, "review-a-diff");
        let back = STORE.read(&root, &id).unwrap();
        assert_eq!(back.name, "Review a diff");
        assert_eq!(back.body, "Read the diff, then list what is wrong.");

        // Two prompts, listed by name, and a stray file in the folder is not one.
        write(&root, "Explain this error", "Say what went wrong.\n").unwrap();
        std::fs::write(root.join("notes.txt"), "not a prompt").unwrap();
        let all = list(&root);
        assert_eq!(all.len(), 2, "{all:?}");
        assert_eq!(all[0].name, "Explain this error", "listed by name");

        remove(&root, &id).unwrap();
        assert!(STORE.read(&root, &id).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_prompt_id_cannot_name_a_file_outside_the_folder() {
        // The empty prefix means the separator rule is doing all the work here,
        // so it is the one worth asserting on this store.
        for bad in ["../secret", "a/b", "a\\b", ".."] {
            assert!(STORE.read(Path::new("/prompts"), bad).is_none(), "{bad} must not read");
            assert!(remove(Path::new("/prompts"), bad).is_err(), "{bad} must not delete");
        }
    }
}
