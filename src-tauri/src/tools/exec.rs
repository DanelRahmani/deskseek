//! Implementations of the built-in coding tools. Everything path-taking goes
//! through `sandbox::resolve` first.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use serde_json::Value;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;

use crate::runs::Cancel;
use crate::tools::{sandbox, walk};

const MAX_READ_BYTES: u64 = 256 * 1024;
const MAX_RESULT_CHARS: usize = 60_000;
const COMMAND_TIMEOUT_SECS: u64 = 120;
const MAX_GLOB_RESULTS: usize = 200;
const MAX_GREP_MATCHES: usize = 100;

/// Never handed to a child process, however the user configured the app.
const SENSITIVE_ENV: &[&str] = &[
    "DEEPSEEK_API_KEY",
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "GITHUB_TOKEN",
    "GH_TOKEN",
];

pub(crate) fn truncate(mut s: String) -> String {
    if s.chars().count() > MAX_RESULT_CHARS {
        let cut: String = s.chars().take(MAX_RESULT_CHARS).collect();
        s = format!("{cut}\n… (output truncated)");
    }
    s
}

fn require_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("missing required argument '{key}'"))
}

fn read_file(args: &Value, ws: &Path) -> Result<String, String> {
    let path = require_str(args, "path")?;
    let full = sandbox::resolve(ws, path)?;
    let meta = std::fs::metadata(&full).map_err(|e| format!("cannot read '{path}': {e}"))?;
    if meta.is_dir() {
        return Err(format!("'{path}' is a directory — use list_dir"));
    }
    if meta.len() > MAX_READ_BYTES {
        return Err(format!(
            "'{path}' is {} bytes, over the {MAX_READ_BYTES} byte read limit",
            meta.len()
        ));
    }
    let content = std::fs::read_to_string(&full).map_err(|e| format!("cannot read '{path}': {e}"))?;
    Ok(truncate(content))
}

fn list_dir(args: &Value, ws: &Path) -> Result<String, String> {
    let path = args.get("path").and_then(Value::as_str).unwrap_or(".");
    let full = sandbox::resolve(ws, path)?;
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(&full).map_err(|e| format!("cannot list '{path}': {e}"))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().to_string();
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        entries.push(if is_dir { format!("{name}/") } else { name });
    }
    entries.sort();
    if entries.is_empty() {
        return Ok(format!("'{path}' is empty"));
    }
    Ok(entries.join("\n"))
}

fn glob(args: &Value, ws: &Path) -> Result<String, String> {
    let pattern = require_str(args, "pattern")?;
    let mut hits = Vec::new();
    for file in walk::walk_files(ws) {
        let rel = sandbox::display(ws, &file);
        if walk::wildcard_match(pattern, &rel) {
            hits.push(rel);
            if hits.len() >= MAX_GLOB_RESULTS {
                break;
            }
        }
    }
    hits.sort();
    if hits.is_empty() {
        return Ok(format!("no files match '{pattern}'"));
    }
    Ok(hits.join("\n"))
}

fn grep(args: &Value, ws: &Path) -> Result<String, String> {
    let pattern = require_str(args, "pattern")?;
    let scope = args.get("path").and_then(Value::as_str).unwrap_or(".");
    let base = sandbox::resolve(ws, scope)?;

    let files = if base.is_dir() {
        walk::walk_files(&base)
    } else {
        vec![base]
    };

    let mut hits = Vec::new();
    'outer: for file in files {
        // Non-UTF8 (binary) files are skipped rather than reported as matches.
        let Ok(content) = std::fs::read_to_string(&file) else {
            continue;
        };
        for (index, line) in content.lines().enumerate() {
            if line.contains(pattern) {
                hits.push(format!(
                    "{}:{}: {}",
                    sandbox::display(ws, &file),
                    index + 1,
                    line.trim()
                ));
                if hits.len() >= MAX_GREP_MATCHES {
                    break 'outer;
                }
            }
        }
    }

    if hits.is_empty() {
        return Ok(format!("no matches for '{pattern}'"));
    }
    Ok(truncate(hits.join("\n")))
}

fn write_file(args: &Value, ws: &Path) -> Result<String, String> {
    let path = require_str(args, "path")?;
    let content = require_str(args, "content")?;
    let full = sandbox::resolve(ws, path)?;
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("cannot create directory: {e}"))?;
    }
    let existed = full.exists();
    std::fs::write(&full, content).map_err(|e| format!("cannot write '{path}': {e}"))?;
    Ok(format!(
        "{} {path} ({} bytes)",
        if existed { "overwrote" } else { "created" },
        content.len()
    ))
}

fn edit_file(args: &Value, ws: &Path) -> Result<String, String> {
    let path = require_str(args, "path")?;
    let old = require_str(args, "old_string")?;
    let new = args
        .get("new_string")
        .and_then(Value::as_str)
        .ok_or("missing required argument 'new_string'")?;

    let full = sandbox::resolve(ws, path)?;
    let content = std::fs::read_to_string(&full).map_err(|e| format!("cannot read '{path}': {e}"))?;

    match content.matches(old).count() {
        0 => Err(format!("old_string was not found in '{path}'")),
        1 => {
            let updated = content.replacen(old, new, 1);
            std::fs::write(&full, &updated).map_err(|e| format!("cannot write '{path}': {e}"))?;
            Ok(format!("edited {path}"))
        }
        n => Err(format!(
            "old_string appears {n} times in '{path}' — include more surrounding context so it is unique"
        )),
    }
}

async fn run_shell(command: &str, ws: &Path, cancel: &Cancel) -> Result<String, String> {
    let cmd = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(command);
        c
    } else {
        let mut c = Command::new("sh");
        c.arg("-c").arg(command);
        c
    };
    run_process(cmd, Some(ws), COMMAND_TIMEOUT_SECS, true, Some(cancel)).await
}

/// Runs a prepared command, draining both pipes and enforcing a timeout.
/// Shared by the shell tool and the headless browser.
///
/// `include_stderr` is false for the browser, which writes Chromium debug
/// chatter to stderr that has nothing to do with the page.
///
/// `cancel` kills the child the moment the user stops the run; a browser fetch
/// passes `None` because it is short-lived and never worth interrupting.
pub(crate) async fn run_process(
    mut cmd: Command,
    cwd: Option<&Path>,
    timeout_secs: u64,
    include_stderr: bool,
    cancel: Option<&Cancel>,
) -> Result<String, String> {
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for key in SENSITIVE_ENV {
        cmd.env_remove(key);
    }

    let mut child = cmd.spawn().map_err(|e| format!("could not start command: {e}"))?;
    let mut stdout = child.stdout.take().ok_or("no stdout")?;
    let mut stderr = child.stderr.take().ok_or("no stderr")?;

    // Drain both pipes concurrently — reading them in sequence can deadlock
    // when a command fills one buffer.
    let out_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf).await;
        buf
    });
    let err_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        let _ = stderr.read_to_end(&mut buf).await;
        buf
    });

    let stop = async {
        match cancel {
            Some(cancel) => cancel.cancelled().await,
            None => std::future::pending::<()>().await,
        }
    };
    tokio::pin!(stop);

    let mut cancelled = false;
    let status = tokio::select! {
        _ = &mut stop => {
            cancelled = true;
            None
        }
        waited = timeout(Duration::from_secs(timeout_secs), child.wait()) => match waited {
            Ok(Ok(status)) => Some(status),
            Ok(Err(e)) => return Err(format!("command failed: {e}")),
            Err(_) => None,
        },
    };
    if status.is_none() {
        let _ = child.kill().await;
    }

    let stdout = out_task.await.unwrap_or_default();
    let stderr = err_task.await.unwrap_or_default();
    let mut text = String::from_utf8_lossy(&stdout).to_string();
    if include_stderr {
        text.push_str(&String::from_utf8_lossy(&stderr));
    }

    match status {
        Some(status) if !status.success() => {
            text.push_str(&format!("\n(exit code {:?})", status.code()));
        }
        None if cancelled => {
            text.push_str("\n(stopped by the user)");
        }
        None => {
            text.push_str(&format!("\n(timed out after {timeout_secs}s)"));
        }
        _ => {}
    }

    if text.trim().is_empty() {
        return Ok("(no output)".to_string());
    }
    Ok(truncate(text.trim_end().to_string()))
}

/// Runs `git` with literal arguments — no shell, so nothing can be injected
/// through a commit message or path.
async fn git(ws: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(ws)
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(|e| format!("could not run git: {e}"))?;

    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    if text.trim().is_empty() {
        text = format!("(git {} produced no output)", args.join(" "));
    }
    if !output.status.success() {
        return Err(truncate(text.trim().to_string()));
    }
    Ok(truncate(text.trim_end().to_string()))
}

pub async fn execute(
    tool: &str,
    args: &Value,
    ws: &Path,
    cancel: &Cancel,
) -> Result<String, String> {
    match tool {
        "read_file" => read_file(args, ws),
        "list_dir" => list_dir(args, ws),
        "glob" => glob(args, ws),
        "grep" => grep(args, ws),
        "write_file" => write_file(args, ws),
        "edit_file" => edit_file(args, ws),
        "run_command" => run_shell(require_str(args, "command")?, ws, cancel).await,
        "git_status" => git(ws, &["status", "--short", "--branch"]).await,
        "git_diff" => git(ws, &["--no-pager", "diff", "HEAD"]).await,
        "git_log" => {
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(20);
            let limit = limit.clamp(1, 100).to_string();
            git(ws, &["--no-pager", "log", "--oneline", "-n", &limit]).await
        }
        "git_commit" => {
            let message = require_str(args, "message")?;
            git(ws, &["add", "-A"]).await?;
            git(ws, &["commit", "-m", message]).await
        }
        "git_push" => git(ws, &["--no-pager", "push"]).await,
        other => Err(format!("unknown tool '{other}'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn workspace() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("deskseek-exec-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src").join("main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(dir.join("notes.txt"), "alpha\nbeta\nbeta\n").unwrap();
        dir
    }

    #[test]
    fn read_file_returns_contents_and_refuses_directories() {
        let ws = workspace();
        let out = read_file(&json!({"path": "notes.txt"}), &ws).unwrap();
        assert!(out.contains("alpha"));
        assert!(read_file(&json!({"path": "src"}), &ws).is_err());
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn edit_file_requires_a_unique_anchor() {
        let ws = workspace();
        // "beta" appears twice -> refuse rather than guess.
        let err = edit_file(
            &json!({"path": "notes.txt", "old_string": "beta", "new_string": "gamma"}),
            &ws,
        )
        .unwrap_err();
        assert!(err.contains("appears 2 times"), "{err}");

        // A unique anchor works.
        edit_file(
            &json!({"path": "notes.txt", "old_string": "alpha", "new_string": "ALPHA"}),
            &ws,
        )
        .unwrap();
        let now = std::fs::read_to_string(ws.join("notes.txt")).unwrap();
        assert!(now.starts_with("ALPHA"));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn edit_file_reports_a_missing_anchor() {
        let ws = workspace();
        let err = edit_file(
            &json!({"path": "notes.txt", "old_string": "nope", "new_string": "x"}),
            &ws,
        )
        .unwrap_err();
        assert!(err.contains("was not found"), "{err}");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn write_file_creates_missing_directories() {
        let ws = workspace();
        write_file(
            &json!({"path": "deep/nested/thing.txt", "content": "hi"}),
            &ws,
        )
        .unwrap();
        assert!(ws.join("deep").join("nested").join("thing.txt").exists());
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn write_file_still_obeys_the_sandbox() {
        let ws = workspace();
        assert!(write_file(&json!({"path": "../escape.txt", "content": "no"}), &ws).is_err());
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn glob_matches_workspace_relative_paths() {
        let ws = workspace();
        let out = glob(&json!({"pattern": "**/*.rs"}), &ws).unwrap();
        assert!(out.contains("src/main.rs"), "{out}");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn grep_reports_file_and_line() {
        let ws = workspace();
        let out = grep(&json!({"pattern": "alpha"}), &ws).unwrap();
        assert!(out.contains("notes.txt:1"), "{out}");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn missing_arguments_are_reported_clearly() {
        let ws = workspace();
        let err = read_file(&json!({}), &ws).unwrap_err();
        assert!(err.contains("missing required argument 'path'"), "{err}");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[tokio::test]
    async fn run_command_captures_output_and_survives_sandbox() {
        let ws = workspace();
        let cmd = if cfg!(windows) { "echo hello" } else { "echo hello" };
        let out = run_shell(cmd, &ws, &Cancel::new()).await.unwrap();
        assert!(out.contains("hello"), "{out}");
        let _ = std::fs::remove_dir_all(&ws);
    }
}
