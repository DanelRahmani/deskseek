//! The built-in coding toolset: what the model may call, and how risky each
//! call is (which decides whether the user is asked to approve it).

pub mod browse;
pub mod exec;
pub mod guard;
pub mod sandbox;
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
    /// MEMORY.md is the only one.
    Internal,
    /// Modifies the workspace; the user approves each call.
    Write,
    /// Executes something; the user approves each call.
    Command,
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

/// (spec, class) for every built-in coding tool.
pub fn builtin_tools() -> Vec<(Tool, ToolClass)> {
    vec![
        (
            Tool::function(
                "read_file",
                "Read a UTF-8 text file from the workspace.",
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
                "write_file",
                "Create a file, or replace it entirely if it exists.",
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
                "Append durable facts about the user to their MEMORY.md. This is the only way to remember something across chats; never create or rename files for memory.",
                params(
                    json!({
                        "entries": {
                            "type": "array",
                            "description": "One self-contained sentence per fact. No bullets, no dates.",
                            "items": { "type": "string" }
                        }
                    }),
                    &["entries"],
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
                "fetch_url",
                "Fetch a web page and read it as text. Use for documentation links and articles. Does not run JavaScript.",
                params(json!({"url": {"type": "string", "description": "Full URL, or a bare domain"}}), &["url"]),
            ),
            ToolClass::Read,
        ),
        (
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
    fn reads_do_not_need_approval_but_writes_and_commands_do() {
        assert_eq!(class_of("read_file"), ToolClass::Read);
        assert_eq!(class_of("grep"), ToolClass::Read);
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
    fn update_tasks_is_internal_so_it_never_prompts() {
        assert_eq!(class_of("update_tasks"), ToolClass::Internal);
        // But a plan is useful in both sections, unlike the git tools.
        assert!(tools_for("chat").iter().any(|(t, _)| t.function.name == "update_tasks"));
        assert!(tools_for("coding").iter().any(|(t, _)| t.function.name == "update_tasks"));
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
}
