# DeskSeek roadmap

A phased plan for this repo, written 2026-10-12 and updated as phases land.
Phases 1–3 are **done and pushed**: Phase 1 (`b437c6e`), Phase 2 (2.1–2.9) and
Phase 3 (3.1–3.6). **Phases 4–7** (4.1–4.4, 5.1–5.4, 6.1–6.4, 7.1) are done too, and
pushed: `7eb16c2` and then `e85c6f8`, both ancestors of the `v0.4.0` tag
(`3cd175e`, which is `origin/main`). 5.2's file tree keys landed with 5.3, because
the tree's roles and its keys are one edit to the same recursion. **Phase 8
(8.1–8.13) is done and committed, still local, and unreleased**: its items are
below, and the analysis behind them (what Claude, ChatGPT and Gemini each do, and
which of it is worth having here) is
[`ReverseEngineering.md`](ReverseEngineering.md). **Phase 9 (9.1–9.6)** is done and
committed too, still local and unreleased with Phase 8's, and so is **Phase 10 (10.1, 10.2,
10.4 and 9.2)** and **Phase 11 (7.2, 7.3 and 3.3)** — four phases and thirty commits sitting on
top of `origin/main`, and nothing in any of them has been in a real window yet. Phase 11 ends with
one small fix commit of its own: the git check had been spawning `git` directly, which on Windows
flashes a console window — every interval, forever — and leaves a child behind on a hard kill, so it
was moved onto `exec::capture`, the one recipe this app has for running a program. **Phase 12
(2.1–2.3, 13.2, 9.1, 9.5, 8.5, 10.3)** is the widest-blast-radius phase in the whole document — the
two wire dialects and then everything they unblock — and it is the one the 2026-10-13 decision was
recorded for. Phase 10's other
archive item, **9.4**, is deliberately not here: it is "V1 once 6.3 exists", and 6.3 — `recorder.rs`,
reached through 6.2's plugin bundles — does not. One commit in Phase 11 covers two items, **7.3 and
3.3 together**, because they are the same enum, the same form and the same panel row: splitting them
would have meant hand-reverting a file twice, and a commit that does not build is a worse break of the
one-item rule than a commit that names two items. **Phase 13 (4.1, 5.4, 8.2, 8.3, 8.4)** is written up below and is done: the archive's last phase, and
its sixth item, **5.3**, is deliberately left unbuilt with the reason given in its own block. The next
release is **0.5.0**, and `CHANGELOG.md`'s top section is what its tag will print.

What is *not* covered by those ✓s: the E2E checklist has not been walked end to
end in a real window since Phase 1, so the newer lines (the crash screen and its
draft, the interrupted-run banner, backup/restore, the database sweep, the
decision log, a pulled network) are written down and armed but unrun — and Phase 9
added seven more of the same kind: a fold's note, an `Always allow cargo` that then
does not ask again, a chat set to *No network* refusing a fetch, a delete still
prompting in *Skip*, the *By what spent it* rows, and an *Auto* run whose reviewer
cannot be reached. Only a window can confirm them; none is reachable from
`cargo test`. Phase 10 added twenty-three more, and they are the least reachable of
the lot: a global chord pressed from inside another application, a window that is
photographed, a tray whose left click brings the window back, and a browser whose
console and network log only exist while a real page is running. Phase 11 added
thirteen, and they are the least reachable yet: a folder that has to be written to
while the app watches it, a page that has to change, and a server that has to push
something it almost certainly never pushes. Phase 12 added a handful of its own, and Phase 13 seventeen — which are the ones a window is most likely to *change* rather than confirm, because a notebook shell is new UI and three of its parts can only be settled by hand: the ask loop actually reaching `source_search` against a live model, a digest written with no endpoint configured at all, and an `.xlsx` opened in Excel with its citation column intact.

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

## Decisions taken (2026-10-13)

Recorded when Phase 8 was planned, from [`ReverseEngineering.md`](ReverseEngineering.md):

1. **Phase 8 first, in the order written.** Interop and daily-habit wins. Nothing in the batch
   needs a server, a new subsystem or a new dependency, which is why it is the safest thing in the
   plan to hand to somebody (or something) else.
2. **The model layer will be two wire dialects, not a client per vendor.** `openai-compatible` and
   `anthropic-compatible`, each one endpoint record of
   `{ id, label, dialect, base_url, key_ref, models[] }`. DeepSeek is already OpenAI-compatible, so
   the existing client becomes the first endpoint rather than being replaced, and OpenRouter,
   Google's OpenAI-compatible endpoint and local runtimes (Ollama, LM Studio, vLLM) become
   configuration rather than code. This is **Phase 12** scope, not Phase 8; it is recorded here so
   that no Phase 8 item forecloses it. One consequence is worth writing down now: an endpoint with
   no published rate must record its tokens and report its cost as **unknown** rather than guessing
   a number, or the spend view quietly stops agreeing with the sidebar.
3. **The cloud boundary is parked, not decided.** The binding consequence is negative: nothing in
   Phase 8 may require a server, and nothing may foreclose one. Concretely, keep task, queue and
   session state in the database rather than only in memory, and keep identifiers stable.

---

## Phase 1 — Close the holes ✅ done and pushed

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
  duplicate 6.1, which is the honest home for release notes. *6.1 has since built
  that home: `CHANGELOG.md`, with 0.4.0's section already at the top.*

### Found and fixed while gating Phase 1
- `tools::desktop::tests::actions_are_spaced_so_a_loop_cannot_run_away` was flaky
  under load: it measured from *after* the first action, so the interval it meant
  to assert was subtracted from its own measurement. One line moved. A flaky gate
  makes every later verification unreliable.
- `multi_replace_string_in_file` reported success while mangling two JSX blocks.
  Only `tsc` caught it. **Read the region back after a multi-file replace.**

---

## Phase 2 — Capability (make it do more) ✅ done and pushed

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
- **Found in use (2026-10-12), the first time the panel met a real window**: the
  panel and its types were written against `total_usd` / `cost_usd` / `chat_id`,
  but the three structs are `#[serde(rename_all = "camelCase")]` — so every money
  field arrived as `undefined`, and `money()` took the **whole window** down
  formatting it. The TypeScript types were the lie, not the Rust (everything
  else in `types.ts` is camelCase). Both sides are fixed, and
  `the_spend_payload_is_camel_case_on_the_wire` now asserts the *wire* keys, which
  is the check a type can never give. That blank screen is also why 3.1's
  boundary was pulled forward into this batch.
- **Follow-up, asked for 2026-10-12 and deliberately deferred**: the panel shows
  *what* everything cost, not *where the tokens went* — a pie of input against
  output, split by coding and chatting. See **2.9**.

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

### 2.3 [M] Prompt library (saved prompts) ✅ done
- **Why**: the same requests get retyped. A skill is a *rule* the model follows and
  costs catalogue space; a prompt is a *one-shot text* the user wants in the box.
  Sharing a folder would blur the only distinction that matters.
- **Now**: `prompts/<slug>.md`, one markdown file each, with the first `# `
  heading as the name. **`design.rs`'s store became the shared one**
  (`store/files.rs`): a `Store` is a prefix + a noun + a slug fallback over the
  same rules — the file format, the id from the name, and the guard that stops an
  id walking out of its folder. `design.rs` keeps its public API and delegates;
  `prompts.rs` is the same shape with an empty prefix. One copy of a path guard
  rather than two is the point: a second copy is how guards rot.
- **Use**: `/p:<id>` in the composer's slash menu — appended to the menu at the
  call site, *not* into `COMMANDS`, which Settings displays — inserts the body
  rather than sending it (a prompt is text you edit first). `Ctrl+K` does the
  same through an `insert` prop on `ChatView` carrying a counter, so picking the
  same prompt twice lands twice. `/prompts` opens the hub's new section.
- **Test**: `prompts.rs` — the bare-slug id rule, a round trip through a file, a
  stray `.txt` in the folder not counting, and the id guard with an *empty*
  prefix (where the separator rule is the only guard left, which is exactly why
  the prefix was never what protected us). `store/files.rs` — the name→id and
  fallback rules, the newline-in-a-name collapse, first-heading-is-the-name,
  no-heading-falls-back-to-the-id, and the empty-prefix guard.
  **The plan asked for a check script for the name → slug rule; that was wrong** —
  the rule lives in Rust, so a node script could only test a second copy of it,
  which is the duplication this item exists to remove. A Rust test is the honest
  version.
- **Stop**: a saved prompt inserts into the composer and is reachable by `/p:` and
  the palette. ✅

### 2.4 [M] Message branching (edit without destroying) ✅ done
- **Why**: `resend_last_turn` deletes the tail — right for "replace", wrong for
  "try the other answer too", which cost the first answer with no way back.
- **Now**: the tail is *moved* into `discarded_tails` (one row per turn, keyed
  `UNIQUE(chat_id, after_seq)`, holding the rows whole as JSON) instead of being
  deleted, and a ↺ on that turn swaps the two answers. `restore_tail` writes the
  current tail into the pile and puts the stored one back, in one transaction —
  which is what makes the button a **toggle**, and also what makes the seq
  arithmetic safe: everything above `after_seq` is cleared before the stored rows
  go back, so their original seqs are free. No `parent_id`, no tree.
  `MessageRow.discarded` is computed on read (`EXISTS` over the pile) rather than
  stored, so the flag cannot come to disagree with the pile it describes.
- **Test**: `a_resend_moves_the_tail_aside_and_can_rewrite_the_turn`,
  `a_replaced_answer_is_swapped_back_not_rerun` (there and back, plus an error for
  a turn with nothing behind it), and `a_tail_moves_with_its_tool_results_intact`
  — the invariant that has bitten this area twice, asserted at three points: in
  the conversation, after being discarded, and after being restored.
- **Stop**: a previous answer can be brought back without a re-run. ✅
- **Honest limit**: the pile holds one answer per turn, so this is a toggle rather
  than a branching tree. Real branching is still worth doing *later* if this
  proves too small — which is what the plan said when it chose the cheap version.

### 2.5 [S] Duplicate a conversation, and export/import it ✅ done
- **Why**: `export_chat` wrote markdown and there was no way to fork a
  conversation or move one between machines.
- **Now**: `db::duplicate_chat` — a new row with the same settings, the same
  per-chat switches, and its own copies of the messages, inserted through
  `insert_message` so they are numbered by the one thing that decides what a seq
  means. Not copied on purpose: `cost_usd` (nothing was paid for the copy, and a
  second chat claiming it would double-count in the *Spend* view) and `folder`
  (the copy gets its own). `commands::duplicate_chat` then copies the folder —
  **chat mode only**, because a coding chat's folder is the project's and
  duplicating a conversation is not a reason to copy a client's working tree.
  The JSON half is a versioned `ChatBundle { version, chat, messages }`, written
  beside the chat's own files and refused on import when the version is newer
  than this build understands. Import is additive and always creates a new chat.
- **Test**: `a_duplicate_is_its_own_conversation` — new id, title `(copy)`, same
  model, **cost 0 against the original's 1.5**, no folder of its own yet, skills
  carried over, the transcript re-keyed with the tool-call pairing intact, and an
  edit to the copy leaving the original alone; plus `None` for a chat that is gone.
- **Stop**: duplication from the context menu; the original is untouched. ✅
- **Honest limit, stated in the README**: the JSON is the transcript, not the
  conversation. Attachments are referenced by name and stay files in the folder
  the export came from — a bundle that swallowed them would be a container, and a
  half-copy that pretends otherwise is worse than saying so.

### 2.6 [L] Let the agent analyse data, not just describe it ✅ done
- **Why**: the sandbox (Docker/WSL rungs) and `sheet_create` existed, but there was
  no route from "a spreadsheet in this chat" to "a chart". The model could *say*
  what a column summed to and had no way to show it.
- **Now**: a sixth artifact kind, `chart`, whose content is **data, not markup** —
  `{"type":"bar"|"line","title":…,"labels":[…],"series":[{"name":…,"values":[…]}]}` —
  which `src/lib/chart.ts` turns into the SVG: bars or a line, grouped series, up
  to 8 series and 200 points, ticks on 1/2/5×10ⁿ so the axis is readable, and the
  data always inside the frame. The panel draws it through the **existing** `svg`
  path, so the sandbox, the kind label and the white background are reused rather
  than re-implemented. `chartSvg` never throws: a bad spec comes back as an
  `{error}` message naming what was sent — the convention `parse_artifact` already
  uses in Rust, because the text is fed back to the model — and the panel shows
  that reason *and* the JSON. Mermaid's failure block was folded into the same
  `CouldNotDraw`, so there is one copy of "say why, then show the source".
- **Why data and not SVG**: a model asked to hand-write a plot produces a picture
  that is subtly wrong and impossible to check; and the check that matters — does
  a bar's height match its value — cannot be written against a library's output
  either. A spec is arithmetic, so `chart.check.ts` *can* pin the mapping, the
  escaping and the failure messages. No charting dependency was added, on either
  side: the plan's "no plotting dependency on the Rust side" holds trivially, and
  the frontend did not want one either.
- **Maths, the other half of the request**: LaTeX between `$…$` and `$$…$$` now
  renders in the chat and in a `markdown` artifact — `katex` + `remark-math` +
  `rehype-katex`, loaded the first time an answer contains a `$` (the same lazy
  pattern `rehype-highlight` already used for a fenced block) with
  `throwOnError: false`, so one malformed formula is drawn in red instead of
  taking the answer down. There is still no error boundary (3.1), which is why
  that mattered. **KaTeX is pinned to `^0.16` on purpose**: `rehype-katex`
  depends on `katex@^0.16`, so letting 0.19 in would have meant two copies and
  markup drawn by one against the CSS of the other.
- **Telling the model, which is the actual feature**: `chart` is in
  `ARTIFACT_KINDS` and in the tool's own description, and the
  `# Writing something to keep` prompt section now names it with the spec, says
  the panel draws maths, and carries the recipe the plan asked for — *total the
  numbers in a command, and chart what you computed.* A capability the model is
  never told about does not exist.
- **Test**: `chart.check.ts` — bar heights ordered by value, one polyline point
  per label, both series of a grouped chart, nothing drawn outside the frame, no
  `NaN` from an all-zero series or from negatives (which must land below the zero
  line), a label containing markup arriving escaped, and every wrong shape — bad
  JSON, a bare value, a missing `type`, a short `values`, a non-number, an empty
  chart, 201 points — producing the message that names it.
  **The plan's "fixture spreadsheet with a known total" is on the E2E checklist,
  not in the gate**: it needs a real sandbox, a real file and a real model, which
  is the class of thing that list exists for.
- **Stop**: "sum this column and chart it" produces both a number and a picture.
- **Honest limits, stated in the README**: the chart is drawn by `chart.ts` rather
  than a chart library, so it is bars and lines on a linear axis — no pie, no
  stacked or log scales, no smoothing. The legend spaces its entries by an
  estimated character width, so a very long series name can overrun the next
  swatch.

### 2.7 [M] Projects grow up ✅
- **Was**: a project was a name, a folder and a membership. It had no rules of
  its own and no view of the memory learned inside it, though that memory was
  already filed per project.
- **Rules reuse the scoped-instruction machinery, which is the point**: a project
  holds a `Vec<Instruction>` in a JSON column (`projects.instructions`), and
  `agent.rs` feeds it through the *same* `push_instructions` helper as the global
  list — switched on, and scoped to this kind of chat or to both. One rule, one
  filter, so a project rule cannot behave differently from a global one. Editing
  a project's rules deliberately does **not** bump `updated_at`, or the list
  (ordered by it) would reshuffle on every keystroke.
- **The memory view reads the file the writer would have written**:
  `project_memory` resolves the project's *folder* through
  `memory_files::project_category` and reads that one file, so the panel cannot
  invent a category the write path would not have used. A project with no folder
  has no shared file — each chat keeps its own — and the panel says so rather
  than showing another project's notes.
- **The cost total is a `project_id` join**: `db::project_spend` groups
  `turn_usage` one level above `spend_report`'s per-chat grouping. All-time, not a
  window, because "what has this project cost me" is a total — and the same rows
  the sidebar's per-chat totals come from, so the two agree.
- **Test**: `a_projects_instructions_round_trip_without_resorting_the_list` (the
  round trip *and* the `updated_at` guard),
  `project_spend_adds_up_a_projects_chats_and_leaves_others_out` (one-off costs,
  so the total is arithmetic that cannot drift), the shared filter's own
  `only_switched_on_instructions_in_scope_reach_the_prompt` (scope respected, and
  every measured part is text that was really appended), and the older upgrade
  test extended with a pre-2.7 `projects` table so the `ALTER` is exercised on a
  real file rather than only on a fresh `CREATE TABLE`.
- **Stop**: a project's rules apply to every chat in it without touching each
  chat — and a chat moved out stops seeing them, which is the point of holding
  them on the project.

### 2.8 [M] Scheduled tasks that can reach a decision ✅
- **Was**: 1.6 made the wait *visible* — a notification and an unread dot — but it
  was still a wait in a chat, and finding that chat was the user's problem.
- **The queue is a table, and the prompt is reused rather than restated**: parking
  writes a `parked_requests` row whose `payload` is *the same JSON that went out as
  the event*. The Scheduled panel renders `ApprovalPrompt` / `QuestionPrompt` from
  it — the components the composer already uses — so the two places cannot describe
  the request differently. `park_request` / `forget_parked` are called by the agent
  at both ends of the park **and** by `respond_approval` / `respond_question`, so
  answering in the chat and answering in the panel are the same act and whichever
  happens second is a quiet no-op.
- **Only unattended runs are written down.** A prompt the user is looking at needs
  no second copy in a panel — so the row is written where `unattended` is already
  true, which is also where the notification is raised.
- **A park does not outlive the process, and says so.** It is an in-memory oneshot,
  so after a restart there is nothing left to resume; startup marks anything still
  `pending` as `lost` and the panel shows it with the reason and a *Dismiss* rather
  than a button that would do nothing. One `UPDATE`, and the alternative — a row
  that silently vanishes, or a button that lies — is worse than either.
- **Test**: `a_parked_request_lists_with_its_chat_and_is_forgettable` (the join that
  supplies the title, and that forgetting twice is quiet),
  `a_park_left_by_a_crash_is_marked_lost_rather_than_deleted`, and
  `deleting_a_chat_takes_its_parked_request_with_it` — which turns on
  `PRAGMA foreign_keys` itself, because `mem()` never does, and the cascade clause
  is otherwise untested. **What the gate cannot reach**: the agent's own park and
  forget call sites need a real `AppState`, so they live on
  `docs/E2E-CHECKLIST.md` rather than in `cargo test`.
- **Stop**: nothing parks invisibly — the notification, the chat's own prompt and the
  panel's list all say so — and everything parked is answerable from one place.

### 2.9 [S] Spend, split: where the tokens went ✅
- **Was**: the spend view answered *how much*, and only in money.
- **The data is one more grouped query, not a new table**: `spend_report` gained
  `sections: Vec<SectionTokens>` — the same `turn_usage` rows grouped by
  `chats.kind` instead of by chat. The panel computes nothing; it draws what the
  report says.
- **The pie lives in `chart.ts`, as the plan hoped.** `type` gained `"pie"`, and
  the legend loop was pulled out into one `legendRows` helper because a pie's
  entries are its slices rather than its series. Two shapes had to be spelled out
  because a path cannot draw either: one non-zero slice is the **whole circle**
  (an arc from a point back to itself is nothing), and all-zero is a **ring**
  rather than an empty frame. `parse` refuses the three pies that mean nothing —
  more than one series, a negative slice, more than `MAX_SLICES` (8) slices — with
  a message naming the reason, because those errors go back to the model.
- **Test, and the one worth having**: `chart.check.ts` asserts a quarter-turn
  geometrically — four equal values, so the first slice must end *due right of the
  centre, level with it*. That single assertion fails if the angles are not the
  values, if the sweep is anticlockwise, or if the radius is not the radius. Plus:
  a single slice is a circle and not a path, an all-zero pie has no slices and no
  `NaN`, a zero-valued slice keeps its `(0.0%)` entry, a label cannot become
  markup, and the pie sits inside the frame. On the Rust side,
  `the_sections_are_a_split_of_the_totals_rather_than_a_second_story` asserts the
  sections **are** the report's own totals and that coding is the input-heavy half
  — which is the whole reason the split is worth showing.
- **Drawing it twice would have been the bug**: `ChartImage` is now the one place
  that knows a chart goes on a white card (the palette is chosen against white),
  and `ToolCallCard` uses it too.
- **Telling the model**: `chart`'s schema description and the
  `# Writing something to keep` prompt section now say `bar`, `line` or `pie`, and
  that a pie is a breakdown — one series, no negatives.
- **Stop**: an input/output split is visible per section, and the slices sum to
  the totals above them.
- **Honest limit, in the README and under the pies**: prompt and completion are
  billed at different rates and cached input at a fraction of fresh, so this is
  where the tokens went rather than what each part cost.

---

## Phase 3 — Trust and robustness ✅ done and pushed

### 3.1 [S] React error boundary and a crash surface ✅
- **Why**: the app has no boundary; a render throw is a blank window with a lost
  draft.
- **Do**: one boundary at the app root that keeps the composer draft, shows what
  failed, and offers *Reload*. Reuse the existing error copy voice.
- **Test**: a temporary throw behind a dev flag, then an E2E checklist line.
- **Stop**: a thrown component leaves a readable screen and an intact draft.
- **Pulled forward and now complete (2026-10-12)**: 2.1's panel blanked the window
  the first time it was opened, so the boundary could not wait. `ErrorBoundary`
  wraps `<App/>` in `main.tsx`: the message, React's component stack, *Try again*
  (a re-render), *Reload the window*, and copy.
- **The draft half is now done.** A half-typed message is mirrored into
  `sessionStorage` by `src/lib/draft.ts` (`loadDraft`/`saveDraft`, tolerant of an
  unavailable store, keyed per chat so two panes cannot overwrite each other) and
  `ChatView` seeds its composer from it — so the draft comes back after *Try
  again* and after a reload, which is the one piece of state the boundary cannot
  preserve because it lives in the tree the boundary replaces.
- **The screen is now forcible**: App throws once if `deskseek.crash` is set in
  `localStorage`, self-clearing so *Try again* recovers. That is the plan's "dev
  flag", and `docs/E2E-CHECKLIST.md` walks it (crash → screen → *Try again* →
  draft intact).

### 3.2 [M] Interrupted-run recovery ✅
- **Why**: a crash or a kill mid-run leaves a chat with a half-written turn and no
  marker.
- **Do**: on open, mark runs with no completion; offer *Retry* / *Rewind* for that
  run. Reuses `runs.rs` and `file_checkpoints`, no new table.
- **Test**: a DB fixture with an unfinished run is detected on open.
- **Stop**: after a hard kill, reopening says what was in flight.
- **Done (2026-10-12).** The plan's "no new table" did not survive contact: a
  marker column on `chats` would have to be threaded through every chat SELECT,
  `ChatRow`, the wire payload and `duplicate_chat` (where a copy would inherit the
  marker and look interrupted) — more surface than it is worth. The marker is its
  own tiny table instead: **`in_flight_runs (chat_id PRIMARY KEY, run_id,
  started_at)`**, one row per chat, matching `runs.rs`'s "one live run per chat".
  A new table needs no `ALTER`, so nothing else had to change.
- **Written and cleared in one place**: `commands::run_turn` wraps
  `runs.begin` → `db::mark_run_started` → `agent::run` → `runs.finish` →
  `db::mark_run_ended`, and both run sites (`send_message`/`start_turn` and
  `resend_last_turn`) go through it. Every ending clears the marker — finished,
  stopped, errored — so only a process that *died* leaves one behind. To make the
  marker's `run_id` the same id the checkpoints and `turn_usage` use,
  `agent::run` now takes it from the caller instead of minting its own.
- **`interrupted_runs()`** returns what is left, newest first, with `canRewind`
  (does this exact run have pre-images). Read once at startup, which is the only
  moment it is meaningful: while a run is live the marker is *supposed* to be
  there.
- **UI**: `ChatView` shows an amber banner for an interrupted chat — *Ask again*
  (which is also the repair: the history ends on a tool call with no result, and
  `resend_last_turn` drops that tail before it re-runs, so the chat is usable
  again), *Put its files back* only when `canRewind`, and *Dismiss*. The banner is
  re-read from the backend after an action rather than assumed away, so a repair
  that failed to start leaves the banner up instead of lying.
- **Tests**: the marker round trip (started → visible, ended → gone), `canRewind`
  keyed to the run rather than the chat, and a second run replacing the first
  marker instead of stacking.

### 3.3 [M] Back up and restore the whole profile ✅
- **Why**: `backup_config` covers settings + `mcp.json` only (and deliberately
  excludes the API key). The database, memory files, skills and attachments are
  the actual value.
- **Do**: one archive — DB, `MEMORY.md` and `memory/`, skills, design systems,
  settings, `mcp.json` — with a documented restore. The API key stays out.
- **Test**: a round trip into a temp dir restores a chat, its messages and an
  attachment.
- **Stop**: a fresh profile can be restored from one file.
- **Done (2026-10-12).** `src-tauri/src/backup.rs` owns it: `ITEMS` is the list
  of what a profile *is*, `write` zips them, `extract` unpacks into a scratch
  folder, and `replace_items` puts that over the live root. One zip replaces the
  old one JSON file. No new dependency — `zip` 8 with `deflate` was already in
  the tree for reading `.docx`/`.xlsx`, which is the whole requirement for a
  container of DEFLATE'd files.
  - Carried: `deskseek.db`, `MEMORY.md`, `memory/`, `skills/`, `design/`,
    `prompts/`, `documents/`, `CHATLOG/`, `settings.json`, `mcp.json`. Not
    carried: `browser/` (the automation profile — re-creatable, and never the
    user's own data) and the API key (the keychain, as before).
  - **The database is the awkward part**, and it is why this is a module rather
    than ten more lines in `commands.rs`: it is held open for the life of the
    app, so it can neither be copied accurately nor overwritten in place without
    care. `db::checkpoint` flushes the WAL before the *write* (in WAL mode the
    newest commits are in `deskseek.db-wal`, exactly what a backup is for), and
    `db::swap_file` parks the connection on an in-memory database to release the
    handle, removes the stale `-wal`/`-shm` sidecars, copies the file in, and
    reopens. Keeping that SQLite knowledge in `db.rs` rather than leaking
    `rusqlite` into `commands.rs` is the "one file owns each rule" line.
  - **All-or-nothing**: the archive is unpacked into `restore-<uuid>/` first, so
    a truncated or foreign file leaves the profile untouched. `extract` checks
    every entry with `enclosed_name()`, because an archive arrives from elsewhere
    and a hand-made one must not write across the disk.
  - `backup_config`/`restore_config` became `backup_profile`/`restore_profile`
    (command, `api.ts`, palette labels) — the old names would have been a lie
    about a scope that grew by an order of magnitude.
  - **The window reloads after a restore.** The running UI's chats, transcripts
    and stored shells all belong to the database that was just replaced;
    re-fetching every one of them by hand is the long way to `location.reload()`.
  - **Tests**: a full round trip (a chat with a message, `MEMORY.md`, a skill, a
    filed document, and a `browser/` folder that must *not* survive), a
    replace-rather-than-overlay test, and a hostile `../` entry that must stay
    inside the destination.

### 3.4 [S] Maintenance the database needs ✅
- **Why**: `file_checkpoints` grows for ever and artifact versions are never
  pruned.
- **Do**: a retention rule (checkpoints older than N runs, unattached artifact
  versions) and a `VACUUM` after a large delete. Show the DB size somewhere
  honest.
- **Test**: the prune keeps the newest pre-image per run (the `UNIQUE(run_id,
  path)` guarantee).
- **Stop**: size is stable across a week of normal use.
- **Done (2026-10-12).** `db::prune` in `db.rs` — the SQL belongs where the rest
  of the SQL is, so no new module for four statements.
  - **Pruned by reachability, not age**, which is the honest rule: a rewind can
    only ever walk back *one* run per chat (`rewind_last_run` says so), so the
    older runs' pre-images are unreachable by construction. Keep the newest
    `CHECKPOINT_RUNS_KEPT` (10) runs per chat. Newest is decided by
    `max(created_at) DESC, max(rowid) DESC` **and has to be**: a run's rows all
    land in the same second, so the clock cannot order them and insertion order
    must. The test fires 14 runs in one second on purpose.
  - Artifact versions: newest `ARTIFACT_VERSIONS_KEPT` (20) per artifact, keyed
    per artifact because a version number only means anything beside its own
    siblings. `current_version` is the maximum by construction, so it can never
    be the row pruned.
  - `vacuum_if_worth_it` only rewrites the file past `VACUUM_AFTER_ROWS` (500):
    `VACUUM` costs the whole database, so running it for ten rows would make the
    cure dearer than the disease.
  - `db::size_bytes` reads `PRAGMA page_count * page_size` rather than the file
    on disk — in WAL mode there is a side file too, and pages are what the tables
    occupy.
  - Surfaces: pruned at startup (silently, with a line to stderr when it did
    something), and on demand via `maintain_db` from the palette, which reports
    the size whether or not it pruned anything. `App`'s notice is the honest
    place for the number.
  - **Tests**: 14 runs pruned to 10 with the newest still intact, 30 versions of
    an artifact pruned to 20 with a *second* artifact left alone (proving the
    rule is per artifact, not global), and a prune of nothing being a no-op that
    does not vacuum.

### 3.5 [S] Guard and approval audit trail ✅
- **Why**: "what have I allowed" exists (`db::all_allowed_tools`) but individual
  decisions vanish.
- **Do**: an append-only line per approval decision (tool, args digest, answer,
  chat). No new prompt surface.
- **Test**: the classifier's own tests extended with the catastrophic cases from
  the README.
- **Stop**: a decision can be looked up after the fact.
- **Done (2026-10-12).**
  - **What was actually missing was the *answer*.** Every tool *call* is in the
    transcript already; an approval leaves no trace of its own, and a denial or a
    guard refusal leaves nothing at all. So the log records exactly those:
    `allowed` / `denied` / `blocked`, with the how-or-why in a `note` (`once`,
    `session`, `always`, `stopped`, `denied`, or the guard's reason). An
    auto-approved call that never asked is not logged — the transcript has it,
    with its arguments.
  - `approval_log` is the **one table with no foreign key to `chats`**, and the
    comment says why: an audit trail that a chat deletion can erase is not one.
    The join is a LEFT, so a decision about a deleted chat reads as unnamed. Its
    own `RETURNING`-free `prune` case keeps the newest `DECISIONS_KEPT` (1000) —
    append-only is not a reason to keep a year of it.
  - **Arguments are a digest, not the arguments** (`approvals::args_digest`,
    FNV-1a 64-bit, no dependency): an approval can cover a command carrying a
    secret, and a log you read back later is not where one should live. It still
    tells two `write_file` approvals apart, which is what it is for.
  - Written at both decision points in `agent.rs` — `approve` (with `stopped`
    distinguished from `denied` by whether the cancel actually fired) and the
    `guard::classify` branch of the run loop.
  - Surface: *Settings → Tools* gained a **Recent decisions** list under the
    grants (a hundred, newest first). `Grants` answers what is switched on; this
    answers what happened. `when()` moved to `src/lib/time.ts` so the panel and
    the Scheduled panel format a moment the same way rather than twice.
  - **Tests**: the classifier's own — and better than the plan asked for. Rather
    than adding more sample commands, it walks the `CATASTROPHIC` and `SUSPICIOUS`
    tables themselves and asserts **every needle can actually fire**, which is the
    failure mode a sample never catches: a needle with the wrong spacing is a rule
    that silently never refuses anything. Plus every `UPLOAD_TOOLS × UPLOAD_FLAGS`
    pair, since exfiltration needs both halves to match. On the DB side: the round
    trip and newest-first ordering, the log surviving `delete_chat`, and its own
    ceiling.

### 3.6 [M] Network resilience ✅
- **Why**: a dropped connection mid-stream is indistinguishable from the model
  going quiet.
- **Do**: distinguish transport failure from a model error, retry with backoff on
  transient statuses, and say which it was. Keep the partial answer.
- **Stop**: pulling the network mid-run produces one clear message and no lost
  text.
- **Done (2026-10-12).**
  - `client.rs` gained `Interruption { transport, message, partial }` in place of
    `anyhow::Error` on the streaming path — `anyhow` cannot be matched on, and the
    distinction *is* the feature. `transport: true` for a connection that failed
    or a stream that died; `false` for a request the API refused.
  - **Retry only where a retry is free.** `MAX_ATTEMPTS` (3) with `backoff`
    (1s, 2s), and only for `408`, `429` and 5xx (`is_transient`): those say the
    endpoint was busy or broken, which is worth a second look, while a `400` or a
    `401` is final and retrying it just costs the user time. The retry wraps the
    *send and the status* only — **never the stream**, because replaying a stream
    that already emitted tokens would say everything twice.
  - **The partial answer is kept.** A mid-stream failure returns what the
    accumulator had, and `agent.rs` writes it as an assistant message *before*
    raising the error. Previously the `?` dropped the accumulator and the UI's
    `finally` cleared `stream`, so a dropped connection lost text the user had
    already read. It is inserted when content *or* reasoning arrived, since a
    stream that dies mid-thinking is still text the user was watching.
  - `finishReason` is now `network` or `error`, decided in the backend rather than
    guessed at in the UI. The words name which it was: *"Could not reach DeepSeek
    (3 tries)"* against *"DeepSeek refused the request (401)"*.
  - Frontend: `send` and `resend` now `await loadMessages` in their `catch` — the
    stored partial is what the transcript shows instead of the stream being
    cleared away with an error under it.
  - **Tests**: the retry rule itself (`is_transient` over both sets of statuses),
    the backoff growing, and a stop before any answer keeping nothing.

---

## Phase 4 — Performance and scale ✅ done and pushed (v0.4.0)

### 4.1 [M] Transcript rendering for long conversations ✅
- **Why**: chat view maps every message; a 500-turn chat pays for all of it on
  every delta.
- **Do**: virtualise or window the transcript, and batch stream deltas on
  `requestAnimationFrame` instead of one render per token.
- **Test**: a fabricated 500-message transcript renders and scrolls; measure
  before/after with the same fixture. Move reshape logic into a pure module so it
  can be asserted.
- **Stop**: a long chat streams smoothly and does not re-render the whole list per
  delta.
- **Done (2026-10-12).**
  - **Measured first, with the fixture the plan asked for.** A temporary harness
    rendered a realistic answer (heading, list, fenced block) through
    `react-markdown` + GFM under `renderToStaticMarkup`: **~2.1 ms a row** for a
    300-row transcript, i.e. ~640 ms of Markdown parsing per re-render against a
    16.7 ms frame budget. Every streamed token triggered one of those. That number
    is the whole justification for the three changes below, and it is why the
    window is sized at 60 and not at 300.
  - **Three changes, each removing a different order of magnitude.**
    1. `windowTranscript` (pure, in `transcript.ts`) draws the newest
       `TRANSCRIPT_WINDOW` (60) items with a *Show N earlier messages* line above
       them, so mounting a long chat is bounded work instead of 300 parses.
    2. `Markdown`, `Bubble` and `ToolStepGroup` are `memo`d. A streaming frame
       changes only the live answer, which is drawn outside the transcript, so
       every stored row bails out on the shallow compare — the per-frame parse
       count drops from *all rows* to *zero*.
    3. Deltas are collected in `pendingDeltas` and committed once per
       `requestAnimationFrame`, so a fast stream costs a render per frame rather
       than one per token. The auto-scroll effect keys off `stream.content`, so it
       also went from one `scrollIntoView` per token to one per frame.
  - **Deviations, and why.** The plan offered "virtualise **or** window": windowing
    won, because variable-height rows (a table, a diagram, an open step group) make
    real virtualisation an estimate-and-correct scroll algorithm, and the memo
    above is what actually removes the per-frame cost. A *sliding* window was also
    rejected in favour of a growing one — reveal only ever adds above the reader,
    so the scroll anchor holds and no row can vanish from under the pointer.
  - **The frame flush cannot leak a token into the next turn.** It skips a chat
    whose `stream` is already `null`, which is the state a finished turn leaves —
    so text that arrives after `done` is dropped, not prepended to the next
    answer. Nothing is lost by that: the text is stored by then and the transcript
    is reloaded from the database.
  - **Tests**: `transcript.check.ts` now builds a **500-message** fixture (100
    turns of question → two calls → two results → answer) and asserts the fold
    (300 items), then walks `limit` from 1 to the full length asserting
    `hidden + shown === total`, that the newest item is never dropped, and that
    the shown count is exactly `min(limit, total)` — an off-by-one here loses a
    turn silently. Plus: the default window sits exactly at the cut with no gap,
    a reveal is monotonic and adds *above* the previous first item, and revealing
    everything leaves nothing hidden.

### 4.2 [M] Real search in the file tree ✅
- **Why**: the box filters what has been loaded, which is not what it looks like
  it does (1.6 made the wording honest; this makes the feature real).
- **Do**: a bounded recursive **name** search on the backend (`list_tree` already
  owns skipping), with the existing `MAX_ENTRIES` discipline. Content search stays
  `grep`.
- **Stop**: a file three levels down is findable without opening its parents.
- **Done (2026-10-12).**
  - `walk.rs` gained `find_named(root, needle, cap)`: it runs the existing
    `walk_files` (so `SKIP_DIRS` and `MAX_FILES` are inherited, not re-stated — a
    second copy of "which directories are noise" would be the bug this codebase
    keeps finding), filters on the **name** case-insensitively, ranks, and caps.
  - `commands.rs` gained `find_in_tree`, returning the same `TreeEntry` shape
    `list_tree` does, with paths made relative by the same `sandbox::display`.
    Below two characters it returns nothing rather than walking: one letter
    matches most of a large tree and is never what was meant.
  - The panel's box **is** the search now, so the old behaviour is gone: the
    `keep` set that filtered only the levels already expanded, and the "open the
    folder you want searched" copy, are deleted rather than kept beside it. Two
    behaviours behind one box would be a worse answer than either.
  - **The ranking was wrong, and the test caught it.** `mod.rs` ranked level with
    `module_helper.rs`, because `starts_with` cannot tell an exact name from a
    longer one that begins the same way. Rank now compares the name **without its
    extension** — the stem a person actually types — before falling back to
    prefix and then contains, and orders by path length inside a rank so a
    shallow hit beats the same name ten folders down. That was a real weakness,
    not a test artefact: it is the first thing anyone would type.
  - Frontend: debounced 200 ms, and it abandons a stale response rather than
    letting a slow earlier search overwrite a newer one. A failed search returns
    to the tree rather than showing an empty list — a broken search must not look
    like a search that found nothing.
  - **Tests**: `find_named_reaches_a_file_three_folders_down` (asserts the *depth*,
    and that a match inside `node_modules` stays skipped) and
    `find_named_ranks_an_exact_name_first_and_trims_the_tail` (all three ranks,
    by asserting the exact `PathBuf`s rather than `ends_with`, plus that the cap
    keeps the best hit rather than the first one walked).

### 4.3 [S] Startup time ✅
- **Why**: MCP discovery, DB open and the file watcher all start on launch.
- **Do**: measure first. The MCP status badge already proves discovery is async;
  make sure nothing new blocks the first paint.
- **Stop**: a cold start with three MCP servers is measured and recorded here.
- **Done (2026-10-12). Measured, and nothing needed changing.**
  - Temporary `Instant` marks were put around every step of `setup()` and around
    the MCP load, with `mcp.json` written to hold **three servers that take five
    seconds each to start** (a `cmd /c ping` stand-in: the property under test is
    a long, known start time, which is exactly what a real `npx` server gives
    unpredictably). Debug build, warm cargo, two runs, both this shape:

    | mark | at |
    | --- | --- |
    | Tauri + webview boot, up to `setup()` | **1184 ms** |
    | profile paths ensured | 3 ms |
    | database opened | 33 ms |
    | startup prune | 2 ms |
    | settings + HTTP client | 2 ms |
    | example skills written | 1 ms |
    | rest of `setup()` → window may show | 2 ms |
    | **everything DeskSeek does itself** | **~42 ms** |
    | MCP ready | 5312 ms of work, **6538 ms from process start** |

  - **The window waits for 42 ms of its own work; the other 1.2 s is Tauri and
    WebView2**, which is not ours to remove. `npm run tauri dev` starts the Vite
    dev server first, so a dev run is slower than a packaged one either way.
  - **MCP is off the critical path, and the fixture proves it rather than the
    code reading suggesting it**: three servers of five seconds each were ready
    5.3 s after discovery began — not 15 s — so `restart_all`'s `join_all` really
    is concurrent — and the window was fully usable for all 5.3 s of it. The
    Docker/WSL probe is spawned the same way, and the scheduled ticker's first
    tick is 30 s out.
  - The webview's own first paint could not be read from here: the JS console is
    not forwarded to the dev terminal, and a `Builder::on_page_load` hook did not
    fire for the dev-server URL. That is a gap in the *instrument*, not in the
    app — and it is the one number the marks above do not need, since nothing
    async is on the path to it.
  - All instrumentation was removed: `git diff src-tauri/src/lib.rs` is empty, and
    the temporary `mcp.json` is deleted. **This item ships documentation only.**

### 4.4 [S] FTS search quality ✅
- **Why**: results are shown without ranking or paging.
- **Do**: rank by bm25, page the results, highlight matched words in the snippet.
- **Stop**: a common word does not return 400 rows with the newest first.
- **Done (2026-10-12).**
  - **Two thirds of this was already true, and saying so matters more than
    re-doing it.** `search_messages` already ordered by `bm25(messages_fts)` and
    already asked SQLite's `snippet()` to bracket the matched words — the plan
    was written from the UI, which did neither justice. What was actually
    missing: the list stopped at 40 with no way on, and the snippet's brackets
    were being drawn *as brackets*, so a result read `[pars]er bug`.
  - **Paging.** `search_messages` gained an `offset` and now returns
    `MessagePage { hits, more }`. `more` comes from asking for `PAGE + 1` rows and
    throwing the extra away: its presence *is* the answer to "is there another
    page", which costs one row where a `count(*)` over the match set costs a full
    scan. The sidebar appends the next page under the list it already has, and
    drops a page whose query has since been replaced by a keystroke.
  - **A real bug the paging exposed:** `ORDER BY bm25` is not a total order. Rows
    that score the same are free to come back in any order, so page two could
    repeat a row from page one and drop another for ever. `m.id` is now the
    tiebreak — arbitrary, but *stable*, which is the only property paging needs.
    `pages_of_a_search_join_up_without_gaps_or_repeats` pins it with seven
    identical messages, so every row scores exactly the same and the order can
    only be right if the tiebreak is doing the work.
  - **Highlighting, and the ambiguity that decides it.** Every message is allowed
    to contain brackets — code does, constantly — so the split cannot simply take
    `[` … `]` as a mark. `src/lib/snippet.ts` treats a bracket pair as a mark only
    when the text inside it is one of the query's own terms; anything else is the
    message's punctuation and stays visible. Written as a pure function with its
    own check (`snippet.check.ts`, the eleventh), because the failure mode is
    invisible to `tsc`: a plausible row with the emphasis in the wrong place.
  - **Tests**: the two store tests above, plus eleven snapshot cases in the
    snippet check — `list[0]` left alone, `items[3]` not lit up, an unclosed
    bracket, a query with nothing searchable, and both terms of a two-word query:
    each asserting the parts *and* what they concatenate back to, so nothing can
    be silently dropped.

---

## Phase 5 — Polish and craft ✅ done and pushed (v0.4.0)

### 5.1 [M] Motion pass ✅ done
- **Why**: the app is careful about state and plain about motion; a few
  transitions would carry information (panel enter/exit, step group open, streaming
  caret).
- **Now**: the audit found nothing decorative to take out — the caret was the only
  real animation, and every pulse and chevron already names a state. Two
  *enter-only* transitions were added where something appears out of nothing:
  `.panel-in-left` / `.panel-in-right` on the seven panels that mount and unmount
  (the direction says which edge it came from) and `.reveal` on a step group's
  contents. Enter-only because React unmounts both outright, so there is no exit to
  animate and holding one on screen would need a second copy of the state. Hover
  and chevron transitions were left exactly as they were.
- **Test**: `motion.check.ts`, the **12th** check script, makes the Stop
  mechanical: every distinct animation in the app must be named in `PURPOSEFUL`
  beside the state it explains. It fails on an unjustified animation, on an
  undeclared Tailwind `animate-*`, on an arbitrary `animate-[…]`, on a
  `@keyframes` nothing plays, and on an `animation:` that points at no keyframe. It
  also owns the reduced-motion contract from both ends: the blanket rule must still
  neutralise animation *and* transition durations for the OS preference **and** the
  in-app setting, and the caret must keep its exception in both. Proved by
  falsification — adding `animate-spin` to a scratch component fails the check, and
  removing it passes.
- **Stop**: every animation either explains a state change or is removed. ✅
- **Found while writing the check, in the check itself**: its first two drafts
  searched a rule's *body* for `.caret {`, which a body never contains — so the
  caret exception looked missing for the in-app setting, written as two sibling
  rules rather than one nested block. `rulesFor` now returns the selector with the
  body, and the comment says why. A check that cannot fail is worth nothing, and
  this one earned its place by failing on itself twice.

### 5.2 [M] Keyboard completeness ✅ done
- **Why**: the shortcut map is pure and tested, so gaps are cheap to close:
  rename (F2 — the menu item 1.5 added), jump-to-message, tree navigation, palette
  scopes.
- **Now**: `F2` opens the sidebar's rename editor on the open chat — the same
  state change the context menu's *Rename* makes. Like `?` and `Escape` it is a
  bare key, so it does not fire while a field has focus, which is also what keeps
  it out of the rename box it opens. `Ctrl+G` jumps to the newest
  message, through a counter prop on `ChatView` so the focused pane scrolls without
  waiting for the next delta; `Ctrl+G` rather than `Ctrl+End`/`Ctrl+Down` because a
  modifier chord fires while the composer has focus and those two move its caret.
  The palette gained **scopes**: a leading `>` is the actions, `@` the
  conversations, either marker alone lists its whole scope, and any other marker
  stays part of the query.
- **Landed with 5.3**: the file tree's arrow keys. The tree's roles and its keys
  are one edit to the same recursion in `FilesPanel.tsx`, so doing them as two
  passes would have meant rewriting it twice.
- **Test**: `shortcuts.check.ts` pins both new chords, that `F2` while typing is
  the field's, that `Ctrl+Shift+G` is not ours, and that the advertised table
  covers every action — so the sheet cannot list a chord that does not fire.
  `commands.check.ts` pins the scopes, including that a bare marker lists the whole
  half and that an unknown marker is text.
- **Stop**: the shortcut sheet lists only chords that work, and all of them do. ✅

### 5.3 [M] Accessibility sweep ✅ done
- **Why**: the palette is a real combobox and there is focus styling, but the
  tree, the transcript and the panels have not been through a pass.
- **Now**: four places where the app said nothing to a screen reader or nothing
  visible to a keyboard.
  - **The dialogs.** Customise, Settings, Setup and MCP opened as bare divs. Each
    is now a named `role="dialog"` with `aria-modal="true"`, and the two big ones
    gained the Escape-and-restore-focus pair the shortcut sheet and the palette
    already had. `McpPanel` picks its role from how it is drawn — `region` inside
    the customise hub, `dialog` over the app — because claiming a role it does
    not have is worse than having none.
  - **The file tree.** `src/lib/tree.ts` is new and pure: `flatten` is the rows
    the tree draws, `move` is where each arrow key goes. It returns `null` for the
    four moves it hands back on purpose — ArrowRight on a closed folder (the panel
    opens it), ArrowLeft on an open one (the panel closes it), ArrowLeft on a
    top-level row, ArrowRight on a file — and that `null` is the difference
    between a folder that moves and one that does nothing. Left closing *before*
    it steps out is the standard tree pattern and the only way to shut a folder
    two levels down: step out first and there is nothing left to press. The panel
    renders that flat list with `role="tree"`/`treeitem`, `aria-level`,
    `aria-expanded`, `aria-selected` and a roving tabindex, so the shape the mouse
    sees and the shape the keyboard walks cannot drift apart. Enter and Space are
    left to the browser: a row is a button, so it is already the click. 5.2's tree
    keys landed here.
  - **The transcript.** A `role="log"` named "Conversation" with
    `aria-busy={busy}`. Busy is the part that matters: a streaming answer
    rewrites the subtree every frame, so the announcements are held back until the
    turn settles and the answer is then read once.
  - **Focus that survives High Contrast Mode.** Every `outline-none` in the app is
    now `outline-hidden`. Both remove the ring, but only the v4 spelling puts it
    back as a system-coloured outline for someone who has asked the OS for high
    contrast — until this pass, every control vanished when focused there. Tab-
    reachable controls that had no focus style of their own got one.
- **Test**: two new check scripts, the 13th and 14th. `src/lib/tree.check.ts`
  drives `flatten` and `move` on a fixture tree — including that collapsing a
  folder hides its subtree, and that the four hand-back cases return `null`
  rather than a number. `src/lib/a11y.check.ts` reads the components: an overlay
  that is not inert must be a named dialog, a `role="dialog"`/`tree`/`log` must
  have a name, the log must set `aria-busy`, `outline-none` may not appear at
  all, and no button whose whole content is one glyph may go unnamed. Both proved
  by falsification — the tree check failed on its own author's wrong arithmetic
  first, and a scratch probe file failed the accessibility check on four counts
  before it was removed.
- **Stop**: the whole app is usable without a mouse, and the contrast check still
  passes. ✅ (`theme.check.ts` still measures its 168 pairs.)
- **Found while gating**: the first draft of the icon-button rule demanded
  `aria-label` and flagged 22 buttons that name themselves with `title` — which
  *is* a real accessible-name source, so the rule was wrong, not the buttons. It
  now looks for the button with no name at all, and that found one: the composer's
  remove-attachment `×`.
- **Known gap, written down rather than pretended away**: nothing traps Tab in
  the big dialogs, so a keyboard user can still Tab past one whose
  `aria-modal="true"` claims otherwise. The upgrade is `inert` on the app's own
  content while a dialog is open — the platform feature, which React 19 and the
  webviews this ships in both support — and it needs the content in one wrapper
  element first, which is a layout change rather than an attribute.
  `a11y.check.ts` says so in its own header.

### 5.4 [S] Empty states and error copy ✅ done
- **Why**: the app's voice is good where it exists; the gaps are the states nobody
  looks at (no results, no projects, failed tool, no API key).
- **Now**: thirteen surfaces that said what was missing and stopped there now say
  what to do about it, and each names the control that is really on screen rather
  than a generic "get started" —
  - the panels that own an add form point at it: *New project*, *+ Chat*, the
    prompt form, *New task*, *Draft from a folder*;
  - the four search boxes name the query or teach the syntax — the palette points
    at the `>` and `@` scopes 5.2 added — instead of a bare "Nothing matches.";
  - the two surfaces the assistant fills say so: memory is filed when it learns
    something or is asked to remember one, and artifacts land in the gallery;
  - the keyboard-only entries name the chord: *No conversations yet. Ctrl+N starts
    one*, and the tab strip out of the same mouth;
  - two errors gained a way out — the clipboard failure suggests selecting the
    text by hand, and a failed MCP server points at the Logs tab holding the
    stderr it printed.
- **Checked and left alone**: the "no API key" case *is* the first-run setup
  panel, which already labels the step and carries the fix, and a failed tool call
  draws its own error and opens itself — neither is an empty surface.
- **Test**: none, deliberately. This is copy, not logic: there is no invariant to
  assert that a reader's judgement call needs, and the rule it follows — the empty
  state names the next action — is the one this ✓ stands for.
- **Stop**: every empty surface says what to do next. ✅

---

## Phase 6 — Release and docs housekeeping ✅ done and pushed (v0.4.0)

### 6.1 [S] CHANGELOG.md ✅
- **Why**: the release notes are reconstructed by hand from `git log` each time,
  and 1.7 found there is nowhere in the README for them.
- **Now**: `CHANGELOG.md` — newest first, one line per user-visible change, and
  *all four* sections written: the three shipped versions backfilled out of the
  log (folding the commits that were one change, dropping the ones that were CI or
  housekeeping) and 0.4.0 at the top. The heading is `## <version> — <date>`, with
  the top one `## 0.4.0 — unreleased` so the tag finds it before anyone edits a
  date. `scripts/release-notes.mjs` prints that section for a tag, **exits non-zero**
  when the file has not got one, and writes the step's output itself when the
  runner sets `GITHUB_OUTPUT` — so `release.yml` is one plain `node …` step whose
  failure is the step's failure, and `releaseBody` is that output. A missing
  section stops the release instead of publishing an empty body (see *Found while
  gating*). The parsing is
  `src/lib/changelog.ts`, pure, because a reader that returned the *next* version's
  notes would be pasted into the release body and nobody proof-reads that.
- **Test**: `src/lib/changelog.check.ts`, the 15th check — the reader over a
  fixture (a v-tag, a bare version, a body that must stop at the next heading, the
  last section, a version that is not there, an empty section), then the real file
  held to its shape (newest first, exactly one unreleased and it is first, dated
  headings, no empty section, no indented heading the reader would miss). Falsified
  twice: renumbering 0.1.0 to 0.5.0 failed the ordering rule on exactly that pair,
  and a PowerShell `Set-Content` round-trip — which writes the em dash as mojibake,
  because PS 5.1 is not UTF-8 by default — failed on all four headings instead of
  letting a mangled changelog reach a release.
- **Stop**: cutting a release reads from the file, not from memory. ✅

### 6.2 [S] CI on push, not only on tag ✅
- **Why**: the gate currently runs when a release is tagged; a broken push is
  found late.
- **Now**: the Why was already answered before this item was written — `ci.yml`
  has run on push to `main` (and on pull requests) since the first commit. What it
  did **not** run was `npm run check`: it built, typechecked and ran `cargo test`,
  so fourteen of the fifteen house checks were not in CI at all. One line added to
  that step is the whole change.
- **Kept deliberately**: the three-OS matrix. This item's *Do* said "no matrix
  here — the release workflow already builds three OSes", but that workflow only
  runs **on a tag**, so it is the opposite of what this item is for: it is the
  only thing that finds a macOS or Linux break at push time. The release workflow
  duplicates the build, not this job's reason to exist, so the matrix stays.
- **Test**: none reachable from here. A workflow file is read by GitHub, not by
  `npm run check`, and the gate it runs is the one the README documents.
- **Stop**: a red push is known within minutes. ✅

### 6.3 [S] ARCHITECTURE.md ✅
- **Why**: the repo's real knowledge lives in files like `design.rs` module docs
  and the `db.rs` comments; there is no single map.
- **Now**: `ARCHITECTURE.md` — the four layers with the flow between them; the
  event table (`chat-delta` … `scheduled-fired`, and which module emits each); the
  two parked events explained as one shape used twice; and the one-owner tables,
  sixteen Rust rules and nine frontend ones, each naming the single file that owns
  it. It links to the README for the directory tree and to this file for the why,
  rather than repeating either — the README's own Architecture section is now four
  lines and a link.
- **Test**: none, deliberately — it is a map, not logic. The nearest thing to a
  check would be "every path it names exists", and that is a list, not a rule; the
  file is short enough to read in a minute, which is the point of it.
- **Stop**: a new session can orient without grepping. ✅

### 6.4 [S] Refresh the E2E checklist to match the app ✅
- Beyond what 1.6 added (attachment filing, design systems from a conversation,
  rename, the job object, parked scheduled runs): a smoking-out step for the new
  panels.
- **Now**: a **Smoke-out** section, placed before the long ones — every sidebar
  panel, every dialog, Escape closing each, an empty panel that has to name the
  next action, and the console left clear. The point is a broken build found in a
  minute instead of by reading 340 lines of tick-boxes. The Setup list now names
  all fifteen scripts and the release-notes step, and the Accessibility section
  gained the one part of 5.1 only a window can confirm: with *Reduce motion* on,
  the panels must still open with nothing sliding, and the streaming caret must
  still be visible.
- **Still true, and said in the file**: the checklist has not been walked end to
  end since Phase 1. It is written and armed, not run.
- **Stop**: the checklist matches the app. ✅

---

### Found while gating Phase 6
- **A shell block swallows the exit status of the command inside it.** The first
  version of the release step read the notes with a bash heredoc, which is the
  pattern every example uses:
  `{ echo "body<<__X__"; node scripts/release-notes.mjs "$TAG"; echo "__X__"; } >> "$GITHUB_OUTPUT"`.
  A `{ …; }` group reports the status of its **last** command, so node failing on a
  missing section was swallowed by the trailing `echo`: the step would have gone
  green and the release body would have been empty — the exact failure 6.1 exists to
  prevent, reintroduced by the plumbing around it. The script now writes
  `GITHUB_OUTPUT` itself and the step is a plain `node …`. The part that looked
  most like boilerplate was the part carrying the guarantee.
- **`readFileSync` is `any` without Node types, and that is infectious.** With the
  `@ts-ignore`'d import (there is no `@types/node`, on purpose), a `.check.ts` that
  reads a file gets `any` back, so every `.map((line) => …)` over it is an *implicit
  any* and fails `tsc` — while every existing check had avoided this by iterating
  with `for … of`. Annotating the const (`const file: string = readFileSync(…)`) is
  the one-line fix, and the house way to fail a check is `throw new Error(…)`, not
  `process.exit`, which has no type either.
- **Not fixed, because 6.2 was told to keep the floor: the Node floor disagrees
  with itself.** The README's *Requirements* says "Node.js 20+", `package.json`'s
  `engines` says `>=22.12`, and CI pins 24. They cannot all be right: `npm run
  check` runs `.ts` files directly, so the floor is wherever Node strips types
  without a flag (Node's own docs date the feature to v22.6.0). The README's "20+"
  is the one that is certainly wrong. Left as a note rather than a guess — whoever
  bumps it should verify the minimum on that version rather than on this machine's
  24.

---

## Phase 7 — Setup that finishes the job ✅ done and pushed (v0.4.0)

### 7.1 [S] A button that stands SearXNG up ✅
- **Why**: `web_search` needs a SearXNG instance, and the setup screen asked the
  user to leave the app, run a docker command, edit a YAML file inside a volume, and
  paste a URL back in. Every step was documented, and the whole errand fell on the
  person least able to debug it — while the screen promised something the app could
  do itself.
- **Now**: `src-tauri/src/searxng.rs` owns everything about the container (image,
  port, name, the config it runs on) and `commands::install_searxng` owns the app's
  side of it: where the config lives, and that the setting follows *only* once the
  instance actually answers. `components/SearxngInstall.tsx` is the one control, used by
  the setup screen **and** by *Settings → Browsing* beside the URL field: the first-run
  screen is where somebody starts, but a person whose `web_search` has stopped working
  looks where the URL already is, and a button on only one of the two screens is a button
  half the users cannot find. It is disabled with the reason when Docker is not answering,
  with **Start Docker Desktop** beside it when the engine is installed but asleep.
  Two decisions inside it are worth the words:
  - **The config is ours, not the image's.** A fresh SearXNG answers HTML and
    refuses `format=json`, so a perfectly healthy instance fails `web_search` with
    the reason buried in a file inside a docker volume — the failure the screen
    already warned about, and the one everybody hits first. We write `settings.yml`
    into a folder we own and bind-mount it, four lines over
    `use_default_settings: true`. Written once and never rewritten, so a tuning the
    user makes there is kept.
  - **Published on loopback only** (`127.0.0.1:8888`), and the copy-paste command
    in the screen was corrected to match. `-p 8888:8080` puts an unauthenticated
    search instance on the local network, which is not what "install it for me"
    should mean.
  Success is decided by the instance *answering* (`browse::searxng_ready`), not by
  docker's exit code: a container that starts and then dies has the second answer
  wrong. Docker's own output is only ever used to explain a failure.
- **Why not ship it**: SearXNG is a Python application, so bundling it means an
  embedded runtime or a per-platform sidecar, in a release that already builds three
  OSes — and it is **AGPL-3.0**, a real licensing obligation to carry in exchange
  for saving one `docker run`. The container is the honest middle: nothing ships
  with the app, and the image updates with a pull rather than with a release of
  DeskSeek. A headless browser would not help either: `browser_read` already drives
  the Edge or Chrome that is on the machine, and a browser fetches pages rather than
  searching for them.
- **Test**: `searxng.rs`'s own tests, all pure. The config carries `json` as a
  *list* nested under `search` — the nesting the loader accepts and the API ignores,
  which is the failure the file exists to prevent; a fresh secret is not the shipped
  placeholder and differs every time; the container is named and bound to our
  folder; the published port is loopback; the published port and the URL are the
  same number; and `exists()` reads a status as a status and a failure sentence as
  *not there* rather than as a container worth starting.
- **Stop**: `web_search` works after one click on a machine with Docker running. ✅
  Run on a real machine the same day: the button built `deskseek-searxng` (Up,
  `127.0.0.1:8888->8080/tcp`), wrote `searxng/settings.yml`, and saved the URL into
  settings. `GET /search?q=deskseek&format=json` — the exact request the tool makes
  — answered `200` with 14 results. The only thing not exercised is the model's own
  choice to call the tool, which is model behaviour rather than code; the
  checklist's "a question that needs current facts makes the assistant call
  `web_search`" line is the last box to tick.

---

## Phase 8: Interop and the daily habit ✅ done, not yet pushed

Planned 2026-10-13, from [`ReverseEngineering.md`](ReverseEngineering.md). Every item is small and
self-contained: one file owns the rule, one runnable check proves it, and nothing here needs a
server, a new subsystem or a new dependency. The order is value per unit of effort, not dependency.

### 8.1 [S] Skills move in and out of DeskSeek ✅
- **Why**: `skills/<id>/SKILL.md` is not a DeskSeek format, it is the open one. Anthropic publishes
  it as Agent Skills, and Google states that its own skills are built entirely on the open,
  Markdown-native `SKILL.md` standard, so that a skill written elsewhere can be copied in. A store
  nothing can be imported into is a library nobody can move, and this is the cheapest item in the
  batch: the reader and the writer are the store that already exists, seeing a second root.
- **Files**: `skills.rs` (`skill_dirs`, `import_folder`, `export_all`, and a `SkillImport` report —
  a byte copy of the *folder*, so frontmatter this build does not read and any reference file beside
  `SKILL.md` survive verbatim); `store/files.rs` (the path guard, extracted as `safe_component` and
  now asked by `Store::file_for` too, so the importer keeps no second copy of it); `commands.rs` and
  `src/api.ts` (`import_skills`, `export_skills`); `src/types.ts` (`SkillImport`); `SkillsPanel.tsx`
  (*Import folder…*, *Export all…*).
- **Test**: in `skills.rs`, `a_skill_folder_round_trips_through_export_and_import_unchanged` (a
  `license:`/`allowed-tools:` frontmatter key and a `references/` file both survive out and back
  byte for byte; a second import is a skip, not an overwrite), `a_stray_file_beside_a_skill_is_not_a_skill`
  (a `.txt` and a `README.md` never become skills), and `the_importer_uses_the_one_path_guard` (a
  folder carrying `..` is refused, and the store refuses to export into itself). In `files.rs`,
  `the_safe_component_guard_is_the_same_for_a_prefixed_store` shows the same names refused for an
  empty prefix and a prefixed store, because the prefix was never what protected it. `cargo test`
  330 passed.
- **Stop**: a skill directory exported here imports into Claude- or Gemini-shaped tooling unchanged,
  and one written there imports here with no edit.

### 8.2 [S] Import what another assistant wrote ✅
- **Why**: ChatGPT imports instructions, settings, skills and projects from Claude Code, Cowork and
  Cursor. The skills half is 8.1. The memory half is a markdown file mapped into the category files,
  and it is how a person arrives here without retyping a year of context.
- **Files**: `memory_files.rs` (`parse_export` + a `note_text` that drops a bullet marker and a
  stored `(date)`, reusing the one heading reader now exposed as `tools::docs::heading_text`);
  `prompts.rs` (`import_folder`, `PromptImport`); `commands.rs` and `src/api.ts` (`import_memory`,
  `import_prompts`); `src/types.ts` (`MemoryImport`, `PromptImport`); `MemorySection.tsx`
  (*Import from a file…*) and `PromptsSection.tsx` (*Import folder…*). Both are a *byte* import of
  another tool's files, so an edit already saved here is never touched.
- **Test**: in `memory_files.rs`, `an_exported_memory_file_lands_in_categories_and_invents_none` (a
  heading names the category and its bullets become that category's notes; an unknown heading and
  the unlabelled preamble both fall to `other`; no `hobbies.md` is invented; a stored `(date)` is
  dropped; importing the same export twice adds nothing). In `prompts.rs`,
  `a_prompts_folder_imports_as_files_and_never_overwrites_one` (a `.txt` is not a prompt; a name
  already saved keeps the user's own text). `cargo test` 332 passed.
- **Stop**: an exported memory file from another assistant becomes editable categories here, and
  nothing already written is overwritten.

### 8.3 [S] A chat that memory does not learn from ✅
- **Why**: one switch that is the difference between using the assistant for something private and
  not using it at all. Claude calls it incognito, ChatGPT calls it temporary; the honest description
  of the behaviour is "this chat does not feed memory".
- **Files**: `store/db.rs` (`chats.incognito`, in the `CREATE TABLE` **and** as a guarded `ALTER`,
  plus `set_chat_incognito` and the copy `duplicate_chat` makes); `memory.rs` (`enabled` — the one
  rule, asked by both the read and the write so they cannot disagree); `conversation/agent.rs` (it
  gates the memory guide, the injected profile *and* recall, and refuses `save_memory` before it even
  parses the arguments); `commands.rs` (`set_chat_incognito`); `src/api.ts` and `src/types.ts`
  (`ChatRow.incognito`); `ChatView.tsx` (the toggle and a line that says what it means).
- **Test**: `memory::tests::an_incognito_chat_neither_reads_nor_writes_memory` pins the single rule
  the prompt assembly and `save_memory` both ask; `db::tests::an_older_database_upgrades_instead_of_refusing_to_open`
  now also asserts a pre-8.3 `chats` table gains the column on a **real temp file**. No test can
  build an `AppState`, so the prompt omission itself is proved by construction — one predicate, both
  call sites — rather than by a `system_prompt` test. `cargo test` 333 passed.
- **Stop**: nothing said in a chat that memory does not learn from can be quoted back in another one.

### 8.4 [S] A project's memory is editable where it is shown ✅
- **Why**: 2.7 reads the project's own `project-<folder>.md` and shows it read-only, so forgetting
  one entry means leaving the panel for *Settings → Memory*. The writer already exists; the panel is
  simply not wired to it.
- **Files**: `commands.rs` (`project_memory_category`, the one place a project's file is resolved,
  plus `project_memory`, `append_project_memory` and `delete_project_memory` — the last two refuse
  with a plain sentence when the project has no folder); `memory_files.rs` (`project_category` stays
  the owner of the file name, `append`/`remove` do the work); `MemorySection.tsx` (the row is now an
  exported `MemoryRow`, used by both surfaces); `ProjectsPanel.tsx` (forget a line, add a line);
  `src/api.ts`.
- **Test**: `memory_files::tests::forgetting_one_entry_leaves_every_other_line_alone` (one line goes,
  its neighbours stay, and a miss changes nothing) and `a_project_with_no_folder_owns_no_memory_file`
  (no folder means no file, and two folders never resolve to one — the rule that stops a project
  showing another's notes). `cargo test` 335 passed.
- **Stop**: a project's memory can be changed from the panel that displays it.

### 8.5 [S] Hourly and weekdays ✅
- **Why**: the three fields *are* the schedule, and that stays. Two more values close the gap with
  Claude, ChatGPT and Gemini without teaching anyone a cron string.
- **Files**: `schedule.rs` (`Freq::Hourly` and `Freq::Weekdays`, their ids, and the two `next_run_at`
  arms — an hourly task reads only the *minute* of `time` and works on the timestamp so 23:30 rolls
  to 00:30; a weekday task skips Saturday and Sunday); `ScheduledPanel.tsx` (both options, and a line
  saying the hourly shape reads the minute only). `create_scheduled` needed no change: it already
  sends the shape through `Schedule::parse`, which normalises whatever the panel writes.
- **Test**: in `schedule.rs`, `hourly_reads_the_minute_and_crosses_midnight` (later the same hour →
  the next hour → over a day boundary, and exactly-on-the-minute is already gone),
  `weekdays_skip_the_weekend` (Friday evening → Monday, and both weekend days land on Monday),
  `the_two_new_shapes_round_trip_through_their_ids`, and the strictly-after invariant extended to all
  five shapes. The old "unknown value" test moved off `"hourly"` — now a real shape — onto
  `"fortnightly"`. `cargo test` 338 passed.
- **Stop**: a task can be set to run hourly, or on weekdays only, and the panel says when it is next.

### 8.6 [S] A scheduled task shows its runs ✅
- **Why**: a schedule you cannot audit is one you stop trusting. Claude keeps a past-runs view, and
  the honesty rule here is already set: a run that was stopped or errored still counts for what it
  really spent.
- **Files**: `store/db.rs` (`turn_usage.task_id` in the `CREATE TABLE` **and** as a guarded `ALTER`,
  an index over it, `record_turn_usage` carrying it through every round with `COALESCE`, and
  `task_runs`); `conversation/agent.rs` and `commands.rs` (`run_turn`/`start_turn` thread the task id
  — `None` for anything the user typed, `Some` from the scheduler, including *Run now*); the
  `task_runs` command; `src/api.ts`; `src/types.ts` (`TaskRun`); `ScheduledPanel.tsx` (*Runs* on a
  task: newest first, each with its chat and its cost).
- **Test**: `db::tests::a_task_lists_its_runs_and_they_agree_with_the_spend_view` — two runs of one
  task listed newest-first with their cost, a turn the user typed absent from the list, the sum over
  the task plus that turn equal to the window's whole `spend_report` total, a task that never ran
  listing nothing, and the limit honoured. The upgrade test now also builds a pre-8.6 `turn_usage`
  table on a **real temp file**, so the `task_id` ALTER *and* the index over it are exercised through
  `open()`. `cargo test` 339 passed.
  The plan's *outcome* is deliberately **not** stored: `turn_usage` has nowhere for it and the
  **Files** line above does not ask for one, so a run is described by when, rounds, tokens, cost and
  its chat. A run that spent nothing therefore leaves no row — the same rule the spend view already
  follows, not a gap of this item's making. `last_run_at` on the task still says it ran.
- **Stop**: the panel shows what a task has done and what it has cost.

### 8.7 [S] The model can schedule a task ✅
- **Why**: a task can only be created by filling in a form, so the model can never offer one
  mid-conversation. Claude proposes a name, a cadence and the instructions and asks the user to
  confirm; nothing about that needs a new mechanism here, only a tool.
- **Files**: `tools/mod.rs` (a `ScheduleDraft` and `parse_schedule`, a reader that tolerates a day
  name, a number sent as a string, and a field missing altogether; and the `schedule_task` tool in
  the `Write` class, whose doc now says it covers "sets up something that will run later");
  `commands.rs` (`create_scheduled_task` — the one create path, now shared by the form's command and
  the tool, so a task made by talking gets the same clock as one made by filling the form in);
  `conversation/agent.rs` (the dispatch, which reports the cadence it actually ended up with, and a
  `# Scheduling` prompt section). No `src/api.ts` change: the model reaches this through its tool
  list, and the Scheduled panel re-reads on open.
- **Test**: `tools::tests::the_schedule_reader_survives_every_shape_a_model_sends` — a missing,
  empty or non-string prompt refused by name; a day as a name, a short name, a number, a number
  sent as a string, a negative and an absurd one all landing in 0..=6; `nth` as a word or a number;
  and a number where text belongs not panicking. `scheduling_a_task_asks_first` — `class_of`
  returns `Write` and the tool is offered in both sections, which is what makes a refused approval
  create nothing: the tool never runs. `cargo test` 341 passed.
- **Stop**: asking for a recurring task in chat produces one, after an approval, with no form.

### 8.8 [S] Edit an artifact in place ✅
- **Why**: the best small idea in the whole teardown. Highlight the sentence you want changed, type
  the change, and it lands where you marked it instead of re-describing the paragraph in chat.
  Claude, ChatGPT and Gemini all ship it, and this repo's artifacts already carry versions, which is
  the half that is expensive to add later.
- **Files**: `src/lib/artifact.ts` (`windowAround`, `patchRange`, and the one `clampRange` they both
  go through — pure, offsets in and text out); `src/lib/artifact.check.ts` (extended, not replaced);
  `ArtifactPanel.tsx` (a selection in the editor, a *Change it to…* bar, and Rewrite, which asks,
  patches and saves); `conversation/agent.rs` (`revise_selection` — one non-streaming call, sent the
  selection and a window of each side rather than the document, plus `unwrap_fence`); `commands.rs`
  and `src/api.ts` (`revise_selection`, `ReviseRequest`).
- **Test**: `artifact.check.ts` covers the same word twice patched at each occurrence separately (the
  case a search by text gets wrong), a selection at the start, in the middle and running to the end,
  an insert and a delete, and a stale pair (reversed, out of range, `NaN`, fractional) clamped rather
  than thrown. `conversation::agent::tests::a_revision_reply_that_arrived_fenced_is_unwrapped_before_it_lands_in_the_document`
  covers the one reply shape that would corrupt the document. "A revision always adds a version" is
  already pinned by `db::tests::an_artifact_keeps_every_version`, so it is cited rather than copied.
  `cargo test` 342 passed.
  One limit said out loud: the offsets are the *editor's*. The rendered preview is a sandboxed frame
  without `allow-same-origin`, so the app deliberately cannot read a selection made in it — the
  selection is made in the editor, which also shows the source the patch indexes into.
- **Stop**: a sentence in an artifact can be changed without a chat turn and without retyping the
  document.

### 8.9 [S] Break reminders and quiet hours ✅
- **Why**: the least glamorous item in Claude's settings and the most used. It is also the direct
  answer to "reminders", which the assistant currently cannot do at all.
- **Files**: `store/settings.rs` (`reminders`, `reminder_minutes`, `quiet_hours`, `quiet_from`,
  `quiet_to` — each with a serde default, so a `settings.json` written before them still loads *and*
  the switches arrive off); `notify.rs` (`wants_toast`, the one pure rule that now combines the
  master switch, quiet hours and the focus check, so three settings cannot be combined wrongly in
  three places; `in_quiet_hours`; and `reminder`, which goes through `ping` so a nudge obeys the same
  rules as a finished run — `ping` emits the in-app notice *before* it asks, so quiet hours can never
  hide that something happened); `lib.rs` (a reminder ticker beside the scheduler's, counting minutes
  in the loop rather than in the database, because forgetting the count on a restart is right);
  `src/types.ts`; `SettingsDialog.tsx` (the controls, in Tools beside the notification switches).
- **Test**: `notify::tests::a_quiet_window_holds_the_popup_back` — the overnight window on both sides
  of midnight, a daytime window, the end being exclusive, `9:00` reading as `09:00`, and a degenerate
  or unparseable window read as *no* window rather than a silent day.
  `the_three_switches_decide_whether_a_popup_is_wanted` — each switch alone and combined.
  `settings::tests::missing_file_yields_defaults` gained the new defaults and
  `round_trips_through_disk` carries all five through the file. The in-app notice surviving quiet
  hours is structural (the emit is the first thing `ping` does), and no test can build an `AppHandle`
  to observe an OS toast. `cargo test` 344 passed.
- **Stop**: reminders fire, and quiet hours silence the toast without hiding that something happened.

### 8.10 [S] A recap of the week or month ✅
- **Why**: Claude's Reflect reports the topics you spent time on, your most active day and your peak
  hour. Everything it needs is already in `turn_usage` and `chats`; this is a read model, not a new
  store.
- **Files**: `store/db.rs` (`Recap` and `HourCount`, and a `recap` query that *takes its totals, days
  and conversations from `spend_report` itself* rather than querying again — so agreement between the
  two views is structural — plus the one query the spend view has no use for, the hour histogram);
  `commands.rs` and `src/api.ts` (`recap`, the same `days` argument and the same cap as
  `spend_report`); `src/types.ts`; `src/lib/chart.ts` (`recapSpec`, the recap as a bar chart or `null`
  when there is nothing to draw, sitting beside the renderer so the rule is checked by the file that
  draws the bars); `src/lib/chart.check.ts` (extended, not replaced); `SpendPanel.tsx` (*The shape of
  it* — the chart, the busiest day and the peak hour, loaded in the same round trip as the bill).
- **Test**: `db::tests::the_recap_adds_up_and_agrees_with_the_spend_view` — the totals, days and chats
  match `spend_report`'s, the day rows and the hour rows each sum to the same turn count, the day
  costs sum to the total, hours come busiest-first and inside 0..24, and an empty window is empty.
  `the_spend_payload_is_camel_case_on_the_wire` grew a `Recap` value, because `hours` is a shape
  nothing else in the app has and a snake_case key there would blank the section silently.
  `chart.check.ts` gained the adapter's cases (oldest day first) and the two ways a window can be
  empty, which is what stops a row of zero-height bars being drawn. `cargo test` 345 passed.
- **Stop**: a week's shape is readable, and its numbers agree with the spend view.

### 8.11 [S] A notification that takes you to the thing ✅
- **Why**: a toast that says a task finished and cannot open it is half a feature. The hard half is
  already done, because the park queue can be answered from the Scheduled panel.
- **Files**: `notify.rs` (`ping` records a target when it raises a toast — `EVENT_NOTIFY_OPEN`,
  `OPEN_GRACE_SECS`, `route_to_open`, `take_open_target`); `lib.rs` (`AppState.pending_open`, and a
  window-event handler that reads a focus arriving after a toast as a click on it); `src/types.ts`
  (`NotifyOpenEvent`); `src/api.ts` (`onNotifyOpen`); `App.tsx` (its own effect, registered after
  `selectChat`, that opens the chat the toast was about).
- **Test**: `notify::tests::only_a_fresh_toast_routes_a_focus` — the target survives, a focus after
  the grace expires is an ordinary alt-tab and routes nothing, and taking it twice yields nothing
  the second time. The click itself is on the E2E checklist, with the quiet-hour reminder, because a
  real toast and a real focus change cannot be built in a unit test. `cargo test` 346 passed.
  The route is reconstructed from a focus rather than a click: `tauri-plugin-notification` 2.6
  reports no activation callback (checked in its source), so a plugin-level click action is the
  upgrade path, and the ceiling is named in a `ponytail:` note on `route_to_open`.
- **Stop**: clicking a finish opens the conversation that finished.

### 8.12 [S] `@` and `$` in the composer ✅
- **Why**: the composer already parses `/` commands and `/p:<id>` prompts, so the parser exists.
  ChatGPT's quick controls use `@` for context and `$` for a skill, and this is the same small
  feature reached from the same keystroke.
- **Files**: `src/lib/commands.ts` — `composerOffer` (which menu, filtered on what: `/` owns the
  whole box as before, `@` and `$` are word-scoped mentions, an unknown marker is text),
  `matchMentions`, and the ranking `matchPalette` already had, lifted into one `rank` both use
  rather than copied. `src/lib/commands.check.ts` (extended, not replaced). `ChatView.tsx` (one
  row shape for all three menus, the `@` list read from `find_in_tree`/`list_tree` and the `$` list
  from `list_skills` when that menu opens, and the fill-don't-send branch in `runCommand`).
- **Test**: `commands.check.ts` — an unknown marker (`!files`, `#note`) opens nothing, the marker
  nearest the cursor wins, a bare `@`/`$` lists its whole scope, a file is found by a word in its
  folder as well as by name, a path that begins with the query outranks one that merely contains
  it, and a skill is found by its id and by what it is for. The check was falsified: breaking the
  `@` branch fails it (`read @App.tsx should offer a menu`) and restoring passes. The insertion
  itself — a mention fills the box, and the trailing space ends it — is pinned structurally: the
  offer's scope is not `command`, which is the branch that sends. `npm run check` 15/15,
  `cargo test` 346 passed.
- **Stop**: a file or a skill can be referenced from the composer without leaving the keyboard.

### 8.13 [S] The log and the spend leave the app ✅
- **Why**: the decision log (3.5) and the run history exist so that a person can look something up,
  but only from inside the app. An export is a file that gets mailed around, so the rule from 3.5
  holds and is now load-bearing: arguments are stored as a digest, never as themselves.
- **Files**: `store/db.rs` (`Audit`, `AUDIT_VERSION`, `audit` — it assembles the *same two reads the
  panels make* rather than querying again); `commands.rs` (`export_audit`, which writes the JSON to
  the documents folder and returns the path, and `spend_window`, lifted out of `spend_report` and
  `recap` so a third caller cannot pick a third window); `lib.rs`; `src/api.ts`; `src/lib/commands.ts`
  (the palette entry beside *Maintain the database*); `App.tsx` (the notice naming the file);
  `SpendPanel.tsx` (`DEFAULT_SPEND_DAYS`, so "the panel's window" is one number, not two).
- **Test**: `db::tests::the_exported_audit_is_what_the_panels_show_and_carries_no_arguments` — the
  exported decisions and spend equal `recent_decisions`/`spend_report` for the same window; the
  window is honoured (one turn of two is in it); the digest of a call whose arguments held a secret
  is in the file and the secret is not; a call with no arguments has an empty digest rather than a
  missing key; and the keys are camelCase, which is what the format was written in. Falsified:
  making the export its own decision query with a different cap fails it. `cargo test` 347 passed.
- **Stop**: what was allowed, and what it cost, can be exported to a file.

---

## Phase 9: Containment and trust ✅ done, not yet pushed

Planned 2026-10-13, from [`ReverseEngineering.md`](ReverseEngineering.md). Phase 8 made deskseek do
more; this is the batch that makes what it does trustworthy when nobody is watching — what it may
spend, what it may send out, what it may delete, and who checks it when the mode is Auto. The order
is load-bearing rather than cosmetic: 9.5 must land before 9.6, because a safety review costs money
and a bill that cannot show the review is a bill that lies. Nothing here needs a server, and nothing
here weakens a rule already standing — the guard stays the guard, and anything that survives a
restart is something the user can see and revoke.

### 9.1 [S] A long chat folds itself ✅
- **Why**: Claude compacts near the limit automatically; this repo offers `/summarise` and a button
  that appears at 80% of the window, so the user is the one who has to notice. Automating the same
  step costs one cheap call and removes a decision — and a scheduled run, which has nobody watching
  the meter, is exactly where the window runs out.
- **Files**: `conversation/summarize.rs` (`AUTO_COMPACT_AT`, `should_compact`, `compacted_note`,
  and `fold` — the one place `/compact` and the automatic step both go through, so there is one
  shape of folded transcript rather than two); `conversation/agent.rs` (the trigger, after the loop
  and before the done event, so the window's own re-read shows the fold); `api/models.rs`
  (`context_for`); `store/db.rs` (`delete_messages` returns how many it dropped, which is what the
  note reports); `commands.rs` (`compact_chat` keeps its command and loses its inline body).
- **Test**: `summarize::tests::a_chat_over_the_window_is_folded_once_and_not_again` — 80% of the
  window is the meter's warning and not the fold's mark, 95% is, a transcript that is already one
  folded message is never folded again (which is what stops a chat whose system prompt alone fills
  the window from folding on every turn), and an unknown window is not a reason to spend.
  `a_folded_note_says_how_much_it_folded` pins the count in the note. The test deliberately does
  **not** assert the exact boundary: `0.9 * 1_000_000` is a floating-point question rather than a
  fact about the rule, so the cases sit well inside and well outside it instead. Falsified: relaxing
  the guard from `stored > 1` to `stored > 0` fails it on
  `assertion failed: !should_compact(950_000, 1_000_000, 1)`, and restoring passes.
  `cargo test` 349 passed. Two things the plan guessed differently, both said out loud: the measure
  is the provider's own `prompt_tokens` for the request just sent rather than a character estimate,
  so it cannot drift from the meter or from what the endpoint counted; and `src/lib/context.ts`
  needed no change, because the meter's 0.8 is a warning and the fold's 0.9 is an act.
- **Stop**: a long chat stops at a summary instead of at a length error, without the user pressing
  anything. One limit named: the summariser's own call is not priced into the spend view yet, which
  is 9.5's job — and it is the same call `/summarise` has always made.

### 9.2 [M] A grant that remembers its target ✅
- **Why**: every new write or command asks today. Claude remembers an "always for this site" and an
  "always for this app", and generalising that to a target — a site, a command prefix, a path root —
  is what cuts approval fatigue without loosening the model.
- **Files**: `approvals.rs` (`target` — the one rule turning a call into the thing it touches:
  a site from a URL, the program a command line runs, the folder a write lands in, the app a
  desktop click names; `target_is_the_page` for the four tools whose target is only on the page;
  and the grant key, now the triple); `store/db.rs` (`tool_permissions.target` in the `CREATE TABLE`
  **and** rebuilt through `open()`, `is_tool_allowed` taking a target, `allow_tool`/`revoke_tool`/
  `all_allowed_tools` following); `tools/browser.rs` (`Browser::current_url` — the page, asked with
  `try_lock` so a busy browser yields "no site" rather than a prompt that waits); `commands.rs`
  (`ToolGrant.target`, `revoke_tool_permission` taking one); `conversation/agent.rs` (`approve`
  computes the target once and uses it for all four questions — the session check, the stored check,
  what the user is offered, what is stored); `src/types.ts`, `src/api.ts`, `SettingsDialog.tsx`
  (the grant's target, `(the whole tool)` when there is none), `ApprovalPrompt.tsx` (*Always allow
  `cargo` for this folder*).
- **Test**: `approvals::tests::a_target_is_the_place_a_call_touches` (a site loses its scheme, port,
  credentials and `www.`; `about:blank`, `data:` and `file:` have none; one command is that
  command while `cd x && rm -rf y` is not; a write is its top folder or, at the root, the file
  itself; `../secrets` and `C:/…` get none) and `a_target_cannot_smuggle_a_separator_into_the_key`.
  In `db.rs`, `a_grant_covers_its_own_target_and_nothing_else` (cargo covered, npm not, no-target
  not, another folder not, a second grant not overwriting the first, revoking one leaving the
  other) and `a_whole_tool_grant_still_covers_every_target_of_it` (the old meaning, preserved).
  The rebuild is exercised on a **real temp file**: `an_older_database_upgrades_instead_of_refusing_to_open`
  now carries a pair-keyed `tool_permissions` with a row and asserts the grant survives as a
  whole-tool one — and that a second grant for the same tool now fits, which the old key could not
  hold. Two things the plan guessed differently, both said out loud: the table was **rebuilt**, not
  widened with an ALTER plus a `tool_grants` beside it, because SQLite cannot widen a primary key in
  place and two tables for one rule is the drift the repo avoids; and the target lives in
  `approvals.rs` rather than `tools/mod.rs`, beside the grant key it has to agree with. The test
  caught a real bug while being written — `about:blank` reduced to a site called `about`, which is
  why a host now has to have a dot (or be `localhost`). Falsified: making a grant cover every
  target (`target = ''` → `1 = 1` in the match) fails it on
  `assertion failed: !is_tool_allowed(&conn, "/repo-a", "run_command", "npm")`, and restoring
  passes. `cargo test` 352 passed.
- **Stop**: a repeated action on one site asks once — and a repeated `cargo` in one folder asks once.
  The decision log is untouched by any of this: it still records each decision with its arguments'
  digest, and a grant that is given still writes the `allowed` line that made it. One limit named:
  a grant is matched on the target *text*, so `ui_click`'s window ("notepad") and a site are
  compared as the app spelled them — case-sensitively, because a path and a program name are.

### 9.3 [S] A chat that declares what it may send out ✅
- **Why**: the guard already *tries* to catch an upload by pattern-matching arguments; a policy the
  user states is stronger and can say *why* it refused. Claude Cowork respects session egress
  permissions, and this is the honest local equivalent.
- **Files**: `tools/guard.rs` (`Egress` — the three rungs, `parse`/`as_str`, and `egress_blocks`,
  the one rule saying what a declared policy refuses); `store/db.rs` (`chats.egress` in the
  `CREATE TABLE` **and** as a guarded `ALTER`, through `ChatRow`, both SELECTs, `duplicate_chat` and
  `set_chat_egress`); `conversation/agent.rs` (the policy is asked before the mode, before the guard
  and before the user — a refusal rather than a second opinion — plus a `# Network` prompt section
  so the model plans around it); `commands.rs` and `src/api.ts` (`set_chat_egress`); `src/types.ts`
  (`Egress`, `ChatRow.egress`); `ChatView.tsx` (the three-way switch beside *Incognito*, a banner
  when it is not `all`, and the sentence that says it is a policy and not a sandbox); `App.tsx`
  (the setter and its notice).
- **Test**: `guard::tests::a_declared_policy_refuses_going_out_and_leaves_local_work_alone` — with
  nothing declared the policy says nothing at all (so it cannot become a second, quieter copy of
  the heuristic); `none` refuses a POST, a `git push`, a fetch, a search and a browser action,
  while `cargo test` and a `write_file` are untouched; `read-only` allows a fetch, a search and a
  bare `curl https://…` while refusing `curl -d`, a `browser_type` and a write; and the rungs
  round-trip through the strings the database holds. The upgrade test on a **real temp file** now
  also asserts `kept.egress == "all"`, which is a real read of the column only the ALTER could
  have added. Falsified: making the `none` rung stop checking the outbound list
  (`OUTBOUND.iter().any(…)` → `false`) fails it on the `curl -X POST` case, and restoring passes.
  `cargo test` 353 passed. Deliberate deviation from the plan, said out loud: the
  policy is **per chat** (a column on `chats`, like incognito) rather than per run — a per-run
  switch would be a second control for one rule, and the chat is the unit the user thinks in.
- **Stop**: a chat set to "no uploads" cannot send a local file out, and says why when refused.

### 9.4 [S] A delete is never auto-approved ✅
- **Why**: a delete under a pile of approvals is the one action that cannot be undone. Claude asks
  for it separately from every other write, and the hole here is the same one the plan already
  admits: file tools rewind, but a `rm` through `run_command` does not.
- **Files**: `tools/guard.rs` (`is_delete` — the one rule saying which commands delete, and
  `leading_words`, which reads the *leading* word of each command in a line rather than searching
  the text, so `npm run formatter` is not an `rm` and `grep -r rm .` is not one either);
  `conversation/agent.rs` (`must_ask`, computed once beside the policy check and read by the one
  branch that forces an approval, so "a delete is never waived" cannot become three answers in
  three branches; plus a line in the auto and skip prompt sections so the model says what it is
  removing instead of discovering the rule a prompt at a time); `README.md`, `ChatView.tsx` (the
  Skip banner and the Agent rung now say a delete is still put to you).
- **Test**: `guard::tests::a_delete_is_never_waived_by_a_mode` — `rm`, `sudo rm`, `Remove-Item`,
  `del`, `erase`, `rd`, a chained `cd src && rm old.rs`, `git clean -fd`, `git rm` and `/bin/rm`
  all count; `cargo test`, `npm run formatter`, `grep -r rm .` and a `write_file` do not; a missing
  argument is not a delete; and the catastrophic tier still refuses `rm -rf /` outright. The loop
  half is structural — one predicate, one branch — because `approve()` cannot be built without an
  `AppHandle`. Falsified: dropping `"rm"` from the delete list fails it on the first case
  (`rm -rf target`), and restoring passes. `cargo test` 354 passed. Two deviations, both said out
  loud: there is **no fifth
  `ToolClass`** (a class is a property of a *tool*, and `run_command` is both an ordinary command
  and a delete — the predicate is the only shape that can tell two calls of one tool apart), and
  **`tools/mod.rs` needed no change** for the same reason. One limit named: a *grant* still waives
  the prompt — an "always allow" is the user's explicit, visible, revocable answer for one tool and
  target, and 9.2 made that target the program (`rm`, not `run_command`), so answering it is a
  narrow thing to have said. What a mode may never do is decide it on the user's behalf.
- **Stop**: no approval mode deletes a file without being asked.

### 9.5 [S] Every spender is on the bill ✅
- **Why**: a sub-agent, a safety review, dictation and semantic lookup all spend money, and this
  repo's own rule is that the sidebar total and the spend view are two readings of one number. A new
  subsystem that spends invisibly breaks that rule the day it ships — and two subsystems already
  did: the summariser and the in-place rewrite both make a real call and neither leaves a row.
- **Files**: `store/db.rs` (`turn_usage.kind` and `turn_usage.priced`, each in the `CREATE TABLE`
  **and** as a guarded `ALTER`; `TURN`/`SUMMARY`/`REVISION`; `TurnUsage` carrying both; `KindSpend`
  and `SpendReport.kinds`; `ModelSpend.priced` from `MIN(priced)`); `api/client.rs` (`complete`
  returns `(text, Usage)` — a call cannot be priced from its text); `conversation/mod.rs`
  (`record_overhead`, the one writer for a call nobody typed, in the parent of the two modules that
  make such calls); `conversation/summarize.rs` (`SUMMARY_MODEL`, named so the price on the row is
  the price of the model that was called, and the row itself); `conversation/agent.rs` (the turn row
  carries `TURN` and a price-known flag, and `revise_selection` takes a `chat_id` so its call has
  somewhere to be billed); `commands.rs` (that `chat_id`, and the frontend's `ReviseRequest`
  carrying it); `src/types.ts`, `SpendPanel.tsx` (*By what spent it*, and the note that an unpriced
  call's money is unknown rather than free).
- **Test**: `db::tests::an_overhead_call_is_on_the_same_bill_as_the_turn_that_caused_it` — a turn
  and the summary the fold made of it are two rows in one total, `TURN` and `SUMMARY` each carry
  their own cost and call count, and the kinds *sum* to the total, which is what makes the split
  checkable rather than decorative. Then an unpriced model: it is still on the bill with
  `priced: false`, its tokens are counted, its money is not added, and one unpriced row makes its
  whole kind-group unpriced rather than averaged away. `the_spend_payload_is_camel_case_on_the_wire`
  grew `kinds` and `priced`, because `costUsd` arriving as `cost_usd` is what took the window down
  once before. The upgrade test on a **real temp file** now asserts a row written through the
  upgraded table reads back as a priced turn, which is the guard that both ALTERs ran. Falsified:
  making the kinds query drop the overhead rows (`AND kind = 'turn'`) fails it on
  `expect("the summary")`, and restoring passes. `cargo test` 355 passed. Three deliberate deviations, all said out loud: `record_overhead` lives in
  `conversation/mod.rs` rather than in `summarize.rs` or `agent.rs`, because both call it and
  Rust modules cannot have two owners of one rule; the revision **had to gain a chat id** (it had
  none, so there was no conversation to bill — the frontend already knew it from the artifact); and
  the plan's "a split by kind" is per *call kind*, not per model kind — the model split already
  exists beside it.
- **Stop**: nothing can spend money without appearing on the spend view. Two honest limits named:
  the summariser's spend is written *after* the call returns, so a process killed mid-call records
  nothing for it (the same shape as a turn's, which is written per round for exactly this reason —
  a non-streaming call has no rounds to write); and an unpriced model makes the window's total a
  floor rather than the whole bill, which the panel says in words rather than leaving a zero to be
  read as free.

### 9.6 [M] A safety pass before Auto acts ✅
- **Why**: Auto mode reviews nothing but a string table, and `guard.rs` says so out loud. A small
  model call over the tool name, a redacted argument summary and the last few turns is the honest
  second layer — and the fallback when it cannot answer is *ask*, never *proceed*.
- **Files**: `conversation/review.rs` (new: `Verdict` and its `parse`, `verdict_from` — the whole
  rule for turning an answer into a decision, failure included — `needs_review`, the prompt, and
  the call itself); `conversation/agent.rs` (`refuse`, the one function a refusal goes through now,
  extracted from the guard's own branch so the reviewer's refusal cannot be a near-copy that
  forgets to count a strike; and the branch that asks the reviewer between "the guard let it
  through" and "it runs"); `conversation/mod.rs` (`pub mod review`); `store/db.rs` (`REVIEW`, the
  fourth kind, so the call is on the bill); `mcp/manager.rs` (`redact_args` becomes `pub(crate)` —
  it is the app's one rule for handing arguments to something outside the app, and a second
  redactor would be a second thing to keep blunt); `README.md`, `ARCHITECTURE.md`, `ChatView.tsx`
  (the Auto banner says the reviewer is there).
- **Test**: `review::tests::nothing_unrecognised_is_an_allow` — the half of the verdict that is
  ours: an empty reply, a refusal to answer, a word nobody taught it, a number, `unsure`, and a
  *failed* call all become `Ask`; `allow`/`ALLOW.`/`This is safe to run` and `refuse — it uploads a
  key`/`it is unsafe` are read as themselves; and `verdict_from(Err(…))` is an `Ask`, which is the
  line that matters most. Falsified: turning the failure into an `Allow`
  (`Err(_) => Verdict::Ask` → `Allow`) fails it on `left: Allow, right: Ask`, and restoring passes.
  `the_golden_set_reaches_the_reviewer_and_the_other_modes_do_not` — the
  plan's golden set, with the honest correction that a test cannot reach a model's *opinion*: what
  it asserts is who gets asked. A download-and-execute pipeline, an `rm -rf`, an upload, an
  ordinary write, an edit, a browser click and a desktop type all reach the reviewer in Auto;
  Manual and Skip never do (one already asks the user about every one of them, the other *is* the
  off switch); a read is not reviewed; and neither is a delete, which is on its way to the user
  anyway. `cargo test` 357 passed. Three things said out loud: the reviewer runs **after** the
  guard and never on a call the guard already refused, so the cheap, certain checks keep working
  when the model is unreachable; `refuse` was **extracted from the existing guard branch** rather
  than copied, which is the only edit here to code that already worked; and the review has **no
  setting** — Auto is where it runs and Skip is the way to decline it, which is one mode switch
  rather than a second knob for one rule. One limit named: it is one cheap non-streaming call per
  action, so Auto is slower than Skip by roughly one round trip per tool call, and its cost is on
  the bill under *Reviews* so that is visible rather than implied.
- **Stop**: Auto no longer relies on string matches alone, and a failed review asks.

---

## Phase 10: Quick entry and the desktop surfaces ✅ done, not yet pushed

Planned 2026-10-14, from [`ReverseEngineering.md`](ReverseEngineering.md). Phase 9 made what the app
does trustworthy when nobody is watching; this is the batch that makes it *reachable* — a question
asked without leaving the application you are in, a picture of what you were looking at, a way back
to the window after it is closed, and (the one browser item) a way to see why a page is broken. The
archive's own reason for putting it here is the ratio: one more window, one more chord and one tray
icon is the largest daily-habit change in the whole document for the least architecture, and all
three competitors ship it.

Two rules hold across every item, and neither is negotiable. **The quick window is the same app** —
it calls the same `create_chat` and `send_message` the main window calls, so there is one run path
rather than a second one that drifts. And **a chord that cannot be registered is reported**, not
swallowed: a launcher that collides with another app is the first thing a user hits, so the failure
has to reach the screen that set it.

Order is the archive's, with one change said out loud: **9.4 (record a browser workflow) is not in
this batch at all.** The archive marks it "V1 once 6.3 exists", and 6.3 — `recorder.rs`, reached
through 6.2's plugin bundles — does not exist. It stays in the archive rather than becoming a dead
line here.

### 10.1 [M] Quick entry on a global chord ✅
- **Why**: a small always-on-top window over any app, on a shortcut, is how users stop switching
  windows to ask something. It is one extra window and one shortcut — and the pieces are already
  here: the global-shortcut plugin is registered for the `Ctrl+Alt+Esc` kill switch, and the app
  already reacts to a window's focus.
- **Files**: `src-tauri/src/quick.rs` (new — the one rule for a chord: `WINDOW`, `DEFAULT_CHORD`,
  `parse`, `label`, and `rebind`, which is what "changeable and reportable" means); `lib.rs` (the
  chord registered beside the kill switch, the handler branch that says which shortcut fired, and
  the `show_quick_entry` registration); `store/settings.rs` (`quick_chord`, `quick_target`, their
  defaults and both round-trip assertions); `commands.rs` (`save_settings` rebinds before it
  writes and puts the chord back if the write fails; `show_quick_entry`); `tauri.conf.json` (the
  window: 560×132, undecorated, transparent, always-on-top, off the taskbar, `visible: false`);
  `src/components/QuickEntry.tsx` (new); `src/main.tsx` (which window to render);
  `src/lib/commands.ts` + `App.tsx` (the palette's *Quick entry*); `src/api.ts`, `src/types.ts`,
  `SettingsDialog.tsx` (the chord, the target, *Try it*).
- **Test**: `quick::tests` — the default parses; `Ctrl+Alt+Space`, `control+alt+space` and
  `ctrl + alt + space` all produce the same `label`, which is the one the handler compares against;
  a chord that is not a chord is refused by name (`Banana+Shift` is quoted back, `Ctrl+Alt` with no
  key is refused, empty is refused); and an unparseable chord yields an **empty** label rather than
  a panic, because the handler asks for it on every keypress and a hand-edited settings file must
  not be able to take the app down. `store::settings::tests` gained the pair: a file with no
  `quickChord` at all gets the default rather than an empty string (an upgrading install must not
  lose the chord it never had to set), and a chord survives the file in both directions.
  Falsified: making `label` return the user's text instead of the plugin's spelling
  (`parse(chord).map(…).unwrap_or_default()` → `chord.trim().to_string()`) fails it on
  `a_chord_is_the_same_however_it_is_spelled` and `an_unknown_chord_has_no_label_to_match_against`,
  and restoring passes. `cargo test` 361 passed. Three things said out loud: the window is declared
  in `tauri.conf.json` rather than built in `setup`, so it is a config it is hard to get wrong and
  the front end can tell the two windows apart by label; the controls live in **Settings →
  Commands** beside the keyboard list rather than in a section of their own, because the quick chord
  *is* a keyboard shortcut; and the chord is stored as the user typed it and compared as the plugin
  prints it, which is what makes `ctrl+alt+space` and `Ctrl+Alt+Space` the same chord in both
  directions.
- **Stop**: a question can be asked from inside another application without leaving it. One limit
  named: a chord another application already owns cannot be registered, and the save is refused by
  name rather than half-applied — the shortcut you had keeps working, and the rest of that screen
  stays unsaved, which is what refusing means.

### 10.2 [S] An Appshot from the focused window ✅
- **Why**: ChatGPT's Appshots bring a screenshot and the text of the frontmost window into a
  conversation. DeskSeek has the text half (`ui_snapshot`); a picture of the window, filed into the
  chat like any other attachment, is the missing half — and it is what makes "this error, here" a
  question you can ask without typing the error.
- **Files**: `src-tauri/src/tools/desktop.rs` (`Shot`, `encode_png`, `remember_foreground`,
  `capture_focused`, and a `shot` module holding the handle remembered at chord time and the GDI
  capture); `Cargo.toml` (`png`, which the lockfile already resolved through `tauri-codegen` — the
  diff is one line in `deskseek`'s own dependency list and no new crate; `windows-sys` gains
  `Win32_Graphics_Gdi` and `Win32_UI_WindowsAndMessaging`); `quick.rs` (`open` remembers the window
  before it takes the focus); `store/paths.rs` (`shots_dir`, one fixed name overwritten each time);
  `commands.rs` (`capture_focused_window`) and `lib.rs` (its registration); `src/components/QuickEntry.tsx`
  (the 📷 button, the thumbnail row, and the two attachments on send); `src/api.ts`, `src/types.ts`.
- **Shape**: the *identity* of the window that was in front is remembered when the quick window is
  **raised**, and the button captures *that* handle by `PrintWindow` — because by the time a button
  is clickable our own window has the focus, which is precisely the window nobody wants a picture
  of. Both halves come back: the PNG and the window's readable text, read from the same
  accessibility tree `ui_snapshot` walks. They are staged under `shots/` and only reach the chat
  when the message is sent, through the same `attach_path` a dropped file uses.
- **Test**: `desktop::tests::a_png_round_trips_through_our_own_encoder` — four pixels with distinct
  colours *and* a distinct alpha encode to bytes that carry the PNG signature, decode back at the
  same size and colour type, and come out byte-identical, so neither a channel swap nor a forced
  alpha can pass. `nothing_to_encode_is_refused_rather_than_written` covers the guard: a zero-sized
  capture and a buffer too small for the size it claims are both refused rather than half-written.
  Falsified: making the encoder write `Grayscale` instead of `Rgba` fails the round trip on the
  `write_image_data` panic inside it, and restoring passes. `cargo test` 363 passed. The capture
  itself needs a real window and a real compositor, so it is E2E. Three things said out loud:
  `PrintWindow` is **not** in `windows-sys`, so it is declared in a four-line `unsafe extern`
  block against the user32 the build already links rather than pulling in the far larger `windows`
  crate; a window that will not draw itself is **refused with a sentence** rather than falling back
  to a screen grab, because a screen grab of a window we are covering would photograph *us*; and
  the alpha byte GDI leaves at zero is forced to 255, which is the difference between a picture and
  a fully transparent rectangle.
- **Stop**: a picture of the window you were just looking at lands in a conversation with one press.
  One limit named: it is Windows-only, matching `ui_snapshot`, and the text half is whatever the
  accessibility tree holds — a canvas or a game gives the picture and says the text could not be
  read.

### 10.4 [S] A tray entry ✅
- **Why**: all three competitors ship one, and it is the natural home for the things you want when
  the window is closed: a new chat, quick entry, and a way back. It is also the only surface that
  exists while the window is just the tray.
- **Files**: `src-tauri/src/tray.rs` (**new** — the menu as one table, `install`, the event, and
  the two tests); `Cargo.toml` (`tauri`'s `tray-icon` feature); `lib.rs` (`tray::install` in
  `setup`, and `CloseRequested` hiding the main window instead of ending the process);
  `src/App.tsx` (the `tray-action` listener, into the palette's own action switch); `src/api.ts`
  (`onTray`), `src/types.ts` (`TrayEvent`); `src/lib/tray.check.ts` (**new**).
- **Shape**: four items, and each row in the table says which side runs it. *New chat* and *Quick
  entry* are **emitted** and handed to the same `runPalette` switch the command palette uses, so
  there is one implementation of "new chat" rather than a third one that happens to look like it;
  *Show DeskSeek* and *Quit* are done in Rust, because it owns the window and the process. A left
  click on the icon is *Show DeskSeek* — what every other tray icon on the platform does — and the
  menu is on right-click.
- **Test**: `tray::tests::the_tray_menu_is_the_four_things_the_window_can_do` — four rows, four
  unique ids, no blank labels, and *Show*/*Quit* pinned as the two this process runs (`run` reads
  the same flag, so a flipped bit would silently make *New chat* quit the app).
  `the_ids_the_window_runs_are_the_palette_s_own` pins the other two. The cross-language half is
  `src/lib/tray.check.ts`, the **sixteenth** check script: it reads the table out of `tray.rs` and
  the action ids out of `src/lib/commands.ts` and holds one to the other, so an id renamed on either
  side fails the gate instead of leaving a menu item that does nothing. Falsified: renaming the
  forwarded `quick-entry` to `quick-entry-window` fails it with
  `the tray forwards "quick-entry-window" to the window, but the palette has no such action`, and
  restoring passes. `cargo test` 365 passed.
- **Stop**: the app is reachable with its window closed. One limit named: a machine whose shell has
  the notification area switched off cannot create the tray, and the close handler asks whether one
  exists before it hides anything — so there, closing the window quits the way it always did, rather
  than leaving a process nobody can reach.

### 9.2 [S] The browser's console, network and tabs ✅
- **Why**: Claude in Chrome reads console errors, network requests and DOM state, which is what
  makes it useful for debugging a web app — and it is the archive's named prerequisite for every
  later browser feature. CDP already offers all three; the browser session simply threw its events
  away.
- **Files**: `src-tauri/src/tools/browser.rs` (`ConsoleEntry`, `NetEntry` and the two bounded ring
  buffers on the `Session`; the pump now files events instead of skipping them; `collect` and the
  four `record_*` functions; `render_console`/`render_network`/`render_tabs`; `list_pages`,
  `wait_for_new_tab`, `close_target`, `attach` and `switch_to`; the four new tools and the
  tab-aware `browser_open`); `tools/mod.rs` (four tool records, three `Read` and `browser_tab`
  `Computer`, and `new_tab` on `browser_open`); `conversation/agent.rs` (four prompt lines);
  `src/components/ToolCallCard.tsx` (icons and descriptions); `README.md`.
- **Test**: four new tests in `browser.rs`, all over the pure half — `a_console_entry_keeps_its_level_and_text`
  (a message keeps its level and source; a missing level falls back to `log` rather than rendering
  as a blank; a shape with no text is dropped rather than becoming an empty line),
  `a_request_and_its_response_become_one_line` (a request in flight says `pending`; the response
  joins it instead of adding a row; a response for a request we never saw is dropped; a failure
  replaces the status), `the_buffers_keep_the_newest_and_drop_the_oldest` (305 requests leave 300,
  and the five that went are the five oldest), and `a_page_list_marks_the_one_being_driven` (exactly
  one tab is marked, an untitled one still reads, and an empty list says so). The class tests in
  `tools/mod.rs` grew the four names: the three reads are offered in `Plan` mode and the acting one
  is not. Falsified: dropping the response join (making `record_response` return immediately) fails
  `a_request_and_its_response_become_one_line` on the row count, and restoring passes.
  `cargo test` 369 passed. Four things said out loud: the plan named `ToolStepGroup.tsx` and the
  card it meant is `ToolCallCard.tsx`; tabs are done by **re-attaching** the page socket and reading
  the tab list over the debugging port's own HTTP endpoints, rather than by opening a second,
  browser-level CDP connection — `window.open` creates a tab from the page itself, and one socket is
  one page; `Runtime.consoleAPICalled` is deliberately **not** enabled, because `Log` already
  reports the same messages and a log with every line twice is worse than one with every line once;
  and switching tabs **drops** the console and the network buffer, because they described the page
  you left and keeping them would attribute one page's errors to another. The capture itself needs a
  real browser, so the end-to-end half is `it_drives_a_real_browser_end_to_end` plus the checklist.
- **Stop**: a broken page can be explained from what it logged, not guessed at. One limit named: the
  log starts when DeskSeek attaches — Chrome reports events as they happen and does not replay what
  a page logged before — so `browser_console` says that in words rather than showing an empty list
  that reads like "nothing went wrong".

---

## Phase 11: Triggers ✅ done, not yet pushed

Planned 2026-10-15, from [`ReverseEngineering.md`](ReverseEngineering.md). Phase 10 made the app
*reachable*; this is the batch that makes the scheduler an automation engine — a task that starts
because something changed rather than because a clock said so. All three items are the archive's,
and all three stay local: no cloud account, no new dependency, no server.

One rule holds across all three, and it is the reason they are one batch rather than three
features. **A trigger is a scheduled task with a different kind.** It has the same prompt, the same
chat-or-project, the same `enabled`, the same run path (`fire_scheduled`), the same run history and
the same row in the Scheduled panel. What changes is what makes it due. A second table would have
been a second answer to "what is a task", and the panel, the history and the spend attribution
would each have had to learn about it. The consequence is that `next_run_at` means "next **check**"
for a poll trigger, and the one 30-second tick in `lib.rs::scheduled_tick` drives both kinds — no
second scheduler.

The second rule is the edge. **A poll is not a fire.** Every trigger keeps the last thing it saw in
`last_state`, and fires only when a fresh look *differs* from it. A condition that is true is not
the same as a condition that just *became* true, and without that distinction a folder that stays
busy, or a page whose text merely exists, would start a run every interval forever. A `NULL`
`last_state` means "never looked": the first observation stores the fingerprint and does not fire,
which is also what makes a trigger that was added while the app was shut start at most one run when
it opens rather than replaying the whole time it was closed.

### 7.2 [M] Event triggers — a file or a git ref ✅
- **Why**: ChatGPT starts a task when something happens in Gmail, Slack or GitHub; a cadence-only
  scheduler either fires too often or too late. The two local kinds need no cloud account: a file
  appearing in a folder, and a git ref moving.
- **Files**: `src-tauri/src/triggers.rs` (**new** — `Kind`, `Spec`, the two fingerprints, the
  decision function and `tick`); `store/db.rs` (seven columns on `scheduled_tasks`, `touch_scheduled`,
  `set_scheduled_note`, `set_scheduled_state`, `fires_since`, and the `Spec` threaded through create
  and update, which now clears `last_state`); `lib.rs` (`mod triggers`, and the tick branching on the
  kind); `commands.rs` (`create_scheduled_task` and `update_scheduled` carry the `Spec` and ask
  `Spec::first_check` for the first look); `conversation/agent.rs` (the model's `schedule_task` still
  makes a cadence); `src/components/ScheduledPanel.tsx` (the kind picker, the target/filter/interval
  form and the trigger's own row), `src/api.ts` + `src/types.ts` (the four fields on the draft and
  the seven on the task).
- **Shape**: `Spec { kind, target, filter, interval_secs }`. A *file* trigger fingerprints the
  folder — the matching entries' names and mtimes, sorted — so a new file and a changed file both
  move it and a file that merely sits there does not. A *git* trigger fingerprints
  `git rev-parse <ref>` from the folder you point it at. Both reuse what is here:
  `walk::wildcard_match` is the glob, and the process is asked rather than `.git` being read, because
  a branch may be packed, HEAD may be detached and a worktree keeps its refs elsewhere.
- **Test**: `triggers::tests`, eight of them — `decide(None, …)` is `Arm` and never `Fire`, a
  repeated fingerprint is `Same` and a changed one is `Fire` (including to and from the empty
  string, which is a fingerprint like any other); a scratch folder read twice fingerprints the same
  and a new `*.csv` moves it while a new `*.txt` does not; a blank filter matches every name; a
  folder that is not there is an error rather than an empty fingerprint, which would read as "the
  folder is empty"; a trigger with no target is refused by the name of what is missing; a misspelt
  kind keeps the calendar rather than becoming a trigger that could never fire; and a one-second
  interval is clamped to the tick. `store::db::tests` gained
  `a_trigger_keeps_what_it_watches_and_forgets_its_state_when_edited` (the seven columns survive
  create, an edit re-arms) and `the_trigger_cap_counts_runs_inside_its_window_and_no_others` (the
  run from before the hour is not counted, and another task's runs are not this task's), and
  `an_older_database_upgrades_instead_of_refusing_to_open` now creates a pre-trigger
  `scheduled_tasks` on a real file and asserts the seven ALTERs landed. Falsified: making `decide`
  fire on the first look (`None => Decision::Arm` → `Decision::Fire`) fails
  `a_condition_that_stays_true_fires_once` on `left == right` and nothing else, and restoring
  passes. `cargo test` 379 passed. Five things said out loud: the kinds live in `triggers.rs` rather
  than in `schedule.rs` as the plan guessed, because that file owns the calendar and this owns the
  edge; `fs_watch.rs` is untouched and the check is a **poll** — a folder that writes fifty files in
  one interval is one run rather than fifty, and there is no baseline to reconstruct for the time
  the app was closed; the cap is **derived** from `turn_usage` rather than kept as a second counter
  that could disagree with the run history; a capped change leaves the fingerprint alone, so the cap
  delays a run rather than dropping it; and the model's `schedule_task` tool still only makes
  cadences, because a folder or a URL is something a person picks. The git check asks `git`
  through `exec::capture` rather than spawning it directly — `exec.rs` is the one recipe for
  running a program, and it is the difference between a hidden child and a console window
  flashing every interval — and `is_object_id` holds the answer to its shape (hex, 40 or 64
  characters), because `rev-parse` prints either a commit or its reason for having none and an
  error message stored as a fingerprint would look exactly like a commit. That last part arrived in
  the phase's fix commit rather than this one, and is why Phase 11's last `cargo test` reads **385
  passed** rather than the 379 this line first recorded.
- **Stop**: "when a new file lands in this folder" starts a task with no cadence involved. One
  limit named: a file is noticed up to one interval late, because the check is a poll rather than an
  OS watch — and a change that happened and was undone while the app was shut is simply never seen.

### 7.3 [S] A monitor — a page condition, on an interval ✅
- **Why**: half the useful automations are "tell me when this changes". The archive calls it a
  monitor: a prompt, an interval, a bounded check and a state.
- **Files**: `src-tauri/src/triggers.rs` (`Kind::Url`, `page_fingerprint` and `url_fingerprint`,
  and `look` takes the app's HTTP client again); `src/components/ScheduledPanel.tsx` (a fourth kind,
  and the labels that differ per kind instead of a single guess). `commands.rs`, `lib.rs` and the
  database are untouched: the branch, the columns and the run path are the ones 7.2 landed. So is
  `tools/browse.rs`, which the archive guessed would need changing: `fetch_url` already did exactly
  what a monitor needs, and the whole item was picking it up.
- **Shape**: a *url* trigger fingerprints the page. With a filter it fingerprints *whether the page
  says it* (`yes`/`no`) and so fires on the transition to present — the predicate case. With a blank
  filter it fingerprints a hash of the page's text and so fires on any change — the "tell me when
  this changes" case. Both are the same trigger with a different fingerprint, and both reuse the
  edge rule and the cap. The fetch is `browse::fetch_url`, the one the `fetch_url` tool already
  uses, so nothing new decides how this app reads a page; no new dependency was needed.
- **Test**: `triggers::tests` grew two, both over the pure half —
  `a_page_condition_fires_on_the_transition_it_describes` (a page that does not say it is `no`,
  `Now IN STOCK.` is `yes` because the page's capitalisation is not the point, becoming true is a
  `Fire` and staying true is `Same`) and `an_empty_page_filter_watches_the_whole_page` (a blank
  filter and no filter are the same trigger, one character is a change).
  `a_trigger_needs_something_to_watch` now loops all three kinds and holds each refusal to naming
  its own field — `folder`, `repository`, `URL`. Falsified: making the filtered branch return the
  page's text instead of `yes`/`no` fails on the string length, and restoring passes. `cargo test`
  381 passed. Three things said out loud: the check is a **fetch of a page the user typed**, not a
  model-chosen destination, so it is a declared destination and needs no egress decision — a monitor
  has no chat until it fires, and there is no chat policy to consult at the moment it looks; the
  fingerprint is of the page's *text*, with the markup stripped, so a rotating ad script or a
  changing nonce in an attribute does not wake a task; and the archive's "or a file stat" half of
  this item is the file trigger 7.2 already built, deliberately not a second one.
- **Stop**: "tell me when this changes" starts a task without a cadence. One limit named: it looks
  on the interval, so a page that changes and changes back between two looks is never seen, and a
  page that needs JavaScript to say anything reads as whatever the server sent.

### 3.3 [M] MCP events — a server wakes a task ✅
- **Why**: ChatGPT lets a user act on updates a server pushes, turning a polling schedule into an
  event. DeskSeek's MCP client was complete apart from one thing: it was built with no handler at
  all — `()` — so a notification from a server had nowhere to land and was dropped without a trace.
- **Files**: `src-tauri/src/mcp/manager.rs` (`Events`, the `ClientHandler` the client is now built
  with, `Events::new`, `Logs::handle`, and the two `serve` calls that install it); `triggers.rs`
  (`Kind::Mcp`, `NEVER`, `matching_tasks`, `on_mcp_event`, and the `polled` guard the tick needs);
  `src/components/ScheduledPanel.tsx` (the fifth kind, its own labels, a server picker reading
  `api.mcpServers()`, and a row that never renders the never-due sentinel as a date). `db.rs` and
  `commands.rs` are untouched, and that is a deliberate deviation from the archive's Files list: it
  guessed an *inbound event command*, but the notification arrives inside the MCP client, so the
  handler hands it straight to `triggers::on_mcp_event` — a round trip through the window to say
  "a server pushed something" would add a hop and a failure mode for nothing. The subscription list
  is the `mcp` rows themselves rather than a second table, for the same reason the triggers share
  `scheduled_tasks`: one place owns "what is a task".
- **Shape**: rmcp already routes any `notifications/*` a server sends to `on_custom_notification`,
  so the handler is a struct holding the server's name and the window, and the `Client` alias grows
  the handler from `()` to that struct — the one-line change the archive meant by "an extension
  rather than a rewrite". The notification is handed to `triggers::on_mcp_event` on a spawned task,
  because it arrives inside the client's own read loop and a run must not hold the handshake open.
  Every enabled `mcp` trigger whose target is that server, and whose filter is the method (or blank,
  or `*`), fires; the run path is `fire_scheduled`, so it parks and is billed like any other. An
  `mcp` trigger is never polled, so its `next_run_at` is `NEVER` and the tick skips it before it
  looks — and recording a run keeps it never-due rather than accidentally scheduling a check.
- **Test**: `triggers::tests` grew three — `one_notification_matches_the_tasks_that_asked_for_it`
  (the named method and the wildcard both match; a different method matches only the wildcard; a
  server nobody subscribed to wakes nothing; and a `file` trigger naming the same folder is not an
  MCP trigger), `a_disabled_trigger_is_not_woken` (pausing has to stop an event, not just a poll),
  and `an_mcp_trigger_is_never_due_and_is_never_looked_at` (`first_check` is `NEVER`, `polled` is
  false, and the sentinel did not leak into the polled kinds). Falsified: making `matching_tasks`
  stop comparing the server's name (`…eq_ignore_ascii_case(server.trim())` → `!row.target.trim()
  .is_empty()`) fails `one_notification_matches_the_tasks_that_asked_for_it` with `left: 3, right: 2`
  — a server nobody subscribed to woke a task — and restoring passes. `cargo test` 384 passed. The
  handler's own half needs a server that pushes something, so it is E2E. Three things said out loud:
  the client
  used to be `RunningService<RoleClient, ()>` and is now built with a handler, which is the whole
  of the protocol-side change; the `Events` struct **deliberately implements nothing else**, because
  sampling, elicitation and roots are the server asking the app for something and there is nothing
  to ask for; and the archive's open decision — whether app-closed events are replayed on launch —
  is taken as **no**. A notification that arrives while the app is shut is gone, because there was
  nothing listening and a backlog would mean a server able to queue runs inside DeskSeek.
- **Stop**: a server can wake a task without a cadence. One limit named: almost nothing in the MCP
  ecosystem sends a server-initiated notification today, so in practice this is a hook waiting for
  one — which is why its test is over the matching rule rather than over a real server, and why the
  panel's own text says the task runs only while the app is open rather than promising otherwise.

---

## Phase 12: Providers, then everything they unblock ✅ done, not yet pushed

Eight items in the archive's order, and the order is the argument: the two wire dialects first,
because a half-migrated provider layer is worse than a single-provider app; then the three decisions
that ride on it (per-task model, effort, the routing suggestion); then the four capabilities that
only exist once a model can see or speak (vision, computer use, audio, dictation). The 2026-10-13
decision recorded at the top of this file is what makes this cheaper than the teardown the archive
first implied — two dialects, each one wire shape, and every named vendor is configuration rather
than a module.

### 2.1 [M] Two wire dialects, N endpoints ✅
- **Why**: every competitor lets the model be chosen, and it is a cost decision before it is a brand
  one — a cheap model for a nightly triage, an expensive one for a hard refactor. It is also the gate
  on the last four items of this phase: nothing that needs vision or speech starts until a second
  wire shape exists.
- **Files**: new `api/provider.rs` (`Dialect`, `Endpoint`, `Model`, `Price`, the `DialectImpl` seam,
  `Wire`, `WirePart`, `merge_usage`, `messages_for`, and the seed endpoint); new
  `api/openai_compat.rs` and `api/anthropic_compat.rs` (one wire shape each, every method pure so a
  wire format is testable without a network); `api/client.rs` (the HTTP/SSE/retry engine, now
  parameterised by an endpoint and delegating every shape question to the dialect);
  `api/stream.rs` (the accumulator, rewritten over `WirePart`s and keyed by the dialect's own tool
  index rather than by position — Anthropic numbers *content blocks*, so a text block can occupy 0
  and the first tool call 1, and a list would hand the model a nameless call at the front);
  `api/models.rs` (reduced to the clock and the budget, since a rate belongs on the endpoint's model
  record); `store/settings.rs` (`endpoints` + `default_endpoint`, the legacy `base_url` folded in and
  blanked once, and the three lookups — `model_info`, `context_for`, `summary_model`);
  `store/keychain.rs` (a key per `key_ref`); the five `Client::new` call sites; `commands.rs`
  (endpoint-scoped key commands, `test_endpoint`, `list_models` from the endpoint, and one
  `ready_to_send` that both turn paths and the environment probe share);
  `src/components/EndpointsSection.tsx` (new), `SettingsDialog.tsx`, `types.ts`, `api.ts`.
- **Shape**: one place per dialect knows a wire shape and nothing else does — the seam is a trait,
  so "exactly one place" is structural rather than a rule somebody has to remember. DeepSeek is
  endpoint one, speaks the OpenAI dialect and keeps the keychain account the key was already stored
  under, so an existing install re-enters nothing; the migration is idempotent and blanks the old
  field as it folds it in. Three deviations from the archive's Files list, all of them smaller: the
  price table lives on the model records rather than in `models.rs`, because a rate belongs to an
  *endpoint and* a model — the same id can cost different money in two places; `api/models.rs`
  keeps only the clock rule and the budget rule; and the front end needed a component the archive
  did not name, because an endpoint list is a list of forms. Two things the OpenAI dialect does that
  the other cannot, said out loud rather than hidden: it takes `max_tokens` as optional (so nothing
  is sent, and DeepSeek's own maximum applies) where Anthropic requires it and gets the model's
  declared output capped at 32,000; and its `thinking` field and `reasoning_content` echo are
  DeepSeek's extensions to the dialect, which a stricter server would call a third dialect — a
  dialect, not a vendor client. Anthropic's prior-turn reasoning is deliberately **not** echoed
  back, because this wire refuses a `thinking` block without its signature, which is what lets one
  history serve both.
- **Test**: `cargo test` — per dialect, the exact request body asserted field by field, the stream
  assembly, the tool-call round trip across fragmented chunks, a payload neither dialect knows being
  skipped rather than fatal, the base-URL join with a trailing slash, the non-stream reply, and the
  unlisted-model and unknown-price paths. The Anthropic side gets the three shapes that make it a
  second dialect rather than a copy: `system` as a parameter, `tool_use`/`tool_result` blocks, and
  parallel results gathered into **one** user message. Falsified: making a missing rate read as zero
  (`let price = self.price?` → `unwrap_or(Price { .. 0.0 })`) fails
  `a_rate_nobody_published_is_unknown_and_not_zero` with `left: Some(0.0), right: None` — the spend
  view quietly agreeing with a guessed number — and restoring passes. `cargo test` 385 → 430. Two
  things need a real window and are on `docs/E2E-CHECKLIST.md` instead: a live turn against a real
  endpoint per dialect, and one against a local model with no key at all.
- **Stop**: adding an endpoint is a form and no code, and switching endpoint runs a turn.

### 2.2 [S] Per-task and per-chat model selection ✅
- **Why**: a scheduled triage at 06:00 does not need the frontier model, and its bill should say so.
  The chat side already has a model picker; what is missing is the unattended half.
- **Files**: `store/db.rs` (a `model` column on `scheduled_tasks`, in the `CREATE TABLE` *and* as a
  guarded `ALTER`, appended last in both so a fresh file and an upgraded one hold it in the same
  place; `scheduled_model`, the one reader); `commands.rs` (the field through `create_scheduled_task`
  and both scheduling commands, and the fresh chat a run creates); `conversation/agent.rs` (the pin
  resolved once for a run, and used for the request, the price, the window and the usage row);
  `ScheduledPanel.tsx` (a picker on the form, the pin on the row, and the model chip in the run
  list), `types.ts`, `api.ts`.
- **Shape**: the pin is a **model id and nothing more** — the endpoint is whatever is default when
  the task fires, because a task outlives any particular endpoint. Blank is the ordinary value and
  means the chat's own model, which is exactly what every task made before this column existed gets.
  The pin wins for the **run only**: `run` resolves it once into a local `model`, so a task pointed
  at an existing conversation does not quietly move that conversation on to a cheaper model. Two
  smaller decisions, taken to keep one answer per question: the fresh chat a run creates is *also*
  created on the pin, so opening it later and carrying on does not change what ran; and `run` reads
  the pin from the row rather than taking it as a parameter, so the tick and *Run now* cannot pass
  it differently.
- **Test**: `cargo test` — `a_task_can_pin_a_model_and_no_pin_reads_as_none` (blank reads as none, a
  task that does not exist reads as none, an edit writes a pin, and blanking it puts the task back on
  the chat's model); the trigger round-trip test gained the pin surviving an edit; and the upgrade
  test asserts an old task's `model` is `''` on a real file. Falsified: dropping the blank filter in
  `scheduled_model` fails that test with `left: Some(""), right: None` — the empty string handed to
  the run as a model id — after checking the mutation had reached the disk, and restoring passes.
  `cargo test` 430 → 431. What a unit test cannot reach is the wiring from a fired task to the
  request, so the six lines for it are on `docs/E2E-CHECKLIST.md`.
- **Stop**: a scheduled task can be pinned to a cheaper model and its spend rows say which ran.

### 2.3 [S] Reasoning-effort control ✅
- **Why**: a slider over how much thought a model puts in — the cheapest way to buy quality only
  where it is needed.
- **Files**: `api/provider.rs` (`Model::effort`, the flag that answers the question, and its
  serde default), `api/openai_compat.rs` (the level as `reasoning_effort`, unchanged and already
  asserted on the wire), `api/anthropic_compat.rs` (`effort_budget`, and a `thinking_budget` that
  now *requires* a level), `conversation/agent.rs` (the pair of thinking fields sent only when the
  model takes them), `ChatView.tsx` (the controls, or the sentence that replaces them),
  `EndpointsSection.tsx` and `types.ts`.
- **Shape**: the effort control and the chat's model picker already existed, so what this item
  really settles is **where the question lives**. The archive said per *provider*; both shipped
  dialects map a level — one to `reasoning_effort`, the other to a thinking budget — so a
  dialect-level `supports_effort` could only ever have answered `true`, and it was **deleted**
  rather than kept as a second answer to a question the model record already answers. What actually
  varies is the model: a small local one has no thinking to budget. `Model::effort` is on by
  default — the interesting half, because a settings file written before the field existed keeps
  the controls it already had — and off means the chat shows a sentence instead of a dead switch,
  and the request carries **neither** field. That last part is the load-bearing rule: on the
  Anthropic wire the level *is* the switch, so sending `thinking` without a level would be a
  request the model never agreed to.
- **Test**: `cargo test` — `thinking_budget` for each level and for the two ways it is absent
  (thinking off; thinking on with no level, which is the new case), the OpenAI body still carrying
  `reasoning_effort`, and a model record written before `effort` existed loading with it *on* —
  the JSON analogue of a guarded `ALTER`. Falsified: letting `thinking_budget` fall back to a
  default level (`as_deref()?` → `as_deref().unwrap_or("high")`) fails
  `effort_becomes_a_thinking_budget_that_fits_under_the_ceiling` on
  `assertion failed: ANTHROPIC.body(&model(), &no_level, &[]).get("thinking").is_none()` after
  checking the mutation reached the disk, and restoring passes. `cargo test` 431 → 432.
- **Stop**: the effort control either works or says it cannot, and never silently does nothing.

### 13.2 [S] Cheap-model routing with a recommendation ✅
- **Why**: every vendor picks a model per surface and prices it; the honest app-side counterpart of
  a routing habit is a suggestion at the point of the decision.
- **Files**: `api/provider.rs` (`Advice`, `advise`, and the `WORTH_MENTIONING` threshold);
  `conversation/agent.rs` (the suggestion computed from the round's own tokens and carried on the
  `chat-done` event); `ChatView.tsx` (the line beside the model picker, which is also the switch);
  `ScheduledPanel.tsx` (the published rate on each model in the pin picker), `types.ts`.
- **Shape**: **never silently route.** The suggestion is a line of text with a number — what this
  turn's tokens would have cost on a cheaper model of the same endpoint — and the model only
  changes if the user clicks it. It carries the turn's own tokens rather than a rate comparison, so
  the number is about work that actually happened; it is computed in Rust, because the price table
  lives there and a window that multiplied tokens by a rate would be a second answer to "what did
  that cost". Silence is the common answer and the right one: no tokens, an unpriced model, one
  model to choose from, or a saving small enough that mentioning it would be noise. One deviation
  from the archive's Files list, and it is the panel's half: the pin picker shows each model's
  *published* output rate in its label rather than a second estimate, because a task has no tokens
  to compare until it has run — the rate is a fact the backend published, and no arithmetic on it
  happens in the window.
- **Test**: `cargo test` — the suggestion names the cheapest model that would really have saved
  money, and stays quiet for each of the four reasons above; a saving of half a cent is not worth a
  line while half the rate is. Falsified: dropping the threshold (`*usd < spent * WORTH_MENTIONING`
  → `*usd < spent`) fails `a_saving_too_small_to_act_on_is_not_worth_saying` with
  `left: Some(Advice { model: "nearly", usd: 1.99, spent: 2.0 }), right: None` — a suggestion
  nobody would act on — after checking the mutation reached the disk, and restoring passes.
  `cargo test` 432 → 434 (two of the new tests are the quiet cases).
- **Stop**: a cheaper model is suggested with a number, and never chosen for the user.

### 9.1 [M] Vision into tool results ✅
- **Why**: the README says screenshots go to the user's eyes because DeepSeek's API is text-only. A
  vision-capable model removes the reason, and one change upgrades both browser control and computer
  use from blind to able to see what happened.
- **Files**: `tools/mod.rs` (`ToolOutput`, `ToolImage`, the `MODEL_MAX_SIDE` ceiling, and the
  `ui_screenshot` declaration); `tools/desktop.rs` (`capture`/`grab_now`, `scale_factor`,
  `downscale_rgba`, and the screenshot tool that files the full-size PNG and sends the smaller copy);
  `tools/browser.rs` (`viewport_of`, `model_scale`, and a `screenshot` that hands over what it
  captured); `api/types.rs` (`data_url_mime`, one reader for a `data:` URL's type);
  `conversation/agent.rs` (the tool result carries an image, and the loop files it as an attachment);
  `api/provider.rs` (2.1's `messages_for` is the filter that drops it for a model that cannot see);
  `conversation/assemble.rs` (the shape test); `README.md` in both places it promised text-only.
- **Shape**: a tool result grew from a `String` to `ToolOutput` — text plus an optional picture —
  because the alternative was a second return channel beside the tool's own that the loop would
  have to remember to join. The picture rides to the model as an **attachment on the tool message**,
  the same shape a user's own attachment uses, so nothing downstream needed a case for it: the
  message layer already turns an image attachment into content parts, and 2.1's `messages_for`
  already strips images for a model that cannot see. **No new dependency**: the browser asks CDP for
  a scaled capture (`clip.scale`, one extra `Page.getLayoutMetrics`), and the desktop does integer
  box-averaging — picking one pixel per block instead would alias a screenshot's text into mush,
  which is most of what a screenshot is for. Both stop at 1280px on the longest side, in one shared
  constant. Three smaller decisions: the desktop keeps the full-size file and the browser does not
  (a browser shot is in the chat, where the user was looking; the Appshot is the one that has always
  been a file); the tool's text says what happened in both cases, because *whether* a picture is sent
  is decided later and by the endpoint; and `ui_screenshot` photographs whatever is in front, which
  is the same window `ui_snapshot` reads, so the picture and the tree agree.
- **Test**: `cargo test` — `scale_factor` (only shrunk when too big, the longest side decides),
  `downscale_rgba` (the average and not a picked pixel, factor 1 is the same picture, and the four
  ways there is nothing to scale), `model_scale` and `viewport_of` (the visual viewport wins, an
  older browser's layout-only metrics still answer, nothing usable is not an answer), and the
  result shape the archive asked for: `a_tool_result_that_took_a_picture_carries_both_halves`, which
  asserts a text part *and* an image part on a `tool` message that keeps its `tool_call_id`. Two of
  these failed first and both were mine: an average I had worked out wrong (191, not 63), and a real
  bug the test caught — `viewport_of` used `?` inside its loop, so an older browser's missing
  `cssVisualViewport` aborted the search instead of falling through to the layout viewport. Falsified:
  making the vision filter unconditional (`if model.vision` → `if true`) fails
  `only_a_model_that_can_see_is_sent_a_picture` with `the image must be dropped, not sent — left: 2,
  right: 1`, after checking the mutation reached the disk, and restoring passes. `cargo test` 435 →
  442. What a unit test cannot reach is a screen and a browser, so the six lines for those are on
  `docs/E2E-CHECKLIST.md`.
- **Stop**: a screenshot can be read by the model, and a model that cannot see is told so.

### 9.5 [M] Computer use: a coordinate click and an app inventory ✅
- **Why**: the desktop tool reads the accessibility tree, so a canvas or a game reports nothing and
  there is no raw coordinate click. Vision (9.1) plus a screenshot-anchored click closes that gap the
  way every vendor's computer use does.
- **Files**: `tools/desktop.rs` (`screen_point`, the `LAST_RECT` the capture records,
  `shot::click_screen`, `click_at`, `windows`, and `com::window_list`); `tools/mod.rs` (the two new
  declarations, and `is_action`); `conversation/agent.rs` (the ordering policy and the coordinate
  rule in the desktop guidance); `Cargo.toml` (the `Win32_UI_Input_KeyboardAndMouse` feature, which
  adds no crate); `README.md`.
- **Shape**: the rails are untouched — off by default, ask every time, the 25-action cap, the 250 ms
  spacing, `Ctrl+Alt+Esc` — and `ui_click_at` joins the list the budget counts, because it presses the
  mouse whatever it is aimed at. The one design decision worth writing down is **what the coordinates
  mean**. They are pixels in the picture the model was last shown, not pixels on the screen: the
  window's origin is recorded when it is photographed and added back when the click is aimed, so the
  two can never disagree about which window they describe. That is also why a click with no picture
  behind it is **refused with a sentence** rather than attempted — (120, 40) against no screenshot is
  a click somewhere arbitrary, which is worse than a refusal. Three smaller things: `SendInput` rather
  than the deprecated `mouse_event`; a 60 ms settle between moving the pointer and pressing, because a
  press that arrives first lands wherever the pointer still is; and `ui_windows` exposes the
  enumeration `ui_snapshot` already had as its fallback, so the agent names a window that exists
  instead of guessing a title and reading the refusal.
- **Test**: `cargo test` — `a_pixel_in_the_picture_becomes_a_point_on_the_screen` (including a window
  on a monitor above or left of the primary one, where the origin is negative) and
  `a_point_outside_the_picture_is_refused_rather_than_clamped` (the far edge is one *past* the last
  pixel, which is the off-by-one that clicks a neighbour); the tool-class test now covers both new
  names, and asserts that looking at the screen is not budgeted while clicking it is. Falsified:
  dropping the window's origin (`Some((origin.0 + x, origin.1 + y))` → `Some((x, y))`) fails that
  first test with `left: Some((0, 0)), right: Some((100, 50))` — a click inside the picture's bounds,
  so nothing is refused, landing on another application — after checking the mutation reached the
  disk, and restoring passes. `cargo test` 442 → 444. The click itself needs a real screen and a real
  canvas, so that is on `docs/E2E-CHECKLIST.md`.
- **Stop**: a canvas window can be driven by coordinate, behind the same rails as every other action.

### 8.5 [M] Audio Overview (text to speech) ✅
- **Why**: the most-loved notebook feature is two voices discussing the sources. It is also the one
  that needs a paid third-party service, so it has to be an explicit, priced action.
- **Files**: `api/speech.rs` (new — the one audio wire, the estimate, the script prompt and the
  spoken-script system prompt), `store/settings.rs` (a `Speech` block: the endpoint that speaks, the
  model, the voice, the published rate), `commands.rs` (`audio_script`, `audio_estimate`,
  `audio_overview`, and the one `speech_target` that resolves the endpoint and its key),
  `components/AudioOverviewDialog.tsx` (new — the quote, the editable script and the player),
  `components/EndpointsSection.tsx` (the Voice block, because voice *is* endpoint configuration),
  `lib/audio.ts` + `lib/audio.check.ts` (new — how a length and a price read), `App.tsx` (`/audio`),
  `ChatView.tsx` (the menu entry), and the `types`/`api` pairs for the three payloads.
- **Shape**: the plan's two rules are the shape of it. **Priced before it runs**: the script is
  written by one cheap call on the summariser's model (recorded on the same bill as everything else),
  and the clip is a second, deliberate action behind a quote that names the voice, the length and
  the money. **A file like any other**: the mp3 lands in the chat's own folder — where the file panel
  already browses and 📁 opens — so it is reopened the way every other file in a chat is. It refuses
  rather than guesses when no speech endpoint is named, and the script can be edited with the price
  following it.
- **Test**: `cargo test` — six in `api/speech.rs` (the estimate grows with the script, rounds *up*
  rather than down, reports an unpublished rate as unknown rather than free, keeps the cap inside
  what one request takes, and builds the body and the path this shape needs) and two in
  `store/settings.rs` (`the_speech_endpoint_is_matched_by_id_or_not_at_all`,
  `a_settings_file_written_before_voice_existed_still_loads`); `npm run check` gained
  `audio.check.ts` (a length never reads as zero minutes, a fraction of a cent is not rounded away,
  and an unknown rate never reads as `$0.00`), and `a11y.check.ts` now covers the new dialog. `cargo
  test` 444 → 453. Falsified twice, on the two rules this item is about: `priced: usd_per_minute >
  0.0` → `priced: true` fails `no_published_rate_is_unknown_money_rather_than_free` (an unpublished
  rate reported as free is the one money bug this app refuses), and dropping the empty-id guard in
  `speech_endpoint` so it falls back to `endpoints.first()` fails
  `the_speech_endpoint_is_matched_by_id_or_not_at_all` (a script sent to whichever endpoint happens
  to be first) — after checking each mutation reached the disk, and restoring passes. What needs a
  real endpoint and a real screen is on `docs/E2E-CHECKLIST.md`; the script's own quality is not
  asserted anywhere, because it is prose. **Five deviations, said out loud**: the archive's
  `tools/audio.rs` is `api/speech.rs`, because `tools/` is what the *model* can call and neither of
  these is — this is a wire beside the dialects and two commands, and the plan's Files list predates
  that distinction; the clip is **not** an artifact, because an artifact's every version is text in a
  table and an mp3 has no business in a text column, so the folder (this app's existing home for the
  files a chat owns) took that job and no schema moved; it is **one voice, not two** — the archive's
  inspiration is a two-voice discussion, and mixing two voices is an audio editor rather than a
  setting, so the honest v1 is one voice reading a script and the deviation is named here rather
  than discovered; the trigger is a **command** rather than a button, because a chat's own actions
  (`/export`, `/compact`, `/rewind`) already live in the composer and the slash menu is where they
  are found; and there is a **third command**, `audio_estimate`, which exists only because the
  script is editable — the price has to follow the words, and computing it in TypeScript would be a
  second copy of "how many characters is a minute" that could drift from the rate actually recorded.
- **Stop**: an audio overview can be produced, with its price shown first.

### 10.3 [M] Dictation ✅
- **Why**: talking is faster than typing for some inputs, and both vendors ship it.
- **Files**: `api/speech.rs` (`transcribe_url`, the hand-built `multipart` body, `transcribe` and its reply shape), `store/settings.rs` (the transcription model on the `Speech` block), `commands.rs` (`transcribe_audio`), `components/ChatView.tsx` (the 🎙 button and the recorder), `lib/audio.ts` + `lib/audio.check.ts` (`dictatedText`, `dictationName`, the cap), `lib/attachments.ts` (the one `readAsDataUrl`, widened to take a `Blob` — a recording has no name to satisfy a `File`), and the `types`/`api` pair.
- **Shape**: as the plan said — record, transcribe, **insert into the composer rather than sending**. The words are appended to whatever is in the box, and nothing leaves until Send is pressed. Which platform is supported is said in the message the webview itself produces, not guessed at.
- **Test**: `cargo test` — `the_multipart_body_is_the_shape_the_endpoint_reads` (byte-for-byte, because the body is hand-built) and `the_transcription_path_joins_once_and_keeps_the_version`, plus the settings test now covering `sttModel`; `npm run check` — `audio.check.ts` gained the two rules a dictation can get wrong (`dictatedText` never glues words together and never leads the box with a space; `dictationName` maps six MIME types — and an unknown one — onto the extension the endpoint sniffs, with a fallback so no part is ever nameless). `cargo test` 453 → 455. Falsified twice: dropping the CRLF that ends the file's part in `multipart` fails the byte-exact test (`left: 179, right: 181` — the two bytes, and the model field left inside the recording), and replacing the join rule with "always one space" fails `an empty box takes the words alone` — after checking each mutation reached the disk, and restoring passes. **Three deviations, said out loud**: **push-to-talk is a toggle**, because a control that needs a held mouse button cannot be used from the keyboard at all and the composer is where a keyboard user is; `multipart` is **hand-built** rather than taken from reqwest's `multipart` feature, which would compile another crate for one fixed two-field form — the shape is pinned byte-for-byte by a test instead of trusted; and **the transcription is not on the spend view**, which is the one place this app is quieter than usual: the endpoint reports no usage for it and it is billed by the minute of audio, so a row would carry a number this app cannot know — where the audio *overview* is priced, because that rate is one the user publishes in the settings. The cap (five minutes, recorder-stopped) lives in `lib/audio.ts` rather than in Rust because the recorder is what enforces it; what needs a real microphone and a real endpoint is on `docs/E2E-CHECKLIST.md`.
- **Stop**: a spoken sentence lands in the composer, editable, and is not sent until the user says so.

---

## Phase 13: Grounded research ✅ done, not yet pushed

Planned 2026-10-14, from [`ReverseEngineering.md`](ReverseEngineering.md). The archive's last phase,
and the largest single product surface in it. The order is the argument: **4.1 is the model and 5.4
is the retrieval that reads it**, so the surface (8.2) is a view over both rather than a third way
to hold sources; the output set (8.3) and the fact tables (8.4) are menus over that surface, not
subsystems. The archive wanted this built on a settled memory and retrieval path rather than beside
one, which is what Phases 8 through 11 were, and nothing here needs a server, an account or a new
dependency.

Two decisions were taken with the owner on 2026-10-14 and they shape every item, so they are written
here rather than discovered later.

**A source is a catalogue entry, not a prompt attachment.** Adding one runs a pipeline exactly once —
copy the file into the notebook's own folder, extract its text, chunk and index it, and produce a
**digest**: what that source contains. The notebook's prompt carries the *catalogue* — each enabled
source's label and its digest — and never the extracts; the model pulls what it needs with two
tools. That is the difference between a notebook that answers from forty documents and one that pays
for forty documents on every question. The digest is a model call made **once, when the source is
added**, with the free derived outline (headings, word count, size) as the fallback when no endpoint
is reachable, so adding a source never fails for want of a model — and a re-sync regenerates it only
when the content actually changed.

**A notebook is a tab and a pane member**, so it sits beside a chat and comes back on the next
launch. `Shell.tabs` and `Shell.panes` already hold opaque ids and `pruneShell` is already generic,
so `lib/shell.ts` is **not** widened: what changes is the *resolver*, which must learn that a tab id
is a chat, a notebook, or gone.

The order inside the phase: 4.1 and 5.4 are the substrate and block everything; 8.2 needs both; 8.3
and 8.4 are independent of each other and sit on 8.2. **5.3 is deliberately not built** — see below.

### 4.1 [L] A notebooks, sources and outputs model ✅
- **Why**: the substrate for 8.2 and for every "answer only from these sources, with citations"
  feature. A notebook is a title, a folder, an instruction block, a source list with a sync state, an
  ask log and an output set.
- **Files**: `store/db.rs` (the four tables plus their rows and CRUD); `store/paths.rs`
  (`notebooks_dir`, `notebook_dir`, `notebook_sources_dir`); new `src-tauri/src/retrieval.rs` (the
  label format and the rule that decides whether a citation resolves); new
  `conversation/notebook.rs` (the fingerprint rule, the free outline, the digest call, the catalogue,
  the budget, the system prompt and the ask); `commands.rs` (fifteen commands); `lib.rs` (`mod
  retrieval`, and the handler list); `conversation/agent.rs` (one word: `push_instructions` is now
  `pub(crate)`); `src/api.ts`, `src/types.ts`.
- **Shape**: `notebooks(id, title, instructions, model, created_at, updated_at)`;
  `sources(id, notebook_id, name, kind, origin, path, enabled, extract, digest, digest_at,
  fingerprint, created_at)`; `notebook_messages(id, notebook_id, seq, role, content, citations_json,
  created_at)`; `notebook_outputs(id, notebook_id, kind, title, target, created_at)`. Deletes cascade
  by foreign key, as `messages` does to `chats`. A source's `state` is derived from its
  `fingerprint` — the copy's size and mtime — so a document that changed reports *stale* rather than
  being silently reused; a file that is gone reports *missing*, which is a different problem and does
  not invite a re-sync that cannot work. One comparison answers both questions a source has —
  "has this changed" and "is the digest worth paying for again" — because both are about the same
  fact, so it is stored once. A disabled source is filtered at read time and never deleted, which is
  what makes the switch reversible.
- **Test**: `cargo test` 455 → **465**. `retrieval::tests` — `a_citation_names_its_source_and_the_part_it_used`,
  `a_citation_that_names_nothing_is_caught` (two real citations and one invented, and only the
  invented one is reported), and `prose_and_links_are_not_mistaken_for_citations` (a markdown link,
  `[sic]`, a footnote `[1]` and a bracketed aside are none of them citations, and the real one
  beside them still is).
  `conversation::notebook::tests` — `a_digest_is_made_once_and_not_every_time` (the same comparison
  is the rail's *stale*: a written file is `ok`, an edit of a different length is `stale` against the
  old fingerprint, and a deleted one is `missing`), `the_free_outline_describes_the_shape_of_a_document`,
  `only_the_sources_that_are_switched_on_are_offered` (a switched-off source is absent from both the
  catalogue and the citable labels, and an empty set is said in words), `the_budget_is_shared_in_rounds_rather_than_divided_evenly`
  (a small source gives all it has and the rest of the budget goes to the one that needs it, with
  nothing invented), and `the_notebooks_own_rules_reach_its_prompt` (a disabled rule and a
  coding-only rule stay out).
  `store::db::tests` — `a_notebook_keeps_what_it_holds_and_a_delete_takes_it_all` (three sources in
  the order they were added, the switch reversible with the text kept, the digest and the fingerprint
  both round-tripping, one source removed without touching the others, and then the notebook's rows
  gone together) and `the_notebook_payloads_are_camel_case_and_never_carry_a_documents_text` (all
  four payloads walked for a `_`, the names the window reads asserted, and the extract — which the row
  really does hold — asserted *absent* from the wire). `an_older_database_upgrades_instead_of_refusing_to_open`
  now asserts all four tables exist after opening a database written before them.
  **Falsified**: making `needs_digest` answer "yes, always" (`stored != current` →
  `stored != current || !stored.is_empty()`) fails `a_digest_is_made_once_and_not_every_time` with
  `assertion failed: !needs_digest("120-77", "120-77")` — a source that never moved paying for a
  second digest, which is the bug the owner asked not to have — after checking the mutation reached
  the disk, and restoring passes. **Six deviations, said out loud**: `retrieval.rs` is born here
  rather than in 5.4, because a notebook's sources carry labels before anything is indexed and the
  label format and the resolve rule are one rule — 5.4 grows the index and the query into the same
  file; there is **no `folder` column**, because the notebook's folder is derived from its id in
  `paths.rs` and a path written down twice can come to disagree with itself; a **`model` column was
  added** that the plan did not have, so a notebook can be pinned to a model the way a scheduled task
  is, blank meaning the chat default; a **URL source is a file**, fetched once and written into the
  sources folder, so every source is a file and staleness has one rule rather than two; the output's
  column is **`target`** rather than `ref`, which is a Rust keyword and the name `scheduled_tasks`
  already uses for the same idea; and the source row **carries the extract but the wire never does**,
  which is asserted rather than assumed. **Two things the tests found, both mine**: the outline
  counted a markdown `#` as a word, which inflates the one figure a reader uses to guess how long a
  document is — the marker is stripped before counting now; and the first version of `list_sources`
  ordered by `created_at, id`, which has one-second resolution and a random uuid for a tiebreak, so
  three documents added in a row came back in an arbitrary order — it orders by `rowid` now, which is
  the insertion order this always meant. One limit named: a source's staleness is about **the copy
  this notebook owns**, so re-sync re-copies from the original when it is still there and reads the
  copy when it is not, which is what copying it in was for.
- **Stop**: sources, questions and outputs persist and can be added to later.

### 5.4 [M] Grounded retrieval, and the two ways it is consumed ✅
- **Why**: the cheapest way to make "answer from these documents, with citations" real, and the
  index a notebook cannot work without.
- **Files**: `store/db.rs` (the `chunks` table, `chunks_fts` and its three triggers, `ChunkHit`,
  `chunk_fingerprint`, `indexed_paths`, `replace_chunks`, `delete_path_chunks`, `search_chunks`);
  `retrieval.rs` (the index and the query: `is_indexable`, `chunks_of`, `index_roots`, `search`,
  `label_for`, and the four caps); `conversation/assemble.rs` (`chunks_block`, beside
  `memory_block`); `conversation/agent.rs` (the injection in `system_prompt`, and two words:
  `argument` and `run_browse_tool` are now `pub(crate)`); `conversation/notebook.rs` (the bounded
  read-only loop, `run_tool`, `source_search`, `read_source`, `live_hits`, and the prompt's account
  of what the model may do); `tools/mod.rs` (`is_notebook_tool`, `tools_for_notebook`, the two
  declarations, and the filter in `tools_for`); new `src/lib/citation.ts` + `citation.check.ts`
  (the 18th check script), `package.json`.
- **Shape**: chunk by heading, falling back to fixed windows for a document without headings, and
  label every chunk `path#Heading` relative to the root it was found under. The index is
  external-content FTS5 over `chunks`, so a chunk is always rebuildable and the table is the one
  copy — the same relationship `messages_fts` has with `messages`. Freshness is per file, by
  fingerprint, and **one comparison answers two questions**: the rail's *stale* and "is this worth
  reading again". Two consumers, one index: a **chat** has the top-k excerpts injected into its
  prompt with their labels, and a **notebook** is offered `source_search` and `read_source` instead,
  both scoped to the sources that are switched on. The citation rule is the module's, not the
  prompt's: a citation naming a source that was not there to be read is a bug, and it is caught.
- **Test**: `cargo test` 465 → **471**. `retrieval::tests` — `a_document_becomes_chunks_at_its_headings`
  (the preamble is a chunk with no heading, a long section is split at a line boundary and keeps its
  heading, nothing is lost in the split, and blank text is no chunk at all),
  `only_markdown_is_chunked_by_its_hashes` (a Python file's `#` is a comment and stays one; the same
  text read as markdown is two sections; a heading with nothing under it contributes nothing), and
  `the_index_reads_a_file_once_and_again_only_when_it_moves` — the freshness rule and the sweep, on
  a **real file-backed database and a real folder**: two files read and a picture skipped, a second
  pass reading nothing, the words found and labelled `notes.md#Revenue`, a changed file re-read with
  its old chunk gone rather than answering alongside the new one, a deleted file swept out, and a
  spent budget stopping a pass that the next one finishes.
  `conversation::notebook::tests` gained `a_source_that_is_switched_off_cannot_be_searched` (a
  switched-off source and a file whose row is gone are both refused by the tool, not just hidden
  from the catalogue). `conversation::assemble::tests` gained
  `the_excerpts_block_labels_every_chunk_and_stays_in_its_budget` (no excerpts is no section; every
  label is relative rather than a path; the contract travels with the excerpts; the budget stops a
  chunk rather than half-including one). `tools::tests` gained
  `the_notebooks_tools_are_offered_to_a_notebook_and_to_nothing_else` (neither section, in any mode,
  is offered either tool; a notebook gets both plus the three internet ones; everything a notebook
  is offered is `Read`, so the loop needs no approval path).
  **Falsified**: inverting the citation predicate
  (`filter(|c| !known(c))` → `filter(|c| known(c))`) fails `a_citation_that_names_nothing_is_caught`
  with `left: ["q3.pdf", "notes.md#Costs"], right: ["strategy-2030.docx"]` — the two sources the
  answer really used reported as the invented ones, and the fabricated citation waved through, which
  is the exact bug this item's own spec names — after checking the mutation reached the disk, and
  restoring passes. **Seven deviations, said out loud**: `tools/walk.rs` is **untouched**, because
  `walk_files` and `should_skip` already are the walk and "how a document becomes chunks" is the
  index's rule rather than the walker's; the corpus is **the one folder the chat works in**
  (`agent::workspace_for`) rather than "its own folder and the workspace", because this app already
  treats those as one thing for a chat and that function is the single answer the file tree uses
  too; **only markdown is chunked by its hashes**, since a `#` in a `.py`, `.yml` or shell file is a
  comment and would otherwise make every comment line a section; indexing happens **inside the
  prompt build**, one `stat` per file per turn and at most 4 MB read per pass, with the ceiling and
  the upgrade path written down at the constant; `Markdown.tsx` is **untouched** — an inline chip
  needs an mdast plugin, and the honest v1 is a reference strip under the answer assembled from the
  stored labels, which 8.2 draws and `citation.ts` describes; a **chat's** answer stores no citation
  list, so a chat renders no chips yet and only a notebook does, the upgrade being one column on a
  message the way `notebook_messages` already has one; and the notebook's question is now a **bounded
  read-only loop** (its two tools, the three internet ones, no approvals, no browser session, no chat
  row), which is what the owner asked for and is why two existing tool-count assertions were
  rewritten to compare against a list rather than a number.
- **Stop**: asking about a folder answers from it, and every citation names a file that was actually
  retrieved.

### 8.2 [L] The notebook shell ✅
- **Why**: the user-facing half — one surface holding its sources, a grounded conversation and a set
  of outputs, which is the shape a notebook product has to have.
- **Files**: new `src/components/NotebookShell.tsx`; `src/App.tsx` (the notebook map, the union at
  startup, the pane branch, `/notebook`, and the opener); `src/components/TabStrip.tsx` (a `Tab`
  rather than a `ChatRow`, because a tab is not always a chat); `src/components/Icon.tsx` (a book
  glyph); `src/components/Sidebar.tsx` (one button); `src/api.ts` (`copyNotebook`,
  `saveNotebookOutput`); `src/lib/shell.check.ts` (the restore case); `commands.rs`
  (`copy_notebook`, `save_notebook_output`); `store/db.rs` (`fill_notebook_copy`, tested);
  `conversation/agent.rs` and `commands.rs` (the two `docs::execute` callers); `tools/docs.rs`.
- **Shape**: three regions — a sources rail (add a file or a page, switch a source on or off, its
  sync state, its digest, *Re-sync*), the conversation, and what it produced — with the notebook
  picker and the model pin in the header, so there is no second list panel. A question is asked of
  the notebook, not of a chat: `open_notebook` is one read, `ask_notebook` one call, and the answer's
  citations come back as chips through `src/lib/citation.ts`, which marks one whose source is gone
  rather than linking to nothing. Outputs go through `doc_create` — the writer a chat already uses —
  into the notebook's own folder, and the row remembers the path so tomorrow's click works. Copying
  carries the sources, their files, their digests and their switches, and the outputs; it carries no
  questions.
- **Test**: `cargo test` 471 → **472**. `store::db::tests`
  — `a_copy_carries_the_sources_and_the_outputs_but_not_the_questions`, over a notebook with a model
  pin, an instruction block, two sources (one switched off, one with a digest), an output and two
  messages: the copy has the rules and the model, both sources in order with their digests, their
  extracts, their switches **and their paths rewritten into the copy's own folder**, the output's
  target rewritten with them, and an empty message list; the original keeps everything, including
  the messages it had. `npm run check` — `shell.check.ts` gained the resolver case (an id held by a
  chat or a notebook survives a restore and one held by neither is dropped, with each pane keeping
  its own), and `a11y.check.ts` now walks 39 components rather than 38 because the shell is a
  component like any other. `tools::docs::tests::it_writes_a_real_file_of_every_format` now asserts
  the sentence and the structured path agree, through `path_of`.
  **Falsified**: dropping the path substitution in the copy
  (`|path: &str| path.replace(&out, &into)` → `|path: &str| format!("{path}")`) fails
  `a_copy_carries_the_sources_and_the_outputs_but_not_the_questions` on
  `"a copied source must point at the copy's own file"` — a copy listing the original's files, so
  deleting the original silently breaks it — after checking the mutation reached the disk, and
  restoring passes. **Six deviations, said out loud**: the sidebar got **one button rather than a
  list**, because the picker lives in the shell's own header and a second list of notebooks would be
  a second answer to "which notebooks are there"; `src/lib/commands.ts` is **not touched** — the
  palette lists chats, and a notebook is opened from the sidebar button or `/notebook`; a notebook's
  outputs are **documents, not artifacts**, because `artifacts.chat_id` is a foreign key to `chats`
  and a notebook deliberately has no chat, which is the same reason its answer is not a turn;
  `tools/docs.rs::execute` now returns **the path beside the text** rather than leaving it inside the
  sentence for a caller to parse back out, which is what makes an output row able to remember where
  its file is; a notebook's rules are edited in the shell but are the **same `Instruction` shape and
  the same rule** about which of them apply, pushed by `agent::push_instructions`; and `lib/shell.ts`
  itself is **untouched**, because its ids were already opaque — only the caller's `known` set and
  its check changed. One limit named: `save_notebook_output` offers the three formats `doc_create`
  writes, so a notebook output is a document rather than a spreadsheet or a deck, and the fact-table
  export in 8.4 is the one place a spreadsheet is wanted.
- **Stop**: one source set supports several questions and several outputs, durably, and a notebook
  can sit beside a chat across a restart.

### 8.3 [M] A study and briefing output set ✅
- **Why**: the generator menu is why a notebook is popular, and this app already has the writers and
  the chart engine.
- **Files**: `conversation/notebook.rs` (`Generator`, `GENERATORS`, `generator()`);
  `tools/docs.rs` (`DOC_FORMATS`, one list, now used by `doc_create`'s own validation);
  `commands.rs` (`write_output`, `notebook_generators`, `run_notebook_generator`); `lib.rs`;
  `src/components/NotebookShell.tsx` (the rail's *Make* column); `src/api.ts`, `src/types.ts`.
- **Shape**: **a table, not code.** A generator is an id, a title, the format the writer takes and
  the question that asks for that shape — so a seventh generator is one entry and nothing anywhere
  else, and the menu is sent to the window rather than written into it. Six entries: flashcards, a
  quiz, a briefing, a timeline, a study guide and a mind map. **No new artifact kind**, and no
  artifact at all: a notebook's output is a document, because an artifact's `chat_id` is a foreign
  key to `chats` and a notebook has none — which is the same reason its question is not a turn. A
  mind map is a `mermaid` fence inside Markdown and an infographic would be raw `html` in one, so the
  diagram shapes this app already knows are carried by the document rather than by a second renderer.
- **Test**: `cargo test` 472 → **473**. `conversation::notebook::tests::every_generator_names_a_format_the_writer_takes`
  — every entry's format is held against `tools::docs::DOC_FORMATS` (**the writer's own list**,
  which is the point: a copy of that list in the table is exactly what would drift), every entry has
  a title and a prompt, every prompt asks for a citation in the one spine every generator shares
  (`[name#Heading]`), and no id is listed twice. **Falsified**: giving the mind map a format nothing
  writes (`format: "md",` → `format: "xlsx",`, in a three-line CRLF needle because `format: "md",`
  appears four times in the table) fails that test with `mind-map names the format xlsx` — a menu
  entry that only fails when somebody presses it — after checking the mutation reached the disk, and
  restoring passes. **Two deviations, said out loud**: nothing new was invented to carry a mind map
  or a quiz, so a quiz ships with its answer key rather than marking itself and the archive's own V2
  rating stands; and `DOC_FORMATS` is new in `tools/docs.rs` because `doc_create` had its three
  formats spelled inline, which meant a second list anywhere else could only ever be a copy.
- **Stop**: a notebook produces a flashcard set, a quiz, a briefing, a timeline and a study guide,
  each from its sources.

### 8.4 [S] Data tables over sources ✅
- **Why**: synthesising facts across sources into a table and exporting it is the one notebook output
  this app can already produce, because the `.xlsx` writer is here.
- **Files**: `conversation/notebook.rs` (`TABLE_PROMPT`, `table_rows`, `cell_text`,
  `unlabelled_rows`, and `enabled_sources` made public); `commands.rs` (`run_notebook_table`);
  `lib.rs`; `src/components/NotebookShell.tsx` (the *Fact table* button); `src/api.ts`.
- **Shape**: the answer is asked for as a Markdown table and then read as **rows**, through the
  document writer's own block parser — so "what a table is" has one answer in this app, and a table
  inside a fence is an example rather than the result. Two rules decide whether anything is written:
  every row's last cell has to name a source the notebook holds — held against the *same* rule an
  answer's citations go through, because a table's citation column is the only thing that makes it a
  fact table rather than a list of claims — and a cell beginning with `=` is written as text, since a
  fact copied out of a document is never something the workbook should evaluate. Cells carry the
  source's own words in a column; nothing is a live formula, and nothing is a comment either, because
  a comment is invisible to the reader who has to check the row.
- **Test**: `cargo test` 473 → **475**. `conversation::notebook::tests::a_fact_table_has_to_say_which_source_each_row_came_from`
  — a three-row answer parses into header plus two facts, a header is not asked to cite, a row naming
  a stranger and a row naming nothing at all are both reported **with the row number a reader
  counts**, prose with no table is no table, a header alone is no table, and a table inside a fence
  is not the result. And `a_cell_that_begins_with_an_equals_sign_is_text_not_a_formula` — `=1+1` and
  a leading-whitespace `=SUM(...)` are both escaped, while a number, a date, a cell that merely
  mentions an equals sign and an empty cell are left exactly as they were.
  **Falsified**: removing the escape (`return format!("'{cell}")` → `return cell.to_string()`) fails
  that test on `assertion left == right failed` for `cell_text("=1+1")` — a document's text arriving
  as something the spreadsheet runs — after checking the mutation reached the disk, and restoring
  passes. **Three deviations, said out loud**: the table is **not an entry in `GENERATORS`**, because
  its answer is read as rows and written by `sheet_create` rather than `doc_create`, which is a
  different path through the writers and would have broken the invariant that holds every generator
  to a document format; cells carry the source's words rather than a comment column, because a
  comment is invisible to the person checking the row, and the plan's own wording ("a citation per
  cell") is what a column already is; and the row/header width check is there because a ragged table
  is the one malformed shape the writer would happily write. One limit named: every cell is **text**,
  including a cell that happens to be a bare number, because the cell holds the source's own words —
  so a column of figures arrives in Excel as text and will not sum until it is converted. That is the
  price of a table you can check against the document rather than one you have to trust.
- **Stop**: a fact table can be built across sources and exported to `.xlsx`.

### 5.3 [M] Semantic memory lookup — decided, not built
- **Why** (the archive's): ranking is by shared words, so "the car" does not surface "vehicle".
- **Decision, 2026-10-14**: **not built this phase, and not by accident.** This file already lists
  *Embeddings for memory recall* under *Explicitly not doing (and why)*, and the README repeats the
  trade at its honest limits: the price is a model download and network at query time, and it is
  worth paying only if keyword recall actually fails in use. There is no evidence for that yet. Both
  routes are also real commitments rather than one line — a local model is a new dependency and a
  real download, and a provider is a decision about which endpoint serves embeddings, which the
  endpoint this app ships against does not — so building it now would reverse a recorded decision to
  satisfy a hunch. Left as a proposal, with the keyword path the one that exists.

**So Phase 13 is five items, not six, and the sixth is the one thing this phase left undone** — with
the reason written above rather than left out. Nothing else in the phase is deferred: every other
block's Stop is met and its Test runs in the gate.

---

## Phase 14: The notebook, followed through — two things using it made obvious ✅ done

Planned 2026-10-14, the same day Phase 13 was, out of the owner's first hour with 0.5.0 running. Not
from the archive — the archive's phase plan ends at 13 and this file has taken all of
it — but from two things that were plain the moment a real notebook had real documents in it:

**A digest is clamped to three lines on a 288 px rail.** The rail has room for a name, a state and a
line or two of description, and the description is the only thing in the app that says what a
document *contains*. So the one sentence a reader needs to decide whether a source is worth switching
on is the one sentence that gets cut, and reading the rest of it should not mean finding the file and
opening it in something else.

**The Make column can only produce documents.** Its six entries are fixed prompts whose answers are
prose written to a file, which is right for a briefing and wrong for a quiz: a quiz you take has to be
*structured* — a question, its options, which one is right, why — and prose cannot carry that reliably
enough to grade. The model is the only part of the app that has read the sources, so the set has to
come from a call it makes, in a shape the app declares; then the window can run it. That is the
difference between a quiz you print and a quiz you take, and it is what NotebookLM's versions of these
two features actually are.

Both items were put to the owner as a choice between stated options rather than guessed at, and the
owner was away, so **the recommended option was taken for each and is recorded here to be confirmed**:
14.1 shows the whole description in a dialog rather than the document's own text as well, and 14.2
adds the interactive sets *beside* the Make column rather than converting its Quiz and Flashcards
entries, which keep writing printable files. Neither adds a dependency. The order is 14.1 then 14.2
and they do not touch each other.

**Both items are built, and each was ticked only once its own Stop was met** — the rule at the top of
this file is that a ✅ means the Test runs in the gate, so neither carries one before its gate was
green and its own falsification had failed the right check on purpose.

### 14.1 [S] A source's description can be read in full ✅
- **Why**: the digest is clamped, the file name is truncated, and both are the rail's whole account of
  what the notebook holds. The full description already travels in `NotebookView` — it is only the
  card that has no room for it — so this is a reading surface, not a new pipeline.
- **Files**: `src/components/NotebookShell.tsx` (the card's read control and the dialog);
  `docs/E2E-CHECKLIST.md`, `CHANGELOG.md` and `README.md` (the line each one owes it); `plan.md`. **No
  new file, no new command, no Rust, no dependency** — which is what the item is for.
- **Shape**: the card gains one quiet *Read* control beside the digest, opening a dialog in the shape
  this app already opens five of: backdrop at `fixed inset-0 z-50` that closes on mousedown, Escape to
  close, `role="dialog"` with `aria-modal` and an `aria-label`, `autoFocus` on the close button — the
  same overlay `AudioOverviewDialog` uses, and the shape `a11y.check.ts` exists to enforce. The dialog
  holds the whole digest plus the facts the card has no space for: where the source came from, the copy
  this notebook owns, how much text was read out of it, its sync state, and when the digest was
  written. **No new command**, because every one of those fields is already on the wire.
  The control is drawn whether or not the text overflows *on this machine*: "does it overflow" is a
  layout question, and a `scrollHeight` guess would hide the control exactly when a different font or
  a narrower window pushed the text past three lines. One rule, no measurement, nothing to disagree
  with itself. Reading the *document's own text* inside the app is deliberately not built: the model
  can already read it with `read_source`, the card can show the file, and putting a 120k-character
  extract behind a dialog is a second reading surface for a need nobody has stated — the upgrade is
  one command returning `extract`, and the reason it is not here is that nobody has asked yet.
- **Test**: `npm run check` — `a11y.check.ts` walks the new overlay, so a dialog that shipped without
  its name, or a glyph button without a label, fails the gate; it reports **39 components**, the count
  it reported before, because this is a dialog inside a component that already existed rather than a
  new file. `npm run typecheck` and `npm run build` are clean.
  **No new `.check.ts`**, as the plan said: there is no pure rule here to own — the item renders data
the app already has, and a script asserting a `<p>` is not a test. Its end-to-end line is where the
  copy is checked, like every other line in the checklist.
  **Falsified**: with the dialog's name taken away by a scratch mutator whose needle was confirmed on
  disk and restored afterwards, `node src/lib/a11y.check.ts` exits **1** with
  ``FAIL: 1 accessibility problem(s): src/components/NotebookShell.tsx: a dialog with no `aria-label`
  or `aria-labelledby` is announced as "dialog" and nothing else.`` — the exact regression this item's
  check exists for — and restoring passes again (`a11y.check.ts: all checks passed (39 components)`, A11Y=0).
  **Two deviations, said out loud**: *Read* is drawn only when the source **has** a description, which
  is a rule about emptiness rather than about overflow — a card reading "Not described yet." has
  nothing to open, and drawing a control that opens an empty dialog is the kind of control this app
  does not ship; and the facts line about the copy prints `none — nothing was copied in` for a source
  with no path, because a blank row in a list of facts reads as a bug. `ARCHITECTURE.md` is
  **untouched**: this item adds a surface and no mechanism, so its table has no new rule to hold.
- **Stop**: the whole description of a source can be read without leaving the app.

### 14.2 [M] The model can make a study set, and the window runs it ✅
- **Why**: the Make column's quiz is a document and cannot be taken; a set that can be taken has to be
  structured, and the model is the only thing in the app that has read the sources. So the set is made
  by a tool call in a shape the app declares, stored, and run by the window — question, options, the
  right one, why, and a score at the end.
- **Files**: `store/db.rs` (the `notebook_sets` table, its row, its CRUD, and the copy);
  `conversation/notebook.rs` (the validator, and one sentence in the prompt);
  `tools/mod.rs` (`make_study_set` and its schema); `commands.rs` (`remove_notebook_set`, and `sets` in
  `NotebookView`); `lib.rs`; `src/types.ts`, `src/api.ts`; new `src/lib/quiz.ts` and
  `quiz.check.ts` (the 19th check script); new `src/components/StudySetDialog.tsx`;
  `NotebookShell.tsx` (the list); `package.json` (the chain); `docs/E2E-CHECKLIST.md`, `CHANGELOG.md`,
  `README.md` and `ARCHITECTURE.md` (the line each one owes it); `plan.md`.
- **Shape**: one tool, `make_study_set`, three arguments — `kind` (`quiz` | `flashcards`), `title`, and
  `items`, each `{prompt, answer, options, why, source}`. **Options are what make it a quiz**: present,
  there are 2–6 of them, no two the same, and `answer` has to be one of them (normalised to the option
  as written, so the window can compare exactly); absent, it is a flashcard and the answer is the back.
  Rust validates and **refuses with the reason**, so the model reads what was wrong and fixes its own
  call inside the same question rather than the reader being handed a broken set. A set that arrives is
  stored in `notebook_sets` and listed in the rail under *Study*; pressing one opens a dialog and the
  window runs it — a quiz one question at a time with the right option marked and the `why` under it,
  a score when the last one is answered; a flashcard shows its front, then its back.
  `source` is optional and a name that does not resolve is **marked rather than refused**, which is the
  *answer* path's rule and not the fact table's: a fact table without its source column is worthless,
  and a ten-question quiz thrown away because one question misnamed a file is a worse trade — and the
  resolve rule itself stays in `retrieval::unresolved` and `citation.ts`, not in a third copy.
  The tool is `Internal` rather than `Read`, which is the class the app already uses for a call it
  handles itself and stores itself: it is not a write to the disk, it needs no approval, and the
  notebook's loop stays what 5.4 says it is — nothing in it can touch the machine.
- **Test**: `cargo test` 475 → **478**, no warnings. `conversation::notebook::tests` —
  `a_quiz_question_is_only_usable_when_its_answer_is_one_of_its_options` (a well-formed quiz is a set,
  and then: an answer outside the options is refused *with the options named*, so the model can repair
  the call; the answer is stored as the option is spelled, `gdp` → `GDP`; two identical options are
  refused; one option and seven options are both refused; a flashcard carrying options is refused; an
  unknown kind, a missing title, an empty title, a missing `items` field, an empty list and a list one
  past `MAX_STUDY_ITEMS` are every one of them refused), and
  `a_flashcard_holds_a_front_and_a_back_and_nothing_more` (two cards, no options, no explanation and no
  source required; a card with no back and an item with no front are refused, each naming the item).
  `store::db::tests` — `a_notebook_keeps_what_it_holds_and_a_delete_takes_it_all` stores a set, reads
  its items back, removes it alone (and puts one back so the cascade below is checking something), then
  asserts the delete takes it; `a_copy_carries_the_sources_and_the_outputs_but_not_the_questions`
  asserts the set comes across with its items and its labels intact;
  `the_notebook_payloads_are_camel_case_and_never_carry_a_documents_text` walks the set's payload with
  the others and pins `notebookId`, `items[0].prompt`, `items[0].options` and `items[0].source`;
  `an_older_database_upgrades_instead_of_refusing_to_open` asserts **five** notebook tables now,
  `notebook_sets` among them, so the table is not one that exists on a fresh profile and nowhere else.
  `tools::tests` — the notebook's test became *its three, and the web ones*: neither section in any
  mode is offered any of the three, everything a notebook is offered is `Read` or `Internal` (so the
  loop still has nothing that touches the machine), and every name in the list is a declaration whose
  class agrees with the list's; the new `the_study_set_schema_and_its_validator_are_one_list` holds the
  schema's `kind` enum to `notebook::STUDY_KINDS` itself rather than to a copy of it, holds the required
  fields to the ones the validator refuses for, and holds an item closed with
  `additionalProperties: false`.
  `npm run check` — `quiz.check.ts` is the **19th** script, one past `citation.check.ts`: the right
  choice is the one spelled as the
  answer and a near miss is not; an untouched set has asked nothing and has nothing wrong in it; one
  answered of two gives *1 asked, 0 right, wrong `[0]`*; every answered item is exactly one of right or
  wrong; the boundary with Rust, that marking is by text so duplicate options are invisible here; and
  the wording (`1 card`, `2 cards`, `1 question`, `9 questions`). `a11y.check.ts` walks **40**
  components now, the new dialog among them. `npm run typecheck` and `npm run build` are clean.
  **Falsified**: inverting the validator's answer rule
  (`.find(|option| option.eq_ignore_ascii_case(&answer))` → `!option.eq_ignore_ascii_case(&answer)`), a
  one-line needle confirmed on disk and restored afterwards, fails
  `a_quiz_question_is_only_usable_when_its_answer_is_one_of_its_options`:
  `MUT=101`, `test result: FAILED. 0 passed; 1 failed`, `panicked at src\conversation\notebook.rs:1169:42:
  an answer outside the options is refused: StudySet { kind: "quiz", title: "Revenue quiz", items:
  [StudyItem { prompt: "How much did revenue rise?", answer: "9%", options: ["9%", "12%", "15%"],
  why: "The summary states it.", source: "q3.pdf" }] }` — and after restoring, the whole suite is green
  at **478 passed; 0 failed**.
  **Six deviations, said out loud**: the `1 card`/`9 questions` wording lives in `lib/quiz.ts` rather
  than in the dialog, because the rail and the dialog both count the same set and two copies of a
  wording is how they come to disagree — and `quiz.check.ts` pins it, since pluralising with an `s` is a
  bug this app has already shipped once; `load()` now returns the view it stored and `ask` compares the
  set count, so an answer that made a set **says so** (`Made "X" — it is in the Study list`) instead of
  the reader having to notice a new row: the model's own sentence is a promise, not a notification;
  `MAX_STUDY_ITEMS = 40` is a cap the plan did not name, because the plan bounded the options and left
  the list unbounded and "a model filling a list forever" is a real way to spend someone's money;
  there is a `remove_notebook_set` command and deliberately **no** `add_notebook_set` one — a set is
  questions, and a button cannot write them, so the tool call is the only door in; the dialog draws an
  empty set as "ask for it again" rather than as a blank panel, which is only reachable from a row
  damaged between writes because the validator refuses an empty set; and **`README.md` was corrected
  rather than extended** — its *What it can make* bullet ended "a quiz ships with its key rather than
  marking itself", which the set that *does* mark itself made half-false, so the sentence now says which
  of the two it is describing.
- **Stop**: asking a notebook for a quiz gives one you can take, and the score is the one the answers
  earned.

---

## Phase 15: The Make column can be asked for something specific ✅ done

Asked for by the owner straight after using 0.6.0's study sets, and three things were plain the moment a
real quiz came back:

**The label was lying.** *Quiz .md* and *Flashcards .docx* still name a format, because they are the old
fixed-prompt path that writes a file — while the thing the model makes through `make_study_set` is a set
the window runs. A menu where two entries promise a document and their neighbours in the same feature
are interactive is a menu nobody can aim at.

**Nothing about a generation was the reader's.** Every entry was one fixed prompt: ten questions, every
enabled source, whatever the model decided to ask about. What anyone wants from a generator is *how
many*, *from which documents*, and *a line saying what to aim at* — and the screenshot that prompted this
shows the model itself offering exactly those as a follow-up question, which is a job for a form rather
than for a second turn.

**A quiz whose right answer is always the first option is not a quiz.** Order is the one thing a model
cannot be trusted to randomise, because it writes the options in the order it thought of them. Asking it
more firmly in prose does not fix that; shuffling where the shuffle is real does.

The two choices the owner was asked about and was away for, recorded here for confirmation: **Quiz and
Flashcards become the interactive set with no file at all** (a printable version stays one sentence away
in the ask box, and the exam below *is* the printed path), and **the exam's open questions are a number
defaulting to 0** rather than a checkbox that adds a fixed block. Neither adds a dependency.

**All four items are built, and each was ticked only once its own Stop was met** — the rule at the top
of this file is that a ✅ means the Test runs in the gate, so none carries one before its gate was green
and its own falsification had failed the right check on purpose.

### 15.1 [M] Every entry can be asked for something specific ✅
- **Why**: the reader should not have to start a whole second turn to say "twenty questions, only the
  chapter 3 notes, and nothing about the 1970s".
- **Files**: `conversation/notebook.rs` (the entry's shape, `Options`, `Plan`, the question builder,
  `scoped_sources`/`in_scope`/`no_sources`); `commands.rs` (`run_notebook_entry` replacing
  `run_notebook_generator` and `run_notebook_table`, and `write_table`); `lib.rs` (one handler in, two
  out); new `src/components/MakeDialog.tsx`; `src/components/NotebookShell.tsx`; `src/types.ts`,
  `src/api.ts`; `docs/E2E-CHECKLIST.md`, `CHANGELOG.md`, `README.md` and `ARCHITECTURE.md` (the line
  each one owes it); `plan.md`.
- **Shape**: an entry declares what it can be asked for, and the window draws a field per declaration
  rather than a form per entry: `count: Option<Count>` (a noun, a default and a ceiling), `open:
  Option<Count>` for the one entry that takes a second list, and `format: Option<&'static str>` — which
  is now optional, so "this entry writes no file" is said by the data instead of by a label. A source
  checkbox per source, and a line to steer with, for every entry.
  The question is built in one function from the entry and the options, so the count is phrased once and
  cannot be phrased twice by a prompt that also hardcodes it. The source scope is **real rather than
  wording**: `ask` takes the chosen names, and the catalogue, the citable labels, the two source tools
  and the fact table's citation check all read one `scoped_sources`, which is `enabled_sources` with one
  more filter in the same place — because "switched off" and "not asked for this time" are the same rule
  applied twice, and a second copy of it is how a source stays in the prompt after being ticked out.
- **Test**: `cargo test` 478 → **480**, no warnings. `conversation::notebook::tests` —
  `every_menu_entry_writes_something_the_writer_takes` holds the menu to itself: a document entry names
  only formats `DOC_FORMATS` accepts and asks for a citation, the table entry names `xlsx` and says how
  a row names its source, every declared count has a noun and a default inside its own bounds, and no
  id is listed twice. `what_the_reader_asked_for_is_what_gets_asked` is the item's own rule end to end:
  an untouched form sends the entry's default count **said once** and claims no scope at all; a form
  that asked for 25 says 25 and not the default; the chosen source is named exactly; the reader's own
  line goes last, where it reads as the ask rather than as background; the fact table takes a scope and
  a line with no number; a number sent to an entry with no field for it is dropped rather than refused;
  and five refusals each say what to change — 999 questions, 0 questions, `docx` for the Markdown quiz,
  and a source the notebook does not hold, which names what it does hold.
  `a_source_out_of_scope_is_not_read_and_the_others_still_are` is the scope rule itself: an empty scope
  is everything switched on; a named source is the only one in the catalogue and the only one citable;
  naming is case-insensitive; naming a **switched-off** source cannot bring it back, because the two
  filters are one rule and the second cannot undo the first; and an empty scope says which of the two
  situations it is instead of sending the reader to the wrong switch. `npm run check` —
  `a11y.check.ts` walks **41** components, `MakeDialog.tsx` among them. `npm run typecheck` and
  `npm run build` are clean.
  **Falsified**: putting the entry's default back into the count sentence
  (`Make exactly {count}` → `Make exactly {slot.default}`), a one-line needle confirmed on disk and
  restored afterwards, fails `what_the_reader_asked_for_is_what_gets_asked` — `MUT=101`,
  `test result: FAILED. 0 passed; 1 failed`, `panicked at src\conversation\notebook.rs:1458` — which is
  the assertion that the question says 25, i.e. a form that asks for twenty-five and silently means
  ten. Restoring passes with the whole suite green at **480 passed; 0 failed**.
  **Five deviations, said out loud**: `Plan` carries **no counts** — they are in the question, which is
  the only thing that reads them, because a field written and never read is a warning and this repo
  treats a warning as a failure (the set and the exam add them back where something reads them);
  `Action` has only `Document | Table` so far, since `Set` and `Exam` arrive with the entries that
  construct them for the same reason; `TABLE_PROMPT` is **gone**, inlined into the entry that now takes
  the same options as everything else; the shell's `act()` returns whether the work finished, so a
  refused entry **keeps the form open** with the reason in the status line rather than closing on the
  failure; and the form sends an **empty** source list when every source is ticked, because the wire's
  rule is "empty is everything switched on" — with the form refusing an empty tick list, since that
  would otherwise mean the opposite of what it looks like.
- **Stop**: pressing any entry opens a form, and what the form says is what the model is asked and all it
  may read.

### 15.2 [S] Quiz and Flashcards in Make make a set, not a file ✅
- **Why**: the label is the symptom; the entry was the cause.
- **Files**: `conversation/notebook.rs` (the two entries' action and prompts, `Action::Set`, and the
  count rule in `study_set`); `tools/mod.rs` (`count` on the schema and its description, and the
  assertion that the schema offers what the validator reads); `commands.rs` (`Made`, and the set arm
  that reads the sets back); `src/types.ts`, `src/api.ts`; `src/components/NotebookShell.tsx`
  (`whatWasMade`); `CHANGELOG.md`, `README.md`, `docs/E2E-CHECKLIST.md`; `plan.md`.
- **Shape**: those two entries' action is `set`, their `format` is `None`, and pressing one asks the
  notebook a question whose only job is to make the model call `make_study_set` with exactly the number
  asked for. **The count becomes enforceable**: the tool takes an optional `count` and refuses a set whose
  item count differs from it, naming the difference — the same "refuse with the reason so the model can
  repair its own call inside the same question" rule the options and the answers already go through, and
  the difference between "the reader asked for ten" and "the reader hopes for ten". Nothing is written to
  the notebook's folder, so nothing appears under *Outputs*: the set is the output, under *Study*.
- **Test**: `cargo test` 480 → **481**, no warnings. `conversation::notebook::tests` —
  `a_set_is_the_size_that_was_asked_for_or_it_is_refused` is the count rule: one item is what a count
  of one asks for; ten asked for and one sent is refused **with both numbers** ("1 of the 10 items");
  a count of `null` is not a mismatch, because a question typed into the box has no form behind it; and
  a count of 0 or 400 is refused before anything is built. The menu test's `Action::Set` arm holds the
  two entries to what the model has to get right — no format named, `make_study_set` named, the count
  passed on, and a count declared to pass — and `tools::tests::the_study_set_schema_and_its_validator_are_one_list`
  now asserts the schema offers `count`, since a field the validator reads and the schema omits is a
  rule that could never fire.
  **Falsified**: stopping the tool holding a set to its size (`if raws.len() != wanted` →
  `if wanted > MAX_STUDY_ITEMS`), a one-line needle confirmed on disk and restored afterwards, fails
  `a_set_is_the_size_that_was_asked_for_or_it_is_refused` — `MUT=101`,
  `test result: FAILED. 0 passed; 1 failed`, `panicked at src\conversation\notebook.rs:1767` with the
  returned one-item set printed — which is a reader asking for ten and being handed one. Restoring
  passes with the whole suite green at **481 passed; 0 failed**.
  **Five deviations, said out loud**: the command returns a **`Made { outputs, sets }`** rather than a
  list of outputs, because whether a set happened is not something the window can infer from an empty
  list — it is the difference between "nothing was written" and "something was stored"; the command
  reads the notebook's sets **before** the ask so what the ask produced can be told from what was
  already there, and refuses with "the answer came back without a set" when the model answers in prose
  instead of calling the tool, which is the one failure this path can have and the one the reader
  cannot see for themselves; **a test of mine from 15.1 was wrong and 15.2 found it** — it asserted
  `plan(quiz).format == "md"`, which this item made false, so the format rules moved to the `timeline`
  entry and the set now asserts an empty format and that a format sent to it is dropped; the count
  refusal is **worded so that no number is followed by a noun** ("came back with 1 of the 10 items"),
  because "1 items" is the plural bug this repo has already shipped once; and the tool's `count` is
  optional, so a typed question that never asked for a number is not held to one.
- **Stop**: pressing Quiz or Flashcards gives a set to take, and no file.

### 15.3 [S] A set is dealt fresh every time it is taken ✅
- **Why**: order is not something a model does randomly, and a quiz whose right answer is always the
  first option teaches the wrong thing.
- **Files**: `src/lib/quiz.ts` (`shuffled`, `sitting`); `src/lib/quiz.check.ts` (the dealing section);
  `src/components/StudySetDialog.tsx` (the sitting per attempt); `conversation/notebook.rs` (one
  instruction taken *out* of the quiz entry's prompt); `CHANGELOG.md`, `README.md`, `ARCHITECTURE.md`,
  `docs/E2E-CHECKLIST.md`; `plan.md`.
- **Shape**: `shuffled(values, random)` and `sitting(items, random)` — the items' order and each item's
  options, with the answer riding on its option's **text**, so marking is untouched by the shuffle (which
  is the whole reason 14.2 normalised the answer to the option it belongs to). The dialog deals a new
  sitting every time it opens and every time the reader starts again, from `Math.random`. A document's
  options cannot be shuffled by the app — they are prose in a file — so there the one thing that works is
  an instruction in the prompt, and it goes in.
- **Test**: `npm run check` — `quiz.check.ts` gained the dealing section: over twenty seeds a sitting
  holds exactly the same items with the same options each, the answer is still one of the options it
  was normalised to, and at least one seed deals a **different** order, which is the assertion that
  fails when a shuffle is a no-op; marking a sitting with its own answers is all right and a different
  option is still wrong (the answer rides on the option's text, so the deal cannot move it); a constant
  source deals the same order twice and what it deals is a permutation of what it was given, so
  "shuffled" cannot pass as a coin toss either; and nothing, and one item, deal as nothing and itself
  rather than looping. `cargo test` **481** — unchanged, and deliberately: this item's Rust change is a
  sentence *removed*, not a case added. `npm run typecheck` and `npm run build` are clean.
  **Falsified**: making the shuffle a no-op (`const j = Math.floor(random() * (i + 1))` → `const j = i`),
  a one-line needle confirmed on disk and restored afterwards, exits **1** from
  `node src/lib/quiz.check.ts` with `FAIL: at least one seed must deal a different order, or this is
  not a shuffle` — a set dealt in the order it was written, which is the strategy this item exists to
  take away — and restoring passes (`quiz.check.ts: all checks passed`, exit 0).
  **Three deviations, said out loud**: the plan said the *document* prompts gain an instruction to vary
  the answer's position, and what happened is the opposite — one instruction was **deleted** from the
  quiz entry, because that entry is a set now and the window deals its order; asking a model for a
  random order is asking for the one thing it cannot do, so the instruction belongs to the exam
  (`15.4`), which is a document nothing shuffles; the order is dealt into **state** on open and re-dealt
  on *Start again* rather than on every render, which the plan did not say and without which the
  question would reshuffle while the reader was reading it — and the dialog's counts stay the *set's*,
  which is the same length as the sitting, with a comment saying so; and my new check script did not
  typecheck the first time (`map(order)` applied per element rather than to the array), which the gate
  caught before the phase did.
- **Stop**: taking the same set twice does not ask it in the same order.

### 15.4 [M] An exam, as a paper and a key ✅
- **Why**: the owner's actual need — forty multiple-choice questions to print, with the answers on a
  separate sheet so the paper can be handed out.
- **Files**: `conversation/notebook.rs` (the entry, `Action::Exam`, `EXAM_KEY_PROMPT`, `follow_up_for`,
  `Plan.then`); `commands.rs` (the two-ask arm); `src/components/NotebookShell.tsx` (`whatWasMade`
  naming both documents); `src/types.ts` (the action vocabulary); `CHANGELOG.md`, `README.md`,
  `ARCHITECTURE.md`, `docs/E2E-CHECKLIST.md`; `plan.md`. No new command, no new file, no dependency.
- **Shape**: a seventh entry, `exam`, whose defaults are the ones asked for — **40 questions, 0 open,
  `.docx`** (`.pdf` offered beside it). Pressing it runs **two asks rather than one**: the paper, whose
  prompt says there are no answers anywhere in it, and then the key, whose question is short because the
  paper is the *previous turn of the notebook's own conversation* — which is what that conversation is
  for. Two documents land under *Outputs*, the second named so the two can be told apart at a glance.
  Open questions are added to the paper and answered in the key only when their number is not zero. Both
  counts are stated in one place, so they cannot be stated twice.
- **Test**: `cargo test` 481 → **482**, no warnings.
  `an_exam_is_a_paper_and_a_key_and_the_paper_carries_no_answers` is the item's own rule: the defaults
  are asserted as the defaults (**40**, and **0**) because "forty by standard" is the whole request; an
  untouched form asks for forty multiple-choice questions, asks for no open questions, and builds a
  second question that refers to "the paper above" and asks for the key rather than for more questions;
  a key with no open questions must not so much as mention them, and a key must not carry a count of
  its own because it is written to the paper's numbers; with five open questions the sentence appears
  in **both** halves — the paper asks for them, the key answers them; and **no other entry makes a
  second document**, which is what keeps `then` a field one entry carries rather than one every entry
  ignores. The menu test's `Action::Exam` arm holds both formats to the writer that has to take them,
  holds the paper to carrying no answers, holds the paper to **varying which letter is the right
  answer** — the instruction 15.3 removed from the quiz entry on the promise that it belonged here,
  where nothing can shuffle for it — and holds the entry to declaring both of its numbers.
  `npm run typecheck`, `npm run check` (19 scripts) and `npm run build` are clean.
  **Falsified**: never building the key question (`if entry.action != Action::Exam` → `if true`), a
  one-line needle confirmed on disk and restored afterwards, fails the test — `MUT=101`,
  `test result: FAILED. 0 passed; 1 failed`, `panicked at src\conversation\notebook.rs:1864:41: an exam
  has a key to write` — which is an exam whose answers never arrive. Restoring passes with the whole
  suite green at **482 passed; 0 failed**.
  **Five deviations, said out loud**: the key's question carries **no counts** where the plan said both
  halves would carry them, because it asks for the same questions in the same order and the paper has
  just stated the numbers — so it is a constant plus one conditional sentence rather than a rebuilt
  question; the paper is told to carry **no citations either**, which the plan did not say: a citation in
  the margin is the answer's neighbour because it says where the answer is, so the paper has neither and
  the exam takes its own arm in the menu test rather than sharing the document rule about citations; the
  two outputs are named **"Exam paper" and "Exam answer key"** by constant rather than derived from the
  entry's title, which would have produced "Exam exam" — and the window names both in the status line,
  because a reader who is not told the key exists will look for the answers on the paper; the exam is
  **not** bounded by `MAX_STUDY_ITEMS` (a set's cap of 40), because a paper of sixty questions is a
  paper and the entry declares its own ceiling of 60 with the open list capped at 20; and the second ask
  rests on one assumption worth naming — the paper travels as the previous turn of the notebook's own
  conversation, which holds the last eight messages, so the key is written to a paper it can still see.
- **Stop**: pressing Exam produces a paper and its key as two documents.

---

## Phase 16: A document that looks like a document ✅ done

Planned from the owner's first look at an exam paper in Word: *"it looks like trash"*. Three causes, and
only the first is about the exam:

**The writer never reads the emphasis it is given.** `Block::Paragraph(text)` keeps the text as it
arrived, so the model's `**bold**` reaches Word as two asterisks and its `*italic*` as two more. The
`.md` writer is right to keep them — that is what Markdown is — but a `.docx` is a typeset document, and
the reader of a printed paper should never see the markup that asked for the emphasis.

**Nothing sets a font or a size.** Every run is whatever the library's default is, which in Word means
Calibri 11 — nothing to do with the document's own `PRINT_CSS`, which has been Times New Roman's cousin
Georgia all along. Two writers, two looks, and only one of them was chosen.

**Headings point at styles that are not in the file.** `Paragraph::style("Heading1")` names a Word style
this document never defines, so a heading is either plain text or whatever Word decides "Heading 1"
means — which is how a paper comes out as one undifferentiated wall.

**16.1 [M] The house look of a generated document ✅**
- **Why**: a document a reader prints has to look like one, and the markup that asked for emphasis is
  not part of what was asked for.
- **Files**: `tools/docs.rs` (the look constants, `heading_size`, `Run`/`runs_of`/`push_run`,
  `docx_bytes`, `inline_html`, `PRINT_CSS`, and three tests); `conversation/notebook.rs` (the exam's two
  prompts, and the menu test's own-case comparison); `CHANGELOG.md`, `README.md`, `ARCHITECTURE.md`,
  `docs/E2E-CHECKLIST.md`; `plan.md`. No new dependency: `zip` was already one, and it is what lets a
  test read the very file Word will open.
- **Shape**: one inline reader — `**bold**` and `*italic*` into runs, with anything unmatched left as
  the text it is, because a stray asterisk in a document about tax rates is not emphasis. One place
  decides the look: **Times New Roman 11 for body text**, and bold Times New Roman at 16/14/12 for
  headings of level 1/2/3+, applied as *direct formatting* on the run rather than by naming a style the
  file does not define. Both writers read the same runs, so the `.docx` and the `.pdf` cannot come to
  disagree about what the document looks like any more than they can about what it says. The exam's
  prompts then ask for each question as a level-2 heading — which is what makes a question bold 14 with
  its options at 11 underneath — and the same for the key's numbered answers.
- **Test**: `cargo test` 482 → **485**, no warnings. `the_emphasis_a_model_writes_becomes_runs_rather_than_asterisks`
  — a line with no marker is one plain run and an empty line is no runs; `**Q1.** What is GDP?` is a bold
  run then a plain one; `a *b* c` is italic in the middle and `**a** *b*` is three runs in order; and
  `****`, `2 * 3 = 6`, `**unclosed` and `q3_file.docx` are each left exactly as written, the last because
  an underscore is not a marker here. `a_word_document_is_set_in_the_house_look` — the built `.docx` is
  unzipped and `word/document.xml` read, then **the run a piece of text sits in** is inspected rather
  than looking for a size somewhere in the file: the question's run is bold and carries 28 half-points
  (14pt), an option's run carries 22 (11pt) and is not bold, `Times New Roman` is in the document, and
  neither `**` nor `##` reaches the page. `the_printable_page_carries_the_same_look_and_the_same_emphasis`
  — emphasis becomes `<strong>`/`<em>`, a **code block keeps its asterisks** because there they are the
  content, and the stylesheet names the same font and the same three sizes. `npm run typecheck`,
  `npm run check` and `npm run build` are untouched by this item: nothing in the window changed.
  **Falsified**: making the emphasis reader never accept a marker
  (`Some(end) if end > 0` → `Some(end) if end > text.len()`), a one-line needle confirmed on disk and
  restored afterwards, fails `the_emphasis_a_model_writes_becomes_runs_rather_than_asterisks` —
  `MUT=101`, `test result: FAILED. 0 passed; 1 failed`, `assertion left == right failed` with
  `left: [Run { text: "**Q1.** What is GDP?", bold: false, italic: false }]` against `right: [Run { text:
  "Q1.", bold: true, italic: false }, Run { text: " What is GDP?" ... }]` — the whole line arriving in
  Word as one plain run with the asterisks still in it, which is the complaint verbatim. Restoring
  passes with the whole suite green at **485 passed; 0 failed**.
  **Four deviations, said out loud**: the look is **one house style rather than an option per
  document** — the owner asked about the exam paper, and a per-document font would be a parameter
  threaded through `doc_create`, the entry, the form and the writer to let one caller differ; **`_italic_`
  is deliberately not a marker**, because an underscore lives inside file names and half of
  `q3_file.docx` would have come out italic; a **code block keeps monospace** (Consolas 10) instead of
  the body font, because monospace is what makes it code rather than an indented paragraph; and **the
  PDF moved with the `.docx`** — `PRINT_CSS` was 11.5pt Georgia with 20pt headings, so leaving it alone
  would have meant the same document changing its look with its extension. One thing my own test caught:
  the exam arm of the menu test looked for the vary-the-answer instruction in lower case and the sentence
  it now lives in begins with the word, so that comparison is made on a lower-cased copy — the rule is
  about what is said, not about where the capital falls.
- **Stop**: an exam paper opened in Word is Times New Roman 11 with its questions in bold 14, and no
  Markdown punctuation shows anywhere in the document.

---

## Phase 17: The paper, the rail, and getting to a second notebook (planned 2026-10-14, **not built**)

From the owner's second look at a real paper and a real notebook:

**The paper is a wall of questions.** It has no front matter — nothing says what subject it is, when it was
made, or what it was written from — and every option starts at the same left edge as the question above
it, so the eye has nowhere to land between questions.

**The rail has three lists fighting over one column.** *Make* grew from six entries to eight plus a line
of explanation, and with *Study* under it the *Outputs* list — the documents the reader actually wants to
open — is pushed off the bottom of the window.

**There is no way to reach a second notebook, or even to see that there is more than one.** The sidebar
button opens *a* notebook: the most recent one open, else the first, else a new one. Nothing lists them and
nothing makes one, so a second notebook is unreachable by any route at all.

17.1 is built and has its ✅ and its record; 17.2 and 17.3 are not, so their Test lines still say what a check
*will* be.

### 17.1 [M] A cover, and options that sit under their question ✅
- **Why**: a paper handed to someone should say what it is, and its options should not read as more
  questions.
- **Files**: `tools/docs.rs` (two new blocks, `PAGE_BREAK`, `INDENT_TWIPS`, `indented()`, the parser, both
  writers, and two tests plus a `paragraph_before` helper); `conversation/notebook.rs` (`exam_cover`,
  `long_date`, `MONTHS`, `without_title`, two new sentences in the exam's prompt, three tests, and two
  assertions in the menu test); `commands.rs` (the Exam arm writes the cover ahead of the paper and drops
  a title the model wrote anyway); `CHANGELOG.md`, `README.md`, `ARCHITECTURE.md`, `docs/E2E-CHECKLIST.md`;
  `plan.md`. No new dependency: the date is arithmetic, and `docx-rs` already had both the page-break
  property and the indent the writer needs.
- **Shape**: the document grammar gains the two things a paper needs and prose never did — a **page
  break** (one line that is exactly `\pagebreak`, exported as `PAGE_BREAK` so the cover that writes it and
  the reader that reads it cannot drift) and an **indented line**, which is read as a *fact* rather than
  kept as the four spaces it arrived with. `Block::Indented` is set in Word as `w:ind w:left="720"` — half
  an inch, Word's own indent step — and on the printed page as `margin-left:36pt`, which is the same
  distance in the unit a stylesheet wants. `Block::Break` is not a paragraph: it is a property of the
  block that **follows** it (`page_break_before`, `break-before:page`), because a paragraph holding a
  break prints an empty line at the top of the page it has just started. The cover itself is the app's,
  not the model's — `exam_cover(subject, sources, stamp)` — because the subject is the notebook's title,
  the date is `now_ts()` and the sources are the list this generation was already scoped to, and a model
  asked for those three gets them wrong about as often as right. The date is a days-to-civil formula from
  the shifted-March epoch rather than a date library, and it is tested against stamps read off a real
  calendar. Finally the paper is passed through `without_title`, which drops a level-1 heading at the top
  of it: the exam prompt now says the paper carries no title of its own, and a model that writes one
  anyway would otherwise put a second title on page two.
- **Test**: `cargo test` 485 → **490**, no warnings. `a_break_line_starts_a_page_and_an_indented_line_is_an_option`
  — a `\pagebreak` line is a break and not a paragraph, a line indented by four spaces is an option whose
  text has no leading spaces, a **tab** means the same as those four spaces, three spaces is a paragraph,
  an indented `- nested` is still the bullet it has always been, and `\pagebreak` inside a sentence is
  prose. `a_break_starts_a_page_and_an_option_sits_in_from_the_margin` — the built `.docx` is unzipped and
  the page break is asserted where it *is*: between `<w:pageBreakBefore />` and the question's own text
  there is no `</w:p>`, so the break is a property of the question's paragraph rather than a paragraph of
  its own, and the title is on the near side of it; the option's paragraph carries `<w:ind w:left="720"`
  and the four spaces are nowhere in the XML; and the same two facts are asserted on the printed page
  (`break-before:page`, `margin-left:36pt`). `a_word_document_is_set_in_the_house_look` now writes its
  options the way the paper does. `a_stamp_becomes_the_day_it_falls_on` — 1 January 1970 at stamp 0 and at
  86_399, 2 January at 86_400, **31 December 1969** at -86_400 (the negative side of the division),
  28 February 1900 and **1 March 1900** for the day after it (1900 was not a leap year, which is what the
  shifted epoch is for), 29 February 2000, 1 January 2100 and 14 October 2026. Every one of those stamps
  was read off a real calendar by asking Node for the date rather than worked out by hand — two of my own
  hand-derived values were wrong, which is exactly the failure my notes warn about.
  `the_cover_says_what_the_paper_is_and_sends_it_to_the_next_page` — the cover names the subject, the
  date and **every** source it was given, and the block parser's last block for it is a `Break`, with no
  paragraph carrying the `\pagebreak` text. `a_title_the_model_wrote_anyway_is_dropped_but_a_question_is_not`
  — a leading `#` title goes, a `## 1.` question stays, `#hashtag` is not a heading and so is not a title,
  and a paper that is nothing but a title leaves nothing behind it. `npm run typecheck`, `npm run check`
  and `npm run build` all pass untouched (the window changed only in 17.2's rail, which is not this item).
  **Falsified twice, both verbatim.** *(a)* Stopping the parser reading an indented line as an option
  (`} else if let Some(text) = indented(trimmed) {` → `indented("")`), the needle confirmed on disk and
  restored afterwards, fails `a_break_starts_a_page_and_an_option_sits_in_from_the_margin` with
  `MUT=101`, `test result: FAILED. 0 passed; 1 failed` and `the option is not indented:` followed by the
  whole XML — in which the option is a plain `<w:p>` with no `w:ind` and its four spaces still in the run.
  *(b)* Three independent rules at once — the break stops starting a page, the cover stops stating the
  date, and the title guard stops caring whether the line was a heading — gives `MUT=101`,
  `test result: FAILED. 487 passed; 3 failed`, and the three failures are exactly the three tests, with
  `left: ""` for the dropped `#hashtag`, a cover printed without its date, and `the break is not in the
  document:` followed by XML whose option paragraph **does** carry `<w:ind w:left="720" w:right="0" />`
  — the indent proved independent of the break. All three lines were confirmed back on disk afterwards and
  the whole suite re-ran green at **490 passed; 0 failed** with `GATE OK`.
  **Three deviations, said out loud**: the cover is **written by the app rather than asked of the model**,
  because the three facts on it are the app's to know; the break is **the grammar's own line** (`\pagebreak`)
  rather than a `<w:br>` the model is asked to emit, so nothing about Word escapes into a prompt; and a
  **table that follows a break** gets an empty paragraph carrying the property first, because a `Table` in
  `docx-rs` cannot hold `page_break_before` and the alternative is a break that silently does nothing.
  One lesson worth the ink: my first multi-line falsifier needle was joined with `\n` against a **CRLF**
  file, so it matched nothing — and the mutator's own uniqueness check threw *before writing*, which is the
  only reason a green suite after it was not mistaken for a passing falsification. The needles are now
  given as lines and joined with the file's own ending.
- **Stop**: a paper opens with a cover page naming the subject, the date and the sources, and its options
  are indented under their question.

### 17.2 [S] The rail stops eating the column ✅
- **Why**: *Outputs* is what a reader comes back for, and it is the section that gets pushed out.
- **Files**: `src/components/NotebookShell.tsx` (two class strings and a comment naming the rule);
  `docs/E2E-CHECKLIST.md`, `CHANGELOG.md`; `plan.md`.
- **Shape**: *Make* gets a ceiling and scrolls inside it, *Study* keeps the one it has, and *Outputs*
  takes what is left and scrolls — so all three are on screen together and the documents stay reachable
  however many entries the menu grows. None of the three is pinned with `shrink-0` any longer: a box that
  cannot shrink keeps its height while the window gets shorter, which is how the list at the bottom is the
  one that goes.
- **Test**: no new check, and said out loud rather than papered over: this is layout, and there is no rule
  here that a Node script can hold — a pure helper written to have something to assert would be a check
  on nothing. So the gate is the whole window side of it: `npm run typecheck`, all **19** check scripts
  (including `a11y.check.ts` over **41** components) and `npm run build`, then the end-to-end line above,
  which is where a rail's three heights can actually be settled. It was gated twice — once inside 17.1's
  run, because the file was already in the tree, and again on its own before this commit — and `cargo test`
  is deliberately not part of this item's gate: no Rust file changed, so its 490 tests say nothing about
  a class name. **No falsification, and that is not an oversight**: there is no logic here to remove, and a
  mutated class name would fail a screenshot, not a check — a "mutation" a test cannot see proves nothing,
  which is the same reason section 17.1's two did.
- **Stop**: with eight menu entries, four sets and two documents, all three headings are on screen and
  every output can be reached without a scroll fight.

### 17.3 [M] A notebooks view, beside Chat and Coding
- **Why**: the sidebar's *Notebooks* button is a sixth item in a list of settings, and it opens one
  notebook without ever showing that there are others — which is why the owner could not find out how to
  make a second one.
- **Files**: `src/components/Sidebar.tsx` (a third segment and a notebook list); `src/App.tsx` (the view
  state, the list, and one command that makes a notebook from it); `src/api.ts` (nothing new — the three
  commands exist); `CHANGELOG.md`, `README.md`, `docs/E2E-CHECKLIST.md`; `plan.md`.
- **Shape**: the segmented control becomes three — **Chat | Coding | Notebooks** — where the third shows
  notebooks *in the sidebar* rather than in a panel, which is what makes it a peer of the other two
  rather than a setting: the list of notebooks where the list of chats is, and **+ New notebook** where
  **+ New chat** is. Picking one opens it in the focused pane; the one already open is marked, so the
  list and the pane cannot disagree. The settings-list button goes: two ways into one thing is how the
  second way comes to be the stale one. The *section* does not change — a notebook is not a third kind of
  chat and the composer's toolset is not its business — so the view is remembered separately from the
  section, and pressing Chat or Coding returns to the section you were in.
- **Test**: `npm run check` — `shell.check.ts` already holds the shell's own rule about what a pane holds;
  what is new here is the view, so the check is the accessibility walk over the new list and buttons plus
  `typecheck`, and the end-to-end line covers the rest. No new `.check.ts`: the rule this item adds is
  "the list shows what exists", and the list is `listNotebooks()`'s answer rendered.
- **Stop**: pressing Notebooks shows every notebook, marks the one open, and makes a second one in one
  press.

---

## Not yet in this plan (still proposals in the archive)

The archive's phase plan runs **8 to 13**, and this file has now taken all six of them. What is left
is the archive's own *Kept in reserve* list — sub-agents, the browser-extension bridge, the activity
timeline, share snapshots, and the plugin and recording work — each of which wants either a real week
of using the phases above or a task that actually needs it. 5.3 is the one item of Phase 13 that was
planned and then deliberately not built, and the reason is written above rather than left out.

So a ✅ on every item in this file means **the plan as written is built**. It does not mean the
roadmap is finished.

---

## Explicitly not doing (and why)

- **First-party OAuth connectors (Gmail, Drive, Calendar, Slack)**: an app registration, a
  verification process and a host for the secrets, which is a product rather than a feature. MCP is
  the honest route, and a third-party server is the user's choice to install.
- **Silent model routing**: a suggestion with an estimate, never a downgrade the user did not see.
- **Generated video**: an Audio Overview (later) is a script plus text to speech, but video is a
  separate media pipeline and a separate budget.

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
