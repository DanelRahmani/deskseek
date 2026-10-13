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
[`ReverseEngineering.md`](ReverseEngineering.md). **Phase 9 (9.1–9.6)** is planned
below and is being built; its items stay local and unreleased with Phase 8's. The next
release is **0.5.0**, and `CHANGELOG.md`'s top section is what its tag will print.

What is *not* covered by those ✓s: the E2E checklist has not been walked end to
end in a real window since Phase 1, so the newer lines (the crash screen and its
draft, the interrupted-run banner, backup/restore, the database sweep, the
decision log, a pulled network) are written down and armed but unrun. Only a
window can confirm them; none is reachable from `cargo test`.

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

## Phase 3 — Trust and robustness

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

## Phase 4 — Performance and scale

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

## Phase 5 — Polish and craft

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

## Phase 6 — Release and docs housekeeping

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

## Phase 7 — Setup that finishes the job

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

## Phase 8: Interop and the daily habit

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

## Phase 9: Containment and trust

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

### 9.6 [M] A safety pass before Auto acts
- **Why**: Auto mode reviews nothing but a string table, and `guard.rs` says so out loud. A small
  model call over the tool name, a redacted argument summary and the last few turns is the honest
  second layer — and the fallback when it cannot answer is *ask*, never *proceed*.
- **Files**: `tools/guard.rs` (an `Advisory` verdict beside `Allow`/`Refuse`); a new
  `conversation/review.rs`; `conversation/agent.rs` (call it in Auto only); `api/client.rs` (a
  non-streaming call, as the summariser already has); the spend kind from 9.5.
- **Test**: a golden set of calls — a download-and-execute pipeline, a recursive delete, an upload of
  a local file, an ordinary write — keeps its verdict, and a review that fails degrades to asking.
- **Stop**: Auto no longer relies on string matches alone, and a failed review asks.

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
