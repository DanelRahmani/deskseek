//! Assembles the `messages` array sent to the API.
//!
//! The one subtle rule lives here: DeepSeek requires `reasoning_content` to be
//! echoed back on every prior assistant turn **when the request carries
//! `tools`**, and ignores it otherwise. Getting this wrong is a silent 400, so
//! it is isolated in a pure function and covered by tests.

use crate::api::types::{Attachment, ChatMessage, Content, ContentPart, ImageUrl, ToolCall};

/// A stored conversation turn, decoupled from the database row type.
#[derive(Debug, Clone, Default)]
pub struct Turn {
    pub role: String,
    pub content: Option<String>,
    pub reasoning_content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_call_id: Option<String>,
    pub attachments: Option<Vec<Attachment>>,
}

/// The always-injected profile exactly as it is appended, or None when there is
/// none to append.
///
/// Split out from `system_with_memory` so a caller can measure the block as well
/// as include it: the context panel reports its size, and a size taken from a
/// second format string would be a copy that could drift out of step with the
/// text actually sent.
pub fn memory_block(memory: Option<&str>) -> Option<String> {
    let memory = memory?.trim();
    if memory.is_empty() {
        return None;
    }
    Some(format!("# Memory\n\n{memory}"))
}

/// Base instructions plus the always-injected memory file.
pub fn system_with_memory(base: &str, memory: Option<&str>) -> String {
    let base = base.trim();
    match memory_block(memory) {
        Some(block) if base.is_empty() => block,
        Some(block) => format!("{base}\n\n{block}"),
        None => base.to_string(),
    }
}

pub fn build_messages(
    system: &str,
    history: &[Turn],
    user_text: Option<&str>,
    include_reasoning: bool,
) -> Vec<ChatMessage> {
    let mut out = Vec::with_capacity(history.len() + 2);

    out.push(ChatMessage {
        role: "system".to_string(),
        content: Some(Content::text(system)),
        ..Default::default()
    });

    for turn in history {
        let mut msg = ChatMessage {
            role: turn.role.clone(),
            ..Default::default()
        };

        // Images become a parts array; everything else stays a plain string.
        let attachments = turn.attachments.as_deref().unwrap_or(&[]);
        let images: Vec<&Attachment> = attachments.iter().filter(|a| a.is_image()).collect();
        msg.content = if images.is_empty() {
            turn.content.clone().map(Content::text)
        } else {
            let mut parts = Vec::with_capacity(images.len() + 1);
            if let Some(text) = &turn.content {
                if !text.trim().is_empty() {
                    parts.push(ContentPart::Text { text: text.clone() });
                }
            }
            for image in images {
                parts.push(ContentPart::ImageUrl {
                    image_url: ImageUrl {
                        url: image.data_url.clone(),
                    },
                });
            }
            Some(Content::Parts(parts))
        };

        match turn.role.as_str() {
            "assistant" => {
                if include_reasoning {
                    msg.reasoning_content = turn.reasoning_content.clone();
                }
                msg.tool_calls = turn.tool_calls.clone();
            }
            "tool" => {
                msg.tool_call_id = turn.tool_call_id.clone();
            }
            _ => {}
        }
        out.push(msg);
    }

    if let Some(text) = user_text {
        out.push(ChatMessage {
            role: "user".to_string(),
            content: Some(Content::text(text)),
            ..Default::default()
        });
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::FunctionCall;

    fn assistant_with_reasoning() -> Turn {
        Turn {
            role: "assistant".to_string(),
            content: Some("answer".to_string()),
            reasoning_content: Some("because".to_string()),
            tool_calls: None,
            tool_call_id: None,
            attachments: None,
        }
    }

    #[test]
    fn system_prompt_gets_memory_appended() {
        let s = system_with_memory("You are helpful.", Some("User is called Dan."));
        assert!(s.starts_with("You are helpful."));
        assert!(s.contains("# Memory"));
        assert!(s.contains("User is called Dan."));
    }

    #[test]
    fn empty_memory_leaves_the_prompt_untouched() {
        assert_eq!(system_with_memory("Hi", Some("   ")), "Hi");
        assert_eq!(system_with_memory("Hi", None), "Hi");
    }

    #[test]
    fn the_caller_can_measure_the_memory_block_it_gets() {
        assert_eq!(memory_block(None), None);
        assert_eq!(memory_block(Some("   ")), None);
        assert_eq!(memory_block(Some("  Dan  ")).as_deref(), Some("# Memory\n\nDan"));

        // The panel measures the block and the prompt carries it, so the two
        // have to come from one place or the reported size would be a lie.
        let block = memory_block(Some("Dan")).unwrap();
        assert_eq!(system_with_memory("base", Some("Dan")), format!("base\n\n{block}"));

        // A base with nothing in it must not leave a stray separator behind.
        assert_eq!(system_with_memory("  ", Some("Dan")), block);
    }

    #[test]
    fn reasoning_is_stripped_when_no_tools_are_sent() {
        let msgs = build_messages("sys", &[assistant_with_reasoning()], Some("next"), false);
        let asst = msgs.iter().find(|m| m.role == "assistant").unwrap();
        assert!(asst.reasoning_content.is_none());
    }

    #[test]
    fn reasoning_is_echoed_when_tools_are_sent() {
        let msgs = build_messages("sys", &[assistant_with_reasoning()], Some("next"), true);
        let asst = msgs.iter().find(|m| m.role == "assistant").unwrap();
        assert_eq!(asst.reasoning_content.as_deref(), Some("because"));
    }

    #[test]
    fn shape_is_system_history_user() {
        let msgs = build_messages("sys", &[assistant_with_reasoning()], Some("hello"), false);
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].role, "system");
        assert_eq!(msgs[1].role, "assistant");
        assert_eq!(msgs[2].role, "user");
        assert_eq!(msgs[2].content.as_ref().and_then(Content::as_text), Some("hello"));
    }

    #[test]
    fn omitting_the_user_turn_leaves_the_stored_history_alone() {
        let msgs = build_messages("sys", &[assistant_with_reasoning()], None, false);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[1].role, "assistant");
    }

    #[test]
    fn an_image_turn_becomes_a_parts_array() {
        let turn = Turn {
            role: "user".to_string(),
            content: Some("what is this?".to_string()),
            attachments: Some(vec![Attachment {
                name: "cat.png".to_string(),
                mime: "image/png".to_string(),
                data_url: "data:image/png;base64,AAAA".to_string(),
            }]),
            ..Default::default()
        };
        let msgs = build_messages("sys", &[turn], None, false);
        match msgs[1].content.as_ref().unwrap() {
            Content::Parts(parts) => {
                assert_eq!(parts.len(), 2);
                assert!(matches!(parts[0], ContentPart::Text { .. }));
                assert!(matches!(parts[1], ContentPart::ImageUrl { .. }));
            }
            other => panic!("expected parts, got {other:?}"),
        }
    }

    #[test]
    fn a_non_image_attachment_does_not_force_a_parts_array() {
        let turn = Turn {
            role: "user".to_string(),
            content: Some("see attached".to_string()),
            attachments: Some(vec![Attachment {
                name: "notes.txt".to_string(),
                mime: "text/plain".to_string(),
                data_url: "data:text/plain;base64,AAAA".to_string(),
            }]),
            ..Default::default()
        };
        let msgs = build_messages("sys", &[turn], None, false);
        assert_eq!(msgs[1].content.as_ref().and_then(Content::as_text), Some("see attached"));
    }

    #[test]
    fn tool_messages_keep_their_call_id() {
        let turn = Turn {
            role: "tool".to_string(),
            content: Some("24C".to_string()),
            tool_call_id: Some("call_1".to_string()),
            ..Default::default()
        };
        let msgs = build_messages("sys", &[turn], Some("hi"), true);
        assert_eq!(msgs[1].tool_call_id.as_deref(), Some("call_1"));
    }

    #[test]
    fn assistant_tool_calls_survive_the_round_trip() {
        let turn = Turn {
            role: "assistant".to_string(),
            content: None,
            tool_calls: Some(vec![ToolCall {
                id: "c1".to_string(),
                kind: "function".to_string(),
                function: FunctionCall {
                    name: "get_weather".to_string(),
                    arguments: "{}".to_string(),
                },
            }]),
            ..Default::default()
        };
        let msgs = build_messages("sys", &[turn], Some("hi"), true);
        assert_eq!(msgs[1].tool_calls.as_ref().unwrap()[0].function.name, "get_weather");
    }
}
