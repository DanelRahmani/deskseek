use std::io;
use std::path::{Path, PathBuf};

/// Every file DeskSeek owns lives under one directory.
#[derive(Clone)]
pub struct AppPaths {
    root: PathBuf,
}

impl AppPaths {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn settings_file(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    pub fn memory_file(&self) -> PathBuf {
        self.root.join("memory.md")
    }

    pub fn db_file(&self) -> PathBuf {
        self.root.join("deskseek.db")
    }

    pub fn skills_dir(&self) -> PathBuf {
        self.root.join("skills")
    }

    /// Where chat-mode documents live by default. A chat can point elsewhere.
    pub fn documents_dir(&self) -> PathBuf {
        self.root.join("documents")
    }

    pub fn ensure(&self) -> io::Result<()> {
        std::fs::create_dir_all(&self.root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_hang_off_the_root() {
        let p = AppPaths::new(PathBuf::from("/data"));
        assert!(p.db_file().starts_with("/data"));
        assert_eq!(p.memory_file().file_name().unwrap(), "memory.md");
    }
}
