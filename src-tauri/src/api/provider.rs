//! Endpoints, model records, and the seam between this app and a wire.
//!
//! An endpoint is a base URL, a dialect, a keychain reference and a list of
//! models. There are exactly two dialects — `openai-compatible` and
//! `anthropic-compatible` — and each one is a single module that knows a wire
//! shape. Everything a per-vendor client would have been for (OpenRouter, Groq,
//! Together, xAI, Google's OpenAI-compatible endpoint, Ollama, LM Studio, vLLM)
//! is a row in the settings file rather than code in this crate.
//!
//! The rule the whole file exists to hold: **a price nobody published is not
//! zero**. A model with no rate records its tokens and reports its cost as
//! unknown, because a spend view that guesses is a spend view that disagrees with
//! itself.

use serde::{Deserialize, Serialize};

use crate::api::models;
use crate::api::types::{ChatMessage, ChatRequest, Content, ContentPart, Usage};
use crate::api::{anthropic_compat, openai_compat};

/// Which wire shape an endpoint speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Dialect {
    #[serde(rename = "openai-compatible")]
    OpenAi,
    #[serde(rename = "anthropic-compatible")]
    Anthropic,
}

impl Dialect {
    pub fn id(self) -> &'static str {
        match self {
            Dialect::OpenAi => "openai-compatible",
            Dialect::Anthropic => "anthropic-compatible",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Dialect::OpenAi => "OpenAI-compatible",
            Dialect::Anthropic => "Anthropic-compatible",
        }
    }

    /// The id, or the bare vendor name — so a hand-written settings file saying
    /// `"anthropic"` means what its author meant rather than falling back to the
    /// other dialect and failing on every call.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "openai-compatible" | "openai" => Some(Dialect::OpenAi),
            "anthropic-compatible" | "anthropic" => Some(Dialect::Anthropic),
            _ => None,
        }
    }

    pub fn all() -> [Dialect; 2] {
        [Dialect::OpenAi, Dialect::Anthropic]
    }

    /// Whether this dialect has a way to ask for more thought. Both do, and
    /// differently — `reasoning_effort` on one wire, a thinking budget on the
    /// other — so this is a property of the dialect rather than of either module.
    /// A dialect added later that has neither says `false` here, and the UI's rule
    /// is that an effort control either works or explains itself.
    pub fn supports_effort(self) -> bool {
        true
    }

    pub fn default_base_url(self) -> &'static str {
        match self {
            Dialect::OpenAi => "https://api.deepseek.com",
            Dialect::Anthropic => "https://api.anthropic.com",
        }
    }

    /// The one place a dialect becomes a wire shape.
    pub fn imp(self) -> &'static dyn DialectImpl {
        match self {
            Dialect::OpenAi => &openai_compat::OPENAI,
            Dialect::Anthropic => &anthropic_compat::ANTHROPIC,
        }
    }
}

/// USD per 1M tokens.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Price {
    /// Cache-hit input.
    pub hit: f64,
    /// Cache-miss input. The whole prompt is billed here when the endpoint
    /// reports no cache breakdown.
    pub miss: f64,
    /// Output.
    pub out: f64,
    /// What peak hours multiply the three by. `1.0` for every endpoint that does
    /// not price by the clock, which is all of them but DeepSeek.
    #[serde(default = "one")]
    pub peak_multiplier: f64,
}

fn one() -> f64 {
    1.0
}

/// One model an endpoint offers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub id: String,
    pub label: String,
    /// Context window in tokens. Guessing *small* is the dangerous direction: it
    /// would fold a chat that had room left and spend a summariser call nobody
    /// needed, which is why an unlisted model gets the largest window we know.
    pub context: u32,
    pub max_output: u32,
    /// Whether this model can be sent an image. Off by default, because a local
    /// runtime that cannot see must be told so rather than handed a screenshot it
    /// will reject or, worse, silently ignore.
    #[serde(default)]
    pub vision: bool,
    /// `None` when the endpoint publishes no rate — a local model, or a gateway
    /// that prices per request. The tokens are still recorded; the money reads as
    /// unknown.
    #[serde(default)]
    pub price: Option<Price>,
}

impl Model {
    /// A model id this endpoint does not list.
    ///
    /// The app still sends the id it was given — the endpoint is the authority on
    /// what it serves, and refusing here would break a user who typed a model the
    /// vendor added yesterday. It simply has no price and no vision, so its spend
    /// row says unknown and a screenshot is left as text.
    pub fn unlisted(id: &str) -> Self {
        Self {
            id: id.to_string(),
            label: id.to_string(),
            context: models::LARGEST_CONTEXT,
            max_output: DEFAULT_MAX_OUTPUT,
            vision: false,
            price: None,
        }
    }

    /// Cost in USD for one response, or `None` when the rate is unknown.
    pub fn cost(&self, usage: &Usage, peak: bool) -> Option<f64> {
        let price = self.price?;
        let multiplier = if peak { price.peak_multiplier } else { 1.0 };
        let miss_tokens = if usage.prompt_cache_hit_tokens + usage.prompt_cache_miss_tokens == 0 {
            usage.prompt_tokens
        } else {
            usage.prompt_cache_miss_tokens
        };
        Some(
            (usage.prompt_cache_hit_tokens as f64 / 1_000_000.0) * price.hit * multiplier
                + (miss_tokens as f64 / 1_000_000.0) * price.miss * multiplier
                + (usage.completion_tokens as f64 / 1_000_000.0) * price.out * multiplier,
        )
    }
}

/// The output budget used when the endpoint does not say and the model is not
/// one we ship. Small enough to be a sane answer, large enough not to truncate a
/// normal reply.
pub const DEFAULT_MAX_OUTPUT: u32 = 8_192;

/// A base URL and a path, joined once so both dialects produce the same shape and
/// a trailing slash on either side cannot produce a doubled one.
///
/// A user pasting `https://api.deepseek.com/` out of the docs is the ordinary
/// case, and `//chat/completions` is a 404 that reads as a wrong key.
pub fn join(base: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base.trim().trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

/// Merges a reported usage into the running total, field by field.
///
/// Field-wise rather than wholesale, because one dialect announces the input side
/// when the turn starts and the output side when it ends — replacing would drop
/// whichever half came first, and the token counts are the one thing on that row
/// that has to be right. A field the endpoint did not report is left as it was,
/// which is what makes the two reports combine instead of the second wiping the
/// first.
pub fn merge_usage(total: &mut Usage, reported: &Usage) {
    if reported.prompt_tokens > 0 {
        total.prompt_tokens = reported.prompt_tokens;
    }
    if reported.completion_tokens > 0 {
        total.completion_tokens = reported.completion_tokens;
    }
    if reported.prompt_cache_hit_tokens > 0 {
        total.prompt_cache_hit_tokens = reported.prompt_cache_hit_tokens;
    }
    if reported.prompt_cache_miss_tokens > 0 {
        total.prompt_cache_miss_tokens = reported.prompt_cache_miss_tokens;
    }
    // Never smaller than its own parts: an endpoint that reports a total of its own
    // is believed, but one that reports only half the pair must not leave a total
    // that contradicts the fields beside it.
    total.total_tokens = total
        .total_tokens
        .max(total.prompt_tokens + total.completion_tokens);
}

/// One place to talk to: where, in which dialect, with which key, offering what.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub id: String,
    pub label: String,
    pub dialect: Dialect,
    /// Without the dialect's path: `https://api.deepseek.com`, not
    /// `…/chat/completions`. The dialect appends its own suffix, so a user
    /// copying a base URL out of a vendor's docs does not have to know which one
    /// this app wants.
    pub base_url: String,
    /// The keychain account this endpoint's key lives under. Blank means "no
    /// key", which is the honest setting for a model on localhost.
    #[serde(default)]
    pub key_ref: String,
    #[serde(default)]
    pub models: Vec<Model>,
    /// Free text for the user, and the place to say what an endpoint is for.
    #[serde(default)]
    pub notes: String,
}

impl Endpoint {
    pub fn model(&self, id: &str) -> Option<&Model> {
        self.models.iter().find(|m| m.id == id)
    }

    /// The model record to send with, falling back to an unlisted placeholder so
    /// the caller never has to handle "the endpoint does not list this id" twice.
    pub fn model_or_unlisted(&self, id: &str) -> Model {
        self.model(id).cloned().unwrap_or_else(|| Model::unlisted(id))
    }

    pub fn first_model(&self) -> Option<&Model> {
        self.models.first()
    }

    pub fn has_key(&self) -> bool {
        !self.key_ref.trim().is_empty()
    }

    /// What is wrong with this endpoint, in words, or nothing.
    ///
    /// An empty base URL is the one thing that cannot be tolerated: every other
    /// field has a sensible default and a caller that can cope, but a request with
    /// nowhere to go fails at the point of use with a confusing error.
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() {
            return Err("An endpoint needs an id.".to_string());
        }
        let url = self.base_url.trim();
        if url.is_empty() {
            return Err(format!("{} has no base URL.", self.label));
        }
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(format!(
                "{}: the base URL must start with http:// or https://",
                self.label
            ));
        }
        Ok(())
    }

    /// The request as the wire needs it.
    pub fn wire(&self, model: &Model, req: &ChatRequest, key: Option<&str>) -> Wire {
        let imp = self.dialect.imp();
        Wire {
            url: imp.chat_url(&self.base_url),
            headers: imp.headers(key),
            body: imp.body(model, req, &messages_for(model, req)),
        }
    }
}

/// A request, ready to send: where, with which headers, and the body.
pub struct Wire {
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: serde_json::Value,
}

/// The messages as this model can take them.
///
/// One place decides what an endpoint that cannot see is sent, so neither dialect
/// has to remember. An image is dropped rather than refused: the text part is
/// already there — the tool that took the screenshot describes it in words for
/// exactly this reason — so the turn stays coherent and says less rather than
/// failing.
pub fn messages_for(model: &Model, req: &ChatRequest) -> Vec<ChatMessage> {
    if model.vision {
        return req.messages.clone();
    }
    req.messages
        .iter()
        .map(|message| {
            let Some(Content::Parts(parts)) = &message.content else {
                return message.clone();
            };
            let kept: Vec<ContentPart> = parts
                .iter()
                .filter(|part| !matches!(part, ContentPart::ImageUrl { .. }))
                .cloned()
                .collect();
            let mut out = message.clone();
            out.content = match kept.len() {
                0 => None,
                _ => Some(Content::Parts(kept)),
            };
            out
        })
        .collect()
}

/// One piece of a streamed turn, in this app's terms rather than a vendor's.
///
/// Both dialects translate their payloads into these, so the accumulator that
/// rebuilds a message is written once and a new dialect cannot change how a turn
/// is assembled.
#[derive(Debug, Clone, PartialEq)]
pub enum WirePart {
    Text(String),
    Reasoning(String),
    ToolDelta {
        /// The dialect's own number for this call. OpenAI numbers them from zero;
        /// Anthropic numbers content blocks, so a text block sits at 0 and the
        /// first tool call at 1. The accumulator keys on the number instead of
        /// counting, which is what lets both be right.
        index: usize,
        id: Option<String>,
        name: Option<String>,
        args: Option<String>,
    },
    /// Merged field-wise by the accumulator, because Anthropic reports the input
    /// side once at the start and the output side at the end.
    Usage(Usage),
    Finish(String),
}

/// The half of a dialect that differs from the other half.
///
/// A trait rather than a `match` at each call site, so that "exactly one place
/// knows this wire shape" is structural rather than a rule somebody has to
/// remember. Nothing here is async: every method is pure, which is what makes a
/// wire shape testable without a network.
pub trait DialectImpl: Sync {
    /// The endpoint path for a chat turn.
    fn chat_url(&self, base: &str) -> String;
    /// The endpoint path for listing models.
    fn models_url(&self, base: &str) -> String;
    /// Auth and version headers. `None` key means a local endpoint, and the
    /// header is then omitted rather than sent empty — an empty bearer token is a
    /// 401 on some servers and a valid anonymous request on others.
    fn headers(&self, key: Option<&str>) -> Vec<(&'static str, String)>;
    fn body(&self, model: &Model, req: &ChatRequest, messages: &[ChatMessage]) -> serde_json::Value;
    fn nonstream_body(
        &self,
        model: &str,
        system: &str,
        user: &str,
        json_mode: bool,
    ) -> serde_json::Value;
    /// One SSE `data:` payload into parts. Nothing for a payload this dialect
    /// does not understand — a keep-alive or a field added after this was written.
    fn parts(&self, data: &str) -> Vec<WirePart>;
    /// The text and usage of a non-streaming reply.
    fn reply(&self, body: &serde_json::Value) -> Option<(String, Usage)>;
}

/// The id the endpoint a fresh install starts on carries. Named once so the seed
/// and the settings default cannot drift apart.
pub const SEED_ENDPOINT_ID: &str = "deepseek";

/// The endpoint a fresh install starts with.
///
/// DeepSeek is endpoint one and keeps the keychain account its key is already
/// stored under, so an existing install does not re-enter anything. The
/// OpenAI-compatible entry mentions the vendors that are configuration rather
/// than code, because a field nobody can find is a field nobody fills in.
pub fn seed() -> Vec<Endpoint> {
    vec![Endpoint {
        id: SEED_ENDPOINT_ID.to_string(),
        label: "DeepSeek".to_string(),
        dialect: Dialect::OpenAi,
        base_url: Dialect::OpenAi.default_base_url().to_string(),
        key_ref: crate::store::keychain::DEFAULT_REF.to_string(),
        models: vec![
            Model {
                id: "deepseek-flash".to_string(),
                label: "DeepSeek Flash (V4.1)".to_string(),
                context: 1_000_000,
                max_output: 393_216,
                vision: false,
                price: Some(Price {
                    hit: 0.003,
                    miss: 0.15,
                    out: 0.60,
                    peak_multiplier: 2.0,
                }),
            },
            Model {
                id: "deepseek-v4-pro".to_string(),
                label: "DeepSeek V4 Pro".to_string(),
                context: 1_000_000,
                max_output: 393_216,
                vision: false,
                price: Some(Price {
                    hit: 0.022,
                    miss: 0.66,
                    out: 1.98,
                    peak_multiplier: 2.0,
                }),
            },
        ],
        notes: "The default. Prices are for DeepSeek's own endpoint.".to_string(),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model_with(id: &str, price: Option<Price>, vision: bool) -> Model {
        Model {
            id: id.to_string(),
            label: id.to_string(),
            context: 1_000,
            max_output: 100,
            vision,
            price,
        }
    }

    fn request(messages: Vec<ChatMessage>) -> ChatRequest {
        ChatRequest {
            model: "m".to_string(),
            messages,
            stream: true,
            stream_options: None,
            thinking: None,
            reasoning_effort: None,
            max_tokens: None,
            tools: None,
        }
    }

    #[test]
    fn a_rate_nobody_published_is_unknown_and_not_zero() {
        let usage = Usage {
            prompt_tokens: 1_000,
            completion_tokens: 500,
            ..Default::default()
        };
        let free = model_with("local", None, false);
        assert_eq!(free.cost(&usage, false), None);

        let priced = model_with(
            "cheap",
            Some(Price {
                hit: 0.0,
                miss: 1.0,
                out: 2.0,
                peak_multiplier: 1.0,
            }),
            false,
        );
        assert!(priced.cost(&usage, false).is_some());
    }

    #[test]
    fn one_million_miss_tokens_on_flash_off_peak_costs_fifteen_cents() {
        let flash = seed()
            .remove(0)
            .model("deepseek-flash")
            .cloned()
            .expect("the seed ships flash");
        let usage = Usage {
            prompt_tokens: 1_000_000,
            prompt_cache_miss_tokens: 1_000_000,
            ..Default::default()
        };
        let cost = flash.cost(&usage, false).unwrap();
        assert!((cost - 0.15).abs() < 1e-9, "got {cost}");
    }

    #[test]
    fn a_missing_cache_breakdown_is_billed_as_a_miss_rather_than_free() {
        let flash = seed().remove(0).model("deepseek-flash").cloned().unwrap();
        let usage = Usage {
            prompt_tokens: 1_000_000,
            ..Default::default()
        };
        assert!((flash.cost(&usage, false).unwrap() - 0.15).abs() < 1e-9);
    }

    #[test]
    fn peak_doubles_a_rate_that_says_it_is_priced_by_the_clock() {
        let flash = seed().remove(0).model("deepseek-flash").cloned().unwrap();
        let usage = Usage {
            prompt_cache_miss_tokens: 1_000_000,
            ..Default::default()
        };
        let off = flash.cost(&usage, false).unwrap();
        let peak = flash.cost(&usage, true).unwrap();
        assert!((peak - off * 2.0).abs() < 1e-9);

        // An endpoint that does not price by the clock is unchanged by it, which
        // is what the default multiplier of 1.0 buys.
        let flat = model_with(
            "flat",
            Some(Price {
                hit: 0.0,
                miss: 1.0,
                out: 1.0,
                peak_multiplier: 1.0,
            }),
            false,
        );
        assert_eq!(flat.cost(&usage, false), flat.cost(&usage, true));
    }

    #[test]
    fn an_unlisted_model_is_priced_unknown_and_cannot_see() {
        let unlisted = Model::unlisted("something-new");
        assert_eq!(unlisted.id, "something-new");
        assert_eq!(unlisted.label, "something-new");
        assert!(unlisted.price.is_none());
        assert!(!unlisted.vision);
        assert_eq!(unlisted.context, models::LARGEST_CONTEXT);
    }

    #[test]
    fn only_a_model_that_can_see_is_sent_a_picture() {
        let message = ChatMessage {
            role: "tool".to_string(),
            content: Some(Content::Parts(vec![
                ContentPart::Text {
                    text: "a screenshot of the window".to_string(),
                },
                ContentPart::ImageUrl {
                    image_url: crate::api::types::ImageUrl {
                        url: "data:image/png;base64,AAAA".to_string(),
                    },
                },
            ])),
            tool_call_id: Some("call_1".to_string()),
            ..Default::default()
        };
        let req = request(vec![message]);

        let blind = messages_for(&model_with("text-only", None, false), &req);
        let parts = match blind[0].content.as_ref() {
            Some(Content::Parts(parts)) => parts.clone(),
            other => panic!("a text part should survive, got {other:?}"),
        };
        assert_eq!(parts.len(), 1, "the image must be dropped, not sent");
        assert!(matches!(&parts[0], ContentPart::Text { text } if text.contains("screenshot")));
        assert_eq!(blind[0].tool_call_id.as_deref(), Some("call_1"));

        let seeing = messages_for(&model_with("vision", None, true), &req);
        let parts = match seeing[0].content.as_ref() {
            Some(Content::Parts(parts)) => parts,
            other => panic!("both parts should survive, got {other:?}"),
        };
        assert_eq!(parts.len(), 2);
    }

    #[test]
    fn a_message_with_no_parts_left_is_not_sent_as_an_empty_array() {
        let message = ChatMessage {
            role: "user".to_string(),
            content: Some(Content::Parts(vec![ContentPart::ImageUrl {
                image_url: crate::api::types::ImageUrl {
                    url: "data:image/png;base64,AAAA".to_string(),
                },
            }])),
            ..Default::default()
        };
        let out = messages_for(&model_with("text-only", None, false), &request(vec![message]));
        // An empty `content` array is rejected by both dialects, so the message
        // arrives with nothing rather than with `[]`.
        assert!(out[0].content.is_none());
    }

    #[test]
    fn a_dialect_is_parsed_from_its_id_or_its_vendor_name() {
        assert_eq!(Dialect::parse("openai-compatible"), Some(Dialect::OpenAi));
        assert_eq!(Dialect::parse(" OpenAI "), Some(Dialect::OpenAi));
        assert_eq!(Dialect::parse("anthropic"), Some(Dialect::Anthropic));
        assert_eq!(Dialect::parse("gemini"), None);
        for dialect in Dialect::all() {
            assert_eq!(Dialect::parse(dialect.id()), Some(dialect));
        }
    }

    #[test]
    fn every_dialect_joins_a_base_url_the_same_way() {
        // The join is the same rule for both, and the trailing slash is the case
        // a hand-typed URL always has: `…/v1/messages` must not become `//v1`.
        for dialect in Dialect::all() {
            let imp = dialect.imp();
            let joined = imp.chat_url("https://example.com/");
            assert!(!joined.contains("//v1"), "{joined} has a doubled slash");
            assert!(!joined.ends_with('/'), "{joined} ends with a slash");
            assert!(joined.starts_with("https://example.com/"));
        }
    }

    #[test]
    fn an_endpoint_that_cannot_be_called_says_so_before_it_is_used() {
        let mut endpoint = seed().remove(0);
        assert!(endpoint.validate().is_ok());
        assert_eq!(endpoint.model("deepseek-flash").unwrap().label, "DeepSeek Flash (V4.1)");
        assert!(endpoint.model("nope").is_none());
        assert_eq!(endpoint.model_or_unlisted("nope").id, "nope");
        assert_eq!(endpoint.first_model().unwrap().id, "deepseek-flash");
        assert!(endpoint.has_key());

        endpoint.base_url = "   ".to_string();
        assert!(endpoint.validate().is_err());

        endpoint.base_url = "api.deepseek.com".to_string();
        let err = endpoint.validate().unwrap_err();
        assert!(err.contains("http"), "{err}");
    }

    #[test]
    fn the_seed_endpoint_keeps_the_key_the_app_already_stored() {
        // A migration that made the user paste their key again would be the one
        // visible cost of this whole feature.
        let endpoint = seed().remove(0);
        assert_eq!(endpoint.key_ref, crate::store::keychain::DEFAULT_REF);
        assert_eq!(endpoint.base_url, "https://api.deepseek.com");
    }
}
