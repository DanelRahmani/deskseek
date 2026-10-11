//! HTTP client for the DeepSeek chat-completions API.

use std::time::Duration;

use anyhow::{anyhow, Context};
use futures_util::StreamExt;

use crate::api::stream::{MessageAccumulator, SseBuffer};
use crate::api::types::{ChatMessage, ChatRequest, StreamChunk, Usage};

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
    /// ponytail: there is no cancellation token yet — a request runs to
    /// completion or the app closes. Ceiling: a runaway answer cannot be
    /// aborted from the UI. Upgrade path: thread a `CancellationToken` in here
    /// and check it each loop iteration, plus a `cancel_stream` command.
    pub async fn stream_chat<F>(
        &self,
        req: &ChatRequest,
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

        while let Some(item) = stream.next().await {
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
            finish_reason,
        })
    }
}
