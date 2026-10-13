//! The OpenAI-compatible wire shape.
//!
//! This is the shape the app shipped with, kept as the reference implementation
//! of the dialect rather than replaced. The `thinking` field and
//! `reasoning_content` echoing are DeepSeek's own extensions to the dialect; a
//! local server or a gateway ignores unknown fields, and a server that rejects
//! them would be a third dialect — a dialect, not a vendor client, which is the
//! distinction the whole provider layer rests on.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::api::provider::{join, DialectImpl, Model, WirePart};
use crate::api::types::{ChatMessage, ChatRequest, Content, StreamChunk, Usage};

pub struct OpenAi;

pub const OPENAI: OpenAi = OpenAi;

impl DialectImpl for OpenAi {
    fn chat_url(&self, base: &str) -> String {
        join(base, "chat/completions")
    }

    fn models_url(&self, base: &str) -> String {
        join(base, "models")
    }

    fn headers(&self, key: Option<&str>) -> Vec<(&'static str, String)> {
        let mut headers = vec![("content-type", "application/json".to_string())];
        if let Some(key) = key.map(str::trim).filter(|k| !k.is_empty()) {
            headers.push(("authorization", format!("Bearer {key}")));
        }
        headers
    }

    fn body(&self, _model: &Model, req: &ChatRequest, messages: &[ChatMessage]) -> Value {
        // Serialising the request is the shape, with one substitution: the
        // messages are the ones the model can actually take, so an endpoint that
        // cannot see is never handed an image part.
        let mut body = serde_json::to_value(req).unwrap_or_else(|_| json!({}));
        body["messages"] = serde_json::to_value(messages).unwrap_or_else(|_| json!([]));
        body
    }

    fn nonstream_body(&self, model: &str, system: &str, user: &str, json_mode: bool) -> Value {
        json!({
            "model": model,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": user }
            ],
            "stream": false,
            "thinking": { "type": "disabled" },
            "response_format": { "type": if json_mode { "json_object" } else { "text" } },
            "max_tokens": 2048
        })
    }

    fn parts(&self, data: &str) -> Vec<WirePart> {
        let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) else {
            // A payload this dialect does not understand is skipped rather than
            // aborting the answer: an endpoint that adds a field must not break a
            // stream, and a keep-alive must not look like one.
            return Vec::new();
        };
        let mut out = Vec::new();
        if let Some(usage) = &chunk.usage {
            out.push(WirePart::Usage(usage.clone()));
        }
        for choice in &chunk.choices {
            if let Some(reason) = choice.finish_reason.as_deref().filter(|r| !r.is_empty()) {
                out.push(WirePart::Finish(reason.to_string()));
            }
            if let Some(text) = choice.delta.content.as_deref().filter(|t| !t.is_empty()) {
                out.push(WirePart::Text(text.to_string()));
            }
            if let Some(thought) = choice
                .delta
                .reasoning_content
                .as_deref()
                .filter(|t| !t.is_empty())
            {
                out.push(WirePart::Reasoning(thought.to_string()));
            }
            for call in choice.delta.tool_calls.iter().flatten() {
                out.push(WirePart::ToolDelta {
                    index: call.index,
                    id: call.id.clone().filter(|id| !id.is_empty()),
                    name: call
                        .function
                        .as_ref()
                        .and_then(|f| f.name.clone())
                        .filter(|name| !name.is_empty()),
                    args: call
                        .function
                        .as_ref()
                        .and_then(|f| f.arguments.clone())
                        .filter(|args| !args.is_empty()),
                });
            }
        }
        out
    }

    fn reply(&self, body: &Value) -> Option<(String, Usage)> {
        let parsed: CompletionResponse = serde_json::from_value(body.clone()).ok()?;
        let text = parsed
            .choices
            .into_iter()
            .next()
            .and_then(|choice| choice.message.content)
            .map(|content| match content {
                Content::Text(text) => text,
                // Purely defensive: a non-streaming reply has no reason to be
                // multimodal, but reading the text out of one costs nothing.
                Content::Parts(parts) => parts
                    .into_iter()
                    .filter_map(|part| match part {
                        crate::api::types::ContentPart::Text { text } => Some(text),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(""),
            })
            .filter(|text| !text.trim().is_empty())?;
        Some((text, parsed.usage.unwrap_or_default()))
    }
}

#[derive(Deserialize)]
struct CompletionResponse {
    #[serde(default)]
    choices: Vec<CompletionChoice>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct CompletionChoice {
    message: ChatMessage,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::provider::Dialect;
    use crate::api::types::{ContentPart, ImageUrl};

    fn request() -> ChatRequest {
        ChatRequest {
            model: "deepseek-flash".to_string(),
            messages: vec![ChatMessage {
                role: "system".to_string(),
                content: Some(Content::text("be brief")),
                ..Default::default()
            }],
            stream: true,
            stream_options: Some(crate::api::types::StreamOptions {
                include_usage: true,
            }),
            thinking: Some(crate::api::types::Thinking::enabled(true)),
            reasoning_effort: Some("high".to_string()),
            max_tokens: None,
            tools: None,
        }
    }

    fn model() -> Model {
        Model::unlisted("deepseek-flash")
    }

    #[test]
    fn the_wire_body_is_the_shape_the_endpoint_expects() {
        let req = request();
        let body = OPENAI.body(&model(), &req, &req.messages);
        // The wire shape, asserted exactly: this is the contract a test over a
        // mapper would miss and a real turn would find.
        assert_eq!(body["model"], "deepseek-flash");
        assert_eq!(body["stream"], true);
        assert_eq!(body["stream_options"]["include_usage"], true);
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["reasoning_effort"], "high");
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][0]["content"], "be brief");
        // Absent rather than null: a `max_tokens: null` is a 400 on more than one
        // OpenAI-compatible server.
        assert!(body.get("max_tokens").is_none());
        assert!(body.get("tools").is_none());
    }

    #[test]
    fn the_body_carries_the_messages_it_was_given_rather_than_the_requests() {
        // The one substitution: an endpoint that cannot see gets the stripped
        // list, which is the whole reason the caller passes a second one.
        let req = request();
        let stripped = vec![ChatMessage {
            role: "user".to_string(),
            content: Some(Content::Parts(vec![ContentPart::Text {
                text: "a screenshot".to_string(),
            }])),
            ..Default::default()
        }];
        let body = OPENAI.body(&model(), &req, &stripped);
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
        assert_eq!(body["messages"][0]["content"][0]["type"], "text");
    }

    #[test]
    fn an_image_part_survives_the_body_when_the_model_is_allowed_it() {
        let req = ChatRequest {
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: Some(Content::Parts(vec![ContentPart::ImageUrl {
                    image_url: ImageUrl {
                        url: "data:image/png;base64,AAAA".to_string(),
                    },
                }])),
                ..Default::default()
            }],
            ..request()
        };
        let body = OPENAI.body(&model(), &req, &req.messages);
        assert_eq!(body["messages"][0]["content"][0]["type"], "image_url");
        assert_eq!(
            body["messages"][0]["content"][0]["image_url"]["url"],
            "data:image/png;base64,AAAA"
        );
    }

    #[test]
    fn the_two_urls_are_built_from_the_base_without_doubling_a_slash() {
        assert_eq!(
            OPENAI.chat_url("https://api.deepseek.com/"),
            "https://api.deepseek.com/chat/completions"
        );
        assert_eq!(
            OPENAI.models_url("http://localhost:11434"),
            "http://localhost:11434/models"
        );
    }

    #[test]
    fn a_local_endpoint_sends_no_authorization_header_at_all() {
        let anon = OPENAI.headers(None);
        assert!(anon.iter().all(|(name, _)| *name != "authorization"));
        let keyed = OPENAI.headers(Some("sk-abc"));
        assert!(keyed
            .iter()
            .any(|(name, value)| *name == "authorization" && value == "Bearer sk-abc"));
        // A blank key is the same as no key, or the header would carry "Bearer "
        // and be read as an invalid one rather than as an anonymous request.
        assert!(OPENAI
            .headers(Some("  "))
            .iter()
            .all(|(name, _)| *name != "authorization"));
    }

    #[test]
    fn a_streamed_turn_is_read_out_of_its_deltas() {
        let mut parts = Vec::new();
        for payload in [
            r#"{"choices":[{"delta":{"reasoning_content":"think"}}]}"#,
            r#"{"choices":[{"delta":{"content":"Hel"}}]}"#,
            r#"{"choices":[{"delta":{"content":"lo"}}]}"#,
            r#"{"choices":[{"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":7,"completion_tokens":2,"total_tokens":9}}"#,
            "[DONE]",
        ] {
            parts.extend(OPENAI.parts(payload));
        }
        assert_eq!(
            parts,
            vec![
                WirePart::Reasoning("think".to_string()),
                WirePart::Text("Hel".to_string()),
                WirePart::Text("lo".to_string()),
                // Usage is a sibling of `choices` on this wire, so it is read out
                // before the stop reason even though the reason is what the
                // reader cares about. The two are not ordered relative to each
                // other anywhere that matters: the loop collects each.
                WirePart::Usage(Usage {
                    prompt_tokens: 7,
                    completion_tokens: 2,
                    total_tokens: 9,
                    ..Default::default()
                }),
                WirePart::Finish("stop".to_string()),
            ]
        );
    }

    #[test]
    fn a_tool_call_round_trips_across_fragmented_chunks() {
        let mut parts = Vec::new();
        for payload in [
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"read_file","arguments":"{\"pa"}}]}}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"th\":\"a.txt\"}"}}]}}]}"#,
        ] {
            parts.extend(OPENAI.parts(payload));
        }
        assert_eq!(
            parts,
            vec![
                WirePart::ToolDelta {
                    index: 0,
                    id: Some("call_1".to_string()),
                    name: Some("read_file".to_string()),
                    args: Some("{\"pa".to_string()),
                },
                WirePart::ToolDelta {
                    index: 0,
                    id: None,
                    name: None,
                    args: Some("th\":\"a.txt\"}".to_string()),
                },
            ]
        );
    }

    #[test]
    fn a_payload_this_dialect_does_not_know_is_skipped_rather_than_fatal() {
        assert!(OPENAI.parts("").is_empty());
        assert!(OPENAI.parts("[DONE]").is_empty());
        assert!(OPENAI.parts(": keep-alive").is_empty());
        assert!(OPENAI.parts("{\"choices\":[],\"something_new\":1}").is_empty());
    }

    #[test]
    fn a_nonstream_reply_yields_its_text_and_its_usage() {
        let body = json!({
            "choices": [{ "message": { "role": "assistant", "content": "hello" } }],
            "usage": { "prompt_tokens": 3, "completion_tokens": 1, "total_tokens": 4 }
        });
        let (text, usage) = OPENAI.reply(&body).expect("a reply");
        assert_eq!(text, "hello");
        assert_eq!(usage.total_tokens, 4);

        // Nothing to say is not a reply: the caller writes an error rather than an
        // empty string into the transcript.
        assert!(OPENAI.reply(&json!({ "choices": [] })).is_none());
        assert!(OPENAI
            .reply(&json!({"choices":[{"message":{"role":"assistant","content":"   "}}]}))
            .is_none());
    }

    #[test]
    fn the_nonstream_body_turns_thinking_off_and_asks_for_the_shape_it_wants() {
        let body = OPENAI.nonstream_body("deepseek-flash", "sys", "hi", true);
        assert_eq!(body["stream"], false);
        assert_eq!(body["thinking"]["type"], "disabled");
        assert_eq!(body["response_format"]["type"], "json_object");
        assert_eq!(body["messages"][1]["content"], "hi");
        let plain = OPENAI.nonstream_body("deepseek-flash", "sys", "hi", false);
        assert_eq!(plain["response_format"]["type"], "text");
    }

    #[test]
    fn the_registry_points_here() {
        assert_eq!(
            Dialect::OpenAi.imp().chat_url("https://x"),
            OPENAI.chat_url("https://x")
        );
        assert!(Dialect::OpenAi.supports_effort());
    }
}
