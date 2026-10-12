//! Implementations of the built-in coding tools. Everything path-taking goes
//! through `sandbox::resolve` first.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use serde_json::Value;
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::time::timeout;

use crate::runs::Cancel;
use crate::tools::sandbox_exec::Plan;
use crate::tools::{job, sandbox, sandbox_exec, walk};

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
    // A Word, Excel, PowerPoint or PDF file is a container of its own: its bytes
    // are not text, so `read_to_string` refuses them and the model is left
    // believing it cannot read the user's document at all. It is opened as a
    // document instead and handed back as the words in it.
    if crate::tools::extract::is_extractable(path) {
        let limit = crate::tools::extract::MAX_DOC_BYTES;
        if meta.len() > limit {
            return Err(format!(
                "'{path}' is {} MB, over the {} MB document limit",
                meta.len() / (1024 * 1024),
                limit / (1024 * 1024)
            ));
        }
        let bytes = std::fs::read(&full).map_err(|e| format!("cannot read '{path}': {e}"))?;
        let text = crate::tools::extract::text_of(path, &bytes)?;
        return Ok(truncate(format!("[text extracted from {path}]\n\n{text}")));
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
    if crate::tools::extract::is_extractable(path) {
        return Err(document_guard(path));
    }
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

/// A Word, Excel, PowerPoint or PDF file cannot be produced by writing text: the
/// bytes would be a file that calls itself a `.docx` and is not one, and the
/// model has no way to notice that from the tool result. The refusal names the
/// tool that does it properly instead, so the correction is one turn rather than
/// a dead end.
fn document_guard(path: &str) -> String {
    format!(
        "'{path}' is a document, not a text file — writing text into it would produce a file \
         that does not open. Use doc_create for md/docx/pdf, sheet_create for xlsx and \
         slides_create for pptx; they are written into this chat's own folder."
    )
}

fn edit_file(args: &Value, ws: &Path) -> Result<String, String> {
    let path = require_str(args, "path")?;
    if crate::tools::extract::is_extractable(path) {
        return Err(document_guard(path));
    }
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

async fn run_shell(
    command: &str,
    ws: &Path,
    cancel: &Cancel,
    plan: &Plan,
) -> Result<String, String> {
    let cmd = sandbox_exec::command(plan, command, ws, COMMAND_TIMEOUT_SECS)?;
    // Only the host rung runs the command from a directory we choose; the
    // container and the distro are told where to work in their own arguments.
    let cwd = (plan.rung == sandbox_exec::Rung::Host).then_some(ws);
    let output = run_process(cmd, cwd, COMMAND_TIMEOUT_SECS, true, Some(cancel)).await?;
    Ok(format!("{}\n{output}", sandbox_exec::banner(plan)))
}

/// Windows flashes a console window for every console program a GUI app
/// spawns. None of our children are interactive, so none of them want one.
#[cfg(windows)]
fn hide_console(cmd: &mut Command) {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console(_cmd: &mut Command) {}

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
    hide_console(&mut cmd);

    let mut child = cmd.spawn().map_err(|e| format!("could not start command: {e}"))?;
    // Every process this app runs joins the job object, so a hard kill of
    // DeskSeek cannot leave a build running with nothing left to stop it.
    job::adopt(&child);
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
        kill_tree(&mut child).await;
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

/// Whether a command exits cleanly — how the setup screen finds out what is on
/// the machine (docker, wsl). Any failure, including a timeout, is simply "no".
///
/// `output()` drains both pipes, so a chatty probe cannot fill a buffer and
/// hang; `kill_on_drop` makes the timeout path kill the child rather than
/// orphan it.
///
/// Note this answers "did the command succeed", which is NOT the same question
/// as "is the tool installed" — see `commands::probe_environment`.
pub(crate) async fn probe(program: &str, args: &[&str], timeout_secs: u64) -> bool {
    let mut cmd = Command::new(program);
    cmd.args(args).stdin(Stdio::null()).kill_on_drop(true);
    hide_console(&mut cmd);
    matches!(
        timeout(Duration::from_secs(timeout_secs), cmd.output()).await,
        Ok(Ok(out)) if out.status.success()
    )
}

/// The same idea, for the probes whose answer is a string rather than a yes or
/// no — listing WSL distributions, for instance. `None` means the program could
/// not be run at all; a non-zero exit still returns whatever it printed, which
/// is where those tools put their explanations.
pub(crate) async fn capture(program: &str, args: &[&str], timeout_secs: u64) -> Option<String> {
    let mut cmd = Command::new(program);
    cmd.args(args).stdin(Stdio::null()).kill_on_drop(true);
    hide_console(&mut cmd);
    let out = timeout(Duration::from_secs(timeout_secs), cmd.output())
        .await
        .ok()?
        .ok()?;
    let mut bytes = out.stdout;
    bytes.extend_from_slice(&out.stderr);
    Some(decode(&bytes))
}

/// Windows tools do not agree on text encoding: `wsl.exe` writes UTF-16LE,
/// which `from_utf8_lossy` renders as NUL-separated ASCII. Dropping the NULs
/// recovers the text, and it cannot damage anything we read this way, because
/// no distro name, path or command output is allowed to contain one.
pub(crate) fn decode(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .filter(|c| *c != '\0')
        .collect()
}

/// Stops a child that did not finish on its own.
///
/// `child.kill()` only reaches the process we spawned, and on Windows that is
/// `cmd.exe` — the build it started carries on, invisibly, after the user has
/// pressed Stop. `taskkill /T` walks the tree instead.
///
/// ponytail: this covers the common case, a long build still running when the
/// run is stopped. A grandchild whose parent already exited has been reparented
/// and escapes it. A Windows Job Object with kill-on-close would close that
/// hole properly, as would a process group on Unix.
async fn kill_tree(child: &mut Child) {
    #[cfg(windows)]
    if let Some(pid) = child.id() {
        let mut cmd = Command::new("taskkill");
        cmd.args(["/F", "/T", "/PID"])
            .arg(pid.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        hide_console(&mut cmd);
        let _ = cmd.status().await;
    }
    let _ = child.kill().await;
}

/// Runs `git` with literal arguments — no shell, so nothing can be injected
/// through a commit message or path.
async fn git(ws: &Path, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new("git");
    cmd.args(args).current_dir(ws).stdin(Stdio::null());
    hide_console(&mut cmd);
    let output = cmd
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
    plan: &Plan,
) -> Result<String, String> {
    match tool {
        "read_file" => read_file(args, ws),
        "list_dir" => list_dir(args, ws),
        "glob" => glob(args, ws),
        "grep" => grep(args, ws),
        "write_file" => write_file(args, ws),
        "edit_file" => edit_file(args, ws),
        "run_command" => run_shell(require_str(args, "command")?, ws, cancel, plan).await,
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
        let cmd = "echo hello";
        let out = run_shell(cmd, &ws, &Cancel::new(), &host_plan())
            .await
            .unwrap();
        assert!(out.contains("hello"), "{out}");
        // Every result says where it ran, so a missing tool is never a mystery.
        assert!(out.starts_with("[ran on this machine]"), "{out}");
        let _ = std::fs::remove_dir_all(&ws);
    }

    fn host_plan() -> Plan {
        sandbox_exec::plan_for(sandbox_exec::Mode::Host, &sandbox_exec::Capability::default(), "x")
    }

    #[test]
    fn a_utf16_listing_decodes_instead_of_arriving_nul_separated() {
        // `wsl.exe` writes UTF-16LE. Without the NUL filter this reads as
        // "U\0b\0u\0n\0t\0u\0", which parses as no distro at all.
        let text = "* Ubuntu  Running  2\n";
        let utf16: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(decode(&utf16), text);
        // Plain UTF-8 is untouched.
        assert_eq!(decode(b"hello"), "hello");
    }

    #[tokio::test]
    async fn probe_reports_only_a_clean_exit() {
        // The distinction the setup screen depends on: a command that ran and
        // failed is not the same as one that is missing. `docker --version`
        // succeeds while `docker info` fails when the engine is stopped.
        let (ok, ok_args): (&str, &[&str]) = if cfg!(windows) {
            ("cmd", &["/C", "exit 0"])
        } else {
            ("true", &[])
        };
        let (bad, bad_args): (&str, &[&str]) = if cfg!(windows) {
            ("cmd", &["/C", "exit 1"])
        } else {
            ("false", &[])
        };

        assert!(probe(ok, ok_args, 5).await);
        assert!(!probe(bad, bad_args, 5).await);
        assert!(!probe("deskseek-no-such-program-here", &[], 5).await);
    }

    #[test]
    fn read_file_opens_a_word_document_rather_than_refusing_it() {
        use docx_rs::{Docx, Paragraph, Run};

        let ws = workspace();
        let doc = Docx::new().add_paragraph(
            Paragraph::new().add_run(Run::new().add_text("Invoice total: 42")),
        );
        let mut bytes = Vec::new();
        doc.build().pack(std::io::Cursor::new(&mut bytes)).unwrap();
        std::fs::write(ws.join("invoice.docx"), &bytes).unwrap();

        // The bytes really are not text, so this is the case that used to fail
        // with "stream did not contain valid UTF-8".
        assert!(std::fs::read_to_string(ws.join("invoice.docx")).is_err());

        let out = read_file(&json!({"path": "invoice.docx"}), &ws).unwrap();
        assert!(out.contains("Invoice total: 42"), "{out}");
        assert!(out.contains("extracted from"), "names the source: {out}");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn a_document_cannot_be_written_as_text() {
        let ws = workspace();
        // Writing text to a `.docx` would produce a file that does not open, so
        // the refusal has to arrive instead of a file nobody can read back.
        for path in ["report.docx", "sales.xlsx", "deck.pptx", "scan.pdf"] {
            let err = write_file(&json!({"path": path, "content": "hello"}), &ws).unwrap_err();
            assert!(err.contains("doc_create"), "{path}: {err}");
            assert!(!ws.join(path).exists(), "{path} must not be created");
        }
        // edit_file says the same thing rather than failing on a UTF-8 error.
        let err = edit_file(
            &json!({"path": "report.docx", "old_string": "a", "new_string": "b"}),
            &ws,
        )
        .unwrap_err();
        assert!(err.contains("doc_create"), "{err}");

        // An ordinary text file is untouched by the guard.
        assert!(write_file(&json!({"path": "notes.md", "content": "hello"}), &ws).is_ok());
        let _ = std::fs::remove_dir_all(&ws);
    }
}
