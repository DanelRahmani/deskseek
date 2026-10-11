//! The built-in coding toolset: what the model may call, and how risky each
//! call is (which decides whether the user is asked to approve it).

pub mod exec;
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
                "run_command",
                "Run a shell command in the workspace root. Use for builds, tests and scripts.",
                params(json!({"command": {"type": "string", "description": "The command line to run"}}), &["command"]),
            ),
            ToolClass::Command,
        ),
        (
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
    }

    #[test]
    fn an_unknown_tool_is_treated_as_the_riskiest_class() {
        assert_eq!(class_of("something_new"), ToolClass::Command);
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
        assert!(!names.contains(&"run_command".to_string()));
        assert!(!names.iter().any(|n| n.starts_with("git_")));
    }

    #[test]
    fn coding_mode_has_everything() {
        assert_eq!(tools_for("coding").len(), builtin_tools().len());
    }
}
