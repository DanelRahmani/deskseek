//! User settings, stored as JSON next to the database. Never contains an API
//! key — those live in the OS keychain, one account per endpoint.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::api::provider::{self, Endpoint, Model};

/// The base prompt a fresh install starts with.
///
/// It names the systems the assistant is actually wired to — memory, tools,
/// skills, MCP — because nothing else in the prompt says they exist: the tool
/// schemas say what a tool takes, not that there is a memory store or that the
/// user can switch skills on. The rules for each of those live in the generated
/// guidance further down, so this stays a statement of what there is.
pub const DEFAULT_SYSTEM_PROMPT: &str = "You are DeskSeek, a desktop assistant running on the user's own machine.\n\n\
You have a long-term memory. Your profile, and any notes recalled for this turn, are in this \
prompt, and `save_memory` adds to it — so something worth keeping across chats (a preference, a \
decision, a fact about the user's work) should be remembered rather than asked again later.\n\n\
You also have tools: their definitions are the complete list of what you can do, and a tool that \
is not defined there does not exist here. Through them you get the user's files and shell, the \
web when the user has it switched on, documents and artifacts for anything worth keeping, and — \
per chat — the user's skills and MCP servers, whose contents appear in this prompt when active.\n\n\
Be concise, say what you are about to do before you do it, and prefer asking over inventing.";

/// Where a voice comes from, and what it costs.
///
/// One named endpoint plus what to ask it for, rather than a third dialect. The
/// speech API is one shape whatever the endpoint speaks, so a dialect would be a
/// trait implemented once and chosen nowhere — and the endpoint is already the
/// thing that holds a base URL and a key.
///
/// The endpoint id is deliberately allowed to be empty *and* to name nothing: a
/// fresh install has no speech provider, and both directions then refuse with a
/// sentence about the setting rather than failing somewhere further in with a
/// 404 that reads as a wrong key.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Speech {
    /// The id of an endpoint that serves audio. Empty means none is configured.
    #[serde(default)]
    pub endpoint: String,
    /// The model that reads a script out. The vendor default, because it is the
    /// name every endpoint that copies this shape accepts.
    #[serde(default = "default_tts_model")]
    pub tts_model: String,
    #[serde(default = "default_voice")]
    pub voice: String,
    /// The model that turns a recording back into words.
    #[serde(default = "default_stt_model")]
    pub stt_model: String,
    /// Published USD per minute of speech. 0 records the clip and reports its
    /// money as unknown rather than as free.
    #[serde(default)]
    pub usd_per_minute: f64,
}

fn default_tts_model() -> String {
    "tts-1".to_string()
}

fn default_voice() -> String {
    "alloy".to_string()
}

/// The name every endpoint that copies this shape accepts for speech to text.
fn default_stt_model() -> String {
    "whisper-1".to_string()
}

impl Default for Speech {
    fn default() -> Self {
        Self {
            endpoint: String::new(),
            tts_model: default_tts_model(),
            voice: default_voice(),
            stt_model: default_stt_model(),
            usd_per_minute: 0.0,
        }
    }
}

/// A named instruction block that is injected only in matching contexts, so
/// "when you write a document, use this type" does not pollute every chat.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instruction {
    pub id: String,
    pub name: String,
    /// `chat` | `coding` | `both`
    pub scope: String,
    pub body: String,
    pub enabled: bool,
}

fn default_instructions() -> Vec<Instruction> {
    vec![
        Instruction {
            id: "documents".to_string(),
            name: "Document design".to_string(),
            scope: "chat".to_string(),
            body: "When you write or edit a document, follow this house style:\n\
- Body text at 11pt in a serif face; headings in a sans-serif face at 1.2x the body size.\n\
- One blank line between paragraphs, never indentation.\n\
- Write documents as Markdown unless asked for another format.\n\
- Offer to save the result to a file rather than only printing it in the chat."
                .to_string(),
            enabled: true,
        },
        Instruction {
            id: "coding-style".to_string(),
            name: "Coding style".to_string(),
            scope: "coding".to_string(),
            body: "Match the conventions already in the repository — read a neighbouring file \
before inventing a new pattern. Prefer the smallest change that works, and leave one runnable \
check behind for non-trivial logic."
                .to_string(),
            enabled: true,
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Every endpoint the user can choose between, each with its own dialect,
    /// base URL, key reference and model list. See `api::provider`.
    #[serde(default)]
    pub endpoints: Vec<Endpoint>,
    /// Which endpoint a new turn uses. An id that names nothing falls back to the
    /// first, so deleting an endpoint cannot leave the app unable to send.
    #[serde(default)]
    pub default_endpoint: String,
    /// Where a voice comes from. See `Speech`.
    #[serde(default)]
    pub speech: Speech,
    /// The single base URL this app had before it knew about endpoints.
    ///
    /// Read once, folded into endpoint one and blanked — never a second answer to
    /// the same question. See `Settings::migrate`.
    #[serde(default)]
    pub base_url: String,
    /// The model a new *chat* starts on.
    pub chat_model: String,
    /// The model a new *coding* chat starts on.
    pub coding_model: String,
    pub system_prompt: String,
    pub thinking: bool,
    pub effort: String,
    /// A theme id from `THEMES` in src/lib/theme.ts. An unknown id resolves to
    /// the first theme rather than failing, so this is free to grow.
    #[serde(default = "default_theme")]
    pub theme: String,
    /// `system` follows the OS; `dark` and `light` override it.
    #[serde(default = "default_theme_mode")]
    pub theme_mode: String,
    /// Accent colour as `#rrggbb`, replacing the theme's own. Empty keeps the
    /// theme's, which is the only value guaranteed to clear the contrast floors.
    #[serde(default)]
    pub accent: String,
    /// UI font family. Empty uses the built-in stack.
    #[serde(default)]
    pub font_ui: String,
    /// Code font family. Empty uses the built-in stack.
    #[serde(default)]
    pub font_code: String,
    /// Base UI font size in px. 0 uses the default.
    #[serde(default)]
    pub font_size: u32,
    /// Suppress transitions and animations.
    #[serde(default)]
    pub reduce_motion: bool,
    /// Folder chat-mode document tools may touch. Empty means the default
    /// `documents` folder inside the app data directory.
    #[serde(default)]
    pub documents_dir: String,
    #[serde(default = "default_instructions")]
    pub instructions: Vec<Instruction>,
    /// Master switch for fetch_url / browser_read / web_search.
    #[serde(default = "default_true")]
    pub browsing_enabled: bool,
    /// Base URL of a SearXNG instance that allows `format=json`. Empty disables
    /// web search but keeps the two fetch tools working.
    #[serde(default)]
    pub searxng_url: String,
    /// Optional explicit path to a Chromium-family browser. Empty means
    /// auto-detect (Edge, then Chrome, then Chromium on PATH).
    #[serde(default)]
    pub browser_path: String,
    /// Permission mode new chats start in: `manual` | `auto` | `skip`.
    #[serde(default = "default_permission_mode")]
    pub permission_mode: String,
    /// Stop a run once it has spent this many dollars. 0 disables the limit.
    /// Checked between rounds, because a loop of rounds is where a bill runs
    /// away — see `conversation::agent::run`.
    #[serde(default)]
    pub cost_limit_usd: f64,
    /// Where shell commands run: `auto` (the strongest rung available) |
    /// `host` | `docker` | `wsl`. Only the first two of those are isolation.
    #[serde(default = "default_sandbox")]
    pub sandbox: String,
    /// Image the Docker rung runs commands in. It must already carry whatever
    /// toolchain the workspace needs — `alpine` is small and starts instantly,
    /// but it has no node, no cargo and no python.
    #[serde(default = "default_sandbox_image")]
    pub sandbox_image: String,
    /// Master switch for reading the desktop (`ui_snapshot`). Off by default:
    /// the accessibility tree is whatever is on screen, and what is on screen
    /// may be private.
    #[serde(default)]
    pub computer_use: bool,
    /// Master switch for OS notifications. Off silences every toast; the in-app
    /// notice is not affected, because it costs nothing and cannot be missed.
    #[serde(default = "default_true")]
    pub notifications: bool,
    /// Raise a toast only when the window is not in front of the user. When it
    /// is, the answer is already on screen and a popup over it is noise.
    #[serde(default = "default_true")]
    pub notify_only_when_unfocused: bool,
    /// A nudge to get up and move, every `reminder_minutes`. Off by default: it
    /// is the one switch here that interrupts on a timer rather than in response
    /// to something that happened.
    #[serde(default)]
    pub reminders: bool,
    /// How often that nudge comes, in minutes.
    #[serde(default = "default_reminder_minutes")]
    pub reminder_minutes: u32,
    /// Hold popups back inside `quiet_from`..`quiet_to`. The in-app notice still
    /// appears — silence is not the same as pretending nothing happened.
    #[serde(default)]
    pub quiet_hours: bool,
    /// Local "HH:MM". The window may wrap midnight; 18:00 → 09:00 is the usual one.
    #[serde(default = "default_quiet_from")]
    pub quiet_from: String,
    #[serde(default = "default_quiet_to")]
    pub quiet_to: String,
    /// The chord that raises the quick-entry window. Empty leaves it to the tray
    /// and the palette, which is the honest answer on a machine where no chord
    /// can be registered.
    #[serde(default = "default_quick_chord")]
    pub quick_chord: String,
    /// Where a quick question goes: `new` (a fresh conversation every time) or
    /// `last` (the most recent one).
    #[serde(default = "default_quick_target")]
    pub quick_target: String,
    /// Set once the first-run setup screen has been finished or dismissed.
    #[serde(default)]
    pub setup_done: bool,
}

fn default_permission_mode() -> String {
    "manual".to_string()
}

fn default_sandbox() -> String {
    "auto".to_string()
}

fn default_sandbox_image() -> String {
    "alpine:latest".to_string()
}

fn default_theme() -> String {
    "slate".to_string()
}

fn default_theme_mode() -> String {
    "system".to_string()
}

fn default_reminder_minutes() -> u32 {
    50
}

fn default_quiet_from() -> String {
    "18:00".to_string()
}

fn default_quiet_to() -> String {
    "09:00".to_string()
}

fn default_quick_chord() -> String {
    crate::quick::DEFAULT_CHORD.to_string()
}

fn default_quick_target() -> String {
    "last".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            endpoints: provider::seed(),
            default_endpoint: provider::SEED_ENDPOINT_ID.to_string(),
            speech: Speech::default(),
            base_url: String::new(),
            chat_model: "deepseek-flash".to_string(),
            coding_model: "deepseek-v4-pro".to_string(),
            system_prompt: DEFAULT_SYSTEM_PROMPT.to_string(),
            thinking: true,
            effort: "high".to_string(),
            theme: default_theme(),
            theme_mode: default_theme_mode(),
            accent: String::new(),
            font_ui: String::new(),
            font_code: String::new(),
            font_size: 0,
            reduce_motion: false,
            documents_dir: String::new(),
            instructions: default_instructions(),
            browsing_enabled: true,
            searxng_url: String::new(),
            browser_path: String::new(),
            permission_mode: default_permission_mode(),
            cost_limit_usd: 0.0,
            sandbox: default_sandbox(),
            sandbox_image: default_sandbox_image(),
            computer_use: false,
            notifications: true,
            notify_only_when_unfocused: true,
            reminders: false,
            reminder_minutes: default_reminder_minutes(),
            quiet_hours: false,
            quiet_from: default_quiet_from(),
            quiet_to: default_quiet_to(),
            quick_chord: default_quick_chord(),
            quick_target: default_quick_target(),
            setup_done: false,
        }
    }
}

/// Loads settings, falling back to defaults when the file is missing or
/// unreadable — a corrupt config should never stop the app starting.
pub fn load(path: &Path) -> Settings {
    let mut settings: Settings = std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    settings.migrate();
    settings
}

impl Settings {
    /// The endpoint a turn runs against: the default one, or the first.
    ///
    /// The fallback is what makes deleting an endpoint safe — an id that names
    /// nothing must not leave the app unable to send anything.
    pub fn endpoint(&self) -> Option<&Endpoint> {
        self.endpoints
            .iter()
            .find(|endpoint| endpoint.id == self.default_endpoint)
            .or_else(|| self.endpoints.first())
    }

    /// The endpoint that speaks, if the user has named one that still exists.
    ///
    /// Same rule as `endpoint()` in one respect and deliberately not in another:
    /// it matches by id rather than falling back to the first, because a chat
    /// key pointed at a vendor with no audio API is a worse answer than "no
    /// speech configured".
    pub fn speech_endpoint(&self) -> Option<&Endpoint> {
        let wanted = self.speech.endpoint.trim();
        if wanted.is_empty() {
            return None;
        }
        self.endpoints.iter().find(|endpoint| endpoint.id == wanted)
    }

    /// The model record for an id, searched in the default endpoint first and then
    /// in every other.
    ///
    /// Everywhere rather than only the default endpoint, because a chat keeps the
    /// model it was created with: switching the default endpoint must not turn an
    /// existing chat's price into "unknown" and its window into a guess. An id
    /// nothing lists gets the unlisted placeholder, which is priced unknown and
    /// cannot see, rather than an error.
    pub fn model_info(&self, id: &str) -> Model {
        self.endpoint()
            .and_then(|endpoint| endpoint.model(id))
            .or_else(|| {
                self.endpoints
                    .iter()
                    .find_map(|endpoint| endpoint.model(id))
            })
            .cloned()
            .unwrap_or_else(|| Model::unlisted(id))
    }

    /// The window a model has here, for the fold decision.
    pub fn context_for(&self, id: &str) -> u32 {
        self.model_info(id).context
    }

    /// The model a new chat of this kind starts on.
    ///
    /// Falls back to the endpoint's first model when the stored preference is not
    /// one it serves. Switching the default endpoint is the ordinary way to end up
    /// here, and a chat created for a model the endpoint does not have fails on its
    /// first turn with a message about a model rather than about a setting.
    pub fn model_for(&self, kind: &str) -> String {
        let preferred = if kind == "coding" {
            &self.coding_model
        } else {
            &self.chat_model
        };
        let Some(endpoint) = self.endpoint() else {
            return preferred.clone();
        };
        if endpoint.model(preferred).is_some() {
            return preferred.clone();
        }
        endpoint
            .first_model()
            .map(|model| model.id.clone())
            .unwrap_or_else(|| preferred.clone())
    }

    /// The model the app's own cheap calls use: the summariser and the reviewer.
    ///
    /// Picked rather than named. The constant this replaced was DeepSeek's id, and
    /// a second endpoint as the default would have failed every summary and every
    /// review call with a 400. The cheapest *priced* model wins, then the first
    /// model the endpoint lists — and with no endpoints at all, the configured chat
    /// model, which is at least a real id and a request that will fail for the
    /// honest reason instead of this one.
    pub fn summary_model(&self) -> String {
        let Some(endpoint) = self.endpoint() else {
            return self.chat_model.clone();
        };
        let out_of = |model: &Model| model.price.map(|price| price.out).unwrap_or(f64::INFINITY);
        endpoint
            .models
            .iter()
            .filter(|model| model.price.is_some())
            .min_by(|a, b| out_of(a).total_cmp(&out_of(b)))
            .or_else(|| endpoint.first_model())
            .map(|model| model.id.clone())
            .unwrap_or_else(|| self.chat_model.clone())
    }

    /// Folds the single base URL this app had before it knew about endpoints into
    /// endpoint one, once.
    ///
    /// Idempotent by construction: the legacy field is blanked here and always
    /// written blank afterwards, so a second load has nothing to fold. The order
    /// matters — the seed is built first so there is an endpoint to fold *into*,
    /// and only then is the legacy URL applied to it.
    fn migrate(&mut self) {
        let legacy = self.base_url.trim().to_string();
        let fresh = self.endpoints.is_empty();
        if fresh {
            self.endpoints = provider::seed();
        }
        if fresh && !legacy.is_empty() {
            if let Some(first) = self.endpoints.first_mut() {
                first.base_url = legacy.clone();
            }
        }
        if !legacy.is_empty() {
            // Blanked in memory as well as on the next save, so nothing later in
            // this process can read it as a second answer to the same question.
            self.base_url.clear();
        }
        let named = self
            .endpoints
            .iter()
            .any(|endpoint| endpoint.id == self.default_endpoint);
        if !named {
            self.default_endpoint = self
                .endpoints
                .first()
                .map(|endpoint| endpoint.id.clone())
                .unwrap_or_default();
        }
    }
}

pub fn save(path: &Path, settings: &Settings) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(settings).expect("settings are serializable");
    std::fs::write(path, json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_yields_defaults() {
        let s = load(Path::new("/definitely/not/here/settings.json"));
        assert_eq!(s.endpoints.len(), 1);
        assert_eq!(s.endpoint().unwrap().id, provider::SEED_ENDPOINT_ID);
        assert_eq!(s.endpoint().unwrap().base_url, "https://api.deepseek.com");
        assert_eq!(s.chat_model, "deepseek-flash");
        // A file written before reminders existed must still load, with the
        // switches off and a window that is never consulted.
        assert!(!s.reminders && !s.quiet_hours);
        assert_eq!(s.reminder_minutes, 50);
        assert_eq!(s.quiet_from, "18:00");
        assert_eq!(s.quiet_to, "09:00");
        // Same for the quick-entry pair, which is newer still: a file with no
        // `quickChord` at all gets the default rather than an empty string, or
        // an upgrading install would lose the chord it never had to set.
        assert_eq!(s.quick_chord, crate::quick::DEFAULT_CHORD);
        assert_eq!(s.quick_target, "last");
    }

    #[test]
    fn a_file_written_before_endpoints_fold_its_url_into_endpoint_one() {
        let dir = std::env::temp_dir().join(format!("deskseek-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        // The shape 0.4.0 wrote: one base URL, and no notion of an endpoint at
        // all. Written by serialising the real struct rather than by hand, so the
        // fixture cannot drift away from what an old install actually holds.
        let mut old = Settings::default();
        old.endpoints.clear();
        old.default_endpoint.clear();
        old.base_url = "https://gateway.internal/v1".to_string();
        save(&path, &old).unwrap();
        let s = load(&path);
        assert_eq!(s.endpoints.len(), 1);
        assert_eq!(s.endpoint().unwrap().base_url, "https://gateway.internal/v1");
        assert_eq!(s.default_endpoint, provider::SEED_ENDPOINT_ID);
        // Blanked, so the migration cannot run twice and the field cannot come
        // back to life as a second source of truth.
        assert!(s.base_url.is_empty());

        // Saving that and loading it again finds nothing left to fold.
        save(&path, &s).unwrap();
        let back = load(&path);
        assert!(back.base_url.is_empty());
        assert_eq!(back.endpoint().unwrap().base_url, "https://gateway.internal/v1");
        assert_eq!(back.endpoints.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_speech_endpoint_is_matched_by_id_or_not_at_all() {
        let mut s = Settings::default();
        // A fresh install has no voice, and says so rather than picking the chat
        // endpoint: a key pointed at a vendor with no audio API is a worse answer
        // than "not configured".
        assert!(s.speech_endpoint().is_none());
        assert_eq!(s.speech.tts_model, "tts-1");
        assert_eq!(s.speech.voice, "alloy");
        assert_eq!(s.speech.stt_model, "whisper-1");
        assert_eq!(s.speech.usd_per_minute, 0.0);

        s.speech.endpoint = provider::SEED_ENDPOINT_ID.to_string();
        assert_eq!(s.speech_endpoint().unwrap().id, provider::SEED_ENDPOINT_ID);

        // Deliberately *not* the fallback `endpoint()` makes: an id the user set
        // and later deleted reads as unconfigured, because the alternative is
        // sending a script to whichever endpoint happens to be first.
        s.speech.endpoint = "deleted-this-morning".to_string();
        assert!(s.speech_endpoint().is_none());
        // Whitespace is not an endpoint either.
        s.speech.endpoint = "   ".to_string();
        assert!(s.speech_endpoint().is_none());
    }

    #[test]
    fn a_settings_file_written_before_voice_existed_still_loads() {
        // Written by serialising the real struct and dropping the field, which is
        // what an older file looks like on disk.
        let mut json = serde_json::to_value(Settings::default()).unwrap();
        json.as_object_mut().unwrap().remove("speech");
        let s: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(s.speech.tts_model, "tts-1");
        assert!(s.speech.endpoint.is_empty());
        assert!(s.speech_endpoint().is_none());
    }

    #[test]
    fn an_endpoint_id_that_names_nothing_falls_back_to_the_first() {
        let mut s = Settings::default();
        s.default_endpoint = "deleted-this-morning".to_string();
        s.migrate();
        assert_eq!(s.default_endpoint, provider::SEED_ENDPOINT_ID);
        assert_eq!(s.endpoint().unwrap().id, provider::SEED_ENDPOINT_ID);

        // And an empty list is seeded rather than left bare: a hand-edited file
        // must not be able to leave the app with nothing to send to.
        s.endpoints.clear();
        s.migrate();
        assert_eq!(s.endpoints.len(), 1);
        assert!(s.endpoint().is_some());
    }

    #[test]
    fn the_models_the_app_itself_picks_come_from_the_default_endpoint() {
        let s = Settings::default();
        // The cheapest priced model, not the first one, because these are the
        // calls the user did not ask for.
        assert_eq!(s.summary_model(), "deepseek-flash");
        assert_eq!(s.model_for("chat"), "deepseek-flash");
        assert_eq!(s.model_for("coding"), "deepseek-v4-pro");
        assert_eq!(s.context_for("deepseek-flash"), 1_000_000);
        // An id nothing lists is priced unknown and cannot see, rather than an
        // error or a zero price.
        let unknown = s.model_info("gpt-9");
        assert!(unknown.price.is_none());
        assert!(!unknown.vision);
        assert_eq!(s.context_for("gpt-9"), crate::api::models::LARGEST_CONTEXT);
    }

    #[test]
    fn a_stale_model_preference_falls_back_to_one_the_endpoint_serves() {
        let mut s = Settings::default();
        s.chat_model = "a-model-from-another-vendor".to_string();
        s.coding_model = "another-one".to_string();
        // New chats would otherwise fail on their first turn with a message about
        // a model rather than about a setting.
        assert_eq!(s.model_for("chat"), "deepseek-flash");
        assert_eq!(s.model_for("coding"), "deepseek-flash");
    }

    #[test]
    fn a_second_endpoint_supplies_its_own_cheap_model() {
        let mut s = Settings::default();
        s.endpoints.push(provider::Endpoint {
            id: "local".to_string(),
            label: "Ollama".to_string(),
            dialect: provider::Dialect::OpenAi,
            base_url: "http://localhost:11434".to_string(),
            key_ref: String::new(),
            models: vec![
                provider::Model {
                    id: "tiny".to_string(),
                    label: "tiny".to_string(),
                    context: 8_192,
                    max_output: 2_048,
                    vision: false,
                    effort: true,
                    price: None,
                },
                provider::Model {
                    id: "small".to_string(),
                    label: "small".to_string(),
                    context: 32_768,
                    max_output: 4_096,
                    vision: true,
                    effort: true,
                    price: Some(provider::Price {
                        hit: 0.0,
                        miss: 0.1,
                        out: 0.2,
                        peak_multiplier: 1.0,
                    }),
                },
            ],
            notes: String::new(),
        });
        s.default_endpoint = "local".to_string();
        // The unpriced local model is not chosen for the app's own calls: a rate
        // nobody published is not a rate of zero, so "cheapest" must not read it
        // as free.
        assert_eq!(s.summary_model(), "small");
        // And the preference guard follows the new endpoint.
        assert_eq!(s.model_for("chat"), "tiny");
        assert_eq!(s.context_for("small"), 32_768);
        assert!(s.model_info("small").vision);
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = std::env::temp_dir().join(format!("deskseek-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        let mut s = Settings::default();
        s.chat_model = "deepseek-v4-pro".to_string();
        s.thinking = false;
        s.reminders = true;
        s.reminder_minutes = 25;
        s.quiet_hours = true;
        s.quiet_from = "20:30".to_string();
        s.quiet_to = "07:15".to_string();
        save(&path, &s).unwrap();
        let back = load(&path);
        assert_eq!(back.chat_model, "deepseek-v4-pro");
        assert!(!back.thinking);
        // The reminder switches and the window survive the file in both
        // directions: a colon in "20:30" is exactly the sort of thing a hand-
        // written serialiser loses.
        assert!(back.reminders);
        assert_eq!(back.reminder_minutes, 25);
        assert!(back.quiet_hours);
        assert_eq!(back.quiet_from, "20:30");
        assert_eq!(back.quiet_to, "07:15");
        // The chord is the one setting whose exact spelling matters to something
        // outside this process, so it is the one most worth proving survives the
        // file in both directions.
        s.quick_chord = "Ctrl+Shift+Q".to_string();
        s.quick_target = "new".to_string();
        save(&path, &s).unwrap();
        let back = load(&path);
        assert_eq!(back.quick_chord, "Ctrl+Shift+Q");
        assert_eq!(back.quick_target, "new");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_file_falls_back_to_defaults() {
        let dir = std::env::temp_dir().join(format!("deskseek-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(load(&path).effort, "high");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
