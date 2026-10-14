//! Speech: a script read out loud.
//!
//! Not a chat dialect. The path is the OpenAI audio shape —
//! `POST {base}/audio/speech` — whatever dialect the endpoint speaks, and the
//! endpoint is only where it lives and which key it uses. That is why this is a
//! module *beside* the dialects rather than a third `DialectImpl`: a dialect
//! whose every method but one would be meaningless, and which half the vendors
//! would refuse, is not a dialect — it is one request.
//!
//! Two rules are here because they are the ones a feature like this gets wrong:
//!
//! - **A clip is one request.** The speech API takes a limited amount of text at
//!   once, so the script is capped before it is sent rather than split after it.
//!   Joining two clips into one file is a second problem (MP3 frames, an ID3 tag
//!   per piece), and the length a user was quoted would no longer be the length
//!   they got.
//! - **The price is known before it is sent.** Speech is billed by the minute
//!   rather than by the token, so the same "a price nobody published is not
//!   zero" rule the spend view keeps applies here: a rate of zero records the
//!   clip and reports the money as unknown.

use serde::Serialize;
use serde_json::{json, Value};

use crate::api::provider::join;

/// The endpoint path for one clip.
pub fn speech_url(base: &str) -> String {
    join(base, "audio/speech")
}

/// The endpoint path for a transcription.
pub fn transcribe_url(base: &str) -> String {
    join(base, "audio/transcriptions")
}

/// How much text a spoken minute is worth.
///
/// Around 150 words a minute, and a shade under six characters a word with the
/// space. Rounded rather than measured: the alternative is counting words in a
/// script that is not written yet, and the difference is seconds on an estimate
/// that says "about".
pub const CHARS_PER_MINUTE: usize = 900;

/// The most text one request will take.
///
/// The published ceiling is 4096 characters on the vendor that defined this
/// shape; the margin is for the servers that trim the JSON around it. About four
/// minutes of speech, which is what an overview is.
pub const MAX_SCRIPT_CHARS: usize = 3_500;

/// What a script will cost, before it is sent.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Estimate {
    pub chars: usize,
    /// Whole seconds, rounded **up**: rounding down is a promise the clip may not
    /// keep, and the number is a quote rather than a measurement either way.
    pub seconds: u32,
    pub usd: f64,
    /// False when no rate is published. The difference between "$0.00" and
    /// "price unknown", which is the distinction the spend view already makes.
    pub priced: bool,
    /// The most characters one request takes. Carried on the estimate so a
    /// caller can say "this will be refused" without a second copy of the
    /// number — the dialog warning and the refusal then cannot disagree.
    pub limit: usize,
}

impl Estimate {
    /// The minutes a person would say out loud, for a sentence rather than a
    /// table of seconds.
    pub fn minutes(&self) -> f64 {
        self.seconds as f64 / 60.0
    }

    /// Whether one request could read this out at all.
    pub fn fits(&self) -> bool {
        self.chars <= self.limit
    }
}

/// What speaking `chars` characters costs at `usd_per_minute`.
pub fn estimate(chars: usize, usd_per_minute: f64) -> Estimate {
    let seconds = ((chars as f64 / CHARS_PER_MINUTE as f64) * 60.0).ceil() as u32;
    Estimate {
        chars,
        seconds,
        usd: usd_per_minute * seconds as f64 / 60.0,
        priced: usd_per_minute > 0.0,
        limit: MAX_SCRIPT_CHARS,
    }
}

/// The body the speech API takes: the model, the voice, and the words.
///
/// MP3 rather than a streaming format, because the clip is a file this app saves
/// and plays rather than a stream it plays as it arrives.
pub fn tts_body(model: &str, voice: &str, text: &str) -> Value {
    json!({
        "model": model,
        "input": text,
        "voice": voice,
        "response_format": "mp3",
    })
}

/// The body for a transcription: one audio file, and the model to read it.
///
/// Hand-built rather than taken from reqwest's `multipart` feature, which would
/// pull another crate in for one fixed two-field form. The shape is pinned
/// byte-for-byte by the test below instead of trusted, and the boundary is a
/// parameter so the test can hold the bytes still.
///
/// The file's *name* is what the endpoint sniffs — a `.webm` that says it is
/// audio tracks or it does not, and the extension is the only part of that a
/// recorder in a browser can tell us.
pub fn multipart(
    boundary: &str,
    model: &str,
    filename: &str,
    mime: &str,
    bytes: &[u8],
) -> Vec<u8> {
    let mut body = Vec::with_capacity(bytes.len() + 512);
    body.extend_from_slice(
        format!(
            "--{boundary}\r\n\
             Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n\
             Content-Type: {mime}\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(bytes);
    body.extend_from_slice(
        format!(
            "\r\n--{boundary}\r\n\
             Content-Disposition: form-data; name=\"model\"\r\n\r\n\
             {model}\r\n\
             --{boundary}--\r\n"
        )
        .as_bytes(),
    );
    body
}

/// Turns a recording into words.
///
/// The same two rules as the other direction: the key is optional, and a failure
/// carries the endpoint's own words — "no such model" and "unsupported format"
/// are the user's to fix and nobody else can tell them which it was.
pub async fn transcribe(
    http: &reqwest::Client,
    base_url: &str,
    key: Option<&str>,
    model: &str,
    filename: &str,
    mime: &str,
    bytes: &[u8],
) -> Result<String, String> {
    let boundary = format!("----deskseekform{}", uuid::Uuid::new_v4().simple());
    let body = multipart(&boundary, model, filename, mime, bytes);
    let mut request = http
        .post(transcribe_url(base_url))
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(body);
    if let Some(key) = key.map(str::trim).filter(|key| !key.is_empty()) {
        request = request.header("authorization", format!("Bearer {key}"));
    }
    let response = request
        .send()
        .await
        .map_err(|e| format!("The transcription request failed: {e}"))?;

    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| format!("The transcription could not be read back: {e}"))?;
    if !status.is_success() {
        return Err(format!("The speech endpoint answered {status}: {}", text.trim()));
    }
    let parsed: Transcription = serde_json::from_str(&text)
        .map_err(|_| format!("The speech endpoint answered something unexpected: {}", text.trim()))?;
    let heard = parsed.text.trim().to_string();
    if heard.is_empty() {
        // Silence is a real answer, and "nothing was said" is a better sentence
        // than the empty string landing in the composer with no explanation.
        return Err("Nothing was heard in that recording.".to_string());
    }
    Ok(heard)
}

/// The reply shape: one field, whatever else the endpoint adds beside it.
#[derive(serde::Deserialize)]
struct Transcription {
    #[serde(default)]
    text: String,
}

/// What the script generator is told, before it is handed the conversation.
/// Every line here is a thing a spoken script does differently from a written
/// one, and each is a defect somebody ships: markdown read out loud, a bullet
/// list with no bullets, "see the table below" with no table.
pub const SCRIPT_SYSTEM: &str = "You write scripts to be heard rather than read.\n\
Write plain spoken prose: no headings, no bullet points, no numbered lists, no markdown, no code, \
no emoji, and no stage directions or speaker labels. Write numbers, symbols and abbreviations the \
way they are said out loud. Address the listener directly, in the present tense. Say what \
something is before saying why it matters, and define a term the first time it is used.\n\
Open by saying what the piece is about in one sentence. Close by stopping — do not ask the \
listener a question, and do not offer to continue.\n\
There is one voice reading to one listener.";

/// The request: the conversation, and how much room the answer has.
///
/// The cap is stated as a hard limit in the words as well as in the arithmetic,
/// because the number is enforced twice over: the estimate the user is shown is
/// computed from it, and the clip is refused rather than trimmed when a script
/// comes back longer.
pub fn script_prompt(transcript: &str, cap: usize) -> String {
    let minutes = (cap as f64 / CHARS_PER_MINUTE as f64).round().max(1.0) as u32;
    format!(
        "Write a spoken overview of the conversation below, for someone who has not read it.\n\
         The script must be no more than {cap} characters — about {minutes} minutes read aloud. \
         That is a hard limit: it is spoken in a single request.\n\
         Say what the conversation was about, what was decided or found, and anything the \
         listener would have to act on. Do not read the conversation back.\n\n\
         --- conversation ---\n{transcript}"
    )
}

/// Reads a script out loud and answers with the clip's bytes.
///
/// The key is optional for the same reason it is everywhere else: a model on
/// localhost has none, and an empty `Authorization` header is a 401 on the
/// servers that do.
pub async fn speak(
    http: &reqwest::Client,
    base_url: &str,
    key: Option<&str>,
    model: &str,
    voice: &str,
    text: &str,
) -> Result<Vec<u8>, String> {
    let mut request = http
        .post(speech_url(base_url))
        .header("accept", "audio/mpeg")
        .json(&tts_body(model, voice, text));
    if let Some(key) = key.map(str::trim).filter(|key| !key.is_empty()) {
        request = request.header("authorization", format!("Bearer {key}"));
    }
    let response = request
        .send()
        .await
        .map_err(|e| format!("The speech request failed: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        // The endpoint's own words, trimmed: "no such model" and "invalid voice"
        // are things only it knows, and both are the user's to fix.
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "The speech endpoint answered {status}: {}",
            body.trim()
        ));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("The clip could not be read back: {e}"))?
        .to_vec();
    if bytes.is_empty() {
        return Err("The speech endpoint returned an empty clip.".to_string());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_longer_script_is_a_longer_estimate() {
        let short = estimate(900, 0.015);
        let long = estimate(1_800, 0.015);
        assert_eq!(short.seconds, 60);
        assert_eq!(long.seconds, 120);
        assert!(long.usd > short.usd);
        assert!((long.usd - 0.03).abs() < 1e-9, "two minutes at $0.015: {long:?}");
    }

    #[test]
    fn an_estimate_rounds_up_rather_than_down() {
        // 100 characters is a tenth of a minute, and a clip is not free.
        let small = estimate(100, 0.015);
        assert_eq!(small.seconds, 7);
        assert_eq!(estimate(1, 0.015).seconds, 1);
        assert_eq!(estimate(0, 0.015).seconds, 0);
    }

    #[test]
    fn no_published_rate_is_unknown_money_rather_than_free() {
        let unknown = estimate(3_500, 0.0);
        assert!(!unknown.priced);
        assert_eq!(unknown.usd, 0.0);
        assert_eq!(unknown.seconds, 234);
        assert!(estimate(3_500, 1.0).priced);
    }

    #[test]
    fn the_cap_is_inside_what_one_request_takes() {
        // The number the estimate is quoted from and the number the request is
        // capped at are the same one, and it has to stay under the published
        // ceiling of this shape.
        assert!(MAX_SCRIPT_CHARS < 4_096);
        assert_eq!(estimate(MAX_SCRIPT_CHARS, 0.015).seconds, 234);
        assert!(estimate(MAX_SCRIPT_CHARS, 0.0).fits());
        assert!(!estimate(MAX_SCRIPT_CHARS + 1, 0.0).fits());
        assert_eq!(estimate(10, 0.0).limit, MAX_SCRIPT_CHARS);
    }

    #[test]
    fn the_multipart_body_is_the_shape_the_endpoint_reads() {
        let audio = [0x1au8, 0x45, 0xdf];
        let body = multipart("B", "whisper-1", "dictation.webm", "audio/webm", &audio);
        let head = "--B\r\nContent-Disposition: form-data; name=\"file\"; filename=\"dictation.webm\"\r\nContent-Type: audio/webm\r\n\r\n";
        // The CRLF that opens the closing boundary is what ends the file's part:
        // without it the model field would be read as more of the recording.
        let tail = "\r\n--B\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\nwhisper-1\r\n--B--\r\n";
        assert_eq!(body.len(), head.len() + audio.len() + tail.len());
        assert_eq!(&body[..head.len()], head.as_bytes());
        // Byte-for-byte through the middle: the audio is the one part that is
        // not text, so it is checked as bytes rather than as a lossy string.
        assert_eq!(&body[head.len()..head.len() + audio.len()], &audio);
        assert_eq!(
            String::from_utf8_lossy(&body[head.len() + audio.len()..]),
            tail
        );
    }

    #[test]
    fn the_transcription_path_joins_once_and_keeps_the_version() {
        assert_eq!(
            transcribe_url("https://api.groq.com/openai/v1"),
            "https://api.groq.com/openai/v1/audio/transcriptions"
        );
        assert_eq!(
            transcribe_url("https://api.openai.com/v1/"),
            "https://api.openai.com/v1/audio/transcriptions"
        );
    }

    #[test]
    fn the_body_asks_for_mp3_and_the_voice_it_was_given() {
        let body = tts_body("tts-1", "alloy", "Hello there.");
        assert_eq!(body["model"], "tts-1");
        assert_eq!(body["voice"], "alloy");
        assert_eq!(body["input"], "Hello there.");
        assert_eq!(body["response_format"], "mp3");
    }

    #[test]
    fn the_prompt_states_the_cap_it_will_be_held_to() {
        let prompt = script_prompt("user: hello", MAX_SCRIPT_CHARS);
        assert!(prompt.contains("3500 characters"), "{prompt}");
        assert!(prompt.contains("4 minutes"), "{prompt}");
        assert!(prompt.contains("user: hello"), "the transcript travels");
    }

    #[test]
    fn the_path_is_appended_to_the_base_url_once() {
        assert_eq!(
            speech_url("https://api.deepseek.com/"),
            "https://api.deepseek.com/audio/speech"
        );
        assert_eq!(speech_url("https://api.openai.com/v1"), "https://api.openai.com/v1/audio/speech");
    }
}
