# DeskSeek roadmap

A phased plan for this repo, written 2026-10-12 and updated as phases land.
Phase 1 is **done and pushed** (commit `b437c6e`). Phase 2 is in progress: 2.1–2.8
are done, the rest of 2 is the plan. Items 3–6 have not been started.

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

### 2.9 [S] Spend, split: where the tokens went
- **Why**: the spend view answers *how much*, and only in money. The tokens are the
  half that is not a bill: a coding chat that reads a lot of files burns **input**,
  and a chat-mode conversation burns **output** — same total, different problem,
  and only one of them is fixed by a cheaper prompt. Asked for on 2026-10-12.
- **Do**: a pie of prompt against completion tokens, split by section (`coding` vs
  `chat`). The data is a join away: `turn_usage.chat_id` → `chats.kind`, and
  `spend_report` already joins `chats` for the titles, so this is one more grouped
  query and one more field on `SpendReport` — no new table, and no arithmetic done
  in the panel.
- **Drawing it**: `chart.ts` has `bar` and `line` only. Two honest options, and
  the choice is for whoever builds it: add a **`pie` type to the chart module**
  (pure, and therefore assertable — the module exists for exactly this), or a
  `conic-gradient` div like the per-day bar in `SpendPanel` (three lines, nothing
  to check). Prefer the first: the slices have to be *right*, and a gradient has no
  way to be wrong out loud.
- **Test**: a Rust test with hand-built rows across both kinds, asserting the
  sections sum to the report's own totals; if `pie` is added, a chart check that
  the angles complete the circle and that an empty or single-slice breakdown does
  not produce `NaN` — the same class of bug the flat-series axis produced in 2.6.
- **Stop**: an input/output split is visible per section, and the slices sum to the
  totals above them.
- **Honest limit to keep**: cached input costs a fraction of fresh input. If the
  split ever claims to explain the *bill*, it has to show cached tokens too; as a
  picture of where the tokens went, two slices is what was asked for.

---

## Phase 3 — Trust and robustness

### 3.1 [S] React error boundary and a crash surface
- **Why**: the app has no boundary; a render throw is a blank window with a lost
  draft.
- **Do**: one boundary at the app root that keeps the composer draft, shows what
  failed, and offers *Reload*. Reuse the existing error copy voice.
- **Test**: a temporary throw behind a dev flag, then an E2E checklist line.
- **Stop**: a thrown component leaves a readable screen and an intact draft.
- **Pulled forward and partly done (2026-10-12)**: 2.1's panel blanked the window
  the first time it was opened, so the boundary could not wait. `ErrorBoundary`
  wraps `<App/>` in `main.tsx`: the message, React's component stack, *Try again*
  (a re-render), *Reload the window*, and copy. **The draft half is not done** —
  the prompt lives in the tree the boundary replaces — and neither is the plan's
  own test, so nothing in the gate *forces* the screen. The Stop condition is
  therefore not met: the boundary is armed, not proven.

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
