# DeskSeek

A lightweight desktop client for the DeepSeek API — built with **Tauri v2** (Rust core) and
**React + TypeScript**.

Two sections share one core:

- **Chat** — a general assistant with streaming replies, a collapsible *thinking* pane, and a
  global `MEMORY.md` that is always injected into the system prompt.
- **Coding** — an agent that works inside a folder you trust.

Bring your own API key: it is stored in the **OS keychain** (Credential Manager / Keychain /
Secret Service), never in a config file and never in a log.

## Status

Both sections are functional:

**Chat**

- Streaming replies with a collapsible *thinking* pane, Markdown and highlighted code
- **Attachments** — drop, paste or pick images and files; images are downscaled and sent to
  the model as multimodal content parts
- **Agentic by default** — the assistant can read, search, write and edit files in your
  documents folder (default `…/documents`, configurable), so it can produce a document for you
  and save it rather than only printing it
- **Memory** — a single fixed file `MEMORY.md` injected into every request and editable in-app.
  The model writes to it with a `save_memory` tool, so it never invents a file name or asks to
  read memory it was already given. *Summarise chat* proposes entries you can accept or reject
- **Chat log** — each summary is also written to `CHATLOG/YYYYMMDD-topic.md`
- **Skills** — drop-in instruction files at `skills/<id>/SKILL.md`, toggled per chat
- **Scoped instructions** — named instruction blocks that apply only in chat, only in coding,
  or both (the bundled *Document design* one is an example)
- `/compact` and friends — `/help` lists the built-in commands, including `/mcp`
- **Task list** — for multi-step work the model publishes its plan with the `update_tasks` tool
  and the checklist is pinned above the composer, ticking over as steps start and finish. It is
  collapsible, and a step in progress pulses so a long job is visible at a glance
- Approximate **context-window meter** in the header

**Coding**

- Point a task at a **folder** it may work in
- Tools: `read_file`, `list_dir`, `glob`, `grep`, `write_file`, `edit_file`, `run_command`,
  `git_status`, `git_diff`, `git_log`, `git_commit`, `git_push`
- **Every write and every command asks first** — allow once, allow for the session, allow
  always for that folder, or deny. Reads run freely
- A **path sandbox** refuses anything outside the workspace, including `..` traversal and
  symlink escapes; the API key is stripped from every child process environment
- **Commands can run in a container** instead of on this machine — see *Where commands run* below
- A **live file tree** (`/files`, or the *Files* button) — the folder is watched, so files
  the agent creates or edits appear as it works. Click any file to read it without
  interrupting the run; if the agent writes to the file you are reading, it re-reads itself
- Project context from `AGENTS.md`, else `.deskseek/context.md`
- A **task list** the model keeps up to date as it works — see the chat section above
- Tool cards **open themselves when a call fails**, so a broken build is readable without a
  click

**Approvals, permission modes and stopping a run**

Each conversation carries its own permission mode, chosen in the header:

- **Ask** (default) — pause for approval before every write and every command
- **Auto** — run without asking. A safety guard refuses known-catastrophic commands (disk
  formatting, `git reset --hard`, `git push --force`, publishing to a registry, `rm -rf /`,
  download-and-execute pipelines) and commands that upload local data to the network. Three
  refusals in one run drop the chat back to *Ask* automatically
- **Skip** — never ask. Only the catastrophic tier is still refused

One honest caveat, stated plainly in the app too: **the guard is a heuristic, not a security
boundary.** It catches the patterns a confused or prompt-injected model reaches for. It is not a
sandbox, and a missing match is not proof of safety. The real containment is the workspace path
sandbox plus the API key being stripped from every child process.

**Where commands run**

A shell command does not have to run on your machine. Under *Where commands run* in `/setup` there
are four choices — the same setting, in order of how much they isolate:

| Setting | What it does |
| --- | --- |
| **Auto** (default) | The strongest rung this machine can actually reach |
| **This machine** | Runs directly, with the guard and the workspace sandbox |
| **Docker** | A throwaway container: your workspace mounted at `/workspace`, 2 GB and 2 CPUs, **no network** |
| **WSL2** | Inside a WSL distribution, at `/mnt/<drive>/...` |

Every command result is prefixed with where it ran, so a missing tool is never a mystery — the
model sees `[ran on docker (alpine:latest, no network)]` and can tell you the image is the reason
rather than inventing a workaround.

Three things are worth saying plainly:

- **A container only has what its image was built with.** `alpine:latest` starts instantly and has
  no node, no cargo and no python. If you sandbox a real project, set the image to one that carries
  its toolchain.
- **The container gets no network.** A build that downloads dependencies will fail there and
  succeed on this machine. That is the trade, not a bug.
- **"This machine" is a guardrail, not a boundary.** It is the workspace sandbox, the scrubbed
  environment, the timeout and the guard — nothing more, and the app says so.

The rung is **refused when it cannot be honoured**: if you pin Docker and the engine is stopped,
commands run on your machine with a note saying so, rather than being handed to a container that
answers nothing. Choosing *Auto* picks Docker only when the engine is answering — the CLI being on
PATH is not enough, because Docker Desktop installed and stopped looks identical to Docker
installed and running if you ask the wrong question.

Best-effort mode is also honest about stopping a run: `Stop` kills the whole process tree rather
than just the shell it started, so a build does not carry on invisibly after you have stopped it.
There is a gap — a grandchild whose parent has already exited is reparented and escapes — and the
code says so where the upgrade path belongs.

**Stop** — while a run is going, the send button becomes *Stop*. It aborts the model stream, kills
any running child process, and records whatever the model had produced so far. A chat runs one
turn at a time: sending again while a turn is in flight stops the old one.

Switching to another conversation does **not** interrupt a run. Each conversation keeps its own
live turn — stream, tool cards and pending approvals — so a long job keeps going while you read
something else. Its entry in the sidebar shows a small pulse until it finishes.

**Tabs and two panes**

Opening a conversation puts it in the tab strip along the top; the strip is the list of what you
are working on, and the sidebar is the list of everything that exists. A tab shows which pane it
is on and a pulse while it is working.

*Split* turns the window into two panes, each showing a different conversation. The badge on a
tab says which pane it is in, and the divider between them drags to resize. Clicking anywhere in
a pane aims the sidebar, the tab strip and the file tree at it — so *Files* shows the folder of
the pane you clicked, not the other one. Closing a tab refills its pane from another open tab
rather than leaving a hole.

The layout is remembered, so reopening the app brings back the same tabs, the same split and
the same divider position. A conversation deleted while the app was closed does not come back
as a tab.

Guard refusals and errors belong to their chat, so a refusal in the pane you are not looking at
is still there when you look at it.

**Internet access** (both sections)

- `fetch_url` reads a page as text, `browser_read` renders it in a real browser first (so
  JavaScript-built pages work), and `web_search` queries a **SearXNG** instance
- All three are read-only, so they never prompt; turn them off in **Settings → Browsing**
- `browser_read` drives the Chromium-family browser already on your machine — nothing to
  download. Point it at Chrome or Edge explicitly if auto-detection picks the wrong one

**MCP servers**

DeskSeek is an MCP *client*. Servers you configure are launched as child processes and their
tools are offered to the model in **both** sections:

- Config lives in `mcp.json` next to your settings, and can be edited in the app under
  **MCP servers** or by hand (the panel has a *Re-read file* button for that)
- Tools are exposed as `mcp__<server>__<tool>`, so two servers offering `search` never collide
- **Every MCP call asks first.** A server's own `readOnlyHint` does not skip the prompt unless
  you tick *trust read-only* for that server — a server describes its own tools, so that
  shortcut is opt-in
- Server output is treated as data, not instructions
- Servers start in the background, so a slow `npx` download never blocks the window
- Child processes are stopped when the app exits

```json
{
  "servers": [
    {
      "name": "everything",
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-everything"],
      "env": {},
      "enabled": true,
      "trustReadOnly": false
    }
  ]
}
```

The protocol work is done by the official [`rmcp`](https://github.com/modelcontextprotocol/rust-sdk)
SDK, so discovery, version negotiation, pagination and multi-round-trip requests are handled for
you. Built with `default-features = false` and only the client + stdio transport features.

## First run

On first launch DeskSeek shows a short setup screen — reopen it any time with `/setup`. It
checks what is already on the machine and asks about the two optional pieces rather than
quietly leaving them broken:

| Piece | Needed for | Required? |
| --- | --- | --- |
| DeepSeek API key | everything | **Yes** — stored in the OS keychain |
| SearXNG instance | `web_search` | No — the other browsing tools work without it |
| Docker or WSL2 | running commands in a container instead of directly | No |

The same screen also chooses where commands run, and shows which rung that setting currently
resolves to.

DeskSeek **never installs either one for you**. It detects what is present, links to the
official install guides, and uses whatever it finds. A machine without Docker is not broken —
it just runs commands directly, with the guard as the only protection.

It also distinguishes *installed* from *running*, because those are different questions with
different answers. `docker --version` is a client-only string that succeeds whenever the CLI is
on your PATH; `docker info` needs the engine's named pipe and fails with exit 1 when Docker
Desktop is merely stopped. Asking the second question to answer the first reports a perfectly
good Docker install as "not found" — so both are asked, and a stopped engine reads as
*Docker installed, engine stopped* with a nudge to start Docker Desktop and press Re-check.

The SearXNG check is honest about the common failure: an instance only answers JSON once
`json` is in the `formats` list in its `settings.yml`, so a plain HTML instance is reported as
*not configured* rather than silently returning nothing.

## Requirements

- [Node.js](https://nodejs.org/) 20+ and npm
- [Rust](https://rustup.rs/) (stable) — Tauri needs the platform toolchain (MSVC on Windows)
- Windows 10/11, macOS, or Linux. **Windows is the only officially tested target today**;
  macOS and Linux build in CI but are experimental.
- Optional: [Docker](https://docs.docker.com/desktop/install/windows-install/) or
  [WSL2](https://learn.microsoft.com/windows/wsl/install) for sandboxed command execution

## Development

```bash
npm install
npm run tauri dev
```

Build a release bundle:

```bash
npm run tauri build
```

Run the checks:

```bash
npm run typecheck
npm run check
cd src-tauri && cargo test
```

`npm run check` is the frontend's own checks (plain Node, no test framework). It pins the tab and
pane rules in `src/lib/shell.ts` — the panes must never show the same chat twice, and closing a
tab must never leave a pane blank while another tab is still open — and the task-list reader in
`src/lib/tasks.ts`, which parses arguments the model wrote and must therefore survive every
malformed shape it can produce.

## Architecture

```
src/                     React UI (App, Sidebar, ChatView, SettingsDialog, Markdown)
src-tauri/src/
  api/                   DeepSeek client, SSE parsing, delta accumulation, model pricing
  conversation/          message-array assembly (owns the reasoning-content rule), agent loop
  store/                 SQLite schema, settings JSON, OS keychain, paths
  tools/                 built-in tools, path sandbox, process runner, permission guard
  mcp/                   MCP client (rmcp) and server config
  runs.rs                in-flight run registry + cancel handles
  fs_watch.rs            workspace watcher, refcounted roots, coalesced events
  commands.rs            the Tauri command surface
```

Notable invariants (covered by tests):

- `reasoning_content` must be echoed back on prior assistant turns **whenever the request
  carries `tools`** — otherwise the API returns 400.
- `tool_choice: required`/named is rejected in thinking mode.
- Model prices are half off-peak; peak is 01:00–04:00 and 06:00–10:00 UTC, Mon–Fri.
- A chat has at most one live run; starting another cancels the first.

## License

MIT — see [LICENSE](LICENSE).
