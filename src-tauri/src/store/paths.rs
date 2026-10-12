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

    /// The one and only memory file. Fixed name, always injected into the
    /// system prompt — the model never has to look for it.
    pub fn memory_file(&self) -> PathBuf {
        self.root.join("MEMORY.md")
    }

    /// One markdown file per category of memory, under a folder of their own.
    ///
    /// `MEMORY.md` deliberately stays at the root: it is the profile, it is read
    /// by a person more often than any other file here, and burying the one file
    /// you actually open would be a strange thing to do to it.
    pub fn memory_dir(&self) -> PathBuf {
        self.root.join("memory")
    }

    /// One markdown file per summarised conversation, `YYYYMMDD-topic.md`.
    pub fn chatlog_dir(&self) -> PathBuf {
        self.root.join("CHATLOG")
    }

    pub fn db_file(&self) -> PathBuf {
        self.root.join("deskseek.db")
    }

    pub fn skills_dir(&self) -> PathBuf {
        self.root.join("skills")
    }

    /// One markdown file per design system, named after its id.
    ///
    /// Its own folder rather than the skills one: they are a different thing, and
    /// sharing a folder is exactly what made the catalogue the model read and the
    /// list the Skills tab showed disagree.
    pub fn design_dir(&self) -> PathBuf {
        self.root.join("design")
    }

    /// Which MCP servers to run. Hand-editable, so it lives beside the settings
    /// rather than inside them.
    pub fn mcp_file(&self) -> PathBuf {
        self.root.join("mcp.json")
    }

    /// The automation browser's own profile. Never the user's: DeskSeek must not
    /// be able to read their tabs, cookies or saved logins.
    pub fn browser_dir(&self) -> PathBuf {
        self.root.join("browser")
    }

    /// Where chat-mode documents live by default. A chat can point elsewhere.
    pub fn documents_dir(&self) -> PathBuf {
        self.root.join("documents")
    }

    pub fn ensure(&self) -> io::Result<()> {
        std::fs::create_dir_all(&self.root)?;
        std::fs::create_dir_all(self.chatlog_dir())?;
        Ok(())
    }

    /// Earlier builds used a lowercase `memory.md`; rename it in place so no
    /// remembered facts are lost.
    pub fn migrate_legacy(&self) {
        let old = self.root.join("memory.md");
        let new = self.memory_file();
        if old.exists() && !new.exists() {
            let _ = std::fs::rename(old, new);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_hang_off_the_root() {
        let p = AppPaths::new(PathBuf::from("/data"));
        assert!(p.db_file().starts_with("/data"));
        assert_eq!(p.memory_file().file_name().unwrap(), "MEMORY.md");
        assert_eq!(p.chatlog_dir().file_name().unwrap(), "CHATLOG");
        // Skills and design systems are separate stores, not one folder with a
        // prefix telling them apart.
        assert_eq!(p.skills_dir().file_name().unwrap(), "skills");
        assert_eq!(p.design_dir().file_name().unwrap(), "design");
        assert_ne!(p.skills_dir(), p.design_dir());
    }

    #[test]
    fn a_legacy_lowercase_memory_file_keeps_its_content() {
        let root = std::env::temp_dir().join(format!("deskseek-paths-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("memory.md"), "# Memory\n\n- old fact\n").unwrap();

        let paths = AppPaths::new(root.clone());
        paths.migrate_legacy();

        // On a case-insensitive filesystem (Windows, macOS) `memory.md` and
        // `MEMORY.md` are the *same* file, so there is nothing to rename — what
        // matters either way is that the remembered facts survive.
        let kept = std::fs::read_to_string(paths.memory_file()).unwrap();
        assert!(kept.contains("old fact"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn migration_does_not_clobber_an_existing_memory_file() {
        let root = std::env::temp_dir().join(format!("deskseek-paths-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("memory.md"), "old").unwrap();
        std::fs::write(root.join("MEMORY.md"), "new").unwrap();

        AppPaths::new(root.clone()).migrate_legacy();

        assert_eq!(std::fs::read_to_string(root.join("MEMORY.md")).unwrap(), "new");
        let _ = std::fs::remove_dir_all(&root);
    }
}
