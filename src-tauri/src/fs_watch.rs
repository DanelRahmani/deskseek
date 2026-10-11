//! Watches a workspace so the file tree updates as the agent writes, instead of
//! the user pressing refresh.
//!
//! One watcher for the whole app. Roots are refcounted, so two chats pointed at
//! the same folder share a single subscription and the OS watch is only released
//! when the last chat lets go.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter};
use tokio::sync::Notify;

use crate::tools::walk;

pub const EVENT_FS: &str = "fs-changed";

/// How long a burst is allowed to settle. One save fires several events; the
/// tree only needs telling once.
const SETTLE: Duration = Duration::from_millis(200);

/// Most changed paths one event may carry. A build that rewrites its output
/// directory would otherwise send thousands.
const MAX_PATHS: usize = 200;

pub struct FileWatcher {
    pending: Arc<Mutex<BTreeSet<PathBuf>>>,
    signal: Arc<Notify>,
    roots: Mutex<HashMap<PathBuf, usize>>,
    watcher: Mutex<Option<RecommendedWatcher>>,
}

impl FileWatcher {
    pub fn new(app: AppHandle) -> Self {
        let pending: Arc<Mutex<BTreeSet<PathBuf>>> = Arc::default();
        let signal = Arc::new(Notify::new());

        let flush_pending = pending.clone();
        let flush_signal = signal.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                // `notify_one` leaves a permit when nobody is waiting, so a
                // change landing between passes is never dropped.
                flush_signal.notified().await;
                tokio::time::sleep(SETTLE).await;
                let paths: Vec<String> = {
                    let mut guard = flush_pending.lock().unwrap();
                    let drained = guard
                        .iter()
                        .take(MAX_PATHS)
                        .map(|path| path.to_string_lossy().to_string())
                        .collect();
                    guard.clear();
                    drained
                };
                if !paths.is_empty() {
                    let _ = app.emit(EVENT_FS, serde_json::json!({ "paths": paths }));
                }
            }
        });

        Self {
            pending,
            signal,
            roots: Mutex::new(HashMap::new()),
            watcher: Mutex::new(None),
        }
    }

    fn ensure_watcher(&self) -> Result<(), String> {
        let mut slot = self.watcher.lock().unwrap();
        if slot.is_some() {
            return Ok(());
        }

        let pending = self.pending.clone();
        let signal = self.signal.clone();
        let watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
            let Ok(event) = result else { return };
            let mut guard = pending.lock().unwrap();
            for path in event.paths {
                if !ignored(&path) {
                    guard.insert(path);
                }
            }
            drop(guard);
            signal.notify_one();
        })
        .map_err(|e| format!("could not start the file watcher: {e}"))?;

        *slot = Some(watcher);
        Ok(())
    }

    /// Watch `root` recursively. Calling this twice is a refcount, not a second
    /// subscription.
    pub fn watch(&self, root: &Path) -> Result<(), String> {
        self.ensure_watcher()?;
        let key = root.to_path_buf();

        let mut roots = self.roots.lock().unwrap();
        if let Some(count) = roots.get_mut(&key) {
            *count += 1;
            return Ok(());
        }

        let mut slot = self.watcher.lock().unwrap();
        let watcher = slot.as_mut().ok_or("the file watcher did not start")?;
        watcher
            .watch(&key, RecursiveMode::Recursive)
            .map_err(|e| format!("could not watch {}: {e}", key.display()))?;
        roots.insert(key, 1);
        Ok(())
    }

    /// Release one hold on `root`; the OS watch goes with the last one.
    pub fn unwatch(&self, root: &Path) {
        let key = root.to_path_buf();
        let mut roots = self.roots.lock().unwrap();
        match roots.get_mut(&key) {
            None => return,
            Some(count) if *count > 1 => {
                *count -= 1;
                return;
            }
            Some(_) => {
                roots.remove(&key);
            }
        }
        drop(roots);

        if let Some(watcher) = self.watcher.lock().unwrap().as_mut() {
            let _ = watcher.unwatch(&key);
        }
    }

    /// Drops every watch — called on exit so no handle outlives the app.
    pub fn clear(&self) {
        self.roots.lock().unwrap().clear();
        // Dropping the watcher is what stops the OS-level subscriptions.
        *self.watcher.lock().unwrap() = None;
    }
}

/// True for paths under a directory the tree never shows anyway, so a build
/// does not push thousands of ignored paths through the event queue.
fn ignored(path: &Path) -> bool {
    path.components()
        .any(|part| part.as_os_str().to_str().is_some_and(walk::should_skip))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_directories_are_filtered_out() {
        assert!(ignored(Path::new("/repo/node_modules/left-pad/index.js")));
        assert!(ignored(Path::new("/repo/target/debug/deskseek.exe")));
        assert!(ignored(Path::new("/repo/.git/HEAD")));
        assert!(!ignored(Path::new("/repo/src/main.rs")));
        assert!(!ignored(Path::new("/repo/.deskseek/context.md")));
    }

    #[test]
    fn a_root_is_released_only_by_its_last_holder() {
        // The refcount is the whole point: two chats in one folder must not
        // leave the second one watcher-less when the first closes.
        let watcher = FileWatcher {
            pending: Arc::default(),
            signal: Arc::new(Notify::new()),
            roots: Mutex::new(HashMap::new()),
            watcher: Mutex::new(None),
        };
        let root = PathBuf::from("/repo");

        watcher.roots.lock().unwrap().insert(root.clone(), 1);
        watcher.unwatch(&root);
        assert!(
            watcher.roots.lock().unwrap().is_empty(),
            "the only holder releases the watch"
        );

        watcher.roots.lock().unwrap().insert(root.clone(), 2);
        watcher.unwatch(&root);
        assert_eq!(
            watcher.roots.lock().unwrap().get(&root).copied(),
            Some(1),
            "the first holder leaving must not release it for the second"
        );
    }
}
