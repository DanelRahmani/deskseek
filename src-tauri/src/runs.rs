//! Tracks in-flight agent runs so the user can stop one.
//!
//! A chat has at most one live run: starting a second cancels the first. Each
//! run hands out a [`Cancel`] that the streaming loop, the approval wait and the
//! child-process runner all watch, so one button stops all three.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::watch;

/// A one-shot, cloneable stop signal.
///
/// `watch` rather than a bare `AtomicBool`: a waiter must be woken, and `watch`
/// stores the value, so a cancel landing between "check" and "await" is never
/// missed (the reason a plain `Notify` is the wrong tool here).
#[derive(Clone)]
pub struct Cancel(Arc<watch::Sender<bool>>);

impl Default for Cancel {
    fn default() -> Self {
        Self::new()
    }
}

impl Cancel {
    pub fn new() -> Self {
        let (tx, _) = watch::channel(false);
        Self(Arc::new(tx))
    }

    pub fn cancel(&self) {
        self.0.send_replace(true);
    }

    pub fn is_cancelled(&self) -> bool {
        *self.0.borrow()
    }

    /// Resolves as soon as the run is cancelled — immediately if it already is.
    pub async fn cancelled(&self) {
        let mut rx = self.0.subscribe();
        let _ = rx.wait_for(|value| *value).await;
    }
}

struct Run {
    id: u64,
    cancel: Cancel,
}

/// Every live run, keyed by chat id.
#[derive(Default)]
pub struct Runs {
    next_id: AtomicU64,
    active: Mutex<HashMap<String, Run>>,
}

impl Runs {
    /// Starts tracking a run and returns its id and cancel handle. Any run
    /// already in flight for the same chat is cancelled — one live turn per chat.
    pub fn begin(&self, chat_id: &str) -> (u64, Cancel) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let cancel = Cancel::new();
        let previous = self.active.lock().unwrap().insert(
            chat_id.to_string(),
            Run {
                id,
                cancel: cancel.clone(),
            },
        );
        if let Some(previous) = previous {
            previous.cancel.cancel();
        }
        (id, cancel)
    }

    /// Stops tracking `chat_id`, but only if `id` is still the current run — a
    /// finished run must not evict the one that replaced it.
    pub fn finish(&self, chat_id: &str, id: u64) {
        let mut active = self.active.lock().unwrap();
        if active.get(chat_id).map(|run| run.id) == Some(id) {
            active.remove(chat_id);
        }
    }

    /// Cancels the live run for `chat_id`. False when nothing was running.
    pub fn cancel(&self, chat_id: &str) -> bool {
        match self.active.lock().unwrap().get(chat_id) {
            Some(run) => {
                run.cancel.cancel();
                true
            }
            None => false,
        }
    }

    /// Cancels every live run. This is what the kill switch pulls: with the
    /// agent driving the mouse, "stop the chat I am looking at" is not enough.
    /// Returns how many runs were live. Cancelling does not evict them — each
    /// run leaves the map itself when it reports finished.
    pub fn cancel_all(&self) -> usize {
        let active = self.active.lock().unwrap();
        for run in active.values() {
            run.cancel.cancel();
        }
        active.len()
    }

    pub fn running(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.active.lock().unwrap().keys().cloned().collect();
        ids.sort();
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_run_is_not_cancelled() {
        let runs = Runs::default();
        let (_, cancel) = runs.begin("chat-1");
        assert!(!cancel.is_cancelled());
        assert_eq!(runs.running(), vec!["chat-1".to_string()]);
    }

    #[test]
    fn cancel_reaches_the_handle_the_run_holds() {
        let runs = Runs::default();
        let (_, cancel) = runs.begin("chat-1");
        assert!(runs.cancel("chat-1"));
        assert!(cancel.is_cancelled());
        assert!(!runs.cancel("chat-2"), "nothing was running for chat-2");
    }

    #[test]
    fn the_kill_switch_stops_every_chat_not_just_the_visible_one() {
        let runs = Runs::default();
        let (_, first) = runs.begin("chat-1");
        let (_, second) = runs.begin("chat-2");
        assert_eq!(runs.cancel_all(), 2);
        assert!(first.is_cancelled() && second.is_cancelled());
        // Pressing again before they wind up is harmless: cancelling is
        // idempotent, and the runs stay listed until they report finished.
        assert_eq!(runs.cancel_all(), 2);
    }

    #[test]
    fn starting_a_second_run_cancels_the_first() {
        let runs = Runs::default();
        let (_, first) = runs.begin("chat-1");
        let (_, second) = runs.begin("chat-1");
        assert!(first.is_cancelled());
        assert!(!second.is_cancelled());
    }

    #[test]
    fn finishing_a_stale_run_does_not_evict_its_replacement() {
        let runs = Runs::default();
        let (old_id, _) = runs.begin("chat-1");
        let (new_id, new_cancel) = runs.begin("chat-1");
        runs.finish("chat-1", old_id);
        assert_eq!(
            runs.running(),
            vec!["chat-1".to_string()],
            "the newer run is still tracked"
        );
        runs.finish("chat-1", new_id);
        assert!(runs.running().is_empty());
        assert!(!new_cancel.is_cancelled());
    }

    #[tokio::test]
    async fn awaiting_a_cancel_resolves_whether_it_lands_before_or_after() {
        let runs = Runs::default();
        let (_, cancel) = runs.begin("chat-1");

        // Already cancelled: the await must return without anyone poking it.
        cancel.cancel();
        tokio::time::timeout(std::time::Duration::from_secs(1), cancel.cancelled())
            .await
            .expect("an already-cancelled handle must resolve immediately");

        // Cancelled later: the waiter is woken by the signal.
        let (_, later) = runs.begin("chat-2");
        let waiter = later.clone();
        let handle = tokio::spawn(async move { waiter.cancelled().await });
        runs.cancel("chat-2");
        tokio::time::timeout(std::time::Duration::from_secs(1), handle)
            .await
            .expect("a waiter must wake when the run is cancelled")
            .unwrap();
    }
}
