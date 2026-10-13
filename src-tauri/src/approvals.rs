//! Tracks tool-call approvals that are waiting on the user.
//!
//! The agent loop parks on a oneshot channel; the `respond_approval` command
//! resolves it. Session grants live in memory, "always" grants in the database
//! keyed by workspace and target.

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

fn key(workspace: &str, tool: &str, target: &str) -> String {
    // A unit separator rather than `::`: `target` is sometimes text the model
    // wrote, and a separator that can appear inside a value would let one grant
    // key be forged into another's. `text` below drops control characters for
    // the same reason.
    format!("{workspace}\u{1f}{tool}\u{1f}{target}")
}

/// One string argument, trimmed, with control characters dropped and a sane cap.
///
/// A target ends up as half a grant's key, so the value is sanitised where it
/// enters rather than trusted at every use: a window title the model made up
/// cannot carry a separator into the key or grow it without bound.
fn text(args: &Value, field: &str) -> Option<String> {
    let raw = args.get(field)?.as_str()?.trim();
    let clean: String = raw.chars().filter(|c| !c.is_control()).take(80).collect();
    let clean = clean.trim().to_string();
    (!clean.is_empty()).then_some(clean)
}

/// The site a URL names: its host, lowercased, without a port or a leading `www.`.
///
/// A place reached over anything but http(s) has no site — `about:blank`, `data:`
/// and a local file are where the browser *happens* to be, and a grant for that
/// would be a grant for everywhere.
fn site_of(url: &str) -> Option<String> {
    let rest = url.trim();
    let rest = match rest.split_once("://") {
        Some(("http" | "https", after)) => after,
        Some(_) => return None,
        None => rest,
    };
    let host = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .rsplit('@')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
    // A host has a dot, or is `localhost`. Anything else — `about`, `mailto`,
    // `javascript` — is a scheme with no page behind it, and `about:blank` must
    // not reduce to a "site" called `about`.
    let plausible = host == "localhost" || host.contains('.');
    plausible.then_some(host)
}

/// The program a command line runs, when the line runs exactly one.
///
/// A line that chains, pipes or redirects is not one program: `cd x && rm -rf y`
/// starts with `cd`, and a grant for `cd` would quietly cover everything after
/// it. Those get no target, so the grant the user is offered stays the whole tool
/// — which is what it has always been, and never a wider promise than that.
fn program(command: &str) -> Option<String> {
    let line = command.trim();
    if line.contains(['&', '|', ';', '>', '<', '`', '$', '(', ')', '\n', '\r']) {
        return None;
    }
    let first = line.split_whitespace().next()?;
    let name = first.rsplit(['/', '\\']).next().unwrap_or(first);
    (!name.is_empty()).then(|| name.to_string())
}

/// The top of the path a write lands in, or the file itself when it is at the
/// root.
///
/// So a grant reads "writes under `src`" or "writes `README.md`" rather than
/// "writes anywhere". A path that points outside the workspace gets no target at
/// all: the sandbox refuses it, and a grant must never be the wider of the two.
fn path_root(path: &str) -> Option<String> {
    let unix = path.trim().replace('\\', "/");
    if unix.starts_with('/') || unix.contains(':') {
        return None;
    }
    let parts: Vec<&str> = unix.split('/').filter(|p| !p.is_empty() && *p != ".").collect();
    if parts.is_empty() || parts.iter().any(|p| *p == "..") {
        return None;
    }
    Some(parts[0].to_string())
}

/// Whether a tool's target can only be read off the page the browser is on.
///
/// The browser's action tools take a selector, not a place, so the page is the
/// only thing that says where they land. Named once, here, because both the
/// caller and `target` below have to agree about it.
pub fn target_is_the_page(tool: &str) -> bool {
    matches!(
        tool,
        "browser_click" | "browser_type" | "browser_eval" | "browser_screenshot"
    )
}

/// What a grant for this call would cover, or `None` for the whole tool.
///
/// `page` is the site the browser is on, offered by the caller because it costs a
/// round trip to the page and only these four tools need it.
pub fn target(tool: &str, args: &Value, page: Option<&str>) -> Option<String> {
    if target_is_the_page(tool) {
        return page.and_then(site_of);
    }
    match tool {
        "run_command" => text(args, "command").and_then(|c| program(&c)),
        "write_file" | "edit_file" => text(args, "path").and_then(|p| path_root(&p)),
        // The app a desktop action presses in, as the model named it. Without a
        // `window` the focused window is whatever it is, so there is no target
        // to offer.
        "ui_click" | "ui_type" => text(args, "window"),
        _ => None,
    }
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

    pub fn grant_session(&self, workspace: &str, tool: &str, target: &str) {
        self.session.lock().unwrap().insert(key(workspace, tool, target));
    }

    pub fn has_session_grant(&self, workspace: &str, tool: &str, target: &str) -> bool {
        self.session.lock().unwrap().contains(&key(workspace, tool, target))
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
    fn a_target_is_the_place_a_call_touches() {
        use serde_json::json;
        // A site is the host: no scheme, no port, no credentials, no www.
        assert_eq!(site_of("https://WWW.Example.com/a/b?q=1"), Some("example.com".into()));
        assert_eq!(site_of("http://user:pw@docs.rs:8080/x"), Some("docs.rs".into()));
        // Places with no site to grant.
        assert_eq!(site_of("about:blank"), None);
        assert_eq!(site_of("data:text/html,<p>x"), None);
        assert_eq!(site_of("file:///c:/x.html"), None);
        assert_eq!(site_of(""), None);

        // One command is that command; a line that chains or pipes is not one.
        assert_eq!(program("cargo test -- --nocapture"), Some("cargo".into()));
        assert_eq!(program("./build.sh now"), Some("build.sh".into()));
        assert_eq!(program("cd x && rm -rf y"), None);
        assert_eq!(program("cargo test | tee out"), None);

        // A write is the folder it lands in, and a file at the root is itself.
        assert_eq!(path_root("src/lib/a.ts"), Some("src".into()));
        assert_eq!(path_root("src\\lib\\a.ts"), Some("src".into()));
        assert_eq!(path_root("README.md"), Some("README.md".into()));
        // Nothing that could point outside the workspace gets a target at all.
        assert_eq!(path_root("../secrets"), None);
        assert_eq!(path_root("C:/Windows/system32"), None);

        // And the whole-call rule, tool by tool.
        assert_eq!(
            target("run_command", &json!({"command": "cargo build"}), None),
            Some("cargo".into())
        );
        assert_eq!(target("git_push", &json!({}), None), None);
        assert_eq!(
            target("write_file", &json!({"path": "src/a"}), None),
            Some("src".into())
        );
        assert_eq!(
            target("ui_click", &json!({"window": "notepad"}), None),
            Some("notepad".into())
        );
        // A browser action's target is the page it is about to act on, and the
        // page is only known when there is one open.
        assert_eq!(
            target("browser_click", &json!({"selector": "#go"}), Some("https://x.test/p")),
            Some("x.test".into())
        );
        assert_eq!(target("browser_click", &json!({"selector": "#go"}), None), None);
        assert_eq!(
            target("browser_click", &json!({"selector": "#go"}), Some("about:blank")),
            None
        );
        assert!(target_is_the_page("browser_type"));
        assert!(!target_is_the_page("run_command"));
    }

    #[test]
    fn a_target_cannot_smuggle_a_separator_into_the_key() {
        use serde_json::json;
        // The model chooses both the tool name and, sometimes, the text a target
        // is made of. A value carrying the key's own separator could otherwise
        // make one grant read as another's, so the separator is stripped out.
        let forged = target("ui_click", &json!({ "window": "run_command\u{1f}cargo" }), None);
        assert_eq!(forged, Some("run_commandcargo".into()));
        assert_ne!(
            key("/repo", "ui_click", forged.as_deref().unwrap()),
            key("/repo", "run_command", "cargo"),
        );
    }

    #[test]
    fn session_grants_are_scoped_per_workspace_tool_and_target() {
        let approvals = Approvals::default();
        approvals.grant_session("/repo", "run_command", "cargo");
        assert!(approvals.has_session_grant("/repo", "run_command", "cargo"));
        // Exactly its own target, folder and tool.
        assert!(!approvals.has_session_grant("/repo", "run_command", "npm"));
        assert!(!approvals.has_session_grant("/repo", "write_file", "cargo"));
        assert!(!approvals.has_session_grant("/other", "run_command", "cargo"));
        approvals.clear_session();
        assert!(!approvals.has_session_grant("/repo", "run_command", "cargo"));
    }
}
