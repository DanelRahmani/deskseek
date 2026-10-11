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
- **Attachments** — drop a file anywhere on the window (an overlay appears), or paste or pick
  one. An image is downscaled and sent to the model as a multimodal content part; a text file
  is sent as its own text, so the model can actually read it; anything else is kept as a name
  and a MIME type with no payload, because a text-only model cannot read it and storing the
  bytes would be a row that is paid for and never sent
- **Agentic by default** — the assistant can read, search, write and edit files in your
  documents folder (default `…/documents`, configurable), so it can produce a document for you
  and save it rather than only printing it
- **Memory** — two tiers, because one file has to be either short or cheap and it cannot be
  both:
  - `MEMORY.md` is the **profile**: who the user is and how they want replies — name, language,
    standing preferences. It is short on purpose, sent with *every* request, and editable in-app.
  - Everything else is a **kept fact**, stored individually and looked up by keyword (SQLite
    FTS5) against what you just asked. Only the matching ones are sent, so the store can grow
    without slowing anything down. Facts are scoped to a project when they were learned inside
    one, so a note about one client does not turn up in another's chat.
  - *Summarise chat* proposes entries and tags each one *always sent* or *facts*, so you can see
    where an Accept will put it. A proposal only reaches `MEMORY.md` when you accept it there.
  - When the assistant remembers something itself — with `save_memory` — it files it the same
    way: only an explicit `profile` argument goes to `MEMORY.md`; everything else is kept as a
    fact, which is the default. A fact the model recalls mid-run therefore cannot quietly grow
    the always-sent file.
  - The panel lists kept facts and lets you forget any of them, and warns when `MEMORY.md` has
    grown past what is worth sending every time.
  - Honest limit: keyword search matches words, not meaning. Asking about "the car" will not
    surface a fact stored as "vehicle". Embeddings would fix that; this trades it for requiring
    no model download and no network.
- **Chat log** — each summary is also written to `CHATLOG/YYYYMMDD-topic.md`
- **Skills** — instruction files at `skills/<id>/SKILL.md`, toggled per chat; install one from
  the panel's file picker and remove one from the same list (the panel also opens the folder)
- **Scoped instructions** — named instruction blocks that apply only in chat, only in coding,
  or both (the bundled *Document design* one is an example)
- `/compact` and friends — `/help` lists the built-in commands, including `/mcp` and `/export`
- **Command palette** (`Ctrl+K`) — type to jump to a conversation in either section, most
  recently used first, or to run the same action a panel button does; every entry is a state
  change the UI already makes
- **Sidebar search** — filter the conversation list by title or folder as you type, and search
  the *text* of every conversation: matching messages are listed with the words in context, from
  both sections, so a chat can be found by what was said in it rather than by its name
- `/export` — write the whole conversation to a Markdown file in your documents folder
- **Task list** — for multi-step work the model publishes its plan with the `update_tasks` tool
  and the checklist is pinned above the composer, ticking over as steps start and finish. It is
  collapsible, and a step in progress pulses so a long job is visible at a glance
- **Artifacts** — a document the model publishes with `artifact_create` opens in a panel beside
  the chat, rendered live and versioned, instead of scrolling away as a giant code block
- **Documents** — `doc_create`, `sheet_create` and `slides_create` write real `.md`/`.docx`/`.pdf`,
  `.xlsx` and `.pptx` files to your documents folder, with an *Open* button on the result
- Approximate **context-window meter** in the header
- **Per-chat cost** — the sidebar shows each conversation's accumulated spend, summed from
  real usage as every turn ends

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

**Keyboard shortcuts**

The keyboard does the same things the buttons do, and nothing more — every key here
reaches the state change its button already makes:

| Shortcut | Does |
| --- | --- |
| `Ctrl+N` | New conversation |
| `Ctrl+W` | Close the focused tab |
| `Ctrl+B` | Show or hide the file tree |
| `Ctrl+M` | Memory panel |
| `Ctrl+K` | Command palette |
| `Ctrl+,` | Settings |
| `Ctrl+\` | Split into two panes, or back to one |
| `Ctrl+1` / `Ctrl+2` | Move focus to the left / right pane |
| `Escape` | Close the open panel or dialog |

`Cmd` works in place of `Ctrl` on macOS. `Escape` belongs to whatever field has
focus: in the composer it dismisses the slash-command menu and leaves the panel
alone. The map is a pure function (`src/lib/shortcuts.ts`) with its own checks, so
a chord can never fire for the wrong key.

**Internet access** (both sections)

- `fetch_url` reads a page as text, `browser_read` renders it in a real browser first (so
  JavaScript-built pages work), and `web_search` queries a **SearXNG** instance
- All three are read-only, so they never prompt; turn them off in **Settings → Browsing**
- `browser_read` drives the Chromium-family browser already on your machine — nothing to
  download. Point it at Chrome or Edge explicitly if auto-detection picks the wrong one

**Driving a browser** (both sections)

`browser_read` reads one page. When a job needs more than that — filling in a form, clicking
through to a result, checking what a site really does — the agent can drive the browser instead:

| Tool | What it does |
| --- | --- |
| `browser_open` | Open a URL and read the page. The browser keeps its state, so open once and work on it |
| `browser_extract` | Read the page that is already open |
| `browser_eval` | Run a JavaScript expression in the page and get the value back |
| `browser_click` / `browser_type` | Click an element or fill a field by CSS selector, then read the page back |
| `browser_screenshot` | Show **you** a picture of the page, in the chat |

It is Edge or Chrome, launched headless with its **own profile directory** under DeskSeek's data
folder on an auto-assigned debugging port. It never touches the browser you are signed into: it
cannot see your tabs, your cookies or your logins, and nothing outside this machine can reach it.
There is no CDP crate and no bundled download — the protocol is JSON over a WebSocket, and the
module speaks it directly.

Four things are worth saying plainly:

- **These always ask, even in *Auto* mode.** No string rule can tell a harmless click from a
  purchase, so the app does not pretend to. Only *Skip* runs them unattended.
- **Page content is data, never instructions.** A page that tells the model to run a command or
  ignore what you asked is a prompt injection — the agent is told to stop and report it.
- **Screenshots go to your eyes, not the model's.** The result the model receives is text, because
  DeepSeek's API is text-only and a picture in a tool result would be a 400 waiting to happen.
- **A missed click names what is actually on the page**, so the agent corrects itself instead of
  guessing at the same selector again.

**The user's desktop** (both sections, off by default)

Windows only, and switched on under *The user's screen* in `/setup`.

`ui_snapshot` reads the desktop as text — the controls of the window that has focus (their names,
types, ids, and which are disabled), or of another open window if the agent names it:

```
Window "Macroeconomics II Summary.docx - Word"
  Pane "MsoDockTop"
    ToolBar ""
      Button "Save" [id=btnSave]
      Button "Share" (disabled)
```

That is how the agent answers "what is on my screen" and reads a dialog you cannot describe.
`ui_click` presses a control by name or id, and `ui_type` types into a field. The agent is told to
work in one-step turns — look, act once, read what the call returns — so a wrong turn costs one
step instead of five.

Four rails, because this is the one feature that really moves your mouse:

- **Ctrl+Alt+Esc stops everything.** It cancels every running chat, not just the visible one, and
  computer control *stays* stopped until you press **Start it again** in the banner. A blue bar
  appears while the agent is acting, so you are never guessing whether it is mid-something.
- **Every press and every keystroke asks.** Even in *Auto* mode: one prompt per call. Only *Skip*
  runs them unattended.
- **A run gets 25 desktop actions.** A confused model can overrun a turn; it cannot click its way
  through your afternoon.
- **Actions are spaced 250 ms apart**, so a runaway loop is visible rather than instantaneous.

And the limits, stated rather than papered over:

- **Off by default**, because whatever is on screen may be private.
- **A window with no standard controls reports nothing.** A game, a canvas, a custom-drawn app has
  no accessibility tree to read — the agent is told to say so and ask, instead of clicking at
  random. There is no raw coordinate clicking: everything goes through a control's own Invoke
  pattern, or the mouse, aimed at a control you named.
- **It is bounded**: six levels deep, 300 controls, off-screen branches skipped. A tree read whole
  would cost more context than it is worth.
- **Screenshots are not implemented.** The agent reads text; your eyes are the picture.
- **What a window reports is data, never instructions.** Text in a window is content you did not
  write, so it is treated exactly like a web page: reported, never obeyed.

**Artifacts** (both sections)

When the answer is a document rather than a reply — a report, a page, a chart, a diagram, a note
or a snippet worth keeping — the agent calls `artifact_create` instead of pasting it into the
chat. The document opens in a panel beside the conversation and is kept, not scrolled away:

| Kind | How it is shown |
| --- | --- |
| `html` | A sandboxed frame, rendered live |
| `svg` / `mermaid` | The diagram, drawn and centred on white |
| `markdown` | The normal Markdown renderer |
| `code` | Monospace, read-only |

Each artifact is **versioned**. Calling `artifact_create` again with the same title publishes a
new version rather than a second document; a version menu goes back to any earlier one, and
*Restore* makes an old version current again — by saving it as a new version, so nothing is ever
overwritten. *Edit* opens the document as text; saving it adds a version attributed to you. The
tool card in the chat names what was published, and the live panel is where the substance lives.

Two things worth saying plainly:

- **The frame is the sandbox.** Artifacts are rendered with `sandbox="allow-scripts allow-forms"`
  and **no** `allow-same-origin`, so an artifact gets an opaque origin and cannot reach the app's
  DOM, its storage, or the Tauri APIs behind it. On top of that the frame carries a
  `Content-Security-Policy` of `default-src 'none'`, so nothing in an artifact can reach the
  network — the styles and scripts must be inline. That is what makes a self-contained page work
  and a tracking pixel not.
- **An artifact is data, not a file.** It lives in the database, is never written into your
  workspace, and the model cannot read one back unless you paste it. It is a reading surface, not
  a second filesystem.

**Documents** (both sections)

When what you need is a *file* — something to send, print, or open in Excel — the agent writes
one instead. Three tools, and every one of them asks before it runs:

| Tool | Writes |
| --- | --- |
| `doc_create` | `.md`, `.docx` (Word) or `.pdf` |
| `sheet_create` | `.xlsx`, from rows |
| `slides_create` | `.pptx`, titles and bullets |

They land in your **documents folder** (default `…/documents`, shown in `/setup` and changeable in
Settings) and the tool card offers an **Open** button, so a generated report is one click from
Word or Excel rather than a hunt through the filesystem.

The content is Markdown — `#` headings, `-` and `1.` lists, `| tables |`, ```` ``` ```` code — and
the same small reader turns it into blocks for every format, so the `.docx` and the `.pdf` cannot
disagree about what the document says. Four things are worth saying plainly:

- **The PDF is printed by the browser you already have.** `--print-to-pdf` on the same
  Chromium-family binary `browser_read` uses — no bundled renderer, no second HTML engine, and the
  best fidelity of the four. It needs a browser to be findable; if none is, the tool says so and
  tells the agent to offer `md` or `docx` instead.
- **Spreadsheet formulas are written, not evaluated.** A cell whose text starts with `=` is stored
  as a formula and Excel calculates it when the file opens. There is no spreadsheet engine in this
  app and the tool description does not pretend otherwise.
- **PowerPoint is title-and-bullets only.** Layouts, images and animation are out of scope, and the
  tool says so rather than producing a deck that looks wrong in the editor.
- **Existing files are never overwritten.** A second document with the same name becomes
  `name-2`, so a re-run costs nothing.

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
- The chat header shows a **🔌 N MCP tools** badge once discovery finishes, or *MCP starting…*
  while it is still in flight — with a fresh chat, the first message can otherwise be sent
  before the model has any of those tools
- The panel's **Logs** tab shows every call (tool, redacted arguments, duration, result size)
  and anything a server writes to stderr, which is where a failing server explains itself

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

**Remote servers.** A server does not have to be a local command. Give it a `url` instead of a
`command` and DeskSeek speaks the streamable-HTTP transport to it directly, with no child process
and no `npx` — `headers` supplies whatever the endpoint wants, usually an
`Authorization=Bearer …` line. The two are alternatives: a server with a `url` ignores its
command and arguments, and the panel greys them out.

```json
{
  "servers": [
    { "name": "hosted", "url": "https://example.com/mcp", "headers": { "Authorization": "Bearer …" } }
  ]
}
```

The same rules apply as for a local server: every call asks first, `readOnlyHint` only skips the
prompt when you have ticked *trust read-only*, and everything the server returns is data rather
than instructions. What is lost is stderr — a local server's complaints land in the panel's
**Logs** tab, but a remote one only has its HTTP status to explain itself.

## First run

On first launch DeskSeek shows a short setup screen — reopen it any time with `/setup`. It
checks what is already on the machine and asks about the two optional pieces rather than
quietly leaving them broken:

| Piece | Needed for | Required? |
| --- | --- | --- |
| DeepSeek API key | everything | **Yes** — stored in the OS keychain |
| SearXNG instance | `web_search` | No — the other browsing tools work without it |
| Docker or WSL2 | running commands in a container instead of directly | No |
| *The user's screen* toggle | reading and operating the desktop (`ui_snapshot`, `ui_click`, `ui_type`) | No — off by default |

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
pane rules in `src/lib/shell.ts`; the task-list reader in `src/lib/tasks.ts`, which parses
arguments the model wrote and must therefore survive every malformed shape it can produce; the
artifact and document readers; and the shortcut map in `src/lib/shortcuts.ts`, so a chord cannot
silently fire for the wrong key or stop firing for the right one.

Before a release, `npm run tauri build` produces the installers (Windows NSIS + MSI, macOS
`.app`/`.dmg`, Linux `.deb`/`.rpm`/AppImage). The behaviour that cannot be tested automatically
is written down as a tick-list in [`docs/E2E-CHECKLIST.md`](docs/E2E-CHECKLIST.md) — run it once
against the built app before publishing. Pushing a `v*` tag runs the same build in CI and drafts
the release, so the tag is the publish button.

**Windows: `cargo test` dies with `0xc0000139`?** That is a loader error, not a test failure: the
test harness links `TaskDialogIndirect`, which only `comctl32` **v6** exports, but a test binary
gets no application manifest, so the loader binds the v5 DLL and kills the process before `main`.
`build.rs` adds the v6 dependency explicitly. Binary targets get it from Tauri's own link args,
which is why only the tests are affected.

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
