# Architecture

A map for whoever opens this repo next — a person or an assistant — with no
context. What the app *does* is the [README](README.md); why it is built this way
and what is next is [plan.md](plan.md); what changed when is
[CHANGELOG.md](CHANGELOG.md); what only a real window can confirm is
[docs/E2E-CHECKLIST.md](docs/E2E-CHECKLIST.md). This file is just the shape: the
layers, the one-owner rules, and how a turn moves through them.

The rule behind most of it: **one file owns each rule.** Where this document names
an owner, a second copy of that rule somewhere else is a bug, not a refactor.

## The layers

A turn goes down through four layers and its results come back as events:

```
src/  (React)                the window: panes, panels, dialogs, transcripts
  |  invoke()                one typed call site per command — src/api.ts
  v
commands.rs                  every command the window may ask for, and nothing else
  |
  v
conversation/agent.rs        the agent loop: stream a turn, run its tools, repeat
  |-- api/client.rs          HTTP + SSE to DeepSeek
  |-- tools/mod.rs           the built-in toolset, and how risky each call is
  |     guard.rs             what may run without asking
  |     sandbox.rs           the workspace path sandbox — the real containment
  |     sandbox_exec.rs      Docker / WSL2 / this machine, and the job object
  |     exec, walk, docs, extract, browse, browser, desktop, job
  |-- mcp/manager.rs         external servers, tools renamed mcp__<server>__<tool>
  |
  v
store/                       every file the app owns, under one root
      paths.rs               the root, and every path in it (the one place)
      db.rs                  the schema, the guarded upgrades, and the queries
      settings.rs            settings JSON
      keychain.rs            the API key — never a file, never a log
      files.rs               the "folder of markdown files" store
```

Two edges are worth naming. `commands.rs` is the *whole* surface the window has,
so "what can the window ask for" is one file rather than a search. And nothing in
`conversation/` touches the filesystem on the model's behalf: a write goes through
`tools/`, which is where the guard and the sandbox live, so a new tool cannot be
the one that quietly skipped them.

## Where the events flow

The window never polls for a run in progress. Event names are emitted from one
place and listened to in one place (`src/api.ts`), which is why a panel can react
to a run without knowing anything about runs:

| Event | Emitted by | Means |
| --- | --- | --- |
| `chat-started`, `chat-delta`, `chat-done`, `chat-error` | `conversation/agent.rs` | a turn's text, as it streams |
| `chat-tool` | `conversation/agent.rs` | a tool call, and its result |
| `chat-approval` | `conversation/agent.rs` | parked, waiting for `respond_approval` |
| `chat-question` | `conversation/agent.rs` | parked, waiting for `respond_question` |
| `chat-guard` | `conversation/agent.rs` | a call the guard refused |
| `chat-artifact` | `conversation/agent.rs` | a document was published |
| `chat-shot`, `computer-state` | `tools/browser.rs`, `tools/desktop.rs` | a screenshot; the desktop-control gate |
| `chat-notify` | `notify.rs` | something finished — the in-app event; `notify.rs` decides whether a toast goes with it |
| `fs-changed` | `fs_watch.rs` | the workspace moved, so redraw the tree |
| `mcp-log`, `mcp-ready` | `mcp/manager.rs` | a server's stderr; a server came up |
| `scheduled-fired` | `commands.rs` | a scheduled task started a run |

The two *parked* events are the interesting pair. An approval or a question parks
the agent loop on a oneshot channel (`approvals.rs`, `questions.rs` — the same
shape twice, because it is the same problem, and the second one exists because the
first one worked); a command resolves it and the run continues. The chat and the
Scheduled panel are therefore two views of one park, not two mechanisms. A park
that outlives the process leaves a marker in the database
(`db::in_flight_runs`), which is what lets the next launch say a run was
interrupted instead of pretending it never happened.

## One rule, one file

The rules this repo has the most to lose from, and the file each one lives in.
Changing behaviour in this app is usually changing one of these.

| Rule | Owner |
| --- | --- |
| The `messages` array sent to the API, including `reasoning_content` echoed back on prior assistant turns whenever the request carries tools | `conversation/assemble.rs` |
| The agent loop, and the system prompt: the profile, the looked-up memory, the skills and design-system catalogue, the instructions, the project's rules | `conversation/agent.rs` |
| Model prices, and which hours are off-peak | `api/models.rs` |
| A tool's risk class, and the tool list each agent mode gets | `tools/mod.rs` |
| What may run without asking (ask / auto / skip, and the catastrophic tier) | `tools/guard.rs` |
| Where a command actually runs, and the job object that kills its children with it | `tools/sandbox_exec.rs`, `tools/job.rs` |
| The workspace path sandbox — the real containment, as opposed to the guard's heuristics | `tools/sandbox.rs` |
| Every path under the app's root | `store/paths.rs` |
| The schema, and every upgrade — a new column is a `CREATE TABLE` entry *and* a guarded `ALTER` | `store/db.rs` |
| The "one markdown file per named thing" store, including the id that may never climb out of its folder | `store/files.rs` |
| The profile (`MEMORY.md`) versus one file per category | `memory.rs`, `memory_files.rs` |
| Whether a finish becomes an OS toast | `notify.rs` |
| Standing a local SearXNG up: the image, the port, the config it runs on, and when it counts as up | `searxng.rs` |
| When a scheduled task next runs — strictly after now, advanced before the run | `schedule.rs` |
| The workspace watcher, and its refcounted roots | `fs_watch.rs` |
| What a backup carries, and what it deliberately leaves out | `backup.rs` |

The frontend has the same shape: a small pure module per rule, each with a
`*.check.ts` beside it that `npm run check` runs (the README lists what each one
pins).

| Rule | Owner |
| --- | --- |
| Tabs, panes, and what comes back on the next launch | `src/lib/shell.ts` |
| The transcript's window over a long chat | `src/lib/transcript.ts` |
| Where an attachment is filed | `src/lib/filing.ts` |
| Keyboard chords, and what they may fire while a field has focus | `src/lib/shortcuts.ts` |
| The palette's scopes and its ranking, and the composer's menus | `src/lib/commands.ts` |
| Colour tokens, and the contrast the themes must keep | `src/lib/theme.ts` |
| The file tree's rows, and what each arrow key moves | `src/lib/tree.ts` |
| Reading one version out of the changelog | `src/lib/changelog.ts` |
| The composer draft that survives a crash | `src/lib/draft.ts` |

## Tests and the gate

`cargo test` covers the Rust: the pure rules above, the schema upgrades against a
real temp file (a fresh in-memory schema proves nothing about an upgrade), and the
Tauri commands. `npm run check` covers the frontend's pure modules with plain Node
and no framework. The gate — `typecheck`, `check`, `build`, `cargo test`, then
`npm run tauri dev` booting — is written out in the README's *Development*
section, and CI runs it on every push to `main`.

What no automated test can reach is listed in
[docs/E2E-CHECKLIST.md](docs/E2E-CHECKLIST.md): a screen reader, a pulled network
cable, a killed process, a real installer.
