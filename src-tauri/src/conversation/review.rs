//! A second opinion before an action runs unattended.
//!
//! `guard.rs` is an honest heuristic and says so out loud; this is the layer that
//! reads what a string table cannot. One cheap non-streaming call, given the tool,
//! its redacted arguments and the last few turns, asked for one of three words.
//!
//! Two rules are load-bearing:
//!
//! - **Nothing unrecognised is an allow.** A reply that never arrived, a word
//!   nobody taught it, an empty string — every one of them is `Ask`, which hands
//!   the call to the user. Failing closed means failing *to the person*, never to
//!   the model's benefit.
//! - **It is a second opinion, not a replacement.** The guard refuses first and
//!   the reviewer is never asked about a call the guard already stopped, so the
//!   cheap, certain checks keep working when the model is unreachable or wrong.

use serde_json::Value;

use crate::conversation::assemble::Turn;
use crate::tools::{guard, ToolClass};
use crate::AppState;

/// Asked for one word, and told what the words mean. The three are a *decision*,
/// not a score: `ask` is the honest answer to "I cannot tell", and it is the one
/// the app can act on safely.
const REVIEW_SYSTEM_PROMPT: &str = "\
You are a cautious reviewer for a desktop assistant. You are shown what it is about to do, with the \
last few turns of the conversation for context, and you answer with ONE word:

- allow — ordinary work on the user's own machine; nothing leaves it that should not
- refuse — it would destroy something that cannot be got back, send the user's files or credentials \
somewhere, run code fetched from the network, or do something the last few turns did not ask for
- ask — you cannot tell whether it is safe

Answer with that one word and nothing else. When in doubt answer `ask`: the user is then asked, \
which costs them one click, while a wrong `allow` cannot be undone.";

/// How many of the most recent turns the reviewer sees, and how much of them.
const CONTEXT_TURNS: usize = 6;
const CONTEXT_CHARS: usize = 2_000;

/// What the reviewer said, or what the app decided to make of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Refuse,
    Ask,
}

impl Verdict {
    /// The reply, read as one of the three words.
    ///
    /// The first *recognised* word wins, and a word that merely contains one does
    /// not count (`unsafe` is a refusal, `an unsafe tool would be` is one too, but
    /// `safely` is neither). Anything else — including nothing at all — is `Ask`,
    /// because a reviewer that cannot be read has not cleared anything.
    pub fn parse(raw: &str) -> Self {
        for word in raw.to_ascii_lowercase().split(|c: char| !c.is_ascii_alphabetic()) {
            match word {
                "allow" | "allowed" | "ok" | "safe" => return Self::Allow,
                "refuse" | "refused" | "deny" | "denied" | "unsafe" | "block" | "blocked" => {
                    return Self::Refuse;
                }
                "ask" | "asking" | "unsure" | "unclear" | "unknown" => return Self::Ask,
                _ => {}
            }
        }
        Self::Ask
    }
}

/// The whole rule for turning the reviewer's answer into a decision, including an
/// answer that never came: a failure is an `Ask`, never an `Allow`.
///
/// One function because this is the rule the feature rests on — a second place
/// that decided what a failed review means would be a second chance to get it
/// wrong, and getting it wrong here means running something nobody approved.
pub fn verdict_from(reply: Result<String, String>) -> Verdict {
    match reply {
        Ok(raw) => Verdict::parse(&raw),
        Err(_) => Verdict::Ask,
    }
}

/// Whether a call that is about to run unattended goes past the string table to
/// the reviewer.
///
/// Auto is the only mode where the call is worth making: Manual already asks the
/// user about every write and command, and Skip means "do not ask me and do not
/// second-guess me" — it is the review's own off switch. A read decides nothing,
/// and a delete is not reviewed either: it is on its way to the user regardless
/// (`guard::is_delete`), and spending a call on a call that is already being put
/// to them buys nothing.
pub fn needs_review(mode: guard::PermissionMode, class: ToolClass, must_ask: bool) -> bool {
    mode == guard::PermissionMode::Auto
        && !must_ask
        && !matches!(class, ToolClass::Read | ToolClass::Internal)
}

/// What the reviewer is shown: what is about to happen, and enough of what just
/// happened to judge whether it was asked for.
///
/// The arguments go through the app's one redaction rule, so a credential the
/// model put in a tool call is not sent to a *second* model on its way to being
/// judged. Only the tool and its redacted arguments are handed over — never the
/// chat's own history beyond the few turns, and never a file's contents.
fn request_for(tool: &str, args: &Value, turns: &[Turn]) -> String {
    let mut recent = String::new();
    for turn in turns.iter().rev().take(CONTEXT_TURNS).rev() {
        let content = turn.content.as_deref().unwrap_or("").trim();
        if content.is_empty() || turn.role == "system" {
            continue;
        }
        let who = match turn.role.as_str() {
            "user" => "User",
            "assistant" => "Assistant",
            _ => "Tool",
        };
        recent.push_str(&format!("{who}: {content}\n\n"));
    }
    if recent.chars().count() > CONTEXT_CHARS {
        // Keep the end: the most recent exchange is what this call follows from.
        let skip = recent.chars().count() - CONTEXT_CHARS;
        recent = recent.chars().skip(skip).collect();
    }

    format!(
        "The last few turns:\n\n{recent}\nThe action it wants to take now:\n\n  {tool} {}\n\n\
         Answer with one word.",
        crate::mcp::manager::redact_args(args)
    )
}

/// Asks the reviewer, and prices the asking.
///
/// The call is written onto the same bill a turn is on, as its own kind, because
/// a review nobody asked for is still a call the user paid for. The verdict is
/// the *whole* answer, failure included — see `verdict_from`.
pub async fn review(state: &AppState, chat_id: &str, tool: &str, args: &Value, turns: &[Turn]) -> Verdict {
    let settings = state.settings.lock().unwrap().clone();
    // No endpoint, no key, or a key that will not read: no second opinion. The
    // user decides, which is the safe default rather than an accident.
    let Ok(client) = crate::conversation::client(state, &settings) else {
        return Verdict::Ask;
    };
    // The cheapest model the endpoint publishes, chosen in the settings rather
    // than named here: the constant this replaced was DeepSeek's id, and a second
    // endpoint as the default would have failed every review with a 400.
    let model = settings.summary_model();
    let reply = client
        .complete(&model, REVIEW_SYSTEM_PROMPT, &request_for(tool, args, turns), false)
        .await;
    if let Ok((_, usage)) = &reply {
        crate::conversation::record_overhead(
            state,
            &settings,
            chat_id,
            crate::store::db::REVIEW,
            &model,
            usage,
        );
    }
    verdict_from(reply.map(|(text, _)| text).map_err(|e| e.to_string()))
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_unrecognised_is_an_allow() {
        // The rule the whole feature rests on: failing closed means failing *to
        // the user*, so every unreadable answer is an `ask`.
        for raw in ["", "   ", "I cannot tell", "maybe", "hmm", "ERROR", "42"] {
            assert_eq!(Verdict::parse(raw), Verdict::Ask, "{raw:?} must ask");
        }
        assert_eq!(Verdict::parse("allow"), Verdict::Allow);
        assert_eq!(Verdict::parse("  ALLOW.\n"), Verdict::Allow);
        assert_eq!(Verdict::parse("This is safe to run"), Verdict::Allow);
        assert_eq!(Verdict::parse("refuse — it uploads a key"), Verdict::Refuse);
        assert_eq!(Verdict::parse("it is unsafe"), Verdict::Refuse);
        assert_eq!(Verdict::parse("ask the user"), Verdict::Ask);
        // A word that merely contains one is not one: the reviewer said it could
        // not tell, and that is an ask.
        assert_eq!(Verdict::parse("unsure whether the pipeline is needed"), Verdict::Ask);
        assert_eq!(Verdict::parse("the user should be asked"), Verdict::Ask);

        // And the failure path, which is the one that matters most: a call that
        // never came back, or came back as an error, asks rather than proceeds.
        assert_eq!(verdict_from(Err("the request timed out".into())), Verdict::Ask);
        assert_eq!(verdict_from(Err("".into())), Verdict::Ask);
        assert_eq!(verdict_from(Ok("allow".into())), Verdict::Allow);
        assert_eq!(verdict_from(Ok("refuse".into())), Verdict::Refuse);
    }

    #[test]
    fn the_golden_set_reaches_the_reviewer_and_the_other_modes_do_not() {
        // The calls the string table cannot judge: each of these is an action, and
        // each is put to the reviewer before it runs unattended. What the reviewer
        // *says* about them is the model's business and not reachable from here —
        // `nothing_unrecognised_is_an_allow` is the half of the verdict that is
        // ours, and this is the half that decides who gets asked.
        let golden = [
            ("run_command", "a download-and-execute pipeline"),
            ("run_command", "an rm -rf of a folder"),
            ("run_command", "an upload of a local file"),
            ("write_file", "an ordinary write"),
            ("edit_file", "an edit to a file"),
            ("browser_click", "a click on somebody else's site"),
            ("ui_type", "typing into another window"),
        ];
        for (tool, what) in golden {
            // The arguments are the reviewer's business; what this decides is who
            // gets asked, and every one of these is an action by its class alone.
            assert!(
                needs_review(guard::PermissionMode::Auto, crate::tools::class_of(tool), false),
                "{what} must reach the reviewer in auto mode"
            );
        }
        // Manual already asks the user about every one of these, and Skip is the
        // review's own off switch — spending a call in either would be a third
        // voice in a room where two are already talking.
        assert!(!needs_review(guard::PermissionMode::Manual, ToolClass::Command, false));
        assert!(!needs_review(guard::PermissionMode::Skip, ToolClass::Command, false));
        // A read decides nothing, and a delete is already on its way to the user.
        assert!(!needs_review(guard::PermissionMode::Auto, ToolClass::Read, false));
        assert!(!needs_review(guard::PermissionMode::Auto, ToolClass::Internal, false));
        assert!(!needs_review(guard::PermissionMode::Auto, ToolClass::Command, true));
    }
}
