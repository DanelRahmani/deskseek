//! The trust boundary for the coding section: every model-supplied path is
//! resolved against the workspace root and refused if it escapes it.
//!
//! Two escapes matter and both are covered: `..` traversal in a path that does
//! not exist yet, and symlinks pointing outside the workspace (caught by
//! canonicalising before the prefix check).

use std::path::{Component, Path, PathBuf};

/// Canonicalises `p`, or — when part of it does not exist yet — canonicalises
/// the deepest existing ancestor and re-attaches the rest. Lets `write_file`
/// create new files (and new directories) while still resolving symlinks in the
/// part of the path that does exist.
fn canonicalize_lenient(p: &Path) -> Result<PathBuf, String> {
    let mut base = p.to_path_buf();
    let mut tail: Vec<std::ffi::OsString> = Vec::new();

    loop {
        if let Ok(canonical) = base.canonicalize() {
            let mut out = canonical;
            for part in tail.iter().rev() {
                out.push(part);
            }
            return Ok(out);
        }
        match base.file_name() {
            Some(name) => {
                tail.push(name.to_os_string());
                base = base
                    .parent()
                    .map(Path::to_path_buf)
                    .ok_or_else(|| format!("no such directory: {}", p.display()))?;
            }
            None => return Err(format!("no such directory: {}", p.display())),
        }
    }
}

pub fn resolve(workspace: &Path, user_path: &str) -> Result<PathBuf, String> {
    let root = workspace
        .canonicalize()
        .map_err(|e| format!("workspace is unavailable: {e}"))?;

    let raw = user_path.trim();
    if raw.is_empty() {
        return Err("path is empty".to_string());
    }
    let given = Path::new(raw);

    let resolved = if given.is_absolute() {
        canonicalize_lenient(given)?
    } else {
        // Walk the components ourselves so a not-yet-existing target with `..`
        // cannot slip out before the filesystem ever sees it.
        let mut rel = PathBuf::new();
        for component in given.components() {
            match component {
                Component::ParentDir => {
                    if !rel.pop() {
                        return Err(format!("refusing '{raw}': it leaves the workspace"));
                    }
                }
                Component::CurDir => {}
                Component::Normal(part) => rel.push(part),
                _ => return Err(format!("unsupported path: {raw}")),
            }
        }
        canonicalize_lenient(&root.join(rel))?
    };

    if !resolved.starts_with(&root) {
        return Err(format!(
            "refusing '{raw}': outside the workspace {}",
            root.display()
        ));
    }
    Ok(resolved)
}

/// Renders a path relative to the workspace for display back to the model.
pub fn display(workspace: &Path, path: &Path) -> String {
    let root = workspace.canonicalize().unwrap_or_else(|_| workspace.to_path_buf());
    path.strip_prefix(&root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("deskseek-ws-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src").join("main.rs"), "fn main() {}").unwrap();
        std::fs::write(dir.join("notes.txt"), "hi").unwrap();
        dir
    }

    #[test]
    fn relative_paths_inside_the_workspace_resolve() {
        let ws = workspace();
        let p = resolve(&ws, "src/main.rs").unwrap();
        assert!(p.ends_with("main.rs"));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn a_new_file_in_an_existing_directory_is_allowed() {
        let ws = workspace();
        let p = resolve(&ws, "src/new_file.rs").unwrap();
        assert!(p.ends_with("new_file.rs"));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn parent_traversal_is_refused() {
        let ws = workspace();
        let err = resolve(&ws, "../outside.txt").unwrap_err();
        assert!(err.contains("leaves the workspace"), "{err}");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn sneaky_traversal_is_refused() {
        let ws = workspace();
        assert!(resolve(&ws, "src/../../outside.txt").is_err());
        assert!(resolve(&ws, "a/../../outside.txt").is_err());
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn absolute_paths_outside_the_workspace_are_refused() {
        let ws = workspace();
        let outside = std::env::temp_dir().join("deskseek-outside.txt");
        std::fs::write(&outside, "no").unwrap();
        let err = resolve(&ws, outside.to_str().unwrap()).unwrap_err();
        assert!(err.contains("outside the workspace"), "{err}");
        let _ = std::fs::remove_file(&outside);
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn an_absolute_path_inside_the_workspace_is_fine() {
        let ws = workspace();
        let inside = ws.join("notes.txt");
        let p = resolve(&ws, inside.to_str().unwrap()).unwrap();
        assert!(p.ends_with("notes.txt"));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn empty_paths_are_refused() {
        let ws = workspace();
        assert!(resolve(&ws, "   ").is_err());
        let _ = std::fs::remove_dir_all(&ws);
    }
}
