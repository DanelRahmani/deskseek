//! Tracks questions the model has asked the user mid-run.
//!
//! The agent loop parks on a oneshot channel; the `respond_question` command
//! resolves it. The same shape as `approvals`, because it is the same problem: a
//! tool call that cannot finish until the person at the keyboard answers it.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

/// One answer, tied to its question by index so a sparse reply still lines up.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QuestionAnswer {
    pub index: usize,
    /// The option labels the user picked. Empty when they typed instead.
    pub selected: Vec<String>,
    /// What the user typed, when they did.
    pub freeform: String,
}

/// What the user sent back. `cancelled` when they dismissed the question, so the
/// model is told plainly rather than handed an empty answer it might misread.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Answer {
    pub answers: Vec<QuestionAnswer>,
    pub cancelled: bool,
}

#[derive(Default)]
pub struct Questions {
    pending: Mutex<HashMap<String, oneshot::Sender<Answer>>>,
}

impl Questions {
    /// Registers a pending question and returns the receiver to await.
    pub fn register(&self, request_id: &str) -> oneshot::Receiver<Answer> {
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(request_id.to_string(), tx);
        rx
    }

    /// Resolves a pending question. Returns false if it was already answered.
    pub fn respond(&self, request_id: &str, answer: Answer) -> bool {
        let sender = self.pending.lock().unwrap().remove(request_id);
        match sender {
            Some(tx) => tx.send(answer).is_ok(),
            None => false,
        }
    }

    /// Drops every pending question — used when the app is going away.
    pub fn clear(&self) {
        self.pending.lock().unwrap().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_registered_question_receives_the_answer() {
        let questions = Questions::default();
        let rx = questions.register("q1");
        assert!(questions.respond(
            "q1",
            Answer {
                answers: vec![QuestionAnswer {
                    index: 0,
                    selected: vec!["Rust".into()],
                    freeform: String::new(),
                }],
                cancelled: false,
            },
        ));
        let got = rx.await.expect("the answer arrives");
        assert_eq!(got.answers[0].selected, vec!["Rust".to_string()]);
        assert!(!got.cancelled);
    }

    #[tokio::test]
    async fn answering_twice_is_refused() {
        let questions = Questions::default();
        let _rx = questions.register("q1");
        assert!(questions.respond("q1", Answer::default()));
        assert!(!questions.respond("q1", Answer::default()));
    }

    #[tokio::test]
    async fn an_unknown_id_is_not_an_error_to_swallow_silently() {
        let questions = Questions::default();
        assert!(!questions.respond("never-asked", Answer::default()));
    }
}
