# DeskSeek roadmap

A phased plan for this repo, written 2026-10-12 and updated as phases land.
Phase 1 is **done and pushed** (commit `b437c6e`). Phase 2 is in progress: 2.1
and 2.2 are done and pushed, the rest of 2 is the plan. Items 3–6 have not been
started.

Each item is independently shippable and the order is the author's own priority,
not a schedule. A `✅` on an item means its Stop condition is met and its Test
runs in the gate; the detail under it is what was actually built, which is
sometimes not what the plan guessed.

## How to read an item

`**[S|M|L]** Title` — then **Why**, **Files**, **Test** (the one runnable check),
**Stop** (the condition that says it is finished). Sizes are the author's own
time, not an estimate in days.

Rules that hold for every item, from this repo's practice:

- One file owns each rule; a second copy of the same rule is a bug.
- Every new column goes in the `CREATE TABLE` **and** as a guarded `ALTER`; any
  index over it is only safe because the ALTERs run before the schema batch.
- A test over a fresh `mem()` schema proves nothing about an upgrade — anything
  touching `open()` needs a real temp file.
- Non-trivial logic leaves exactly one runnable check behind.
- Gate: `npm run typecheck` → `npm run check` → `npm run build` →
  `cd src-tauri; cargo test` → `npm run tauri dev` boots. Gate with
  `cmd; if ($LASTEXITCODE -ne 0) { throw "..." }`, never `;` into git.

## Decisions taken (2026-10-12)

1. **Design systems get their own store, one file per system** — not `skills/`
   behind a `design-` prefix. The store boundary *is* the guard, so `is_design`
   stops being a filter every call site has to remember. **No migration**: the
   `design-` prefix stays on the id only because `chat_skills` is shared, and a
   stale `skills/design-*/` folder is simply never read again. Nothing is deleted,
   so nothing is lost by leaving it.
2. **Branching (2.4): the cheap version.** Keep the discarded tail, offer
   *Restore*. No message tree, no `parent_id`.
3. **Prompt library (2.3): files on disk**, matching the skills precedent.
4. **Usage history (2.1): aggregate at turn level** — one row per completed turn,
   so `messages` stays narrow.
5. **Backup (3.3): the API key stays out** of the archive.

---

## Phase 1 — Close the holes ✅ done

Nothing here was new scope; it is work already promised by the README, plus two
defects the owner reported. All of it is on the working tree, gated, uncommitted.

### 1.1–1.3 Design systems are their own store, and the model can use them ✅
- **Was**: creation lived in Tauri commands only (`tools/mod.rs` had zero `design`
  matches), while the read path was *accidental* — `agent.rs` iterated
  `skills::discover()` with no `is_design` filter, so the model's catalogue and
  the user's Skills list disagreed about the same data.
- **Now**: `design/<id>.md`, one file per system, owned by `design.rs`
  (`DesignSystem { id, name, body }`, `compose`/`parse` — the first `# ` heading is
  the title — `list`/`read`/`write`/`remove`, and `file_for`, which is where the id
  guard lives). Per-chat enable reuses `chat_skills` keyed by the design id.
  `commands.rs` gained `DesignView` and `design_dir`; **`keep_other_store`** is the
  guard for the shared table, so saving one list can no longer switch the other
  store off (tested, including the smuggle case). `agent.rs` has a `# Design
  systems` prompt part beside `# Skills`, `use_skill` reaches either store, and
  `create_design_system` + `draft_design_system` are dispatched — the draft
  resolves its folder through `tools::sandbox::resolve`, so it cannot read outside
  the workspace.
- **Test**: the store round-trip, the first-heading rule, the fallback name, the
  id guard, the shared-table merge, both new tools' class, and the folder refusal.
  `cargo test` 275 passed / 0 failed (up from 267).

### 1.4 Every attachment is filed in the chat's own folder ✅
- **Was**: images were never filed (the image branch returned a `data:` URL and
  exited before any copy); a small text file picked with 📎 was not filed while the
  *same* file dropped was; a picked file over 32 MB was added as a bare name with
  no copy and no word said.
- **Now**: one rule in one place — `src/lib/filing.ts` (`routeForAttachment` →
  `image | text | file | refuse`, `refusalMessage`, `landsInChatFolder`) — with
  `toAttachment` routing on it. `attach_path` files a dropped image;
  `attached_text` returns nothing for an image so the model is never told to
  `read_file` a picture it has just been shown. `filesToAttachments` was deleted:
  its `Promise.all` could not report one bad file without losing the batch, so
  `App.tsx` loops per file.
- **Test**: `filing.check.ts` (the 9th check script) asserts the routing table and
  the filing invariant; a Rust test asserts an image is never described as a file
  to open.

### 1.5 Put `rename_chat` in the UI ✅
- **Was**: implemented in Rust, exposed in `api.ts`, called by nothing.
- **Now**: *Rename* in a conversation's right-click menu, editing the title in
  place in the sidebar (Enter saves, Escape abandons, blur commits once). The
  state lives in `App.tsx` because the menu is at the root; the `Sidebar` rows
  only render the editor.
- **Root cause found while doing it**: `db::rename_chat` bumped `updated_at`, and
  `list_chats` orders by it — so renaming would have shuffled the row to the top.
  The bump is gone, matching the rule `set_chat_archived` and `set_chat_folder`
  already followed, with a test that says so
  (`renaming_a_chat_does_not_reorder_the_sidebar`).

### 1.6 Clear the known-debt list ✅
- **MCP image content** — a tool result reaches the model as text, so a picture a
  server returns genuinely cannot be shown. The block now says exactly that
  (`[image/png image returned but not shown — a tool result is text-only]`)
  instead of a placeholder that read like the picture was on its way.
- **Children surviving a hard crash** — every process the app starts now joins a
  Windows job object created with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`
  (`src-tauri/src/tools/job.rs`). `RunEvent::Exit` covers the polite paths; this
  covers the app being killed, because the kernel closes the handle with the
  process. Both `run_process` children and the MCP servers (by pid, since `rmcp`
  owns that handle) are adopted. A test asserts the child really is in the job.
  The job is best-effort: if it cannot be created, the old behaviour stands.
- **Rewind does not cover shell-command writes** — the gap is now said *before*
  the command runs rather than discovered afterwards: a `run_command` card carries
  a note that `/rewind` cannot put back files the command changes. A whole-run
  workspace checkpoint was out of scope, and still is.
- **File-tree filter only searches what is open** — **already honest**: the box
  says *Filter what is open…*, its tooltip says nothing is read from disk, and
  matching nothing reports that you should open the folder you want searched. No
  code change; the real fix is 4.2.
- **Scheduled run parks on a never-auto-approved tool** — a run the user did not
  start can still wait for an approval, but it no longer waits in silence: the
  moment it parks it raises a notification and marks the chat. Same for a question
  it asks. The full queue that lets it be answered from the *Scheduled* panel is
  2.8.
- **Per-hunk diff accept/reject** — still not doing it; see *Explicitly not
  doing*.

### 1.7 Doc drift ✅
- The check-script list is **9** scripts now (shell, transcript, filing, tasks,
  artifact, docs, shortcuts, commands, theme) and the checklist says so.
- The unreleased 0.4.0 features (document extraction, per-chat folders) **are**
  described in the README where a reader would look for them. There is no
  version-history section in the README to add to; inventing one here would
  duplicate 6.1, which is the honest home for release notes.

### Found and fixed while gating Phase 1
- `tools::desktop::tests::actions_are_spaced_so_a_loop_cannot_run_away` was flaky
  under load: it measured from *after* the first action, so the interval it meant
  to assert was subtracted from its own measurement. One line moved. A flaky gate
  makes every later verification unreliable.
- `multi_replace_string_in_file` reported success while mangling two JSX blocks.
  Only `tsc` caught it. **Read the region back after a multi-file replace.**

---

## Phase 2 — Capability (make it do more)

Ordered by how often the author would actually reach for it.

### 2.1 [L] Usage and cost history, and a spend view ✅ done
- **Why**: `chats.cost_usd` is a running total with no dates; the live usage is
  only ever the last turn. Nothing could answer "what did this cost me this
  month".
- **Now**: `turn_usage`, one row per **turn**, keyed by `UNIQUE(run_id)`.
  `db::record_turn_usage` *upserts* after each round, called right next to
  `add_chat_cost` under the same condition — so a run that is stopped or errors
  still counts for what it really spent, and the history and the chat's total are
  two readings of one number rather than two stories. `db::spend_report(since)`
  groups it by local day, by model and by chat (joined to `chats` for the title);
  the command caps the window at 365 days and names the ten dearest
  conversations. `SpendPanel` (sidebar, `/spend`, palette) shows the total, a bar
  per day, then by model and by conversation — no chart dependency, the bar is a
  div.
- **Test**: `every_round_of_a_turn_lands_in_one_row` (three rounds, one row, three
  rounds recorded), `spend_is_grouped_by_day_model_and_chat` (rows placed days
  apart so the local-day boundary never enters into it, comparing exact binary
  fractions), and `an_older_database_gains_the_usage_table` on a **real temp
  file** — the new table arrives from the schema batch's `CREATE TABLE IF NOT
  EXISTS`, and that batch is all-or-nothing, so it is worth proving it still
  applies to a database written before the table existed.
- **Stop**: cost per day is readable for the last 30 days and matches the summed
  chat totals. ✅

### 2.2 [M] Search past conversations from the chat ✅ done
- **Why**: `messages_fts` already indexed everything and the sidebar showed
  hits, but the *model* could not look. "What did we decide about X" was a guess
  or a re-ask.
- **Now**: a `search_history` **Read** tool, so it is offered in both sections
  and in Plan mode, and never prompts. `db::search_messages_scoped` is the same
  query as the sidebar's with two optional filters, and the unrestricted
  `search_messages` became a wrapper around it — one copy of the SQL, so the two
  cannot drift. The scope comes from the chat: a coding chat reads its own
  `workspace` folder (which is also what a project is, since a project's chats
  share its folder) and filters to that section; a chat-mode conversation, having
  no folder to draw a line at, reads that section. Term quoting is `fts_query`'s,
  already hostile-input-proof.
- **Test**: `a_scoped_search_cannot_quote_another_workspace` — three chats across
  two folders and a chat-mode one, asserting the unrestricted search finds all
  three while each scope finds exactly its own; plus the hostile-input cases
  (`*`, `-`, `:`, `""`, a bare `(a)`) through the scoped path too.
- **Stop**: asking about a past decision returns the message and its chat. ✅

### 2.3 [M] Prompt library (saved prompts)
- **Why**: the same requests get retyped. Skills are for *rules*; a prompt is a
  *one-shot text*.
- **Do**: one markdown file per prompt (the skills precedent), surfaced in the
  composer's slash menu as `/p:<name>`. No template language.
- **Test**: a check script for the `name → slug` rule, reusing `docs::slug_with`.
- **Stop**: a saved prompt inserts into the composer and is reachable by `/p:` and
  the palette.

### 2.4 [M] Message branching (edit without destroying)
- **Why**: `resend_last_turn` deletes the tail — correct for "replace", wrong for
  "try the other answer too".
- **Do** (the cheap version): when the tail is deleted, keep it in a discarded
  pile and offer **Restore** on the message it replaced. No `parent_id`, no tree.
  The tail is *moved*, never filtered, so tool calls stay paired with results.
- **Test**: deleting the tail keeps the tool-call pairing intact.
- **Stop**: a previous answer can be brought back without a re-run.

### 2.5 [S] Duplicate a conversation, and export/import it
- **Why**: `export_chat` writes markdown; there is no way to fork a conversation
  or move one between machines.
- **Do**: `duplicate_chat` (messages + attachments folder copy, new folder) and a
  JSON export/import reusing the settings export shape.
- **Test**: a duplicated chat has independent messages, folder and cost.
- **Stop**: duplication from the context menu; the original is untouched.

### 2.6 [L] Let the agent analyse data, not just describe it
- **Why**: the sandbox (Docker/WSL rungs) and `sheet_create` exist, but there is
  no route from "a spreadsheet in this chat" to "a chart".
- **Do**: a chart artifact kind (reuse `ArtifactPanel`), plus a documented recipe
  that runs the arithmetic in the sandbox rather than in the model's head. No
  plotting dependency on the Rust side — render in the artifact frame.
- **Test**: an artifact check for the chart contract; a fixture spreadsheet with a
  known total.
- **Stop**: "sum this column and chart it" produces both a number and a picture.

### 2.7 [M] Projects grow up
- **Why**: a project is a name + folder + membership. It has no instructions and
  no view of its own memory, though chat memory is already filed per project.
- **Do**: per-project instructions (the existing scoped-instruction machinery), a
  project memory view, and a project-level cost total.
- **Stop**: a project's instructions apply to every chat in it without touching
  each chat.

### 2.8 [M] Scheduled tasks that can reach a decision
- **Why**: 1.6 made the wait visible; it is still a wait. The user has to open the
  chat to answer.
- **Do**: a queue — the run pauses, records what it wanted, notifies, and resumes
  when answered from the *Scheduled* panel rather than in a chat nobody is looking
  at.
- **Test**: a run that hits a parked approval leaves a durable "needs you" row.
- **Stop**: nothing parks invisibly *and* everything parked can be answered from
  one place.

---

## Phase 3 — Trust and robustness

### 3.1 [S] React error boundary and a crash surface
- **Why**: the app has no boundary; a render throw is a blank window with a lost
  draft.
- **Do**: one boundary at the app root that keeps the composer draft, shows what
  failed, and offers *Reload*. Reuse the existing error copy voice.
- **Test**: a temporary throw behind a dev flag, then an E2E checklist line.
- **Stop**: a thrown component leaves a readable screen and an intact draft.

### 3.2 [M] Interrupted-run recovery
- **Why**: a crash or a kill mid-run leaves a chat with a half-written turn and no
  marker.
- **Do**: on open, mark runs with no completion; offer *Retry* / *Rewind* for that
  run. Reuses `runs.rs` and `file_checkpoints`, no new table.
- **Test**: a DB fixture with an unfinished run is detected on open.
- **Stop**: after a hard kill, reopening says what was in flight.

### 3.3 [M] Back up and restore the whole profile
- **Why**: `backup_config` covers settings + `mcp.json` only (and deliberately
  excludes the API key). The database, memory files, skills and attachments are
  the actual value.
- **Do**: one archive — DB, `MEMORY.md` and `memory/`, skills, design systems,
  settings, `mcp.json` — with a documented restore. The API key stays out.
- **Test**: a round trip into a temp dir restores a chat, its messages and an
  attachment.
- **Stop**: a fresh profile can be restored from one file.

### 3.4 [S] Maintenance the database needs
- **Why**: `file_checkpoints` grows for ever and artifact versions are never
  pruned.
- **Do**: a retention rule (checkpoints older than N runs, unattached artifact
  versions) and a `VACUUM` after a large delete. Show the DB size somewhere
  honest.
- **Test**: the prune keeps the newest pre-image per run (the `UNIQUE(run_id,
  path)` guarantee).
- **Stop**: size is stable across a week of normal use.

### 3.5 [S] Guard and approval audit trail
- **Why**: "what have I allowed" exists (`db::all_allowed_tools`) but individual
  decisions vanish.
- **Do**: an append-only line per approval decision (tool, args digest, answer,
  chat). No new prompt surface.
- **Test**: the classifier's own tests extended with the catastrophic cases from
  the README.
- **Stop**: a decision can be looked up after the fact.

### 3.6 [M] Network resilience
- **Why**: a dropped connection mid-stream is indistinguishable from the model
  going quiet.
- **Do**: distinguish transport failure from a model error, retry with backoff on
  transient statuses, and say which it was. Keep the partial answer.
- **Stop**: pulling the network mid-run produces one clear message and no lost
  text.

---

## Phase 4 — Performance and scale

### 4.1 [M] Transcript rendering for long conversations
- **Why**: chat view maps every message; a 500-turn chat pays for all of it on
  every delta.
- **Do**: virtualise or window the transcript, and batch stream deltas on
  `requestAnimationFrame` instead of one render per token.
- **Test**: a fabricated 500-message transcript renders and scrolls; measure
  before/after with the same fixture. Move reshape logic into a pure module so it
  can be asserted.
- **Stop**: a long chat streams smoothly and does not re-render the whole list per
  delta.

### 4.2 [M] Real search in the file tree
- **Why**: the box filters what has been loaded, which is not what it looks like
  it does (1.6 made the wording honest; this makes the feature real).
- **Do**: a bounded recursive **name** search on the backend (`list_tree` already
  owns skipping), with the existing `MAX_ENTRIES` discipline. Content search stays
  `grep`.
- **Stop**: a file three levels down is findable without opening its parents.

### 4.3 [S] Startup time
- **Why**: MCP discovery, DB open and the file watcher all start on launch.
- **Do**: measure first. The MCP status badge already proves discovery is async;
  make sure nothing new blocks the first paint.
- **Stop**: a cold start with three MCP servers is measured and recorded here.

### 4.4 [S] FTS search quality
- **Why**: results are shown without ranking or paging.
- **Do**: rank by bm25, page the results, highlight matched words in the snippet.
- **Stop**: a common word does not return 400 rows with the newest first.

---

## Phase 5 — Polish and craft

### 5.1 [M] Motion pass
- **Why**: the app is careful about state and plain about motion; a few
  transitions would carry information (panel enter/exit, step group open, streaming
  caret).
- **Do**: purposeful motion only, respecting `prefers-reduced-motion` — the caret
  rule already shows the trap: a blanket `animation: none` parked it on the
  invisible half of its blink.
- **Stop**: every animation either explains a state change or is removed.

### 5.2 [M] Keyboard completeness
- **Why**: the shortcut map is pure and tested, so gaps are cheap to close:
  rename (F2 — the menu item 1.5 added), jump-to-message, tree navigation, palette
  scopes.
- **Do**: extend `shortcuts.ts` + `shortcuts.check.ts`; every new chord reaches a
  state change a button already makes.
- **Stop**: the shortcut sheet lists only chords that work, and all of them do.

### 5.3 [M] Accessibility sweep
- **Why**: the palette is a real combobox and there is focus styling, but the
  tree, the transcript and the panels have not been through a pass.
- **Do**: roles and labels for the tree and transcript, focus order, contrast
  against the existing 168-pair `theme.check`, and a screen-reader sanity pass.
  Treat the token ramp as the source of truth.
- **Stop**: the whole app is usable without a mouse, and the contrast check still
  passes.

### 5.4 [S] Empty states and error copy
- **Why**: the app's voice is good where it exists; the gaps are the states nobody
  looks at (no results, no projects, failed tool, no API key).
- **Stop**: every empty surface says what to do next.

---

## Phase 6 — Release and docs housekeeping

### 6.1 [S] CHANGELOG.md
- **Why**: the release notes are reconstructed by hand from `git log` each time,
  and 1.7 found there is nowhere in the README for them.
- **Do**: one file, newest first, one line per user-visible change; the release
  step copies it.
- **Stop**: cutting a release reads from the file, not from memory.

### 6.2 [S] CI on push, not only on tag
- **Why**: the gate currently runs when a release is tagged; a broken push is
  found late.
- **Do**: the same gate (`typecheck`, `check`, `build`, `cargo test`) on push to
  `main`, with the existing Node floor. No matrix here — the release workflow
  already builds three OSes.
- **Stop**: a red push is known within minutes.

### 6.3 [S] ARCHITECTURE.md
- **Why**: the repo's real knowledge lives in files like `design.rs` module docs
  and the `db.rs` comments; there is no single map.
- **Do**: a short orientation doc — the layers, the one-owner rules, where the
  events flow. Link, do not duplicate.
- **Stop**: a new session can orient without grepping.

### 6.4 [S] Refresh the E2E checklist to match the app
- Beyond what 1.6 added (attachment filing, design systems from a conversation,
  rename, the job object, parked scheduled runs): a smoking-out step for the new
  panels.

---

## Explicitly not doing (and why)

- **Auto-update / code signing** — audience is one person; the release workflow
  already produces installers. Revisit only if the app is shared.
- **Onboarding tour** — same reason; the `/setup` screen already does the job.
- **Cloud sync of conversations** — the value is local-first; backups (3.3) cover
  the real need.
- **A second file format for design systems or prompts** — both reuse the
  skills/markdown precedent or they wait.
- **Embeddings for memory recall** — the README already names the trade honestly
  (a model download and network at query time). Revisit only if keyword recall
  actually fails in use.
- **Per-hunk diff accept/reject** — per-run rewind is the honest unit; per-hunk is
  a large feature with a small audience.
- **Cross-platform feature parity** — Windows-first was chosen; macOS/Linux keep
  building.
- **A migration for design systems** — decided against: the new store is read and
  the old folder is not, so there is nothing to move. A stale
  `skills/design-*/SKILL.md` is inert, not broken.
- **A whole-workspace checkpoint for `/rewind`** — a shell command's writes are
  named on the card before it runs (1.6) instead of being undone.
