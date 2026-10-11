//! Assembles the `messages` array sent to the API.
//!
//! The one subtle rule lives here: DeepSeek requires `reasoning_content` to be
//! echoed back on every prior assistant turn **when the request carries
//! `tools`**, and ignores it otherwise. Getting this wrong is a silent 400, so
//! it is isolated in a pure function and covered by tests.

use crate::api::types::{ChatMessage, ToolCall};

/// A stored conversation turn, decoupled from the database row type.
#[derive(Debug, Clone, Default)]
pub struct Turn {
    pub role: String,
    pub content: Option<String>,
    pub reasoning_content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_call_id: Option<String>,
}

/// Base instructions plus the always-injected memory file.
pub fn system_with_memory(base: &str, memory: Option<&str>) -> String {
    let mut out = String::new();
    let base = base.trim();
    if !base.is_empty() {
        out.push_str(base);
    }
    if let Some(memory) = memory {
        let memory = memory.trim();
        if !memory.is_empty() {
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            out.push_str("# Memory\n\n");
            out.push_str(memory);
        }
    }
    out
}

pub fn build_messages(
    system: &str,
    history: &[Turn],
    user_text: &str,
    include_reasoning: bool,
) -> Vec<ChatMessage> {
    let mut out = Vec::with_capacity(history.len() + 2);

    out.push(ChatMessage {
        role: "system".to_string(),
        content: Some(system.to_string()),
        ..Default::default()
    });

    for turn in history {
        let mut msg = ChatMessage {
            role: turn.role.clone(),
            content: turn.content.clone(),
            ..Default::default()
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

    out.push(ChatMessage {
        role: "user".to_string(),
        content: Some(user_text.to_string()),
        ..Default::default()
    });

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
    fn reasoning_is_stripped_when_no_tools_are_sent() {
        let msgs = build_messages("sys", &[assistant_with_reasoning()], "next", false);
        let asst = msgs.iter().find(|m| m.role == "assistant").unwrap();
        assert!(asst.reasoning_content.is_none());
    }

    #[test]
    fn reasoning_is_echoed_when_tools_are_sent() {
        let msgs = build_messages("sys", &[assistant_with_reasoning()], "next", true);
        let asst = msgs.iter().find(|m| m.role == "assistant").unwrap();
        assert_eq!(asst.reasoning_content.as_deref(), Some("because"));
    }

    #[test]
    fn shape_is_system_history_user() {
        let msgs = build_messages("sys", &[assistant_with_reasoning()], "hello", false);
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].role, "system");
        assert_eq!(msgs[1].role, "assistant");
        assert_eq!(msgs[2].role, "user");
        assert_eq!(msgs[2].content.as_deref(), Some("hello"));
    }

    #[test]
    fn tool_messages_keep_their_call_id() {
        let turn = Turn {
            role: "tool".to_string(),
            content: Some("24C".to_string()),
            tool_call_id: Some("call_1".to_string()),
            ..Default::default()
        };
        let msgs = build_messages("sys", &[turn], "hi", true);
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
        let msgs = build_messages("sys", &[turn], "hi", true);
        assert_eq!(msgs[1].tool_calls.as_ref().unwrap()[0].function.name, "get_weather");
    }
}
