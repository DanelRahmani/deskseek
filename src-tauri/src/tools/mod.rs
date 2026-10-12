//! The built-in coding toolset: what the model may call, and how risky each
//! call is (which decides whether the user is asked to approve it).

pub mod browse;
pub mod browser;
pub mod desktop;
pub mod docs;
pub mod exec;
pub mod extract;
pub mod guard;
pub mod job;
pub mod sandbox;
pub mod sandbox_exec;
pub mod walk;

use serde::Serialize;
use serde_json::json;

use crate::api::types::Tool;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolClass {
    /// Runs without asking — reads only.
    Read,
    /// Handled by the app itself and never sandboxed or prompted — writing
    /// memory (the profile file or a kept fact), publishing the task list, and
    /// creating an artifact, which is stored in the database and shown in a
    /// sandboxed panel rather than put on disk anywhere.
    Internal,
    /// Modifies the workspace; the user approves each call.
    Write,
    /// Executes something; the user approves each call.
    Command,
    /// Acts on the world outside the workspace: clicking, typing or running
    /// script in a real browser, which can submit a form or buy something, and
    /// reading the user's desktop. Approved on every call in manual *and* auto
    /// mode: the agent is acting as the user on a site neither of us controls,
    /// and no string heuristic can tell a harmless click from a purchase.
    ///
    /// *Reading* a page through the browser is not in here — `browser_read`,
    /// `browser_open` and `browser_extract` are `Read`, because they do what
    /// `fetch_url` does (in a browser, so JavaScript has run first) and the app
    /// never touches the browser the user is signed into.
    Computer,
}

/// Tools that only exist when the user has switched internet access on. The
/// `browser_` prefix covers `browser_read` and the whole automation set.
pub fn is_browsing_tool(name: &str) -> bool {
    matches!(name, "fetch_url" | "web_search") || name.starts_with("browser_")
}

/// How much rope the assistant has in a chat.
///
/// A ladder, and the rungs are about what is *allowed* rather than tone: the mode
/// picks the tool list, decides whether writes and commands are approved first,
/// and says what the prompt is asking for. Two knobs that both mean "how much may
/// I do" would drift apart, so this is the only one — the approval mode still
/// exists, but only standard mode consults it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentMode {
    /// Conversation only: no tools at all, not even a read.
    Ask,
    /// Everything the section offers; approvals follow the chat's permission mode.
    Standard,
    /// Reads and lookups only: files, and the web through `fetch_url`,
    /// `web_search` and the browser's reading tools. Nothing is written, run or
    /// clicked, so the mode can promise that nothing has changed.
    Plan,
    /// Everything, and nothing asks for approval: work until it is done.
    Agent,
}

impl AgentMode {
    /// An unknown or missing value is standard: the middle of the ladder, rather
    /// than either extreme, is the safe thing to fall back to.
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "ask" => Self::Ask,
            "plan" | "planner" => Self::Plan,
            "agent" | "autonomous" => Self::Agent,
            _ => Self::Standard,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Standard => "standard",
            Self::Plan => "plan",
            Self::Agent => "agent",
        }
    }

    /// Whether a tool of this class is offered at all in this mode.
    pub fn allows(self, class: ToolClass) -> bool {
        match self {
            Self::Ask => false,
            // Internal tools are the app's own bookkeeping — the plan, memory, an
            // artifact — and none of them touches the user's files, which is what
            // plan mode is protecting. Reading the web is a lookup, so it is in
            // here too; clicking or typing on a page is not.
            Self::Plan => matches!(class, ToolClass::Read | ToolClass::Internal),
            Self::Standard | Self::Agent => true,
        }
    }
}

/// Tools that only exist when the user has switched computer use on. Nothing
/// else in the toolset starts with these prefixes.
pub fn is_computer_tool(name: &str) -> bool {
    name.starts_with("ui_") || name.starts_with("screen_")
}

fn params(props: serde_json::Value, required: &[&str]) -> serde_json::Value {
    json!({
        "type": "object",
        "properties": props,
        "required": required,
        "additionalProperties": false
    })
}

/// How many steps a plan may have. The panel is a progress indicator, not a
/// project tracker; a model that needs more than this needs to re-plan.
pub const MAX_TASKS: usize = 20;

/// What the artifact panel knows how to render. A closed set: an unknown kind
/// would render as a blank panel and the model would never learn why.
pub const ARTIFACT_KINDS: [&str; 5] = ["html", "svg", "mermaid", "markdown", "code"];

/// Artifacts live in the database, so this cap is about keeping one runaway
/// generation from bloating it — well past what the panel can usefully show.
pub const MAX_ARTIFACT_CHARS: usize = 200_000;

/// A validated `artifact_create` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactDraft {
    pub title: String,
    pub kind: String,
    pub content: String,
}

/// Validates `artifact_create`. Like `parse_tasks`, the error text is fed back
/// to the model, so it should say what to send instead.
pub fn parse_artifact(args: &serde_json::Value) -> Result<ArtifactDraft, String> {
    let title = args
        .get("title")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or("artifact_create needs a short \"title\"")?;
    let kind = args
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    if !ARTIFACT_KINDS.contains(&kind) {
        return Err(format!(
            "artifact_create kind must be one of {}",
            ARTIFACT_KINDS.join(", ")
        ));
    }
    let content = args
        .get("content")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or("artifact_create needs non-empty \"content\"")?;
    if content.chars().count() > MAX_ARTIFACT_CHARS {
        return Err(format!(
            "artifact_create content is longer than {MAX_ARTIFACT_CHARS} characters; split it into \
             several artifacts"
        ));
    }
    Ok(ArtifactDraft {
        // Capped so a runaway title cannot stretch the tab strip.
        title: title.chars().take(120).collect(),
        kind: kind.to_string(),
        content: content.to_string(),
    })
}

/// A validated `save_memory` call: what to remember and where it belongs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryDraft {
    pub entries: Vec<String>,
    /// `"profile"` for the always-injected `MEMORY.md`, otherwise a category id
    /// from `memory_files::CATEGORIES`. Anything unrecognised becomes `other`.
    pub target: &'static str,
}

/// A validated `create_skill` call: a skill as the model describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillDraft {
    pub name: String,
    pub description: String,
    pub body: String,
    /// Set when the model means to rewrite a skill that is already there.
    pub replace: bool,
}

/// Validates a `name` + `body` + optional `description`/`replace` payload.
///
/// Shared by `create_skill` and `create_design_system`: the two write different
/// stores, but what the model sends is the same shape and an empty body would
/// join the prompt and change nothing either way — worse than an error it can
/// read and correct. `tool` names the caller in those errors, so the message
/// points at the call that was actually made.
fn parse_written(args: &serde_json::Value, tool: &str) -> Result<SkillDraft, String> {
    let field = |key: &str| {
        args.get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string()
    };
    let name = field("name");
    if name.is_empty() {
        return Err(format!("{tool} needs a name."));
    }
    let body = field("body");
    if body.is_empty() {
        return Err(format!("{tool} needs a body: the text itself."));
    }
    Ok(SkillDraft {
        name,
        description: field("description"),
        body,
        replace: args
            .get("replace")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
    })
}

/// Validates `create_skill`.
pub fn parse_skill_draft(args: &serde_json::Value) -> Result<SkillDraft, String> {
    parse_written(args, "create_skill")
}

/// Validates `create_design_system`.
///
/// A design system has no description of its own, so one sent anyway is ignored
/// rather than stored as a field nothing reads.
pub fn parse_design_draft(args: &serde_json::Value) -> Result<SkillDraft, String> {
    parse_written(args, "create_design_system")
}

/// Validates `save_memory`.
///
/// The default is deliberately the fallback category rather than the profile: the
/// profile file is injected into every request, so it must not grow by accident.
/// Only an explicit `profile` reaches it, and only a name that matches a real
/// category lands in that category — a model inventing one gets `other` rather
/// than a new file per conversation. The summariser applies the same rule to its
/// own proposals, so the two paths cannot disagree.
pub fn parse_memory(args: &serde_json::Value) -> Result<MemoryDraft, String> {
    let entries: Vec<String> = args
        .get("entries")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    if entries.is_empty() {
        return Err("save_memory needs at least one non-empty entry".to_string());
    }
    let target = match args.get("category").and_then(serde_json::Value::as_str) {
        Some(c) if c.trim().eq_ignore_ascii_case(crate::memory_files::PROFILE) => {
            crate::memory_files::PROFILE
        }
        Some(c) => crate::memory_files::canonical(c).unwrap_or(crate::memory_files::FALLBACK),
        None => crate::memory_files::FALLBACK,
    };
    Ok(MemoryDraft { entries, target })
}

/// One line of the model's plan. A closed set, so a typo in the tool call is
/// caught here rather than rendering as a mystery glyph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Task {
    pub text: String,
    pub status: TaskStatus,
}

/// Validates the `update_tasks` arguments. Strict about the shape: the error
/// text is fed back to the model, so it should say what to send instead.
///
/// The frontend parses the same arguments for display and is deliberately
/// forgiving — this is where the model gets corrected.
pub fn parse_tasks(args: &serde_json::Value) -> Result<Vec<Task>, String> {
    let raw = args
        .get("tasks")
        .and_then(serde_json::Value::as_array)
        .ok_or("update_tasks needs a \"tasks\" array")?;
    if raw.is_empty() {
        return Err("update_tasks needs at least one task".to_string());
    }
    if raw.len() > MAX_TASKS {
        return Err(format!("update_tasks takes at most {MAX_TASKS} tasks"));
    }

    let mut tasks = Vec::with_capacity(raw.len());
    for (index, item) in raw.iter().enumerate() {
        let at = index + 1;
        let text = item
            .get("text")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| format!("task {at} needs a non-empty \"text\""))?;
        let status = match item.get("status").and_then(serde_json::Value::as_str) {
            None | Some("pending") => TaskStatus::Pending,
            Some("in_progress") => TaskStatus::InProgress,
            Some("completed") => TaskStatus::Completed,
            Some(other) => {
                return Err(format!(
                    "task {at} has status '{other}'; use pending, in_progress or completed"
                ))
            }
        };
        tasks.push(Task {
            // Capped so one runaway line cannot swamp the panel.
            text: text.chars().take(200).collect(),
            status,
        });
    }
    Ok(tasks)
}

/// A question the model asked, as validated from `ask_user`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    pub header: String,
    pub question: String,
    pub options: Vec<QuestionOption>,
    pub multi_select: bool,
}

/// How many questions one `ask_user` call may carry. More than a handful is an
/// interrogation, and the user answers all of them before the run resumes.
pub const MAX_QUESTIONS: usize = 4;
/// Choices per question. The point is a few concrete options, not a menu.
pub const MAX_OPTIONS: usize = 4;

/// Validates `ask_user`. The error text is fed back to the model, so it should
/// say what to send instead.
pub fn parse_questions(args: &serde_json::Value) -> Result<Vec<Question>, String> {
    let raw = args
        .get("questions")
        .and_then(serde_json::Value::as_array)
        .ok_or("ask_user needs a \"questions\" array")?;
    if raw.is_empty() {
        return Err("ask_user needs at least one question".to_string());
    }
    if raw.len() > MAX_QUESTIONS {
        return Err(format!("ask_user takes at most {MAX_QUESTIONS} questions"));
    }

    let mut out = Vec::with_capacity(raw.len());
    for (index, item) in raw.iter().enumerate() {
        let at = index + 1;
        let question = item
            .get("question")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| format!("question {at} needs a non-empty \"question\""))?;
        let header = item
            .get("header")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .unwrap_or("");
        let opts = item
            .get("options")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| format!("question {at} needs an \"options\" array"))?;
        if opts.is_empty() {
            return Err(format!("question {at} needs at least one option"));
        }
        if opts.len() > MAX_OPTIONS {
            return Err(format!("question {at} takes at most {MAX_OPTIONS} options"));
        }
        let mut options = Vec::with_capacity(opts.len());
        for option in opts {
            let label = option
                .get("label")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| format!("question {at} has an option with no \"label\""))?;
            options.push(QuestionOption {
                // Capped so one runaway label cannot stretch the composer.
                label: label.chars().take(80).collect(),
                description: option
                    .get("description")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .trim()
                    .chars()
                    .take(200)
                    .collect(),
            });
        }
        out.push(Question {
            header: header.chars().take(12).collect(),
            question: question.chars().take(400).collect(),
            options,
            multi_select: item
                .get("multiSelect")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
        });
    }
    Ok(out)
}

/// (spec, class) for every built-in coding tool.
pub fn builtin_tools() -> Vec<(Tool, ToolClass)> {
    vec![
        (
            Tool::function(
                "read_file",
                "Read a file from the workspace. A Word (.docx), Excel (.xlsx), PowerPoint (.pptx) or PDF file is opened as a document and handed back as the text in it — formatting, images and comments are left out. Everything else is read as UTF-8 text.",
                params(json!({"path": {"type": "string", "description": "Workspace-relative path"}}), &["path"]),
            ),
            ToolClass::Read,
        ),
        (
            Tool::function(
                "list_dir",
                "List the entries of a directory in the workspace.",
                params(json!({"path": {"type": "string", "description": "Directory, relative to the workspace. Defaults to the root."}}), &[]),
            ),
            ToolClass::Read,
        ),
        (
            Tool::function(
                "glob",
                "Find files by glob pattern. Supports *, ? and **.",
                params(json!({"pattern": {"type": "string", "description": "e.g. src/**/*.rs"}}), &["pattern"]),
            ),
            ToolClass::Read,
        ),
        (
            Tool::function(
                "grep",
                "Search for a literal string in the workspace's text files.",
                params(
                    json!({
                        "pattern": {"type": "string", "description": "Literal text to find"},
                        "path": {"type": "string", "description": "File or directory to search. Defaults to the workspace root."}
                    }),
                    &["pattern"],
                ),
            ),
            ToolClass::Read,
        ),
        (
            Tool::function(
                "search_history",
                "Search what was said in earlier conversations in this same place. Use it when the user refers to something you do not have in this conversation — \"what did we decide about X\", \"the thing we worked out last week\" — instead of guessing or asking again. It searches the conversation history, not the files: for the code or the documents use grep. It is scoped to this conversation's own folder and section, so it cannot read another folder's chats.",
                params(
                    json!({
                        "query": {"type": "string", "description": "Words to look for, as the user would say them"},
                        "limit": {"type": "integer", "description": "How many matches to return. Default 8, maximum 20."}
                    }),
                    &["query"],
                ),
            ),
            ToolClass::Read,
        ),
        (
            Tool::function(
                "write_file",
                "Create a text file, or replace it entirely if it exists. A .docx, .xlsx, .pptx or .pdf cannot be written this way — use doc_create, sheet_create or slides_create for those.",
                params(
                    json!({
                        "path": {"type": "string", "description": "Workspace-relative path"},
                        "content": {"type": "string", "description": "Full new contents"}
                    }),
                    &["path", "content"],
                ),
            ),
            ToolClass::Write,
        ),
        (
            Tool::function(
                "edit_file",
                "Replace one exact occurrence of a string in a file. Prefer this over write_file for edits.",
                params(
                    json!({
                        "path": {"type": "string", "description": "Workspace-relative path"},
                        "old_string": {"type": "string", "description": "Exact text to replace; must appear exactly once"},
                        "new_string": {"type": "string", "description": "Replacement text"}
                    }),
                    &["path", "old_string", "new_string"],
                ),
            ),
            ToolClass::Write,
        ),
        (
            Tool::function(
                "save_memory",
                "Remember something durable across chats. `category: \"profile\"` writes to MEMORY.md, which is sent with every request, so use it only for who the user is and how they want replies — their name, language, and standing preferences. Every other category is one file looked up only when it is relevant, so those can hold anything else. Never create or rename files for memory.",
                params(
                    json!({
                        "entries": {
                            "type": "array",
                            "description": "One self-contained sentence per fact. No bullets, no dates.",
                            "items": { "type": "string" }
                        },
                        "category": {
                            "type": "string",
                            "enum": crate::memory_files::category_names(),
                            "description": "Which part of the user's life this belongs to. \"profile\" is injected into every request (identity and standing preferences only); every other value is looked up only when relevant. Defaults to \"other\"."
                        }
                    }),
                    &["entries"],
                ),
            ),
            ToolClass::Internal,
        ),
        (
            Tool::function(
                "create_skill",
                "Write a reusable instruction file (a skill) for the user, from this conversation. It is saved into their skills folder and switched on for this chat. Use it when the user describes rules they want you to follow from now on — a house style, a checklist, a way of answering — and say what you are saving first. Call it again with the same name and `replace: true` to revise one you wrote.",
                params(
                    json!({
                        "name": {"type": "string", "description": "Short name, e.g. \"Terse answers\". Becomes the folder name."},
                        "description": {"type": "string", "description": "One line saying when this skill applies"},
                        "body": {"type": "string", "description": "The instructions themselves, in Markdown, addressed to you in future chats"},
                        "replace": {"type": "boolean", "description": "Rewrite a skill of this name that already exists. Only for revising one you wrote."}
                    }),
                    &["name", "body"],
                ),
            ),
            ToolClass::Write,
        ),
        (
            Tool::function(
                "create_design_system",
                "Write or revise a design system: the stored style reference you apply when you build or write UI for this user — the type scale, the palette, the spacing, and the rules that hold them together. It is saved to their design systems and switched on for this chat. Use it when the user describes a house style, or once you have worked out from a project what one is. Call it again with the same name and `replace: true` to revise one.",
                params(
                    json!({
                        "name": {"type": "string", "description": "Short name, e.g. \"Axiom House Style\". Becomes the file name."},
                        "body": {"type": "string", "description": "The system itself, in Markdown: type scale, palette, spacing, rules"},
                        "replace": {"type": "boolean", "description": "Rewrite a design system of this name that already exists."}
                    }),
                    &["name", "body"],
                ),
            ),
            ToolClass::Write,
        ),
        (
            Tool::function(
                "draft_design_system",
                "Start a design system from a folder that already has a look: it reads that folder's stylesheets, CSS custom properties, font stacks and dependencies and writes the reference from what is really there. The result is a starting point to revise rather than a finished system — read it back, fill the gaps, and revise it with `create_design_system`. The folder must be inside the workspace.",
                params(
                    json!({
                        "folder": {"type": "string", "description": "Folder to read, relative to the workspace"},
                        "name": {"type": "string", "description": "Name for the new system. Defaults to the folder's own name."}
                    }),
                    &["folder"],
                ),
            ),
            ToolClass::Write,
        ),
        (
            Tool::function(
                "list_skills",
                "List the skills installed on this machine, with their ids, and which are switched on for this chat. Call it when the user mentions a skill, or when a job might match one they already have.",
                params(json!({}), &[]),
            ),
            ToolClass::Read,
        ),
        (
            Tool::function(
                "use_skill",
                "Switch a skill — or a design system from the design-systems list — on for this chat and read its text. Use it when the user names one, or when a job clearly matches one from `list_skills` or from the design systems listed in your prompt; say which one you are using. What it asks for then applies for the rest of this chat.",
                params(
                    json!({
                        "id": {"type": "string", "description": "The skill's or design system's id, as the catalogue reports it"}
                    }),
                    &["id"],
                ),
            ),
            ToolClass::Internal,
        ),
        (
            Tool::function(
                "ask_user",
                "Ask the user a question and wait for their answer. Use it when you genuinely cannot proceed without a decision only they can make — a preference, a choice between approaches, a missing fact — instead of guessing. Each question offers a few options and always lets them type their own answer. Do not use it to ask permission for a tool: the app asks for that itself.",
                params(
                    json!({
                        "questions": {
                            "type": "array",
                            "description": "One to four questions, asked together.",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "header": {"type": "string", "description": "Short label, at most 12 characters, e.g. 'Framework'."},
                                    "question": {"type": "string", "description": "The question itself, one sentence."},
                                    "options": {
                                        "type": "array",
                                        "description": "One to four choices. The user can always type their own as well.",
                                        "items": {
                                            "type": "object",
                                            "properties": {
                                                "label": {"type": "string", "description": "The choice, short."},
                                                "description": {"type": "string", "description": "One line explaining it."}
                                            },
                                            "required": ["label"]
                                        }
                                    },
                                    "multiSelect": {"type": "boolean", "description": "Allow more than one choice. Default false."}
                                },
                                "required": ["question", "options"]
                            }
                        }
                    }),
                    &["questions"],
                ),
            ),
            ToolClass::Internal,
        ),
        (
            Tool::function(
                "notify_user",
                "Ping the user when something is done. Use it at the end of a long job, or when a step finishes that they are waiting on and might not be watching the app — the app raises an OS notification. One short sentence; do not use it to ask a question (that is `ask_user`) or to narrate every step.",
                params(
                    json!({
                        "message": {"type": "string", "description": "What is done, in one sentence."},
                        "title": {"type": "string", "description": "Short heading. Defaults to the app's name."}
                    }),
                    &["message"],
                ),
            ),
            ToolClass::Internal,
        ),
        (
            Tool::function(
                "update_tasks",
                "Publish or replace the plan for the current job — one line per step, with its status. Call it before starting multi-step work and again as steps start and finish. The user watches this list.",
                params(
                    json!({
                        "tasks": {
                            "type": "array",
                            "description": "The whole plan, in order. Send all of it every time.",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "text": {"type": "string", "description": "One step, short and in the imperative"},
                                    "status": {"type": "string", "enum": ["pending", "in_progress", "completed"]}
                                },
                                "required": ["text", "status"],
                                "additionalProperties": false
                            }
                        }
                    }),
                    &["tasks"],
                ),
            ),
            ToolClass::Internal,
        ),
        (
            Tool::function(
                "artifact_create",
                "Put a document in the side panel that the user can read, edit and come back to. Use it whenever the answer is something to keep rather than a chat reply — a report, a page, a chart, a diagram, a note, a snippet — and never paste a long block into the chat when an artifact would do. Calling it again publishes a new version of the same document.",
                params(
                    json!({
                        "title": {"type": "string", "description": "Short name shown on the panel, e.g. \"Q3 revenue chart\""},
                        "kind": {"type": "string", "enum": ARTIFACT_KINDS, "description": "html: a complete self-contained page (styles inline, no network). svg: the <svg> markup. mermaid: Mermaid diagram source. markdown: a Markdown note. code: source code, shown read-only."},
                        "content": {"type": "string", "description": "The whole document. For html, include its styles in a <style> tag; there is no network access, so no CDN links."}
                    }),
                    &["title", "kind", "content"],
                ),
            ),
            ToolClass::Internal,
        ),
        (
            Tool::function(
                "doc_create",
                "Write a document to a file the user can open. Use it when the user asks for a file rather than a message: a report, a letter, a summary to print or send. The content is Markdown — headings, bullets, numbered lists, tables and fenced code all carry through.",
                params(
                    json!({
                        "name": {"type": "string", "description": "File name without the extension, e.g. \"Q3 report\""},
                        "format": {"type": "string", "enum": ["md", "docx", "pdf"], "description": "md: Markdown, written as given. docx: a Word document. pdf: printed by the browser already on this machine."},
                        "content": {"type": "string", "description": "The whole document, in Markdown."}
                    }),
                    &["name", "format", "content"],
                ),
            ),
            ToolClass::Write,
        ),
        (
            Tool::function(
                "sheet_create",
                "Write a spreadsheet. Use it for tabular data the user will want to sort, sum or chart. Put headers in the first row. A cell whose text starts with = is stored as a formula and evaluated by Excel when the file is opened.",
                params(
                    json!({
                        "name": {"type": "string", "description": "File name without the extension"},
                        "sheet": {"type": "string", "description": "Name of the first sheet. Default \"Sheet1\"."},
                        "rows": {
                            "type": "array",
                            "description": "The rows, first row first. Each row is an array of cells.",
                            "items": {"type": "array", "items": {"type": ["string", "number", "boolean", "null"]}}
                        }
                    }),
                    &["name", "rows"],
                ),
            ),
            ToolClass::Write,
        ),
        (
            Tool::function(
                "slides_create",
                "Write a PowerPoint deck. Keep it to titles and bullets — layouts, images and animation are not supported, so a slide is a heading and a few points.",
                params(
                    json!({
                        "name": {"type": "string", "description": "File name without the extension"},
                        "title": {"type": "string", "description": "Deck title shown in the file properties. Defaults to the name."},
                        "slides": {
                            "type": "array",
                            "description": "The slides, in order. A bare string is a section title slide.",
                            "items": {
                                "type": ["object", "string"],
                                "properties": {
                                    "title": {"type": "string", "description": "The slide's heading"},
                                    "bullets": {"type": "array", "items": {"type": "string"}, "description": "One short line per bullet"}
                                },
                                "required": ["title"]
                            }
                        }
                    }),
                    &["name", "slides"],
                ),
            ),
            ToolClass::Write,
        ),
        (
            Tool::function(
                "fetch_url",
                "Fetch a web page and read it as text. Use for documentation links and articles. Does not run JavaScript.",
                params(json!({"url": {"type": "string", "description": "Full URL, or a bare domain"}}), &["url"]),
            ),
            ToolClass::Read,
        ),        (
            Tool::function(
                "browser_read",
                "Fetch a web page with a real browser so JavaScript runs first. Slower than fetch_url — use it when fetch_url comes back empty or the page is a single-page app.",
                params(json!({"url": {"type": "string", "description": "Full URL, or a bare domain"}}), &["url"]),
            ),
            ToolClass::Read,
        ),
        (
            Tool::function(
                "web_search",
                "Search the web and get back a list of titles, URLs and snippets. Follow up with fetch_url or browser_read to read a result.",
                params(json!({"query": {"type": "string", "description": "What to search for"}}), &["query"]),
            ),
            ToolClass::Read,
        ),
        (
            Tool::function(
                "browser_open",
                "Open a URL in a real browser and read the page. Use this instead of fetch_url when the page is built by JavaScript, or when you will need to click or type on it afterwards. The browser keeps its state between calls, so opening a page once is enough.",
                params(json!({"url": {"type": "string", "description": "Full URL, or a bare domain"}}), &["url"]),
            ),
            ToolClass::Read,
        ),
        (
            Tool::function(
                "browser_extract",
                "Read the page currently open in the browser: its title, address and visible text.",
                params(json!({}), &[]),
            ),
            ToolClass::Read,
        ),
        (
            Tool::function(
                "browser_eval",
                "Run a JavaScript expression in the page currently open in the browser and return its value. Use it to read structured data or inspect the DOM. Promises are awaited.",
                params(json!({"expression": {"type": "string", "description": "JavaScript expression, e.g. document.querySelectorAll('h2').length"}}), &["expression"]),
            ),
            ToolClass::Computer,
        ),
        (
            Tool::function(
                "browser_click",
                "Click the element matching a CSS selector in the browser, then read the page. If the selector matches nothing, the result lists the clickable elements that are actually on the page.",
                params(json!({"selector": {"type": "string", "description": "CSS selector, e.g. button#submit or a[href='/pricing']"}}), &["selector"]),
            ),
            ToolClass::Computer,
        ),
        (
            Tool::function(
                "browser_type",
                "Type into the input or textarea matching a CSS selector in the browser. Set submit to true to submit the surrounding form afterwards.",
                params(
                    json!({
                        "selector": {"type": "string", "description": "CSS selector of the field"},
                        "text": {"type": "string", "description": "Text to put in the field"},
                        "submit": {"type": "boolean", "description": "Submit the surrounding form afterwards. Default false."}
                    }),
                    &["selector", "text"],
                ),
            ),
            ToolClass::Computer,
        ),
        (
            Tool::function(
                "browser_screenshot",
                "Take a picture of the page in the browser and show it to the user. You cannot see the picture — read the page with browser_extract or browser_eval instead.",
                params(json!({}), &[]),
            ),
            ToolClass::Computer,
        ),
        (
            Tool::function(
                "ui_snapshot",
                "Read the desktop as text: the controls of the window that has focus, or of another open window you name. Use it to answer what is on the user's screen, to read a dialog, and to see what to press next.",
                params(json!({"window": {"type": "string", "description": "Part of an open window's title (e.g. \"notepad\"). Omit for the window that has focus."}}), &[]),
            ),
            ToolClass::Computer,
        ),
        (
            Tool::function(
                "ui_click",
                "Press a control on the user's desktop — a button, a menu item, a list item — by its name or its automation id. Look with ui_snapshot first. The user approves each press unless the chat is in skip mode.",
                params(
                    json!({
                        "name": {"type": "string", "description": "Text on the control. Case-insensitive; part of the name is enough."},
                        "id": {"type": "string", "description": "The control's automation id, as ui_snapshot reports it"},
                        "type": {"type": "string", "description": "Narrow the search to a control type, e.g. button or menuitem"},
                        "window": {"type": "string", "description": "Part of the window's title. Omit for the window that has focus."}
                    }),
                    &[],
                ),
            ),
            ToolClass::Computer,
        ),
        (
            Tool::function(
                "ui_type",
                "Type into a field on the user's desktop — an edit box, a search box, a document. Identifies the field the same way ui_click does.",
                params(
                    json!({
                        "text": {"type": "string", "description": "What to type"},
                        "name": {"type": "string", "description": "Text labelling the field"},
                        "id": {"type": "string", "description": "The field's automation id, as ui_snapshot reports it"},
                        "type": {"type": "string", "description": "Narrow the search to a control type, e.g. edit"},
                        "window": {"type": "string", "description": "Part of the window's title. Omit for the window that has focus."},
                        "clear": {"type": "boolean", "description": "Select what is already in the field and replace it. Default false."},
                        "submit": {"type": "boolean", "description": "Press Enter afterwards. Default false."}
                    }),
                    &["text"],
                ),
            ),
            ToolClass::Computer,
        ),
        (
            Tool::function(
                "run_command",
                "Run a shell command in the workspace root. Use for builds, tests and scripts.",
                params(json!({"command": {"type": "string", "description": "The command line to run"}}), &["command"]),
            ),
            ToolClass::Command,
        ),        (
            Tool::function("git_status", "Show the working tree status.", params(json!({}), &[])),
            ToolClass::Read,
        ),
        (
            Tool::function(
                "git_diff",
                "Show changes against HEAD.",
                params(json!({}), &[]),
            ),
            ToolClass::Read,
        ),
        (
            Tool::function(
                "git_log",
                "Show recent commits.",
                params(json!({"limit": {"type": "integer", "description": "How many commits, 1-100. Default 20."}}), &[]),
            ),
            ToolClass::Read,
        ),
        (
            Tool::function(
                "git_commit",
                "Stage every change and commit it.",
                params(json!({"message": {"type": "string", "description": "Commit message"}}), &["message"]),
            ),
            ToolClass::Command,
        ),
        (
            Tool::function("git_push", "Push the current branch to its remote.", params(json!({}), &[])),
            ToolClass::Command,
        ),
    ]
}

/// The toolset for a section. Chat mode gets the document tools only — no
/// shell and no git. Coding mode gets everything.
pub fn tools_for(section: &str) -> Vec<(Tool, ToolClass)> {
    builtin_tools()
        .into_iter()
        .filter(|(tool, _)| {
            section == "coding"
                || !matches!(
                    tool.function.name.as_str(),
                    "run_command" | "git_status" | "git_diff" | "git_log" | "git_commit" | "git_push"
                )
        })
        .collect()
}

/// The toolset for a section, cut down to what the chat's mode allows.
///
/// Kept beside `tools_for` rather than folded into it: the section question (may
/// this chat run commands at all?) and the mode question (how much may it do
/// unattended?) are different, and the tests can then ask them separately.
pub fn tools_for_mode(section: &str, mode: AgentMode) -> Vec<(Tool, ToolClass)> {
    tools_for(section)
        .into_iter()
        .filter(|(_, class)| mode.allows(*class))
        .collect()
}

pub fn class_of(tool: &str) -> ToolClass {
    builtin_tools()
        .into_iter()
        .find(|(t, _)| t.function.name == tool)
        .map(|(_, class)| class)
        .unwrap_or(ToolClass::Command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mode_decides_which_tools_are_offered_at_all() {
        // The ladder, and nothing above its rung.
        assert!(tools_for_mode("coding", AgentMode::Ask).is_empty());
        assert!(tools_for_mode("chat", AgentMode::Ask).is_empty());

        // Plan reads: files, and the web both as a plain fetch and through the
        // browser, so a page JavaScript built is still available to look at.
        let plan = tools_for_mode("coding", AgentMode::Plan);
        let names: Vec<&str> = plan.iter().map(|(t, _)| t.function.name.as_str()).collect();
        for name in ["read_file", "grep", "fetch_url", "web_search", "browser_open", "browser_extract"] {
            assert!(names.contains(&name), "plan mode should offer {name}");
        }
        // Anything that can change something, or act on someone else's site,
        // stays out — browser_eval included: arbitrary script in a live page is
        // one `form.submit()` away from an action.
        for name in [
            "write_file",
            "edit_file",
            "run_command",
            "browser_click",
            "browser_type",
            "browser_eval",
        ] {
            assert!(!names.contains(&name), "plan mode must not offer {name}");
        }
        for (tool, class) in &plan {
            assert!(
                matches!(class, ToolClass::Read | ToolClass::Internal),
                "plan mode offered {} ({:?}), which can change something",
                tool.function.name,
                class
            );
        }
        assert!(plan.len() < builtin_tools().len());

        // Standard and agent differ in whether they ask, not in what they may
        // reach, so both offer the section's whole list.
        assert_eq!(
            tools_for_mode("coding", AgentMode::Standard).len(),
            builtin_tools().len()
        );
        assert_eq!(
            tools_for_mode("coding", AgentMode::Agent).len(),
            builtin_tools().len()
        );
        // A permissive mode does not erase the section's own limit.
        assert!(tools_for_mode("chat", AgentMode::Agent).len() < builtin_tools().len());
    }

    #[test]
    fn reads_do_not_need_approval_but_writes_and_commands_do() {
        assert_eq!(class_of("read_file"), ToolClass::Read);
        assert_eq!(class_of("grep"), ToolClass::Read);
        // Reading the history is a read like any other: it goes through the
        // scope filter, so it cannot reach outside this chat's own place.
        assert_eq!(class_of("search_history"), ToolClass::Read);
        assert_eq!(class_of("git_status"), ToolClass::Read);
        assert_eq!(class_of("write_file"), ToolClass::Write);
        assert_eq!(class_of("edit_file"), ToolClass::Write);
        assert_eq!(class_of("run_command"), ToolClass::Command);
        assert_eq!(class_of("git_push"), ToolClass::Command);
        assert_eq!(class_of("save_memory"), ToolClass::Internal);
        assert_eq!(class_of("fetch_url"), ToolClass::Read);
        assert_eq!(class_of("browser_read"), ToolClass::Read);
        assert_eq!(class_of("web_search"), ToolClass::Read);
    }

    #[test]
    fn an_unknown_tool_is_treated_as_the_riskiest_class() {
        assert_eq!(class_of("something_new"), ToolClass::Command);
    }

    #[test]
    fn reading_a_page_through_the_browser_is_a_read() {
        // A lookup, not an action: same class as fetch_url, so it follows the
        // browsing switch, exists in both sections, and is never prompted for.
        // `browser_read` was already classed this way in isolation; opening a page
        // and reading it is the same operation spread over two calls.
        for name in ["browser_read", "browser_open", "browser_extract"] {
            assert_eq!(class_of(name), ToolClass::Read, "{name}");
            assert!(is_browsing_tool(name), "{name} must follow the browsing switch");
            assert!(
                tools_for("chat").iter().any(|(t, _)| t.function.name == name),
                "{name} missing from chat mode"
            );
        }
    }

    #[test]
    fn browser_tools_that_act_are_computer_class_and_are_always_asked_about() {
        for name in [
            "browser_eval",
            "browser_click",
            "browser_type",
            "browser_screenshot",
        ] {
            assert_eq!(class_of(name), ToolClass::Computer, "{name}");
            assert!(is_browsing_tool(name), "{name} must follow the browsing switch");
            // Both sections get them: driving a web page is not a shell tool.
            assert!(
                tools_for("chat").iter().any(|(t, _)| t.function.name == name),
                "{name} missing from chat mode"
            );
        }
        // The plain fetchers follow the same switch, and nothing else does.
        assert!(is_browsing_tool("fetch_url"));
        assert!(is_browsing_tool("web_search"));
        assert!(!is_browsing_tool("read_file"));
        assert!(!is_browsing_tool("write_file"));
    }

    #[test]
    fn desktop_tools_are_computer_class_and_must_be_switched_on() {
        for name in ["ui_snapshot", "ui_click", "ui_type"] {
            assert_eq!(class_of(name), ToolClass::Computer, "{name}");
            assert!(is_computer_tool(name), "{name} must follow the computer-use switch");
        }
        // Only acting spends the run's action budget.
        assert!(crate::tools::desktop::is_action("ui_click"));
        assert!(crate::tools::desktop::is_action("ui_type"));
        assert!(!crate::tools::desktop::is_action("ui_snapshot"));
        // The gate must not catch anything that already existed.
        for name in ["read_file", "browser_open", "run_command", "fetch_url", "update_tasks"] {
            assert!(!is_computer_tool(name), "{name} must not follow the computer-use switch");
        }
        // Available in both sections: reading a window is not a coding tool.
        assert!(tools_for("chat").iter().any(|(t, _)| t.function.name == "ui_snapshot"));
    }

    #[test]
    fn the_things_that_write_a_skill_or_a_design_ask_first() {
        for name in ["create_skill", "create_design_system", "draft_design_system"] {
            assert_eq!(class_of(name), ToolClass::Write, "{name}");
        }
        // Both sections: a house style is not a coding-only idea, and a chat that
        // writes a document has as much use for one as a chat that writes a page.
        for section in ["chat", "coding"] {
            let names: Vec<String> = tools_for(section)
                .into_iter()
                .map(|(t, _)| t.function.name)
                .collect();
            for name in ["create_design_system", "draft_design_system"] {
                assert!(names.contains(&name.to_string()), "{name} missing from {section}");
            }
        }
    }

    #[test]
    fn a_design_draft_needs_a_name_and_a_body_and_names_its_own_tool() {
        let ok = parse_design_draft(&json!({"name": "House", "body": "## Rules"})).unwrap();
        assert_eq!(ok.name, "House");
        assert!(!ok.replace, "revising is opt-in");
        // The error has to name the call that was made, not `create_skill`.
        let error = parse_design_draft(&json!({"name": "House"})).unwrap_err();
        assert!(error.contains("create_design_system"), "{error}");
    }

    #[test]
    fn update_tasks_is_internal_so_it_never_prompts() {
        assert_eq!(class_of("update_tasks"), ToolClass::Internal);
        // But a plan is useful in both sections, unlike the git tools.
        assert!(tools_for("chat").iter().any(|(t, _)| t.function.name == "update_tasks"));
        assert!(tools_for("coding").iter().any(|(t, _)| t.function.name == "update_tasks"));
    }

    #[test]
    fn the_document_tools_write_files_so_they_ask_first() {
        // Every one of these puts a file on disk outside the workspace, so the
        // user sees it before it happens.
        for name in ["doc_create", "sheet_create", "slides_create"] {
            assert_eq!(class_of(name), ToolClass::Write, "{name}");
            assert!(docs::is_document_tool(name), "{name}");
            assert!(
                tools_for("chat").iter().any(|(t, _)| t.function.name == name),
                "{name} missing from chat mode"
            );
        }
        // The gate must not claim anything else.
        for name in ["read_file", "artifact_create", "run_command", "update_tasks"] {
            assert!(!docs::is_document_tool(name), "{name}");
        }
    }

    #[test]
    fn artifact_create_is_internal_and_available_in_both_sections() {        // Stored in the database and rendered sandboxed, so it is not a
        // workspace write and the model must not be stopped by a prompt every
        // time it publishes an update.
        assert_eq!(class_of("artifact_create"), ToolClass::Internal);
        assert!(tools_for("chat").iter().any(|(t, _)| t.function.name == "artifact_create"));
        assert!(tools_for("coding").iter().any(|(t, _)| t.function.name == "artifact_create"));
    }

    #[test]
    fn an_artifact_must_have_a_known_kind_and_content() {
        let good = parse_artifact(&json!({
            "title": "Report", "kind": "html", "content": "<p>hi</p>"
        }))
        .unwrap();
        assert_eq!(good.kind, "html");
        assert_eq!(good.title, "Report");

        // The title is trimmed and capped; the content is kept verbatim.
        let long_title = "x".repeat(500);
        let capped = parse_artifact(&json!({
            "title": format!("  {long_title}  "), "kind": "code", "content": "let a = 1;"
        }))
        .unwrap();
        assert_eq!(capped.title.chars().count(), 120);
        assert_eq!(capped.content, "let a = 1;");

        // Every advertised kind is accepted, and nothing else is.
        for kind in ARTIFACT_KINDS {
            assert!(
                parse_artifact(&json!({"title": "t", "kind": kind, "content": "x"})).is_ok(),
                "{kind} should be a valid kind"
            );
        }
        // Each of these is a shape a model actually produces.
        for bad in [
            json!({"kind": "html", "content": "<p></p>"}),
            json!({"title": "t", "kind": "html"}),
            json!({"title": "t", "kind": "html", "content": "   "}),
            json!({"title": "t", "kind": "python", "content": "x"}),
            json!({"title": " ", "kind": "html", "content": "x"}),
            json!({"title": "t", "kind": "HTML", "content": "x"}),
        ] {
            assert!(parse_artifact(&bad).is_err(), "{bad} should be rejected");
        }
    }

    #[test]
    fn a_skill_draft_needs_a_name_and_a_body() {
        let ok = parse_skill_draft(&json!({"name": "Terse", "body": "Be brief."})).unwrap();
        assert_eq!(ok.name, "Terse");
        assert!(!ok.replace, "a rewrite has to be asked for, never assumed");
        assert!(
            parse_skill_draft(&json!({"body": "x"})).is_err(),
            "a skill with no name has nowhere to live"
        );
        assert!(
            parse_skill_draft(&json!({"name": "x", "body": "   "})).is_err(),
            "an empty body would join the prompt and change nothing"
        );
        assert!(parse_skill_draft(&json!({"name": "x", "body": "y", "replace": true}))
            .unwrap()
            .replace);
    }

    #[test]
    fn save_memory_only_sends_an_explicit_profile_to_the_injected_file() {
        // The default, and anything unrecognised, must stay out of the
        // always-injected file. A made-up category is not a new file either.
        assert_eq!(
            parse_memory(&json!({"entries": ["Likes brevity"]}))
                .unwrap()
                .target,
            "other"
        );
        assert_eq!(
            parse_memory(&json!({"entries": ["x"], "category": "nonsense"}))
                .unwrap()
                .target,
            "other"
        );
        // A real category is honoured, in any case.
        assert_eq!(
            parse_memory(&json!({"entries": ["x"], "category": "Education"}))
                .unwrap()
                .target,
            "education"
        );
        // Only `profile`, in any case, reaches the always-injected file.
        assert_eq!(
            parse_memory(&json!({"entries": ["x"], "category": "PROFILE"}))
                .unwrap()
                .target,
            "profile"
        );
        // Blank entries are dropped, and an empty call is refused rather than
        // silently writing nothing.
        assert_eq!(
            parse_memory(&json!({"entries": ["  ", "Real fact"]}))
                .unwrap()
                .entries,
            vec!["Real fact".to_string()]
        );
        assert!(parse_memory(&json!({"entries": []})).is_err());
        assert!(parse_memory(&json!({})).is_err());
    }

    #[test]
    fn a_well_formed_plan_survives_the_round_trip() {
        let tasks = parse_tasks(&json!({
            "tasks": [
                {"text": "Read the tests", "status": "completed"},
                {"text": "Fix the parser", "status": "in_progress"},
                {"text": "  Ship it  "},
            ]
        }))
        .unwrap();
        assert_eq!(tasks.len(), 3);
        assert_eq!(tasks[0].status, TaskStatus::Completed);
        assert_eq!(tasks[1].status, TaskStatus::InProgress);
        // A missing status defaults to pending, and the text is trimmed.
        assert_eq!(tasks[2].status, TaskStatus::Pending);
        assert_eq!(tasks[2].text, "Ship it");
    }

    #[test]
    fn a_malformed_plan_is_rejected_with_a_reason_the_model_can_act_on() {
        // Every one of these is a shape a model actually produces.
        let bad = [
            json!({}),
            json!({"tasks": []}),
            json!({"tasks": "fix the bug"}),
            json!({"tasks": [{}]}),
            json!({"tasks": [{"text": "   "}]}),
            json!({"tasks": [{"text": "Fix it", "status": "done"}]}),
            json!({"tasks": (0..MAX_TASKS + 1).map(|i| json!({"text": format!("step {i}")})).collect::<Vec<_>>()}),
        ];
        for args in bad {
            let err = parse_tasks(&args).expect_err("should have been rejected");
            assert!(!err.is_empty(), "a rejection must explain itself");
        }
        // The status message names the values it will accept.
        let err = parse_tasks(&json!({"tasks": [{"text": "x", "status": "done"}]})).unwrap_err();
        assert!(err.contains("in_progress"), "got: {err}");
    }

    #[test]
    fn every_spec_names_its_required_arguments() {
        for (tool, _) in builtin_tools() {
            let schema = &tool.function.parameters;
            assert_eq!(schema["type"], "object", "{}", tool.function.name);
            assert!(schema.get("required").is_some(), "{}", tool.function.name);
        }
    }

    #[test]
    fn chat_mode_has_no_shell_and_no_git() {
        let names: Vec<String> = tools_for("chat")
            .into_iter()
            .map(|(t, _)| t.function.name)
            .collect();
        assert!(names.contains(&"write_file".to_string()));
        assert!(names.contains(&"read_file".to_string()));
        assert!(names.contains(&"save_memory".to_string()));
        assert!(!names.contains(&"run_command".to_string()));
        assert!(!names.iter().any(|n| n.starts_with("git_")));
    }

    #[test]
    fn coding_mode_has_everything() {
        assert_eq!(tools_for("coding").len(), builtin_tools().len());
    }

    #[test]
    fn ask_user_is_internal_so_the_app_never_asks_permission_to_ask() {
        assert_eq!(class_of("ask_user"), ToolClass::Internal);
        // Offered wherever there are tools at all, in both sections.
        assert!(tools_for("chat").iter().any(|(t, _)| t.function.name == "ask_user"));
        assert!(tools_for("coding").iter().any(|(t, _)| t.function.name == "ask_user"));
    }

    #[test]
    fn a_question_needs_a_prompt_and_at_least_one_option() {
        let good = parse_questions(&json!({
            "questions": [{
                "header": "Framework",
                "question": "Which one?",
                "options": [{"label": "React", "description": "Components"}, {"label": "Vue"}]
            }]
        }))
        .unwrap();
        assert_eq!(good.len(), 1);
        assert_eq!(good[0].options.len(), 2);
        assert_eq!(good[0].options[0].description, "Components");
        assert!(!good[0].multi_select, "multi-select is opt-in");

        // The header is capped so it cannot stretch the composer.
        let long = parse_questions(&json!({
            "questions": [{
                "header": "x".repeat(40),
                "question": "q",
                "options": [{"label": "a"}]
            }]
        }))
        .unwrap();
        assert_eq!(long[0].header.chars().count(), 12);

        // Each of these is a shape a model actually produces.
        for bad in [
            json!({}),
            json!({"questions": []}),
            json!({"questions": [{}]}),
            json!({"questions": [{"question": "q"}]}),
            json!({"questions": [{"question": "q", "options": []}]}),
            json!({"questions": [{"question": "q", "options": [{}]}]}),
            json!({"questions": (0..MAX_QUESTIONS + 1).map(|i| json!({"question": format!("q{i}"), "options": [{"label": "a"}]})).collect::<Vec<_>>()}),
        ] {
            let err = parse_questions(&bad).expect_err("should have been rejected");
            assert!(!err.is_empty(), "a rejection must explain itself");
        }
    }
}
