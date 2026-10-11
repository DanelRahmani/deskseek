//! HTTP client for the DeepSeek chat-completions API.

use std::time::Duration;

use anyhow::{anyhow, Context};
use futures_util::StreamExt;

use crate::api::stream::{MessageAccumulator, SseBuffer};
use crate::api::types::{ChatMessage, ChatRequest, StreamChunk, Usage};
use crate::runs::Cancel;

/// Long timeout: a thinking-mode answer with a 384K output budget can stream
/// for a long time.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(900);

pub fn build_http() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .expect("failed to build HTTP client")
}

/// Progress callback payload emitted while streaming.
pub enum StreamEvent {
    Content(String),
    Reasoning(String),
}

pub struct StreamOutcome {
    pub message: ChatMessage,
    pub usage: Usage,
    pub finish_reason: Option<String>,
    /// True when the user stopped the run: `message` holds whatever arrived.
    pub cancelled: bool,
}

pub struct Client {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl Client {
    pub fn new(http: reqwest::Client, api_key: String, base_url: String) -> Self {
        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
        }
    }

    /// Verify the key by listing models — cheap, spends no tokens.
    pub async fn check_auth(&self) -> anyhow::Result<()> {
        let resp = self
            .http
            .get(format!("{}/models", self.base_url))
            .bearer_auth(&self.api_key)
            .send()
            .await
            .context("could not reach DeepSeek")?;
        if resp.status().is_success() {
            Ok(())
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(anyhow!("{status}: {body}"))
        }
    }

    /// Stream one assistant turn, invoking `on_event` as deltas arrive.
    ///
    /// `cancel` is checked between chunks; a cancelled run returns the partial
    /// message with `cancelled: true` rather than an error, so the caller can
    /// persist what the model managed to say before it was stopped.
    pub async fn stream_chat<F>(
        &self,
        req: &ChatRequest,
        cancel: &Cancel,
        mut on_event: F,
    ) -> anyhow::Result<StreamOutcome>
    where
        F: FnMut(StreamEvent),
    {
        let resp = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(req)
            .send()
            .await
            .context("request to DeepSeek failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("DeepSeek API error {status}: {body}"));
        }

        let mut acc = MessageAccumulator::default();
        let mut usage = Usage::default();
        let mut finish_reason = None;
        let mut sse = SseBuffer::default();
        let mut stream = resp.bytes_stream();
        let mut cancelled = false;

        // Pinned outside the loop: one subscription per run, not per chunk.
        let stopped = cancel.cancelled();
        tokio::pin!(stopped);

        loop {
            let item = tokio::select! {
                _ = &mut stopped => {
                    cancelled = true;
                    break;
                }
                item = stream.next() => item,
            };
            let Some(item) = item else { break };
            let bytes = item.context("stream interrupted")?;
            for payload in sse.push(&bytes) {
                if payload == "[DONE]" {
                    continue;
                }
                // A malformed chunk is skipped rather than aborting the answer.
                let Ok(chunk) = serde_json::from_str::<StreamChunk>(&payload) else {
                    continue;
                };
                if let Some(u) = &chunk.usage {
                    usage = u.clone();
                }
                for choice in &chunk.choices {
                    if let Some(fr) = &choice.finish_reason {
                        finish_reason = Some(fr.clone());
                    }
                }
                let applied = acc.apply(&chunk);
                if !applied.content.is_empty() {
                    on_event(StreamEvent::Content(applied.content));
                }
                if !applied.reasoning.is_empty() {
                    on_event(StreamEvent::Reasoning(applied.reasoning));
                }
            }
        }

        Ok(StreamOutcome {
            message: acc.into_message(),
            usage,
            finish_reason: if cancelled {
                Some("cancelled".to_string())
            } else {
                finish_reason
            },
            cancelled,
        })
    }

    /// One non-streaming turn, used by the chat summariser. Thinking is off:
    /// summarising does not need reasoning, and it keeps the call cheap.
    pub async fn complete(
        &self,
        model: &str,
        system: &str,
        user: &str,
        json_mode: bool,
    ) -> anyhow::Result<String> {
        let body = serde_json::json!({
            "model": model,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": user }
            ],
            "stream": false,
            "thinking": { "type": "disabled" },
            "response_format": { "type": if json_mode { "json_object" } else { "text" } },
            "max_tokens": 2048
        });

        let resp = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("request to DeepSeek failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("DeepSeek API error {status}: {body}"));
        }

        let parsed: CompletionResponse = resp.json().await.context("unreadable response")?;
        parsed
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .and_then(|content| match content {
                crate::api::types::Content::Text(text) => Some(text),
                crate::api::types::Content::Parts(_) => None,
            })
            .ok_or_else(|| anyhow!("the model returned an empty response"))
    }
}

#[derive(serde::Deserialize)]
struct CompletionResponse {
    #[serde(default)]
    choices: Vec<CompletionChoice>,
}

#[derive(serde::Deserialize)]
struct CompletionChoice {
    message: ChatMessage,
}
