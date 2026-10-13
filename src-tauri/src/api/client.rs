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

/// How many times a request is sent before the endpoint is called dead.
///
/// Retrying is only free *before* the first token: the request may not have been
/// accepted, so nothing has been said and a second attempt cannot duplicate an
/// answer. That is why this covers the send and the status and nothing else — a
/// stream that dies half way through is reported, not replayed.
const MAX_ATTEMPTS: u32 = 3;

/// How long to wait before attempt `n`: 1s, then 2s.
fn backoff(attempt: u32) -> Duration {
    Duration::from_millis(1000 * 2u64.pow(attempt.saturating_sub(1)))
}

/// Statuses worth trying again: the endpoint was busy or briefly broken, rather
/// than the request itself being wrong.
fn is_transient(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::REQUEST_TIMEOUT
        || status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
}

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

/// A turn that did not finish, and why — in the two terms that call for different
/// words rather than one.
#[derive(Debug)]
pub struct Interruption {
    /// The connection or the endpoint, rather than the model refusing the
    /// request. A transport failure is "try again"; a refusal is "the request was
    /// wrong", and telling the user the wrong one wastes their time.
    pub transport: bool,
    pub message: String,
    /// What the model had already said. Kept rather than dropped: a connection
    /// that dies mid-answer must not cost the text that already arrived.
    pub partial: Option<ChatMessage>,
}

pub struct StreamOutcome {
    pub message: ChatMessage,
    pub usage: Usage,
    pub finish_reason: Option<String>,
    /// True when the user stopped the run: `message` holds whatever arrived.
    pub cancelled: bool,
}

impl StreamOutcome {
    /// A turn stopped before the endpoint said anything: nothing arrived, so
    /// there is nothing to keep.
    fn stopped() -> Self {
        Self {
            message: MessageAccumulator::default().into_message(),
            usage: Usage::default(),
            finish_reason: Some("cancelled".to_string()),
            cancelled: true,
        }
    }
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
    ///
    /// A failure before the first token is retried (`MAX_ATTEMPTS`, backing off);
    /// a failure after it returns the partial answer instead, because a replay
    /// would say everything twice.
    pub async fn stream_chat<F>(
        &self,
        req: &ChatRequest,
        cancel: &Cancel,
        mut on_event: F,
    ) -> Result<StreamOutcome, Interruption>
    where
        F: FnMut(StreamEvent),
    {
        let mut attempt = 0u32;
        let resp = loop {
            // Stopped while backing off: nothing was said, so nothing is kept.
            if cancel.is_cancelled() {
                return Ok(StreamOutcome::stopped());
            }
            attempt += 1;
            let sent = self
                .http
                .post(format!("{}/chat/completions", self.base_url))
                .bearer_auth(&self.api_key)
                .json(req)
                .send()
                .await;
            let worth_retrying = match &sent {
                Ok(resp) => is_transient(resp.status()),
                Err(_) => true,
            };
            if !worth_retrying {
                break sent.expect("only a response is not worth retrying");
            }
            if attempt >= MAX_ATTEMPTS {
                return Err(match sent {
                    Ok(resp) => {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        Interruption {
                            transport: true,
                            message: format!(
                                "DeepSeek kept failing ({status}, {MAX_ATTEMPTS} tries): {}",
                                body.trim()
                            ),
                            partial: None,
                        }
                    }
                    Err(e) => Interruption {
                        transport: true,
                        message: format!("Could not reach DeepSeek ({MAX_ATTEMPTS} tries): {e}"),
                        partial: None,
                    },
                });
            }
            tokio::select! {
                _ = cancel.cancelled() => continue,
                _ = tokio::time::sleep(backoff(attempt)) => {}
            }
        };

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(Interruption {
                transport: false,
                message: format!("DeepSeek refused the request ({status}): {}", body.trim()),
                partial: None,
            });
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
            let bytes = match item {
                Ok(bytes) => bytes,
                // Mid-answer, so a retry would duplicate what has already been
                // said. The part that arrived is handed back instead, and the run
                // stops there.
                Err(e) => {
                    return Err(Interruption {
                        transport: true,
                        message: format!("The connection to DeepSeek dropped mid-answer: {e}"),
                        partial: Some(acc.into_message()),
                    });
                }
            };
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

    /// One non-streaming turn, used by the chat summariser and the in-place
    /// revision. Thinking is off: neither needs reasoning, and it keeps the call
    /// cheap.
    ///
    /// The usage comes back with the text because these are calls the user did
    /// not type and still pays for: the caller writes them onto the same bill the
    /// turns are on, and it cannot do that from the text alone.
    pub async fn complete(
        &self,
        model: &str,
        system: &str,
        user: &str,
        json_mode: bool,
    ) -> anyhow::Result<(String, crate::api::types::Usage)> {
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
        let CompletionResponse { choices, usage } = parsed;
        let text = choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .and_then(|content| match content {
                crate::api::types::Content::Text(text) => Some(text),
                crate::api::types::Content::Parts(_) => None,
            })
            .ok_or_else(|| anyhow!("the model returned an empty response"))?;
        Ok((text, usage.unwrap_or_default()))
    }
}

#[derive(serde::Deserialize)]
struct CompletionResponse {
    #[serde(default)]
    choices: Vec<CompletionChoice>,
    /// Absent only if the endpoint declines to report it; a non-streaming reply
    /// carries it in practice.
    #[serde(default)]
    usage: Option<crate::api::types::Usage>,
}

#[derive(serde::Deserialize)]
struct CompletionChoice {
    message: ChatMessage,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_busy_or_broken_endpoint_is_worth_trying_again() {
        // Transient: the endpoint was unhappy, not the request.
        for status in [408u16, 429, 500, 502, 503, 504] {
            assert!(
                is_transient(reqwest::StatusCode::from_u16(status).unwrap()),
                "{status} should be retried"
            );
        }
        // Final: a refusal is about the request, and sending it twice more only
        // costs the user time.
        for status in [400u16, 401, 403, 404, 422] {
            assert!(
                !is_transient(reqwest::StatusCode::from_u16(status).unwrap()),
                "{status} must not be retried"
            );
        }
    }

    #[test]
    fn the_wait_grows_between_attempts() {
        assert_eq!(backoff(1), Duration::from_secs(1));
        assert!(backoff(2) > backoff(1));
    }

    #[test]
    fn a_turn_stopped_before_an_answer_keeps_nothing() {
        let stopped = StreamOutcome::stopped();
        assert!(stopped.cancelled);
        assert!(stopped.message.content.is_none());
        assert!(stopped.message.tool_calls.is_none());
    }
}
