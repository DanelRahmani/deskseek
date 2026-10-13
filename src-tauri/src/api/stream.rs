//! Server-sent-events parsing and turn accumulation.
//!
//! Both pieces are pure so they can be unit-tested without a network, and both
//! work in the app's own terms — `WirePart`s — rather than in either dialect's:
//! that is what lets one accumulator serve two wire shapes and stops a second
//! dialect from becoming a second loop.

use std::collections::BTreeMap;

use crate::api::provider::WirePart;
use crate::api::types::{ChatMessage, Content, ToolCall};

/// Incremental `data:` line extractor. Feed it raw response bytes; it returns
/// every complete payload (including the literal `[DONE]` terminator).
///
/// Works on bytes rather than `&str` because reqwest yields arbitrary chunk
/// boundaries that can split a multi-byte UTF-8 character.
#[derive(Default)]
pub struct SseBuffer {
    buf: Vec<u8>,
}

impl SseBuffer {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        self.buf.extend_from_slice(bytes);
        let mut out = Vec::new();
        let mut start = 0usize;
        while let Some(rel) = self.buf[start..].iter().position(|b| *b == b'\n') {
            let end = start + rel;
            let line = String::from_utf8_lossy(&self.buf[start..end]);
            let line = line.trim_end_matches('\r');
            if let Some(rest) = line.strip_prefix("data:") {
                let payload = rest.trim();
                if !payload.is_empty() {
                    out.push(payload.to_string());
                }
            }
            start = end + 1;
        }
        if start > 0 {
            self.buf.drain(..start);
        }
        out
    }
}

/// The text added by a single payload, for forwarding to the UI.
#[derive(Default, Debug, PartialEq)]
pub struct AppliedDelta {
    pub content: String,
    pub reasoning: String,
}

/// Rebuilds one assistant message from the streamed parts. Tool calls arrive
/// fragmented, so they are merged by the dialect's own index.
#[derive(Default)]
pub struct MessageAccumulator {
    pub content: String,
    pub reasoning: String,
    /// Keyed by the dialect's index rather than held in a list.
    ///
    /// Anthropic numbers content blocks, so a text block occupies 0 and the first
    /// tool call arrives at 1: a list would then carry an empty slot at the front
    /// and hand the model a tool call with no name. Keying on the number and
    /// emitting in order is right for both, because OpenAI's zero-based numbering
    /// is a special case of it.
    tool_calls: BTreeMap<usize, ToolCall>,
}

impl MessageAccumulator {
    pub fn apply(&mut self, parts: &[WirePart]) -> AppliedDelta {
        let mut applied = AppliedDelta::default();
        for part in parts {
            match part {
                WirePart::Text(text) => {
                    self.content.push_str(text);
                    applied.content.push_str(text);
                }
                WirePart::Reasoning(text) => {
                    self.reasoning.push_str(text);
                    applied.reasoning.push_str(text);
                }
                WirePart::ToolDelta {
                    index,
                    id,
                    name,
                    args,
                } => self.merge_tool_call(*index, id.as_deref(), name.as_deref(), args.as_deref()),
                // Usage and the stop reason are the loop's business, not the
                // message's.
                WirePart::Usage(_) | WirePart::Finish(_) => {}
            }
        }
        applied
    }

    fn merge_tool_call(
        &mut self,
        index: usize,
        id: Option<&str>,
        name: Option<&str>,
        args: Option<&str>,
    ) {
        let slot = self.tool_calls.entry(index).or_insert_with(|| {
            ToolCall {
                id: String::new(),
                kind: "function".to_string(),
                function: crate::api::types::FunctionCall {
                    name: String::new(),
                    arguments: String::new(),
                },
            }
        });
        if let Some(id) = id {
            slot.id = id.to_string();
        }
        if let Some(name) = name {
            slot.function.name.push_str(name);
        }
        if let Some(args) = args {
            slot.function.arguments.push_str(args);
        }
    }

    pub fn into_message(self) -> ChatMessage {
        let has_tools = !self.tool_calls.is_empty();
        ChatMessage {
            role: "assistant".to_string(),
            content: if self.content.is_empty() {
                None
            } else {
                Some(Content::text(self.content))
            },
            reasoning_content: if self.reasoning.is_empty() {
                None
            } else {
                Some(self.reasoning)
            },
            tool_calls: if has_tools {
                Some(self.tool_calls.into_values().collect())
            } else {
                None
            },
            tool_call_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_buffer_emits_only_complete_lines_and_survives_split_chunks() {
        let mut buf = SseBuffer::default();
        // Split mid-payload and mid-UTF8-agnostic: only complete lines come out.
        assert!(buf.push(b"data: {\"a\":1}").is_empty());
        let first = buf.push(b"\n\ndata: {\"b\":2}\n");
        assert_eq!(first, vec!["{\"a\":1}".to_string(), "{\"b\":2}".to_string()]);
        let done = buf.push(b"data: [DONE]\n");
        assert_eq!(done, vec!["[DONE]".to_string()]);
    }

    #[test]
    fn sse_buffer_ignores_comment_and_blank_lines() {
        let mut buf = SseBuffer::default();
        let out = buf.push(b": keep-alive\n\nnot-data: x\n");
        assert!(out.is_empty());
    }

    #[test]
    fn accumulator_joins_content_and_reasoning_parts() {
        let mut acc = MessageAccumulator::default();
        acc.apply(&[WirePart::Reasoning("think".to_string())]);
        acc.apply(&[WirePart::Text("Hel".to_string())]);
        let applied = acc.apply(&[WirePart::Text("lo".to_string())]);
        assert_eq!(applied.content, "lo");
        assert_eq!(acc.content, "Hello");
        assert_eq!(acc.reasoning, "think");
    }

    #[test]
    fn tool_call_arguments_are_merged_across_parts() {
        let mut acc = MessageAccumulator::default();
        acc.apply(&[WirePart::ToolDelta {
            index: 0,
            id: Some("call_1".to_string()),
            name: Some("read_file".to_string()),
            args: Some("{\"pa".to_string()),
        }]);
        acc.apply(&[WirePart::ToolDelta {
            index: 0,
            id: None,
            name: None,
            args: Some("th\":\"a.txt\"}".to_string()),
        }]);
        let msg = acc.into_message();
        let calls = msg.tool_calls.expect("tool calls present");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_1");
        assert_eq!(calls[0].function.name, "read_file");
        assert_eq!(calls[0].kind, "function");
        assert_eq!(calls[0].function.arguments, r#"{"path":"a.txt"}"#);
    }

    #[test]
    fn two_parallel_tool_calls_keep_their_own_indices() {
        let mut acc = MessageAccumulator::default();
        acc.apply(&[
            WirePart::ToolDelta {
                index: 0,
                id: Some("a".to_string()),
                name: Some("one".to_string()),
                args: Some("{}".to_string()),
            },
            WirePart::ToolDelta {
                index: 1,
                id: Some("b".to_string()),
                name: Some("two".to_string()),
                args: Some("{}".to_string()),
            },
        ]);
        let calls = acc.into_message().tool_calls.unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].function.name, "one");
        assert_eq!(calls[1].function.name, "two");
    }

    #[test]
    fn a_tool_call_at_a_block_offset_does_not_leave_an_empty_one_behind() {
        // The Anthropic case: a text block took index 0, so the first tool call is
        // at 1. A list would carry a nameless call at the front and send the model
        // a tool that does not exist.
        let mut acc = MessageAccumulator::default();
        acc.apply(&[
            WirePart::Text("let me look".to_string()),
            WirePart::ToolDelta {
                index: 1,
                id: Some("toolu_1".to_string()),
                name: Some("read_file".to_string()),
                args: Some("{}".to_string()),
            },
        ]);
        let calls = acc.into_message().tool_calls.unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "read_file");
        assert_eq!(calls[0].id, "toolu_1");
    }

    #[test]
    fn no_tool_calls_yields_none() {
        let mut acc = MessageAccumulator::default();
        acc.apply(&[WirePart::Text("hi".to_string())]);
        assert!(acc.into_message().tool_calls.is_none());
    }
}
