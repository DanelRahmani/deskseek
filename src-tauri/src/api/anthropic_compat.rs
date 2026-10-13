//! The Anthropic-compatible wire shape.
//!
//! The second dialect, and the reason the provider layer exists: a `messages`
//! array with content blocks, a top-level `system`, `tool_use`/`tool_result`
//! blocks instead of `tool_calls`, and a streaming protocol of named events whose
//! payloads each carry their own `type`. Everything here is pure, so the shape is
//! testable without a network — which is the only way a wire format this
//! different from the first one can be trusted.

use serde_json::{json, Value};

use crate::api::provider::{join, DialectImpl, Model, WirePart};
use crate::api::types::{ChatMessage, ChatRequest, Content, ContentPart, Tool, Usage};

/// The protocol revision this dialect speaks. Required by the API and not
/// optional: a request without it is refused before the model is chosen.
const VERSION: &str = "2023-06-01";

/// The largest output budget this app will ask for, because `max_tokens` is
/// mandatory on this wire and a model's declared maximum can be enormous.
const MAX_OUTPUT_CEILING: u32 = 32_000;

pub struct Anthropic;

pub const ANTHROPIC: Anthropic = Anthropic;

impl DialectImpl for Anthropic {
    fn chat_url(&self, base: &str) -> String {
        join(base, "v1/messages")
    }

    fn models_url(&self, base: &str) -> String {
        join(base, "v1/models")
    }

    fn headers(&self, key: Option<&str>) -> Vec<(&'static str, String)> {
        let mut headers = vec![
            ("content-type", "application/json".to_string()),
            ("anthropic-version", VERSION.to_string()),
        ];
        // This dialect authenticates with `x-api-key`, not a bearer token, and a
        // local gateway that has no key is left anonymous rather than sent an
        // empty one.
        if let Some(key) = key.map(str::trim).filter(|key| !key.is_empty()) {
            headers.push(("x-api-key", key.to_string()));
        }
        headers
    }

    fn body(&self, model: &Model, req: &ChatRequest, messages: &[ChatMessage]) -> Value {
        let max_tokens = req.max_tokens.unwrap_or_else(|| {
            model
                .max_output
                .clamp(1_024, MAX_OUTPUT_CEILING)
        });
        let mut body = json!({
            "model": req.model,
            "max_tokens": max_tokens,
            "stream": true,
            "messages": translate(messages),
        });
        let system = system_text(messages);
        if !system.is_empty() {
            body["system"] = json!(system);
        }
        if let Some(tools) = &req.tools {
            if !tools.is_empty() {
                body["tools"] = json!(tools.iter().map(tool_schema).collect::<Vec<_>>());
            }
        }
        if let Some(budget) = thinking_budget(req, max_tokens) {
            body["thinking"] = json!({ "type": "enabled", "budget_tokens": budget });
        }
        body
    }

    fn nonstream_body(&self, model: &str, system: &str, user: &str, json_mode: bool) -> Value {
        // There is no response-format switch on this wire and no thinking switch
        // either: a one-shot call that wants JSON asks for it in the prompt, and
        // wants no reasoning, so the budget is not set. The caller's `json_mode`
        // is therefore folded into the system text rather than ignored.
        let system = if json_mode {
            format!("{system}\n\nReply with a single JSON object and nothing else.")
        } else {
            system.to_string()
        };
        json!({
            "model": model,
            "max_tokens": 2048,
            "system": system,
            "messages": [{ "role": "user", "content": user }],
            "stream": false
        })
    }

    fn parts(&self, data: &str) -> Vec<WirePart> {
        let Ok(event) = serde_json::from_str::<Value>(data) else {
            return Vec::new();
        };
        match event["type"].as_str() {
            Some("message_start") => {
                let usage = usage_of(&event["message"]["usage"]);
                if usage.total_tokens == 0 {
                    Vec::new()
                } else {
                    vec![WirePart::Usage(usage)]
                }
            }
            Some("content_block_start") => {
                let Some(index) = index_of(&event) else {
                    return Vec::new();
                };
                let block = &event["content_block"];
                if block["type"].as_str() != Some("tool_use") {
                    // Text and thinking blocks are announced empty and filled by
                    // the deltas, so there is nothing to say here.
                    return Vec::new();
                }
                vec![WirePart::ToolDelta {
                    index,
                    id: block["id"].as_str().map(str::to_string),
                    name: block["name"].as_str().map(str::to_string),
                    args: None,
                }]
            }
            Some("content_block_delta") => {
                let Some(index) = index_of(&event) else {
                    return Vec::new();
                };
                let delta = &event["delta"];
                match delta["type"].as_str() {
                    Some("text_delta") => text_part(delta, "text", WirePart::Text),
                    Some("thinking_delta") => text_part(delta, "thinking", WirePart::Reasoning),
                    // Tool arguments arrive as fragments of a JSON string, exactly
                    // like the other dialect's, so the accumulator can append both.
                    Some("input_json_delta") => {
                        match delta["partial_json"].as_str().filter(|j| !j.is_empty()) {
                            Some(json) => vec![WirePart::ToolDelta {
                                index,
                                id: None,
                                name: None,
                                args: Some(json.to_string()),
                            }],
                            None => Vec::new(),
                        }
                    }
                    _ => Vec::new(),
                }
            }
            Some("message_delta") => {
                let mut out = Vec::new();
                let usage = usage_of(&event["usage"]);
                if usage.total_tokens > 0 {
                    out.push(WirePart::Usage(usage));
                }
                if let Some(reason) = event["delta"]["stop_reason"].as_str() {
                    out.push(WirePart::Finish(reason.to_string()));
                }
                out
            }
            // `message_stop` carries nothing, and an event added after this was
            // written must not break a stream that is otherwise fine.
            _ => Vec::new(),
        }
    }

    fn reply(&self, body: &Value) -> Option<(String, Usage)> {
        let text = body["content"]
            .as_array()?
            .iter()
            .filter(|block| block["type"].as_str() == Some("text"))
            .filter_map(|block| block["text"].as_str())
            .collect::<Vec<_>>()
            .join("");
        if text.trim().is_empty() {
            return None;
        }
        Some((text, usage_of(&body["usage"])))
    }
}

/// The messages as this wire takes them.
///
/// Three rules that are not obvious from the shape alone:
///
/// 1. `system` is a parameter, not a message.
/// 2. Tool results are `tool_result` blocks inside a **user** message, and they
///    must all sit in the message immediately after the assistant turn that asked
///    for them — so consecutive results are gathered and flushed together.
/// 3. Prior assistant reasoning is *not* sent back. The other dialect requires it
///    to be echoed; this one would reject a `thinking` block without its
///    signature. Dropping it is what makes one history work on both wires.
fn translate(messages: &[ChatMessage]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    let mut results: Vec<Value> = Vec::new();
    for message in messages {
        if message.role == "system" {
            continue;
        }
        if message.role == "tool" {
            results.push(tool_result(message));
            continue;
        }
        if !results.is_empty() {
            out.push(json!({ "role": "user", "content": std::mem::take(&mut results) }));
        }
        let blocks = blocks_of(message);
        if blocks.is_empty() {
            // A message with nothing readable in it is dropped rather than sent
            // empty, which this wire refuses outright.
            continue;
        }
        let role = if message.role == "assistant" {
            "assistant"
        } else {
            "user"
        };
        out.push(json!({ "role": role, "content": blocks }));
    }
    if !results.is_empty() {
        out.push(json!({ "role": "user", "content": results }));
    }
    out
}

fn system_text(messages: &[ChatMessage]) -> String {
    messages
        .iter()
        .filter(|message| message.role == "system")
        .filter_map(|message| message.content.as_ref().and_then(Content::as_text))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn blocks_of(message: &ChatMessage) -> Vec<Value> {
    let mut blocks: Vec<Value> = Vec::new();
    match &message.content {
        Some(Content::Text(text)) if !text.trim().is_empty() => {
            blocks.push(json!({ "type": "text", "text": text }));
        }
        Some(Content::Parts(parts)) => {
            for part in parts {
                match part {
                    ContentPart::Text { text } => {
                        if !text.trim().is_empty() {
                            blocks.push(json!({ "type": "text", "text": text }));
                        }
                    }
                    ContentPart::ImageUrl { image_url } => {
                        if let Some(block) = image_block(&image_url.url) {
                            blocks.push(block);
                        }
                    }
                }
            }
        }
        _ => {}
    }
    for call in message.tool_calls.iter().flatten() {
        blocks.push(json!({
            "type": "tool_use",
            "id": call.id,
            "name": call.function.name,
            "input": tool_input(&call.function.arguments),
        }));
    }
    blocks
}

fn tool_result(message: &ChatMessage) -> Value {
    let mut content: Vec<Value> = Vec::new();
    if let Some(text) = message.content.as_ref().and_then(Content::as_text) {
        if !text.is_empty() {
            content.push(json!({ "type": "text", "text": text }));
        }
    }
    if let Some(Content::Parts(parts)) = &message.content {
        for part in parts {
            match part {
                ContentPart::Text { text } => {
                    if !text.is_empty() {
                        content.push(json!({ "type": "text", "text": text }));
                    }
                }
                // This wire is the one that can carry a picture *inside* a tool
                // result, which is why a screenshot read by the model needs no
                // special case here.
                ContentPart::ImageUrl { image_url } => {
                    if let Some(block) = image_block(&image_url.url) {
                        content.push(block);
                    }
                }
            }
        }
    }
    if content.is_empty() {
        // A tool_result with an empty body is refused, and an empty body is a real
        // outcome — a command that printed nothing. Saying so is honest and keeps
        // the pair valid.
        content.push(json!({ "type": "text", "text": "(no output)" }));
    }
    json!({
        "type": "tool_result",
        "tool_use_id": message.tool_call_id.clone().unwrap_or_default(),
        "content": content,
    })
}

/// A data URL or an HTTP URL, as an image source. `None` for anything else, which
/// is dropped rather than guessed at.
fn image_block(url: &str) -> Option<Value> {
    if url.starts_with("http://") || url.starts_with("https://") {
        return Some(json!({ "type": "image", "source": { "type": "url", "url": url } }));
    }
    let rest = url.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    if data.is_empty() {
        return None;
    }
    let media_type = meta.strip_suffix(";base64").unwrap_or(meta);
    Some(json!({
        "type": "image",
        "source": { "type": "base64", "media_type": media_type, "data": data }
    }))
}

/// Tool arguments as an object, because that is what `input` is. A fragment that
/// does not parse, or parses to something else, becomes an empty object rather
/// than failing the whole turn: the model already made the call, and the tool's
/// own validation is the place that reports a bad argument.
fn tool_input(arguments: &str) -> Value {
    match serde_json::from_str::<Value>(arguments) {
        Ok(value) if value.is_object() => value,
        _ => json!({}),
    }
}

fn tool_schema(tool: &Tool) -> Value {
    json!({
        "name": tool.function.name,
        "description": tool.function.description,
        "input_schema": tool.function.parameters,
    })
}

/// What an effort level means on this wire.
///
/// The other dialect has an enum to send; this one has a number of tokens the
/// model may think for. The mapping is the app's own opinion and so is written
/// down once, here, where the field it feeds lives.
fn effort_budget(effort: Option<&str>) -> u32 {
    match effort.unwrap_or("high").trim().to_ascii_lowercase().as_str() {
        "low" => 2_048,
        "max" => 24_000,
        _ => 8_000,
    }
}

/// The thinking budget, or nothing when the turn has thinking off or the budget
/// would not fit under `max_tokens` — a budget larger than the output ceiling is
/// a request the API refuses rather than a preference it ignores.
///
/// Never more than half the output budget: the tokens spent thinking are not
/// tokens spent answering, and a model that thought with the whole allowance has
/// nothing left to say.
fn thinking_budget(req: &ChatRequest, max_tokens: u32) -> Option<u32> {
    let enabled = req
        .thinking
        .as_ref()
        .is_some_and(|thinking| thinking.kind == "enabled");
    if !enabled {
        return None;
    }
    let budget = effort_budget(req.reasoning_effort.as_deref()).min(max_tokens / 2);
    (budget >= 1_024).then_some(budget)
}

fn index_of(event: &Value) -> Option<usize> {
    event["index"].as_u64().map(|index| index as usize)
}

fn text_part(delta: &Value, key: &str, wrap: fn(String) -> WirePart) -> Vec<WirePart> {
    match delta[key].as_str().filter(|text| !text.is_empty()) {
        Some(text) => vec![wrap(text.to_string())],
        None => Vec::new(),
    }
}

/// This wire's token fields, in the app's own names.
///
/// Reported in two places rather than one — the input side at the start of the
/// turn and the output side at the end — which is why the accumulator merges
/// usage field-wise instead of replacing it.
fn usage_of(value: &Value) -> Usage {
    let prompt = value["input_tokens"].as_u64().unwrap_or(0) as u32;
    let completion = value["output_tokens"].as_u64().unwrap_or(0) as u32;
    Usage {
        prompt_tokens: prompt,
        completion_tokens: completion,
        total_tokens: prompt + completion,
        prompt_cache_hit_tokens: value["cache_read_input_tokens"].as_u64().unwrap_or(0) as u32,
        prompt_cache_miss_tokens: value["cache_creation_input_tokens"]
            .as_u64()
            .unwrap_or(0) as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::provider::{Dialect, Model};
    use crate::api::provider::DEFAULT_MAX_OUTPUT;
    use crate::api::types::{FunctionCall, ImageUrl, Thinking, ToolCall};

    fn model() -> Model {
        Model::unlisted("claude-sonnet")
    }

    fn request(messages: Vec<ChatMessage>) -> ChatRequest {
        ChatRequest {
            model: "claude-sonnet".to_string(),
            messages,
            stream: true,
            stream_options: None,
            thinking: None,
            reasoning_effort: None,
            max_tokens: None,
            tools: None,
        }
    }

    fn assistant(calls: Vec<ToolCall>) -> ChatMessage {
        ChatMessage {
            role: "assistant".to_string(),
            content: None,
            tool_calls: Some(calls),
            ..Default::default()
        }
    }

    fn call(id: &str, name: &str, arguments: &str) -> ToolCall {
        ToolCall {
            id: id.to_string(),
            kind: "function".to_string(),
            function: FunctionCall {
                name: name.to_string(),
                arguments: arguments.to_string(),
            },
        }
    }

    fn result(id: &str, text: &str) -> ChatMessage {
        ChatMessage {
            role: "tool".to_string(),
            content: Some(Content::text(text)),
            tool_call_id: Some(id.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn the_wire_body_is_the_shape_the_endpoint_expects() {
        let req = ChatRequest {
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: Some(Content::text("be brief")),
                    ..Default::default()
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: Some(Content::text("hello")),
                    ..Default::default()
                },
            ],
            tools: Some(vec![Tool::function(
                "read_file",
                "reads a file",
                json!({"type": "object"}),
            )]),
            ..request(Vec::new())
        };
        let body = ANTHROPIC.body(&model(), &req, &req.messages);
        assert_eq!(body["model"], "claude-sonnet");
        assert_eq!(body["stream"], true);
        // Mandatory on this wire, so it is always present — here the unlisted
        // model's own default, since nothing bigger was declared.
        assert_eq!(body["max_tokens"], DEFAULT_MAX_OUTPUT);
        // `system` is a parameter here, not a message.
        assert_eq!(body["system"], "be brief");
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"][0]["type"], "text");
        // The tool schema is `input_schema`, not `parameters`.
        assert_eq!(body["tools"][0]["name"], "read_file");
        assert!(body["tools"][0]["input_schema"].is_object());
        assert!(body["tools"][0].get("function").is_none());
        // No thinking unless the request asked for it.
        assert!(body.get("thinking").is_none());
    }

    #[test]
    fn a_long_output_ceiling_is_capped_because_this_field_is_mandatory() {
        let mut wide = model();
        wide.max_output = 393_216;
        let body = ANTHROPIC.body(&wide, &request(Vec::new()), &[]);
        assert_eq!(body["max_tokens"], MAX_OUTPUT_CEILING);

        let mut tiny = model();
        tiny.max_output = 64;
        let body = ANTHROPIC.body(&tiny, &request(Vec::new()), &[]);
        assert_eq!(body["max_tokens"], 1_024, "a floor, or nothing could be said");
    }

    #[test]
    fn effort_becomes_a_thinking_budget_that_fits_under_the_ceiling() {
        // Half the output budget is the ceiling, so the model still has room to
        // answer: an unlisted model allows 8,192 out, so even "max" lands at
        // 4,096 rather than the 24,000 the level asks for.
        let req = ChatRequest {
            thinking: Some(Thinking::enabled(true)),
            reasoning_effort: Some("max".to_string()),
            ..request(Vec::new())
        };
        let body = ANTHROPIC.body(&model(), &req, &[]);
        let budget = body["thinking"]["budget_tokens"].as_u64().unwrap();
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(budget, 4_096);

        // A level that fits is used as asked.
        let low = ChatRequest {
            thinking: Some(Thinking::enabled(true)),
            reasoning_effort: Some("low".to_string()),
            ..request(Vec::new())
        };
        assert_eq!(
            ANTHROPIC.body(&model(), &low, &[])["thinking"]["budget_tokens"],
            2_048
        );

        // A wide ceiling still caps the request at 32,000 out, so the largest
        // level lands at half of that rather than at the 24,000 it asks for.
        let mut wide = model();
        wide.max_output = 100_000;
        assert_eq!(
            ANTHROPIC.body(&wide, &req, &[])["thinking"]["budget_tokens"],
            16_000
        );

        // Thinking off means no budget at all, not a zero one.
        let off = request(Vec::new());
        assert!(ANTHROPIC.body(&model(), &off, &[]).get("thinking").is_none());

        // And a budget that could not fit at all is dropped rather than sent as
        // a request the API refuses.
        let cramped = ChatRequest {
            max_tokens: Some(1_500),
            thinking: Some(Thinking::enabled(true)),
            reasoning_effort: Some("max".to_string()),
            ..request(Vec::new())
        };
        assert!(ANTHROPIC.body(&model(), &cramped, &[]).get("thinking").is_none());
    }

    #[test]
    fn a_tool_call_round_trips_as_a_use_block_and_a_result_block() {
        let messages = vec![
            ChatMessage {
                role: "user".to_string(),
                content: Some(Content::text("read it")),
                ..Default::default()
            },
            assistant(vec![call("call_1", "read_file", r#"{"path":"a.txt"}"#)]),
            result("call_1", "the contents"),
        ];
        let out = translate(&messages);
        assert_eq!(out.len(), 3);
        assert_eq!(out[1]["role"], "assistant");
        assert_eq!(out[1]["content"][0]["type"], "tool_use");
        assert_eq!(out[1]["content"][0]["id"], "call_1");
        assert_eq!(out[1]["content"][0]["input"]["path"], "a.txt");
        // The result is a *user* message, which is the shape this wire requires.
        assert_eq!(out[2]["role"], "user");
        assert_eq!(out[2]["content"][0]["type"], "tool_result");
        assert_eq!(out[2]["content"][0]["tool_use_id"], "call_1");
        assert_eq!(out[2]["content"][0]["content"][0]["text"], "the contents");
    }

    #[test]
    fn parallel_tool_results_travel_together_in_one_user_message() {
        let messages = vec![
            assistant(vec![
                call("call_1", "read_file", "{}"),
                call("call_2", "read_file", "{}"),
            ]),
            result("call_1", "one"),
            result("call_2", "two"),
        ];
        let out = translate(&messages);
        // Three messages, not four: two results in one user turn is the shape the
        // wire requires, and sending them apart is a 400.
        assert_eq!(out.len(), 2);
        let blocks = out[1]["content"].as_array().unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0]["tool_use_id"], "call_1");
        assert_eq!(blocks[1]["tool_use_id"], "call_2");
    }

    #[test]
    fn an_empty_tool_result_says_so_rather_than_arriving_empty() {
        let out = translate(&[assistant(vec![call("c", "run_command", "{}")]), result("c", "")]);
        assert_eq!(out[1]["content"][0]["content"][0]["text"], "(no output)");
    }

    #[test]
    fn arguments_that_are_not_an_object_become_an_empty_one() {
        assert_eq!(tool_input(r#"{"a":1}"#), json!({"a":1}));
        assert_eq!(tool_input("not json at all"), json!({}));
        // A valid JSON *string* is still not an object, and `input` must be one.
        assert_eq!(tool_input(r#""hello""#), json!({}));
    }

    #[test]
    fn an_assistant_turn_with_no_readable_content_is_dropped() {
        let empty = ChatMessage {
            role: "assistant".to_string(),
            ..Default::default()
        };
        let blank = ChatMessage {
            role: "assistant".to_string(),
            content: Some(Content::text("   ")),
            ..Default::default()
        };
        assert!(translate(&[empty, blank]).is_empty());
    }

    #[test]
    fn prior_reasoning_is_not_echoed_back_because_this_wire_would_refuse_it() {
        let with_reasoning = ChatMessage {
            role: "assistant".to_string(),
            content: Some(Content::text("the answer")),
            reasoning_content: Some("the working".to_string()),
            ..Default::default()
        };
        let out = translate(&[with_reasoning]);
        assert_eq!(out.len(), 1);
        let blocks = out[0]["content"].as_array().unwrap();
        assert_eq!(blocks.len(), 1, "only the text block travels");
        assert_eq!(blocks[0]["type"], "text");
        assert!(!serde_json::to_string(&out).unwrap().contains("the working"));
    }

    #[test]
    fn an_image_travels_inside_a_tool_result_which_is_the_whole_point() {
        let shot = ChatMessage {
            role: "tool".to_string(),
            content: Some(Content::Parts(vec![
                ContentPart::Text {
                    text: "a screenshot of the window".to_string(),
                },
                ContentPart::ImageUrl {
                    image_url: ImageUrl {
                        url: "data:image/png;base64,AAAA".to_string(),
                    },
                },
            ])),
            tool_call_id: Some("call_1".to_string()),
            ..Default::default()
        };
        let out = translate(&[assistant(vec![call("call_1", "ui_screenshot", "{}")]), shot]);
        let blocks = out[1]["content"][0]["content"].as_array().unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[1]["type"], "image");
        assert_eq!(blocks[1]["source"]["type"], "base64");
        assert_eq!(blocks[1]["source"]["media_type"], "image/png");
        assert_eq!(blocks[1]["source"]["data"], "AAAA");
    }

    #[test]
    fn an_image_source_is_read_out_of_a_data_url_or_given_up_on() {
        let block = image_block("data:image/jpeg;base64,ZZZ").unwrap();
        assert_eq!(block["source"]["media_type"], "image/jpeg");
        let linked = image_block("https://example.com/a.png").unwrap();
        assert_eq!(linked["source"]["type"], "url");
        // Not a data URL and not a link: dropped rather than sent as something
        // the API will reject.
        assert!(image_block("/tmp/a.png").is_none());
        assert!(image_block("data:image/png;base64,").is_none());
    }

    #[test]
    fn a_streamed_turn_is_read_out_of_its_events() {
        let mut parts = Vec::new();
        for payload in [
            r#"{"type":"message_start","message":{"usage":{"input_tokens":100,"output_tokens":1}}}"#,
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hel"}}"#,
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"lo"}}"#,
            r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_1","name":"read_file"}}"#,
            r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"pa"}}"#,
            r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"th\":\"a\"}"}}"#,
            r#"{"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":42}}"#,
            r#"{"type":"message_stop"}"#,
        ] {
            parts.extend(ANTHROPIC.parts(payload));
        }
        assert_eq!(
            parts,
            vec![
                WirePart::Usage(Usage {
                    prompt_tokens: 100,
                    completion_tokens: 1,
                    total_tokens: 101,
                    ..Default::default()
                }),
                WirePart::Text("Hel".to_string()),
                WirePart::Text("lo".to_string()),
                WirePart::ToolDelta {
                    // Content block 1, which is the reason the accumulator keys on
                    // the dialect's own number rather than counting from zero.
                    index: 1,
                    id: Some("toolu_1".to_string()),
                    name: Some("read_file".to_string()),
                    args: None,
                },
                WirePart::ToolDelta {
                    index: 1,
                    id: None,
                    name: None,
                    args: Some("{\"pa".to_string()),
                },
                WirePart::ToolDelta {
                    index: 1,
                    id: None,
                    name: None,
                    args: Some("th\":\"a\"}".to_string()),
                },
                WirePart::Usage(Usage {
                    prompt_tokens: 0,
                    completion_tokens: 42,
                    total_tokens: 42,
                    ..Default::default()
                }),
                WirePart::Finish("tool_use".to_string()),
            ]
        );
    }

    #[test]
    fn a_thinking_delta_is_carried_as_reasoning() {
        let parts = ANTHROPIC.parts(
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"hmm"}}"#,
        );
        assert_eq!(parts, vec![WirePart::Reasoning("hmm".to_string())]);
    }

    #[test]
    fn an_event_this_dialect_does_not_know_is_skipped_rather_than_fatal() {
        assert!(ANTHROPIC.parts("").is_empty());
        assert!(ANTHROPIC.parts("{\"type\":\"ping\"}").is_empty());
        assert!(ANTHROPIC.parts("{\"type\":\"content_block_delta\",\"delta\":{}}").is_empty());
        assert!(ANTHROPIC.parts("[DONE]").is_empty());
    }

    #[test]
    fn a_nonstream_reply_yields_its_text_and_its_usage() {
        let body = json!({
            "content": [
                { "type": "text", "text": "hello " },
                { "type": "text", "text": "there" }
            ],
            "usage": { "input_tokens": 3, "output_tokens": 2 }
        });
        let (text, usage) = ANTHROPIC.reply(&body).expect("a reply");
        // Blocks are joined, because a caller that wants one string must not have
        // to know the reply was split in two.
        assert_eq!(text, "hello there");
        assert_eq!(usage.prompt_tokens, 3);
        assert_eq!(usage.completion_tokens, 2);
        assert!(ANTHROPIC.reply(&json!({ "content": [] })).is_none());
    }

    #[test]
    fn the_urls_are_the_ones_this_wire_uses() {
        assert_eq!(
            ANTHROPIC.chat_url("https://api.anthropic.com/"),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            ANTHROPIC.models_url("https://api.anthropic.com"),
            "https://api.anthropic.com/v1/models"
        );
    }

    #[test]
    fn a_key_is_sent_the_way_this_wire_wants_it() {
        let headers = ANTHROPIC.headers(Some("sk-ant-1"));
        assert!(headers.iter().any(|(name, value)| *name == "x-api-key" && value == "sk-ant-1"));
        assert!(headers
            .iter()
            .any(|(name, value)| *name == "anthropic-version" && value == VERSION));
        // No bearer token anywhere: a wrong auth header is a 401 that names the
        // wrong cause.
        assert!(headers.iter().all(|(name, _)| *name != "authorization"));

        let anonymous = ANTHROPIC.headers(None);
        assert!(anonymous.iter().all(|(name, _)| *name != "x-api-key"));
        assert!(anonymous.iter().any(|(name, _)| *name == "anthropic-version"));
    }

    #[test]
    fn the_registry_points_here_and_effort_is_claimed() {
        assert_eq!(
            Dialect::Anthropic.imp().chat_url("https://x"),
            ANTHROPIC.chat_url("https://x")
        );
        assert!(Dialect::Anthropic.supports_effort());
    }
}
