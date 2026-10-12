//! One archive for the whole profile.
//!
//! The old backup covered settings and `mcp.json`, which is the part that matters
//! least: the database, the memory files, the skills, design systems and prompts,
//! and the documents conversations filed are what a dead disk would actually
//! cost. This is the same idea widened to the whole root. The API key stays out
//! for the same reason as before — it lives in the OS keychain, which is not a
//! file this app can hand out, and a backup is a file that gets mailed around.
//!
//! The database is the awkward part. It is held open for the life of the app, so
//! a restore cannot simply overwrite it; `db::swap_file` releases the handle
//! first, and `db::checkpoint` flushes the write-ahead log before *writing* one,
//! so the single copied file is complete on its own.

use std::fs::File;
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

use zip::write::SimpleFileOptions;

/// The parts of the root a backup carries, in the order they are written.
///
/// Named rather than "everything in the root": `browser/` is the automation
/// profile (re-creatable, and never the user's own data) and a half-written
/// `restore-*` scratch folder is not something to archive.
pub const ITEMS: &[&str] = &[
    "settings.json",
    "mcp.json",
    "deskseek.db",
    "MEMORY.md",
    "memory",
    "skills",
    "design",
    "prompts",
    "documents",
    "CHATLOG",
];

/// The database file's name, which the caller has to treat specially.
pub const DB_FILE: &str = "deskseek.db";

/// Writes every part of the profile that exists into one zip.
///
/// A missing item is skipped rather than an error: a fresh profile has no design
/// store and no documents folder yet, and that is not a failure.
pub fn write(root: &Path, dest: &Path) -> Result<(), String> {
    let file =
        File::create(dest).map_err(|e| format!("could not create {}: {e}", dest.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for item in ITEMS {
        let full = root.join(item);
        if !full.exists() {
            continue;
        }
        add(&mut zip, root, &full, opts)?;
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

fn add<W: Write + Seek>(
    zip: &mut zip::ZipWriter<W>,
    root: &Path,
    path: &Path,
    opts: SimpleFileOptions,
) -> Result<(), String> {
    let name = path
        .strip_prefix(root)
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .replace('\\', "/");
    if path.is_dir() {
        // Zip has no real directories, but an entry of its own is what makes an
        // empty folder survive the round trip instead of vanishing.
        zip.add_directory(format!("{name}/"), opts)
            .map_err(|e| e.to_string())?;
        for entry in std::fs::read_dir(path).map_err(|e| e.to_string())? {
            add(zip, root, &entry.map_err(|e| e.to_string())?.path(), opts)?;
        }
    } else {
        zip.start_file(name, opts).map_err(|e| e.to_string())?;
        let mut file = File::open(path).map_err(|e| format!("could not read {path:?}: {e}"))?;
        std::io::copy(&mut file, zip).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Unpacks an archive into `dest`, which should be an empty scratch folder.
///
/// Every entry is checked to stay inside `dest` before anything is written. An
/// archive is a file that arrives from elsewhere, so a hand-made one with a `..`
/// in it must not be able to write across the disk — the check is the trust
/// boundary, not a nicety.
pub fn extract(archive: &Path, dest: &Path) -> Result<usize, String> {
    let file = File::open(archive)
        .map_err(|e| format!("could not read {}: {e}", archive.display()))?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| format!("{} is not a DeskSeek backup: {e}", archive.display()))?;
    let mut written = 0usize;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let out = dest.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut file = File::create(&out)
            .map_err(|e| format!("could not unpack {}: {e}", out.display()))?;
        let mut buffer = Vec::new();
        entry.read_to_end(&mut buffer).map_err(|e| e.to_string())?;
        file.write_all(&buffer).map_err(|e| e.to_string())?;
        written += 1;
    }
    Ok(written)
}

/// Puts the extracted profile over the live one, item by item.
///
/// Replaces rather than overlays: a file the backup does not have is one the
/// profile should not have either, or a restore would leave a mix of two
/// profiles. The database is left to the caller, which owns the open connection.
pub fn replace_items(scratch: &Path, root: &Path) -> Result<(), String> {
    for item in ITEMS.iter().filter(|item| **item != DB_FILE) {
        let from = scratch.join(item);
        if !from.exists() {
            continue;
        }
        let to = root.join(item);
        if to.is_dir() {
            std::fs::remove_dir_all(&to).map_err(|e| format!("could not clear {to:?}: {e}"))?;
        } else if to.exists() {
            std::fs::remove_file(&to).map_err(|e| format!("could not clear {to:?}: {e}"))?;
        }
        copy_tree(&from, &to)?;
    }
    Ok(())
}

fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    if from.is_dir() {
        std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
        for entry in std::fs::read_dir(from).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            copy_tree(&entry.path(), &to.join(entry.file_name()))?;
        }
    } else {
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::copy(from, to)
            .map_err(|e| format!("could not restore {}: {e}", to.display()))?;
    }
    Ok(())
}

/// A scratch folder for unpacking, beside the profile but never inside it.
///
/// Its own name is deliberately not one of `ITEMS`, so a restore cannot pick up
/// a half-unpacked copy of itself.
pub fn scratch_dir(root: &Path) -> PathBuf {
    root.join(format!("restore-{}", uuid::Uuid::new_v4()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::db;

    #[test]
    fn a_profile_round_trips_through_one_archive() {
        let dir = std::env::temp_dir().join(format!("deskseek-backup-{}", uuid::Uuid::new_v4()));
        let live = dir.join("live");
        std::fs::create_dir_all(&live).unwrap();

        // A chat with a message, so the archive has a database worth checking.
        let chat = {
            let conn = db::open(&live.join(DB_FILE)).unwrap();
            let chat =
                db::create_chat(&conn, "coding", Some("C:/proj"), "m", true, "high", "manual")
                    .unwrap();
            db::insert_message(
                &conn,
                &chat.id,
                "user",
                Some("remember the invoice"),
                None,
                None,
                None,
                None,
            )
            .unwrap();
            // What a backup does before it copies the file.
            db::checkpoint(&conn);
            chat
        };
        // Something in each of the other stores, and one outside the list.
        std::fs::write(live.join("MEMORY.md"), "- the invoice template\n").unwrap();
        std::fs::create_dir_all(live.join("skills/example")).unwrap();
        std::fs::write(live.join("skills/example/SKILL.md"), "# Example\n").unwrap();
        std::fs::create_dir_all(live.join("documents/my-chat")).unwrap();
        std::fs::write(live.join("documents/my-chat/notes.md"), "attached\n").unwrap();
        std::fs::write(live.join("settings.json"), "{}").unwrap();
        std::fs::create_dir_all(live.join("browser")).unwrap();
        std::fs::write(live.join("browser/cache"), "not part of a profile").unwrap();

        let archive = dir.join("profile.zip");
        write(&live, &archive).unwrap();

        // A fresh profile, restored from that one file — in the order a restore
        // really does it: the database first, through the same swap the running
        // app uses, then everything else (`replace_items` skips the database).
        let fresh = dir.join("fresh");
        std::fs::create_dir_all(&fresh).unwrap();
        let scratch = scratch_dir(&dir);
        extract(&archive, &scratch).unwrap();
        let fresh_db = fresh.join(DB_FILE);
        let mut restored = db::open(&fresh_db).unwrap();
        db::swap_file(&mut restored, &fresh_db, &scratch.join(DB_FILE)).unwrap();
        replace_items(&scratch, &fresh).unwrap();

        let chats = db::list_chats(&restored, "coding").unwrap();
        assert_eq!(chats.len(), 1);
        assert_eq!(chats[0].id, chat.id);
        let messages = db::list_messages(&restored, &chat.id).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(
            messages[0].content.as_deref(),
            Some("remember the invoice"),
            "the message came back, so the WAL was checkpointed before the copy"
        );
        assert_eq!(
            std::fs::read_to_string(fresh.join("documents/my-chat/notes.md")).unwrap(),
            "attached\n"
        );
        assert!(fresh.join("MEMORY.md").exists());
        assert!(fresh.join("skills/example/SKILL.md").exists());
        assert!(
            !fresh.join("browser").exists(),
            "the automation profile is not part of a backup"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_restore_replaces_a_file_the_backup_does_not_have() {
        let dir = std::env::temp_dir().join(format!("deskseek-prune-{}", uuid::Uuid::new_v4()));
        let scratch = dir.join("scratch");
        let root = dir.join("root");
        std::fs::create_dir_all(scratch.join("skills")).unwrap();
        std::fs::create_dir_all(root.join("skills/stale")).unwrap();
        std::fs::write(root.join("skills/stale/SKILL.md"), "# Old\n").unwrap();
        std::fs::write(scratch.join("skills/kept.md"), "# New\n").unwrap();

        replace_items(&scratch, &root).unwrap();

        // An overlay would leave `stale` behind and the profile would be a mix.
        assert!(!root.join("skills/stale").exists());
        assert!(root.join("skills/kept.md").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_archive_cannot_write_outside_the_folder_it_is_unpacked_into() {
        let dir = std::env::temp_dir().join(format!("deskseek-escape-{}", uuid::Uuid::new_v4()));
        let dest = dir.join("dest");
        std::fs::create_dir_all(&dest).unwrap();
        let archive = dir.join("hostile.zip");
        {
            let mut zip = zip::ZipWriter::new(File::create(&archive).unwrap());
            let opts = SimpleFileOptions::default();
            zip.start_file("../escaped.txt", opts).unwrap();
            zip.write_all(b"gone").unwrap();
            zip.finish().unwrap();
        }

        extract(&archive, &dest).unwrap();

        // Whatever the writer did with the name, nothing landed above `dest`.
        assert!(!dir.join("escaped.txt").exists());
        assert!(!dest.parent().unwrap().join("escaped.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
