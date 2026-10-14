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
  |-- api/client.rs          HTTP + SSE: the retry ladder, the reader, and which failures are
  |     provider.rs           which endpoint a turn runs against, and the one seam a wire shape
  |                           lives behind; openai_compat.rs / anthropic_compat.rs are the two
  |                           shapes, each pure so a wire format is testable without a network
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
| `chat-guard` | `conversation/agent.rs` | a call the guard refused — or the reviewer did, which is the same event with a different reason |
| `chat-artifact` | `conversation/agent.rs` | a document was published |
| `chat-shot`, `computer-state` | `tools/browser.rs`, `tools/desktop.rs` | a screenshot; the desktop-control gate |
| `chat-notify` | `notify.rs` | something finished — the in-app event; `notify.rs` decides whether a toast goes with it |
| `notify-open` | `lib.rs` | a toast was clicked, as a window focus arriving within the grace: which conversation to open |
| `fs-changed` | `fs_watch.rs` | the workspace moved, so redraw the tree |
| `mcp-log`, `mcp-ready` | `mcp/manager.rs` | a server's stderr; a server came up |
| `scheduled-fired` | `commands.rs` | a scheduled task started a run |
| `quick-sent` | `components/QuickEntry.tsx` | a question was asked from the quick-entry window and handed to a conversation, so open it |
| `tray-action` | `tray.rs` | a tray menu item the window has to run — the id is one the command palette already knows |

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
| When a run folds its own chat, and what a folded transcript is — the one note both `/compact` and the automatic fold go through | `conversation/summarize.rs` |
| Model prices, which hours are off-peak, and each model's context window | `api/provider.rs` (the records), `api/models.rs` (the clock and the budget) |
| Which wire shape a request takes, and the two dialects that own them — one place per dialect, so adding a dialect cannot add a second loop | `api/provider.rs`, `api/openai_compat.rs`, `api/anthropic_compat.rs` |
| The HTTP engine every dialect shares: the retry ladder, the SSE reader, and transport-versus-refusal | `api/client.rs` |
| Which endpoint a turn runs against, and which keychain account its key is in | `store/settings.rs`, `store/keychain.rs` |
| Which model a run is actually on — the chat's own, or the pin on the task that started it — resolved once so the request, the price and the usage row cannot disagree | `conversation/agent.rs`, `store/db.rs` |
| Whether a model takes an effort setting at all, and what each dialect sends for a level — `reasoning_effort` on one wire, a thinking budget on the other | `api/provider.rs`, `api/openai_compat.rs`, `api/anthropic_compat.rs` |
| What a turn would have cost on a cheaper model — the one arithmetic that must not be done twice, and the threshold below which saying so is noise | `api/provider.rs` |
| A tool result that took a picture: one capture, the full-size file kept and the smaller copy sent, the scale rule, and the filter that leaves an endpoint which cannot see with the text | `tools/mod.rs`, `tools/desktop.rs`, `tools/browser.rs`, `api/provider.rs` |
| Where a coordinate click lands: the picture it is anchored to, the window origin remembered with it, and the arithmetic that turns a pixel into a screen point | `tools/desktop.rs` |
| Who spent a row in `turn_usage` (a turn, a summary, a revision) and whether its price is known — and the one writer for a call nobody typed | `store/db.rs`, `conversation/mod.rs` |
| A tool's risk class, and the tool list each agent mode gets | `tools/mod.rs` |
| What may run without asking (ask / auto / skip, and the catastrophic tier) | `tools/guard.rs` |
| Which actions an unattended run puts to a reviewer first, and what an unreadable answer means (an `ask`, never an `allow`) | `conversation/review.rs` |
| What a chat may send out — the three rungs, and which calls each one refuses | `tools/guard.rs` |
| That a delete is never waived by a mode — which commands delete, and why a write can be put back while a removal cannot | `tools/guard.rs` |
| What a grant covers — the target a call reduces to (a site, a program, a folder, an app), and whether a stored grant answers for it | `approvals.rs`, `store/db.rs` |
| Where a command actually runs, and the job object that kills its children with it | `tools/sandbox_exec.rs`, `tools/job.rs` |
| The workspace path sandbox — the real containment, as opposed to the guard's heuristics | `tools/sandbox.rs` |
| Every path under the app's root | `store/paths.rs` |
| The schema, and every upgrade — a new column is a `CREATE TABLE` entry *and* a guarded `ALTER` | `store/db.rs` |
| The "one markdown file per named thing" store, including the id that may never climb out of its folder | `store/files.rs` |
| The profile (`MEMORY.md`) versus one file per category | `memory.rs`, `memory_files.rs` |
| What a notebook is, what a source's `ok`/`stale`/`missing` means and when its digest is worth paying for again (one fingerprint answers both), and how much of a source reaches the model — a catalogue of the enabled ones, never the corpus. Its surface is a pane of its own, and what a copy of it carries is `store/db.rs`'s | `conversation/notebook.rs`, `src/components/NotebookShell.tsx`, `store/db.rs` |
| What a citation is, and whether one names a source that was actually there | `retrieval.rs` |
| What a chat reads out of its own folder, what a document becomes as chunks, and the budget the excerpts stay inside | `retrieval.rs`, `conversation/assemble.rs` |
| Which tools a notebook's question may use, as opposed to a chat's | `tools/mod.rs` |
| What a notebook can be asked to produce, and which formats a document writer really takes | `conversation/notebook.rs`, `tools/docs.rs` |
| What makes a row of a fact table a fact — a source that exists in the last cell — and why a cell beginning with `=` is text | `conversation/notebook.rs` |
| What a menu entry may be asked for, and what a source scope really filters: the catalogue, the citable labels and both source tools read one `in_scope` | `conversation/notebook.rs` |
| An entry that makes two documents — the exam — and why the key's question is short: the paper is the previous turn of the notebook's own conversation | `conversation/notebook.rs`, `commands.rs` |
| The house look of a written document: Times New Roman 11 body, headings at 16/14/12, monospace for a code block, and how `**bold**` becomes a run rather than two asterisks | `tools/docs.rs` |
| The form the Make column opens, drawn from what an entry declares — so the window knows no entry by name | `src/components/MakeDialog.tsx` |
| What a study set is and what makes one usable — a quiz item's answer spelled exactly as one of its options, so the choice can be marked — and where the items live | `conversation/notebook.rs`, `store/db.rs` |
| Which choice is right, what a half-finished set has scored, and what order it is dealt in: the window's half of the same feature, and nothing of its vocabulary | `src/lib/quiz.ts` |
| Whether a finish becomes an OS toast | `notify.rs` |
| Standing a local SearXNG up: the image, the port, the config it runs on, and when it counts as up | `searxng.rs` |
| When a scheduled task next runs — strictly after now, advanced before the run | `schedule.rs` |
| What makes a task due when it is not a clock: the two fingerprints, the edge rule that makes a poll not a run, the cap, and the matching rule a server's notification goes through | `triggers.rs` |
| Where a server-initiated notification lands: the `ClientHandler` the MCP client is built with, and why it implements nothing else | `mcp/manager.rs` |
| The workspace watcher, and its refcounted roots | `fs_watch.rs` |
| What a backup carries, and what it deliberately leaves out | `backup.rs` |
| What the decision-and-spend export carries: the panels' own reads, digests and never arguments | `store/db.rs` |
| The quick-entry chord: how the string in Settings becomes a shortcut the OS registers, and the one spelling that says which shortcut fired | `quick.rs` |
| Capturing a window as a PNG: which window is remembered and when, and the alpha byte GDI leaves at zero | `tools/desktop.rs` |
| The tray menu, and which half of it the window runs rather than this process | `tray.rs` |
| What a script costs before it is spoken, what one request will read out, and how each direction of speech — a script read out, a recording turned back into words — becomes a request. A wire beside the dialects rather than a third one | `api/speech.rs` |

The frontend has the same shape: a small pure module per rule, each with a
`*.check.ts` beside it that `npm run check` runs (the README lists what each one
pins).

| Rule | Owner |
| --- | --- |
| Tabs, panes, and what comes back on the next launch — and that a tab id is a chat, a notebook, or gone | `src/lib/shell.ts` |
| The transcript's window over a long chat | `src/lib/transcript.ts` |
| Where an attachment is filed | `src/lib/filing.ts` |
| Keyboard chords, and what they may fire while a field has focus | `src/lib/shortcuts.ts` |
| The palette's scopes and its ranking, and the composer's menus | `src/lib/commands.ts` |
| Colour tokens, and the contrast the themes must keep | `src/lib/theme.ts` |
| The file tree's rows, and what each arrow key moves | `src/lib/tree.ts` |
| Reading one version out of the changelog | `src/lib/changelog.ts` |
| The composer draft that survives a crash | `src/lib/draft.ts` |
| How a clip's length and price read, why an unknown rate is not zero, and where a dictation lands and what it is called on the wire | `src/lib/audio.ts` |
| How a citation reads once it is stored — the file and the part of it — and whether the notebook still holds the source it names | `src/lib/citation.ts` |

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
