//! Server-sent-events parsing and delta accumulation.
//!
//! Both pieces are pure so they can be unit-tested without a network.

use crate::api::types::{ChatMessage, Content, StreamChunk, ToolCall, ToolCallDelta};

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

/// The text added by a single chunk, for forwarding to the UI.
#[derive(Default, Debug, PartialEq)]
pub struct AppliedDelta {
    pub content: String,
    pub reasoning: String,
}

/// Rebuilds one assistant message from the streamed deltas. Tool calls arrive
/// fragmented, so they are merged by `index`.
#[derive(Default)]
pub struct MessageAccumulator {
    pub content: String,
    pub reasoning: String,
    tool_calls: Vec<ToolCall>,
}

impl MessageAccumulator {
    pub fn apply(&mut self, chunk: &StreamChunk) -> AppliedDelta {
        let mut applied = AppliedDelta::default();
        for choice in &chunk.choices {
            if let Some(c) = &choice.delta.content {
                if !c.is_empty() {
                    self.content.push_str(c);
                    applied.content.push_str(c);
                }
            }
            if let Some(r) = &choice.delta.reasoning_content {
                if !r.is_empty() {
                    self.reasoning.push_str(r);
                    applied.reasoning.push_str(r);
                }
            }
            if let Some(calls) = &choice.delta.tool_calls {
                for call in calls {
                    self.merge_tool_call(call);
                }
            }
        }
        applied
    }

    fn merge_tool_call(&mut self, delta: &ToolCallDelta) {
        while self.tool_calls.len() <= delta.index {
            self.tool_calls.push(ToolCall::new(String::new()));
        }
        let slot = &mut self.tool_calls[delta.index];
        if let Some(id) = &delta.id {
            if !id.is_empty() {
                slot.id = id.clone();
            }
        }
        if let Some(func) = &delta.function {
            if let Some(name) = &func.name {
                slot.function.name.push_str(name);
            }
            if let Some(args) = &func.arguments {
                slot.function.arguments.push_str(args);
            }
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
            tool_calls: if has_tools { Some(self.tool_calls) } else { None },
            tool_call_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(json: &str) -> StreamChunk {
        serde_json::from_str(json).expect("valid chunk")
    }

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
    fn accumulator_joins_content_and_reasoning_deltas() {
        let mut acc = MessageAccumulator::default();
        acc.apply(&chunk(r#"{"choices":[{"delta":{"reasoning_content":"think"}}]}"#));
        acc.apply(&chunk(r#"{"choices":[{"delta":{"content":"Hel"}}]}"#));
        let applied = acc.apply(&chunk(r#"{"choices":[{"delta":{"content":"lo"}}]}"#));
        assert_eq!(applied.content, "lo");
        assert_eq!(acc.content, "Hello");
        assert_eq!(acc.reasoning, "think");
    }

    #[test]
    fn tool_call_arguments_are_merged_across_chunks() {
        let mut acc = MessageAccumulator::default();
        acc.apply(&chunk(
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"get_weather","arguments":"{\"loc"}}]}}]}"#,
        ));
        acc.apply(&chunk(
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"ation\":\"Paris\"}"}}]}}]}"#,
        ));
        let msg = acc.into_message();
        let calls = msg.tool_calls.expect("tool calls present");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_1");
        assert_eq!(calls[0].function.name, "get_weather");
        assert_eq!(calls[0].function.arguments, r#"{"location":"Paris"}"#);
    }

    #[test]
    fn two_parallel_tool_calls_keep_their_own_indices() {
        let mut acc = MessageAccumulator::default();
        acc.apply(&chunk(
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"a","function":{"name":"one","arguments":"{}"}},{"index":1,"id":"b","function":{"name":"two","arguments":"{}"}}]}}]}"#,
        ));
        let calls = acc.into_message().tool_calls.unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].function.name, "one");
        assert_eq!(calls[1].function.name, "two");
    }

    #[test]
    fn no_tool_calls_yields_none() {
        let mut acc = MessageAccumulator::default();
        acc.apply(&chunk(r#"{"choices":[{"delta":{"content":"hi"}}]}"#));
        assert!(acc.into_message().tool_calls.is_none());
    }
}
