//! Tracks tool-call approvals that are waiting on the user.
//!
//! The agent loop parks on a oneshot channel; the `respond_approval` command
//! resolves it. Session grants live in memory, "always" grants in the database
//! keyed by workspace.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use serde_json::Value;
use tokio::sync::oneshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Deny,
    Once,
    Session,
    Always,
}

impl Decision {
    pub fn parse(raw: &str) -> Self {
        match raw {
            "once" => Self::Once,
            "session" => Self::Session,
            "always" => Self::Always,
            _ => Self::Deny,
        }
    }

    pub fn allows(self) -> bool {
        !matches!(self, Self::Deny)
    }
}

#[derive(Default)]
pub struct Approvals {
    pending: Mutex<HashMap<String, oneshot::Sender<Decision>>>,
    session: Mutex<HashSet<String>>,
}

/// A short, stable fingerprint of one call's arguments, for the audit log.
///
/// A fingerprint rather than the arguments themselves: an approval can cover a
/// command carrying a secret, and a log meant to be read back later is not the
/// place to keep one. FNV-1a is enough — this is not defending against anyone
/// choosing a collision, only telling two approvals of the same tool apart.
pub fn args_digest(args: &Value) -> String {
    let text = args.to_string();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn key(workspace: &str, tool: &str) -> String {
    format!("{workspace}::{tool}")
}

impl Approvals {
    /// Registers a pending request and returns the receiver to await.
    pub fn register(&self, request_id: &str) -> oneshot::Receiver<Decision> {
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(request_id.to_string(), tx);
        rx
    }

    /// Resolves a pending request. Returns false if it was already answered.
    pub fn respond(&self, request_id: &str, decision: Decision) -> bool {
        let sender = self.pending.lock().unwrap().remove(request_id);
        match sender {
            Some(tx) => tx.send(decision).is_ok(),
            None => false,
        }
    }

    pub fn grant_session(&self, workspace: &str, tool: &str) {
        self.session.lock().unwrap().insert(key(workspace, tool));
    }

    pub fn has_session_grant(&self, workspace: &str, tool: &str) -> bool {
        self.session.lock().unwrap().contains(&key(workspace, tool))
    }

    pub fn clear_session(&self) {
        self.session.lock().unwrap().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decisions_parse_from_the_wire_and_deny_by_default() {
        assert!(Decision::parse("once").allows());
        assert!(Decision::parse("session").allows());
        assert!(Decision::parse("always").allows());
        assert!(!Decision::parse("deny").allows());
        assert!(!Decision::parse("nonsense").allows());
    }

    #[test]
    fn a_digest_is_stable_and_tells_two_calls_apart() {
        use serde_json::json;
        let call = json!({ "command": "cargo test" });
        assert_eq!(args_digest(&call), args_digest(&call.clone()));
        assert_eq!(args_digest(&call).len(), 16);
        // The point of it: `write_file` twice is two different decisions.
        assert_ne!(
            args_digest(&json!({ "path": "a.md" })),
            args_digest(&json!({ "path": "b.md" }))
        );
    }

    #[tokio::test]
    async fn a_registered_request_receives_the_decision() {
        let approvals = Approvals::default();
        let rx = approvals.register("req-1");
        assert!(approvals.respond("req-1", Decision::Once));
        assert_eq!(rx.await.unwrap(), Decision::Once);
    }

    #[test]
    fn responding_twice_is_refused() {
        let approvals = Approvals::default();
        let _rx = approvals.register("req-1");
        assert!(approvals.respond("req-1", Decision::Once));
        assert!(!approvals.respond("req-1", Decision::Once));
    }

    #[test]
    fn session_grants_are_scoped_per_workspace_and_tool() {
        let approvals = Approvals::default();
        approvals.grant_session("/repo", "run_command");
        assert!(approvals.has_session_grant("/repo", "run_command"));
        assert!(!approvals.has_session_grant("/other", "run_command"));
        assert!(!approvals.has_session_grant("/repo", "write_file"));
        approvals.clear_session();
        assert!(!approvals.has_session_grant("/repo", "run_command"));
    }
}
