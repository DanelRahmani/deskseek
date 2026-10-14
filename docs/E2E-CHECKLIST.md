# Manual end-to-end checklist

Run this before cutting a release. Everything here needs a real window, a real
API key and (for the folder items) a real folder, so it is deliberately manual —
the automated checks live in `cargo test` and `npm run check`.

Tick a box only when the result matches. If something fails, note the step number.

## Setup

- [ ] `npm run typecheck` passes
- [ ] `npm run check` passes (shell / transcript / filing / tasks / artifact / chart / docs / shortcuts / commands / snippet / theme / motion / tree / a11y / tray / changelog / audio / citation)
- [ ] `cd src-tauri; cargo test` passes
- [ ] `npm run tauri build` produces an installer and the app launches from it
- [ ] `CHANGELOG.md` has a dated section for the version being tagged, and `node scripts/release-notes.mjs v<version>` prints exactly it

## Smoke-out

The shortest pass that says the build is alive, before the long sections below. Open
devtools first and leave them open: this section is only about a surface that draws
nothing, or throws.

- [ ] Each sidebar panel opens and draws something: Files, Projects, Artifacts, Scheduled, Spend, Context
- [ ] Opening an artifact opens the artifact panel; closing it goes back to the chat
- [ ] Each dialog opens, and Escape closes it: Settings (`Ctrl+,`), Customise, the command palette (`Ctrl+K`), the shortcut sheet (`?`), `/setup`
- [ ] A panel with nothing in it says what to do about it, rather than only that there is nothing
- [ ] The console stayed clear through all of it

## First run

- [ ] A fresh profile (delete the app data folder) shows the setup screen
- [ ] An invalid API key is rejected with a readable message, not a stack trace
- [ ] Entering a valid key and closing setup lands on an empty chat
- [ ] `/setup` reopens the same screen with the saved values
- [ ] *Settings → Browsing* offers the same *Set it up for me*, and pressing it there fills the URL field and the row stops offering itself
- [ ] With Docker stopped, *Set it up for me* is disabled and the note beside it says why, and *Start Docker Desktop* launches it
- [ ] With Docker running, *Set it up for me* builds the container and the SearXNG row turns to *SearXNG answering*; a `settings.yml` appears in the app's `searxng` folder
- [ ] Pressing it a second time starts the container it already made rather than building a second one, and does not overwrite that `settings.yml`
- [ ] Deleting that `settings.yml` and pressing again writes a new one with a different `secret_key`
- [ ] A question that needs current facts makes the assistant call `web_search` and cite what it read
- [ ] The instance is **not** reachable from another machine on the same network (it binds `127.0.0.1`), and port 8888 is already in use by something else ⇒ the failure names the port rather than hanging
- [ ] With the network unplugged, *Set it up for me* fails with docker's own reason rather than a bare error

## Chat

- [ ] A message streams in; the *thinking* pane appears and collapses
- [ ] The caret sits at the end of the text while it is still arriving
- [ ] *Copy* under an answer puts the Markdown on the clipboard; *Retry* next to it asks again
- [ ] *Edit* under your own last message changes it and asks again
- [ ] Markdown, a table and a fenced code block all render
- [ ] Dropping an image attaches it and the reply accounts for it
- [ ] Dropping a `.docx` (or `.pdf`/`.xlsx`/`.pptx`) copies it into the chat's own folder — the
      📁 in the header reveals the same folder — and asking about it gets an answer from its text
- [ ] Picking the same kind of file with 📎 behaves the same way; dropping a second file with a
      name already in the folder gives `name-2` rather than replacing it
- [ ] A **dropped image** is in the chat's folder as well as in the message — the 📁 shows it
- [ ] A **picked** image, and a **picked** small `.md`/`.csv`, are in the folder too: the
      paperclip and the drop must give the same result, which is what used to differ
- [ ] An image posted to a chat is *not* described to the model as a file to open — it is shown
- [ ] A picture over 25 MB, and a picked file over 32 MB, is refused with a message naming the
      size rather than being added with nothing behind it
- [ ] A file attached *before* the first message of a brand-new chat is read but not filed
      (documented, not a bug) — sending first and attaching after files it
- [ ] Asking for a `.docx`/`.xlsx`/`.pptx` produces a real file **in that chat's folder**, and
      *Open* on the result opens it in Word/Excel/PowerPoint
- [ ] Two chats on the same subject have separate folders, and a chat renamed afterwards keeps
      the folder it already had
- [ ] A chat inside a project still works in the **project's** folder rather than a new per-chat one
- [ ] `Stop` mid-stream keeps the partial reply and ends the turn
- [ ] A tool-heavy turn folds into one *N steps* row; opening it shows each call
- [ ] A `run_command` card says, **before** the command runs, that `/rewind` will not put back
      files it changes — then `/rewind` afterwards undoes the file edits and not the command
- [ ] A failing tool step opens its group by itself
- [ ] The mode and approval controls sit with the composer, and `Shift+Tab` still cycles
- [ ] `ask_user` shows the question prompt; answering resumes the run
- [ ] `/compact` summarises and the context meter drops
- [ ] `/summarise` proposes entries and opens *Settings → Memory* with them

## Long conversations

- [ ] With an answer streaming, the text keeps arriving smoothly — no stutter, and scrolling
      stays responsive while it grows
- [ ] A chat with more than 60 transcript rows opens on the newest ones, with a *Show N earlier
      messages* line above them; clicking it adds a screenful above without jumping the view and
      never removes a row from the bottom
- [ ] The line disappears once everything is revealed, and a newly opened chat starts windowed
      again (the window is per conversation, not remembered)
- [ ] Switching between a long chat and a short one leaves the short one showing all of it
- [ ] A 500-turn chat can be opened and scrolled: the newest answer is at the bottom, and the
      oldest turn is reachable by revealing, not lost
- [ ] `/compact` replaces the transcript with one note that says how many messages it folded, and
      the meter drops to match. (The *automatic* fold fires at 90% of a 1M-token window, so it is
      not walkable by hand — its rule is pinned by `summarize::tests` instead)

## Conversations

- [ ] The arrow on a sidebar row moves a chat into *Archived (n)* at the bottom
- [ ] An archived chat keeps its messages, its spend and its folder; `↑` restores it
- [ ] `Ctrl+K` offers *Archive this chat*, and *Take out of the archive* once it is archived
- [ ] Restoring does not reorder the list: the chat comes back where it was
- [ ] Right-click → *Rename* edits the title in place; Enter saves it, Escape abandons it
- [ ] After *Retry* or an edited resend, the turn shows ↺; clicking it brings the **previous**
      answer back and keeps the new one, so clicking again toggles between them
- [ ] Neither answer costs a second run — the swap makes no API call
- [ ] A replaced turn that ran tool calls comes back with its tool results, and the next message
      in that chat is accepted rather than rejected
- [ ] A renamed chat keeps the new title after a restart and does **not** jump to the top —
      renaming is not activity, so the list order is untouched

## Customise, projects and the gallery

- [ ] *Customise* opens the hub; `/skills` lands on Skills and `/mcp` on Connectors
- [ ] The hub's Skills list does not show design systems; the Design systems tab does
- [ ] Marking a skill *always on for new chats* writes `default: on` into its `SKILL.md`
- [ ] A chat started afterwards has that skill already checked — and an existing chat does not change
- [ ] Unmarking it leaves the chat you are in alone, and stops the next one inheriting it
- [ ] A skill that is off leaves only its one catalogue line in the prompt (Context panel)
- [ ] Asking for something a switched-off skill covers makes the assistant call `use_skill`
- [ ] Revising a skill by talking (`replace: true`) keeps its `default: on` flag
- [ ] *Draft from a folder* writes a design system from a real project
- [ ] Switching a design system on for a chat names it in the context breakdown
- [ ] A design system is one markdown file in the app's `design` folder — not a
      `skills/design-…/SKILL.md` — and the Skills tab never lists it
- [ ] Asking the assistant to build something while one is switched on applies it
- [ ] Asking the assistant to record a house style saves it with `create_design_system` and
      switches it on for that chat; asking again with `replace: true` revises the same file
- [ ] Asking the assistant to start one from a project folder drafts it with
      `draft_design_system`; a folder outside the workspace is refused
- [ ] Saving a design system does not switch a skill off, and saving a skills list does not
      switch a design system off (they share one table, so this is the guard to check)
- [ ] An instruction edited in the hub is still there after reopening Settings
- [ ] *New project…* picks a folder; a chat started in it works there
- [ ] Deleting a project keeps its chats and only unassigns them
- [ ] A project's **Rules** reach every chat in it: a `both` rule on a chat *and* a coding chat, a
      `coding` rule on the coding chat but not the chat, and a switched-off rule on neither
- [ ] Moving a chat out of the project stops the rules reaching it on the next turn, and moving it
      back brings them
- [ ] A project's **Memory** shows the entries from `memory/project-<folder name>.md`; a project
      with no folder says so instead of showing another project's notes
- [ ] A project's spend line matches the sum of its chats' sidebar totals (all-time) and drops a
      chat that is moved out
- [ ] The Artifacts list shows an artifact from another chat, and opens it

## Prompts

- [ ] *Customise → Prompts* saves one; a markdown file appears in the `prompts` folder, named
      after the slug of its title, with the title as its first heading
- [ ] Typing `/p:` in the composer lists the saved prompts; picking one puts its **text in the
      box** — it is not sent, and the caret is in the box ready to edit
- [ ] `Ctrl+K` → the prompt does the same, into the focused pane's composer
- [ ] Editing one in the hub and saving under the same name revises the file rather than adding
      a second
- [ ] A prompt edited in a text editor shows its new text in the menu after the hub is reopened
- [ ] A prompt never appears in the model's catalogue (the Context panel lists no prompts)

- [ ] Right-click → *Duplicate* makes a copy that opens with the same transcript and its own
      folder; the original is unchanged, and the copy's sidebar spend reads $0
- [ ] *Export this chat as JSON* writes a `name.chat.json` in the chat's folder; *Import a
      conversation from JSON…* brings it back as a new chat with the transcript intact
- [ ] Importing a file that is not a bundle, or one from a newer format, is refused with a
      message rather than half-importing
- [ ] An imported conversation says the attachments did not come with it

## Search, here and elsewhere

- [ ] In a coding chat in folder A, asking "what did we decide about X" makes the assistant call
      `search_history` and answer from an *earlier* chat in that same folder
- [ ] A chat in folder B can never be quoted by the agent working in folder A, however the
      question is worded
- [ ] The same question in a chat-mode conversation searches that section's own history only
- [ ] A query of `*`, `-` or `:` reports "no earlier message matches" rather than an error

## The sidebar's search box

- [ ] Typing two or more characters lists matching messages under *In messages*; one character
      does nothing
- [ ] The matched word is **emphasised** in the snippet, and no `[` or `]` of the search's own
      is ever visible
- [ ] A message that contains its own brackets (`items[0]`, `list[i]`) shows them as plain text —
      they are not emphasised
- [ ] A common word (something that appears in hundreds of messages) puts the messages *about* it
      above the ones that merely mention it — the short, on-topic hit comes first, not the newest
- [ ] "Show more matches" appears under a long list, adds the next page **below** what is already
      there, and goes away when the last page is in
- [ ] Scrolling and clicking a hit still opens that conversation
- [ ] Clearing the box empties the list and brings the conversations back

## Spend

- [ ] *Spend* opens the panel; `/spend` and the palette toggle the same panel
- [ ] A turn just run appears in the window, and its figure matches that chat's sidebar total
- [ ] The bars size by share of the busiest day, and switching 7 / 30 / 90 days re-reads
- [ ] A conversation in *By conversation* opens that chat when clicked
- [ ] A turn stopped half-way is still counted, for what it spent up to the stop
- [ ] *Where the tokens went* shows a pie per section, and each pie's two slices add up to the
      totals printed above it
- [ ] The slices are proportional: a section with twice the output of another has the bigger
      orange slice, and the legend's percentages match
- [ ] A window with only one kind of chat draws one pie, and a section with no tokens still
      draws (as a ring) rather than breaking the panel
- [ ] *Export the decision log and the spend* in the palette writes one JSON file to the documents
      folder, the notice names it, and opening it shows the decisions **Settings → Commands** lists
      and the figures the panel is showing — with no command or file contents in it, only digests
- [ ] Run `/compact` (or let a long chat fold itself) and reopen *Spend*: *By what spent it* has a
      *Summaries and folds* row with one call and a cost, and its rows plus *Turns* add up to the
      figure at the top
- [ ] Revise a passage of an artifact, then reload *Spend*: an *In-place revisions* row appears with
      that call's cost, and the conversation it was made in shows a higher total than before

## Scheduling and notifications

- [ ] *Scheduled* opens the panel; `/scheduled` toggles the same panel
- [ ] A task set for a minute's time fires within a minute and does **not** fire twice
- [ ] The fired run lands in a fresh chat titled after the task, visible in the sidebar
- [ ] "The first Wednesday of the month" spells out as a sentence, not a cron string
- [ ] *Run now* runs the task and leaves *Next:* where it was
- [ ] *Pause* stops the clock and leaves the task in the list
- [ ] A scheduled run that hits a never-auto-approved tool appears in the panel's **Needs you**
      list with the same prompt the chat shows; answering there resumes the run and clears the row
- [ ] A scheduled run that asks a question does the same, and *Skip* there cancels it
- [ ] Answering the prompt in the **chat** also clears the panel's row (one park, two places)
- [ ] The row's chat title opens that conversation
- [ ] Closing the app with a run parked and reopening shows it as *lost* with the reason, and
      *Dismiss* clears it; running the task again is the way to pick it up
- [ ] With the window in the background, a run finishing raises an OS notification
- [ ] `notify_user` raises a notification; with the window focused, only the notice
- [ ] Turning *Tell me when something is done* off silences the toast, not the dot
- [ ] A conversation that finishes in the unfocused pane gets a green sidebar dot,
      cleared when it is opened
- [ ] A scheduled run that has to ask something — an approval it can never get automatically, or
      an `ask_user` question — raises a notification and marks the chat, rather than waiting
      invisibly in a chat nobody has opened
- [ ] A toast raised while the window was in the background opens that conversation when the toast
      is clicked: Windows brings the window to the front, and the app reads a focus arriving within
      two minutes of a toast as the click. Alt-tabbing back later must open nothing
- [ ] A break reminder arrives after the interval set in *Settings → Tools*, and inside quiet hours
      the toast is held back while the in-app notice still appears

### Triggers

- [ ] A new task set to *When a file changes* on a folder says *armed — waiting for a change* on
      its first look, without starting a run
- [ ] Dropping a file matching the filter into that folder starts a run within one interval; a file
      that does not match the filter does not
- [ ] Adding fifty files inside one interval starts **one** run, not fifty
- [ ] Touching a file that did not change (same name, same second) does not start another run
- [ ] A *git* task pointed at a repository fires when that ref moves, and not on an unrelated commit
- [ ] A *page* task with a word to look for fires when the page comes to say it, and not on a poll
      where it already said it; one with no word fires on any change to the text
- [ ] A task pointed at a folder that does not exist shows *could not check: …* on its row instead
      of sitting silently at "nothing to report"
- [ ] A trigger that flaps — changed on every look — starts at most six runs in an hour, and the row
      says the cap is what is holding it back
- [ ] Closing the app, changing the watched folder, and reopening starts at most **one** run rather
      than replaying every interval it spent closed
- [ ] Editing a trigger's target re-arms it: the row goes back to *armed* and the next look does not
      fire on the difference between the old folder and the new one
- [ ] Pausing a trigger stops it being woken by a server event as well as being polled
- [ ] An *MCP event* task shows *Waiting on <server>* rather than a next-check time, and never a date
      far in the future

## Two panes

- [ ] *Split* shows two conversations; the divider drags and resizes
- [ ] Right-clicking a sidebar row offers *Open in split view*, which fills the other pane
- [ ] Closing the app and reopening restores the tabs, the split and the divider
- [ ] Starting a run in the left pane, then clicking into the right pane, leaves
      the left run streaming with its pulse dot still lit
- [ ] `Stop` in one pane does not touch the other pane's run

## Keyboard

- [ ] `Ctrl+N` starts a chat; `Ctrl+W` closes the focused tab
- [ ] `Ctrl+B` toggles the file tree; `Ctrl+M` opens the memory settings
- [ ] `F2` renames the open chat in the sidebar; Escape abandons the edit, Enter saves it
- [ ] `Ctrl+G` scrolls the focused pane to its newest message
- [ ] In the palette, `>` lists only the actions and `@` only the conversations
- [ ] `Ctrl+,` opens settings; `Ctrl+\` toggles the split
- [ ] `Ctrl+1` / `Ctrl+2` move focus between the panes
- [ ] `Escape` closes the open panel or dialog; while typing in the composer it
      only dismisses the slash menu
- [ ] `Escape` in Settings or Customise closes it and puts the caret back on the
      button that opened it

## Accessibility

Everything here needs a real window. The structural half of it is checked by
`src/lib/a11y.check.ts` and the tree's keys by `src/lib/tree.check.ts`; these are
the parts only a person can see.

- [ ] The file tree answers the mouse and the keyboard the same way: Tab reaches
      it, Up/Down walk the rows, Right opens a folder then steps into it, Left
      closes it or steps out to the parent, Home/End jump, Enter picks a row
- [ ] Tab reaches every control in every panel and dialog, in an order that
      follows what is on screen; no control is reachable only by pointer
- [ ] The focus ring is visible on every control Tab lands on — and still visible
      with Windows High Contrast Mode on (Settings → Accessibility → Contrast
      themes), where `outline-hidden` keeps it and `outline-none` did not
- [ ] With *Reduce motion* on (Settings → Appearance), opening and closing the
      panels still works and nothing slides — and the caret is still visible while
      an answer is arriving
- [ ] With Narrator (or NVDA) running: opening Settings announces a dialog with a
      name; asking a question announces the answer once it has finished rather
      than reading every streamed frame; the file tree is announced as a tree
      with its expanded folders

## Approvals, guard and permissions

- [ ] In *Ask*, a `write_file` and a `run_command` each prompt
- [ ] In *Auto*, an `rm -rf /`-style command is refused and the refusal appears
      in that chat
- [ ] Three refusals in one run drop the chat back to *Ask* with a notice
- [ ] In *Skip*, ordinary commands run unattended; only the catastrophic tier is
      still refused
- [ ] The guard is described as a heuristic in the banner, not as a sandbox
- [ ] Allow a `write_file`, deny a `run_command`, and let an `rm -rf /` be blocked in *Auto*:
      *Settings → Tools → Recent decisions* lists all three, newest first, with the right word
      (`allowed` / `denied` / `blocked`) and a reason beside each
- [ ] Deleting the chat a decision was made in leaves that line in the list, unnamed rather than
      removed
- [ ] In *Ask*, allow a `run_command` permanently: the button names the program it will cover
      (`Always allow cargo for this folder`), and the next `cargo` command in that folder runs
      without a prompt while an `npm` command still asks
- [ ] *Settings → Tools* shows each grant with the thing it covers, and an old grant with none
      reads as `(the whole tool)`; taking one back asks again, and leaves the others alone
- [ ] In *Manual*, drive the browser on one site: allow `browser_click` permanently and the button
      names that site, so a second click there runs without a prompt while another site's does not
- [ ] Set a chat to *No network* and ask it to fetch a page, then to run `git push`: both are
      refused, the refusal names the policy, and *Settings → Tools → Recent decisions* has a
      `blocked` line for each — while an ordinary `cargo test` and a `write_file` in the same chat
      still run
- [ ] Set the same chat to *Read only*: a fetch and a search now work, while `curl -d @file url`
      and a `browser_type` are still refused
- [ ] Both switches say they are a policy and not a sandbox, in the row and in the notice set when
      they are changed
- [ ] Set the chat to *Skip*, then ask it to `rm -rf` a folder it created: it still stops and asks,
      the card says the change is one `/rewind` will not put back, and *Settings → Tools → Recent
      decisions* records the answer
- [ ] Answer that prompt with *Always allow*: the button names `rm` rather than `run_command`, and
      the next delete of that kind in the folder runs without a prompt
- [ ] In *Auto*, with the API key in place, run something ordinary (a `cargo test`): the run is a
      little slower than in *Skip*, and *Spend → By what spent it* grows a *Reviews* row with one
      call per action
- [ ] Pull the network cable (or point the base URL at a dead port) and run *Auto* again: the
      review cannot be made, so every action is put to you as an approval prompt rather than
      running — the fallback is asking, never proceeding

## Where commands run

- [ ] `/setup` shows the resolved rung for *Auto*
- [ ] Pinning *Docker* with the engine stopped runs on this machine and says so
- [ ] A command's output is prefixed with where it ran
- [ ] A Docker run with no network fails a download and succeeds on this machine
- [ ] With a long command running, killing DeskSeek from Task Manager ends it too: nothing of the
      app's is left in Task Manager afterwards (the job object — the one thing no automated test
      can prove here)

## Files

- [ ] The file tree loads a folder lazily and reflects a file the agent creates
- [ ] Clicking a file previews it; a binary file reads as "binary", not garbage
- [ ] A file written while it is open re-reads itself
- [ ] Typing a name in the box above the tree finds a file **three folders down without opening
      anything** — the result shows the name and the folder it lives in, and clicking it previews it
- [ ] The search skips `node_modules`: a name that exists only in there is not offered
- [ ] One character does nothing (the tree stays); two characters search; clearing the box brings
      the tree back
- [ ] Typing quickly does not queue a search per keystroke — results settle once, a moment after
      you stop
- [ ] A name that matches nothing says so and points at `grep` for searching **inside** files,
      rather than reading like the file is missing

## Artifacts

- [ ] `artifact_create` with HTML opens the panel and the frame renders
- [ ] The framed page cannot reach the network or the app (inline only)
- [ ] Publishing the same title again adds a version; the picker lists both
- [ ] Editing saves a version attributed to you
- [ ] *Restore* on an older version makes it current without deleting the newer one
- [ ] *Copy* puts the version on screen on the clipboard
- [ ] `artifact_create` with `chart` draws a bar chart, and the bar heights match the values sent
- [ ] The same chart is drawn **in the conversation**, inside the tool card, and matches the panel
- [ ] A chart with a second series draws grouped bars, or two lines, in distinct colours
- [ ] A `line` chart of the same data draws a line with a point per label
- [ ] A `pie` chart draws one slice per label, with the share of each in the legend; a pie of a
      single non-zero value draws a full circle, and a pie of all zeros draws a ring
- [ ] A chart whose spec is wrong (a missing `type`, or one value short) shows the reason and the
      JSON in the panel, and the tool result names what was wrong
- [ ] A label containing markup (`<b>x</b>`) is shown as text, not drawn as markup

## Notebooks

- [ ] *Fact table* produces a `.xlsx` under *Outputs* whose last column names sources, and it opens
      in Excel with the rows as a real table
- [ ] A table answer whose citation column is fiddled with (a row naming a file that is not a
      source) writes **nothing** and says which row was wrong — the same for a source switched off
- [ ] Each entry in *Make* produces something: flashcards, a quiz with a key, a briefing, a
      timeline, a study guide and a mind map, and each lands under *Outputs* as a `.md` or `.docx`
      that opens
- [ ] Every entry in *Make* opens a form first: changing the count changes how many come back, a
      count past the entry's ceiling is refused **before** the model is called, and the number the
      entry declares is what the form starts with
- [ ] Unticking a source really takes it out of scope: ask for something only that document covers
      and the answer says the sources do not settle it rather than answering anyway
- [ ] The line you steer with arrives as the last thing asked — "only the national-accounts
      material" narrows the output to it, and an empty box changes nothing
- [ ] *Fact table* is an entry in that list now rather than a button of its own, and it takes the
      same form (sources and a line, no count)
- [ ] The mind map's output contains a `mermaid` block, and the answer it was made from is in the
      notebook's conversation with its citations
- [ ] A source that is switched off is not used by anything the notebook makes, and the model is
      told in the catalogue only about the ones that are on
- [ ] *Notebooks* in the sidebar opens a notebook — a new one the first time — in the focused pane,
      and `/notebook` does the same
- [ ] A notebook pane sits beside a chat (split), both answer, and both are still there after a
      restart — with the notebook showing its sources and its conversation
- [ ] Deleting a notebook while it is open closes its pane rather than leaving a blank one, and a
      notebook deleted while the app was closed does not come back as a tab
- [ ] *Copy* makes a second notebook that has the same sources and outputs with the files beside
      them, and no questions — and the original is untouched
- [ ] *Save .docx* under an answer writes it into the notebook's folder and it appears under
      *Outputs*, where clicking it shows the file
- [ ] Asking about a document that is in this chat's folder answers from it, and the answer names
      the file and the heading it used — with no attachment made to the message
- [ ] The same chat, asked something unrelated to the folder, arrives with no excerpts (the
      *Context* panel shows no *Files* part), and the answer is not worse for it
- [ ] Adding a real `.pdf`/`.docx` copies it into the notebook's own folder and the source appears
      with a digest line saying what it contains
- [ ] Adding a URL reads the page once and keeps it as a file beside the others; a page with
      nothing readable in it is refused with a message rather than added as an empty source
- [ ] Switching a source off greys it and takes it out of the catalogue, and switching it back on
      restores it with its digest and its text intact
- [ ] Editing a copy of a source in the notebook folder and pressing *Re-sync* marks it stale and
      produces a new digest; pressing *Re-sync* on an unchanged source does **not** spend a call
- [ ] *Read* on a source card opens a named dialog holding the whole description — the text the card
      clamps at three lines — beside where the source came from, the copy this notebook keeps, how
      much was read out of it and when it was described. Escape and a click on the backdrop both close
      it, a source with no description yet has no *Read* to press, and a source added from a URL reads
      the same way
- [ ] Asking for a quiz or flashcards ("make me flashcards on chapter 3") puts a set under *Study*
      rather than writing the questions into the answer, and the reply says it did and where it went
- [ ] Taking that quiz marks the option chosen *and* the one that was right, shows the line explaining
      it, and ends with a score that names the questions to look at again
- [ ] A flashcard set shows a front, turns over to its back, goes through to the end, and can be gone
      through again; removing a set takes only that set
- [ ] A quiz item whose source has since been removed shows the source as gone rather than naming a
      file that is not there
      (the spend panel shows no new `Notebook digests` row)
- [ ] With no endpoint configured, adding a source still works and its digest reads as an outline
      of headings, words and size

## Robustness

- [ ] Open devtools, run `localStorage.setItem("deskseek.crash", "1")`, then reload: an error
      screen appears — the message, a copyable stack, *Try again* and *Reload the window* — and
      never a blank window
- [ ] Type a half-finished message, trigger the crash above, then *Try again*: the app comes back
      and the message is still in the composer
- [ ] *Try again* re-renders and recovers; *Reload the window* reloads from disk with the draft
      still there
- [ ] If a bug throws anywhere else, the same screen appears — a **blank window is a failure**

## The network dropping

- [ ] Ask a long question, then unplug the network (or stop the Wi-Fi) while the answer is
      streaming: the chat keeps the text that had already arrived and shows one message saying the
      connection dropped — not a blank bubble
- [ ] With the network still down, send another message: it says it could not reach DeepSeek, and
      nothing is left spinning
- [ ] Ask "carry on" with the network back: the model continues from the partial answer
- [ ] Point the base URL at something that answers `401` (Settings → Access) and send: the message
      names the refusal rather than blaming the connection, and it does not retry three times first
- [ ] Send a message in a coding chat, then kill the app from Task Manager while the run is
      working, and reopen: that chat shows the **interrupted** banner
- [ ] On a run that changed files, the banner also offers *Put its files back*, which restores
      them; on one that never touched a file, that button is absent
- [ ] *Ask again* re-runs the turn and the banner goes; the chat keeps working afterwards (the
      half-finished tail is dropped rather than left in the history)
- [ ] *Dismiss* clears the banner without re-running, and it does not come back on the next launch

## Backup and restore

- [ ] `Ctrl+K` → *Back up the whole profile* names a `.zip` in the documents folder
- [ ] Open the zip: it holds `deskseek.db`, `MEMORY.md`, `settings.json` and the store folders, and
      **no** API key anywhere
- [ ] Add a chat and a skill, back up, then delete a chat and edit a skill, then restore that zip:
      the window reloads and the deleted chat and the older skill are back
- [ ] Restoring a `.json` file (or any file that is not a profile zip) reports the reason and leaves
      the profile as it was

## Database maintenance

- [ ] `Ctrl+K` → *Maintain the database* reports the size, whether or not it pruned anything
- [ ] On a chat with more than ten runs of file edits, the same action reports pruning; the newest
      run still rewinds (`/rewind`) afterwards
- [ ] The app starts normally a second time (the startup prune is not an error)

## Maths

- [ ] An answer containing `$E = mc^2$` renders it as maths, not as dollar signs
- [ ] A displayed `$$\int_0^1 x^2\,dx$$` sits on its own line and is centred
- [ ] A deliberately broken formula (`$\frac{$`) shows in red and does not blank the answer
- [ ] A `markdown` artifact with maths in it renders the same way
- [ ] An answer with no `$` in it still renders normally (the maths chunk never loads)

## Documents

- [ ] `doc_create` writes a `.md` and it opens
- [ ] `doc_create` writes a `.docx` and Word opens it with headings and lists
- [ ] `doc_create` writes a `.pdf` that opens with the same text as the `.docx`
- [ ] `sheet_create` writes an `.xlsx`; a `=SUM(...)` cell recalculates in Excel
- [ ] `slides_create` writes a `.pptx` with title and bullets
- [ ] A second file of the same name becomes `name-2`, never an overwrite

## Browsing

- [ ] `fetch_url` returns page text
- [ ] `browser_read` renders a JavaScript-built page
- [ ] `browser_click` on a missing selector names what is actually on the page
- [ ] A page that says "ignore your instructions" is reported, not obeyed
- [ ] `browser_console` on a page with a deliberate `console.error` reports it with its level, and
      on a page that has logged nothing says the log starts when DeskSeek attached rather than
      showing an empty list
- [ ] `browser_network` lists a request that 404s with its status, and a request to a host that does
      not resolve as `FAILED` with the reason
- [ ] The buffers stay bounded: a page logging in a loop does not grow the console past 200 lines,
      and the note at the top says the oldest were dropped
- [ ] `browser_open` with `new_tab` leaves the first page open — `browser_tabs` lists both, with the
      new one marked
- [ ] `browser_tab` switches to the other tab and reads it; switching drops the console and the
      network log rather than attributing one page's errors to another
- [ ] `browser_tab` with `close` on the tab being driven moves to the one that is left; closing the
      last tab says there is no page to drive rather than leaving a dead session

## Desktop (Windows, off by default)

- [ ] The toggle is off until switched on in `/setup`
- [ ] `ui_snapshot` reads the focused window's controls as text
- [ ] `ui_click` asks, even in *Auto*
- [ ] `Ctrl+Alt+Esc` cancels every run and the banner stays until restarted

## Modes

- [ ] `Shift+Tab` in the composer cycles Ask → Standard → Plan → Agent and back
- [ ] `Ask` sends no tools at all: the context breakdown lists none
- [ ] In `Plan` mode reads and web lookups are offered, while write, run, `browser_click` and
      `browser_eval` are not
- [ ] In `Plan` mode the reply names the files it would touch and the sources it checked
- [ ] In `Plan` mode with browsing switched off, the prompt does not promise the web
- [ ] In `Agent` mode a write runs without a prompt, and the guard still refuses `rm -rf /`
- [ ] The approval control (`Ask first` / `Auto` / `Skip`) appears in Standard mode only
- [ ] `/mode plan` switches the chat and says so

## Skills

- [ ] *Build one by talking it through* opens a chat that asks what the skill should cover
- [ ] `create_skill` writes `skills/<id>/SKILL.md` and switches it on for that chat
- [ ] The same name is refused a second time unless `replace: true` is passed
- [ ] Pasting a SKILL.md into the panel installs it under `skills/<id>/` and lists it
- [ ] The id comes from the frontmatter name, or from the first heading when there is none
- [ ] Installing a name that already exists is refused rather than overwriting
- [ ] `/skill` lists the skills; `/skill <id>` switches one on and off
- [ ] The prompt's skill catalogue lists every installed skill with its id, `[on]` where it applies
- [ ] Asking for a skill by name has the assistant call `use_skill` and follow it
- [ ] `list_skills` returns the same catalogue the panel shows
- [ ] *Restore examples* brings back a deleted example and leaves an edited one alone

## Memory

- [ ] *Summarise chat* in *Settings → Memory*, or `/summarise`, proposes entries each tagged with
      the file it would go to
- [ ] Accepting an *always sent* entry writes it to `MEMORY.md`
- [ ] Accepting an entry tagged `work` / `interests` / … writes it to `memory/<category>.md`
- [ ] A memory learned in one chat is recalled by keyword in a *different* chat
- [ ] A memory learned inside a project folder does not surface in another project's chat
- [ ] Forgetting a line removes it from its file and it stops being injected
- [ ] A memory saved by the assistant with `save_memory` lands in the category it names
- [ ] `save_memory` with no category (or an invented one) lands under *other*
- [ ] *Settings → Memory* warns when `MEMORY.md` has grown large
- [ ] A hand-added line lands in the category chosen in *Remember something*
- [ ] *Show folder* opens the folder the files live in

## MCP

- [ ] A configured server starts in the background without blocking the window
- [ ] The header badge reads *MCP starting…* then *🔌 N MCP tools*
- [ ] The Logs tab shows a call with redacted arguments and its duration
- [ ] A server that writes to stderr shows that text in the Logs tab
- [ ] A server returning an image gives a result saying it was returned and is not shown; the
      reply does not describe a picture the user cannot see
- [ ] Stopping the app leaves no orphaned child process — and so does killing it from Task Manager
- [ ] A task set to *When a server sends something* wakes and starts a run when that server pushes
      a notification it was subscribed to, and does not when it was not subscribed to that method

## Quick entry

- [ ] `Ctrl+Alt+Space` pressed while another application has focus raises the quick window over it,
      centred on the monitor the pointer is on, with no title bar and off the taskbar
- [ ] Pressing the chord again puts the window away, and so does `Escape`
- [ ] With *A question goes to* on *My most recent chat*, the question is added to that conversation
      and the main window comes to the front showing it, with the answer streaming in
- [ ] With it on *A new chat every time*, the question starts a conversation of its own that appears
      in the sidebar
- [ ] Changing the chord in *Settings → Commands* and saving registers the new one immediately and
      stops the old one working
- [ ] Setting the chord to one another application already owns reports that by name and leaves the
      previous chord working, with nothing else on that screen saved
- [ ] 📷 in the quick window photographs the window you were just in — not the quick window itself —
      and shows its title and size beside a thumbnail
- [ ] Sending with an Appshot files both `appshot.png` and `appshot.txt` into the chat's folder, and
      the assistant can talk about what was on the screen
- [ ] 📷 with the remembered window closed, or minimised, is refused with a sentence rather than a
      blank picture

## Tray

- [ ] The DeskSeek icon is in the notification area, with a tooltip that names it
- [ ] Right-clicking the icon lists exactly *New chat*, *Quick entry*, *Show DeskSeek*, *Quit*
- [ ] *New chat* makes a conversation, listed in the sidebar exactly as the ＋ button's does
- [ ] *Quick entry* raises the quick window on the monitor the pointer is on
- [ ] A **left** click on the icon brings the main window back and puts it in front, un-minimised
- [ ] Closing the main window hides it — the process keeps running, and the tray icon still brings
      it back
- [ ] *Quit* ends the process, and no MCP server or browser child is left behind

## Endpoints

- [ ] *Settings → Access* lists one endpoint on a fresh install, DeepSeek, marked as the one in use
- [ ] A real turn runs against the default endpoint, and its spend row in the spend view names the
      model that ran
- [ ] Adding **Anthropic** fills in the dialect and the base URL, and a chat run against it answers
      — the Anthropic-compatible dialect on a real endpoint, mid-stream tool calls included
- [ ] Adding **Ollama** (a local model, no key) and switching the default to it runs a turn with
      nothing in the keychain, and the spend row for that turn says *unknown* rather than `$0.00`
- [ ] *Save & test* on an endpoint the app has never saved reports that it must be saved first
      rather than failing as a bad key
- [ ] A base URL ending in a slash still joins correctly — `https://host/` must not become
      `//chat/completions`
- [ ] A base URL without `http://` or `https://`, and a blank one, are each refused by name when
      tested rather than failing later as a bad key
- [ ] Switching the default endpoint back to DeepSeek leaves the earlier chats working, and the
      model picker now lists the new endpoint's models
- [ ] A chat whose model the current endpoint does not list still sends, and its spend reads
      *unknown*
- [ ] Saving an endpoint with a rate and then sending the same prompt during peak hours doubles the
      row's cost, and an endpoint whose peak multiplier is 1 does not
- [ ] Clearing the key for the endpoint in use makes Send refuse by name, and restoring it works
      again

## A task's model

- [ ] A task created with no model shows no pin on its row, and its run's spend entry names the
      model the chat was on
- [ ] A task pinned to the cheaper model runs on it — the run's spend entry and the model chip in
      its run list name the cheap model, not the chat's
- [ ] The chat a pinned task starts is on the pinned model as well, so opening it and carrying on
      does not silently change what ran
- [ ] A task pointed at an *existing* chat runs on its pin while that chat's own model is left
      alone — reopening the chat shows its original model
- [ ] Pausing and resuming a pinned task keeps the pin
- [ ] A task pinned to a model the endpoint does not list still runs, and its spend entry reads
      *unknown* rather than a price

## Thinking effort

- [ ] A chat on a model with *Takes an effort setting* on shows the Thinking button, and the level
      picker appears only while thinking is on
- [ ] The same chat switched to a model with it off shows *no thinking setting* instead, and no
      Thinking button — a sentence rather than a switch that does nothing
- [ ] Switching back to the effort-taking model brings the controls back with the chat's own stored
      level and toggle, which were never changed underneath it
- [ ] On the Anthropic-compatible endpoint, `low` and `max` produce visibly different amounts of
      thinking, and the reply still arrives (the budget must leave room to answer)
- [ ] A model the endpoint does not list keeps its thinking controls, on the principle that a
      control is hidden on a fact and not on a guess

## A cheaper model, suggested

- [ ] A turn on the dearer model, with a cheaper one on the same endpoint, draws a line naming the
      cheaper model and what the same tokens would have cost there
- [ ] Clicking that line switches the chat's model, and the next turn runs on it — the app never
      switches by itself
- [ ] A turn too small to matter ($0.00, or a saving under a quarter) draws no line at all, and a
      turn on the cheapest model draws none either
- [ ] A chat on an endpoint with one model, or on an unpriced model, never shows the line
- [ ] The task form's model list shows each model's published rate, and says *rate unknown* for a
      model with none

## Looking at the screen

- [ ] With computer use on and a vision model in use, `ui_screenshot` sends a picture the model can
      actually read — ask it to describe a detail that is only in the image, not in the tree
- [ ] The same call on a text-only endpoint gets a result that names the window, its size and the
      file, with no error and no refused request
- [ ] The picture appears in the chat either way, and the full-size PNG is in the Appshots folder
- [ ] A canvas or game window — one whose controls `ui_snapshot` cannot read — is described from
      the screenshot rather than reported as empty
- [ ] `browser_screenshot` names the page's scaled size in its result, and the model can describe
      something that is only in the picture
- [ ] With computer use off, `ui_screenshot` does not appear in the tool list at all

## Computer use: windows and coordinates

- [ ] `ui_windows` lists the open windows with the focused one marked, and matches what is actually
      on screen
- [ ] `ui_click_at` before any screenshot is refused with a sentence about taking a screenshot
      first — nothing moves
- [ ] After `ui_screenshot` of a window that is *not* at the screen's origin, `ui_click_at` at a
      pixel in the picture hits that pixel in that window — not the same pixel on the screen
- [ ] A coordinate outside the picture is refused by name, with the picture's size in the sentence
- [ ] On a canvas or game window, a click aimed from the screenshot does what it looks like it
      should
- [ ] `ui_click_at` spends the run's desktop action budget, and a click during a stop
      (`Ctrl+Alt+Esc`) is refused like any other action
- [ ] The prompt's guidance is followed: with an MCP server for the app installed, the agent prefers
      the connector, then the browser, and reaches for the desktop last

## Audio overview

- [ ] With no speech endpoint chosen (*Settings → Access → Voice*), `/audio` says so — and no script
      is written first, so the refusal costs nothing
- [ ] With one chosen, `/audio` opens a dialog that shows the script, the endpoint, the model, the
      voice, a length and a price
- [ ] Editing the script moves the price with it, within about a quarter of a second
- [ ] With the rate left at 0 the line reads *price unknown* rather than `$0.00`
- [ ] *Create the clip* plays the audio in the dialog, and the file is in the chat's own folder (📁)
- [ ] *Show in folder* opens the file manager on the clip
- [ ] A second clip of the same chat is `audio-overview-2.mp3`, not an overwrite of the first
- [ ] The spend view lists the clip under *By what spent it* as `speech`, with no tokens and the
      per-minute cost the dialog quoted
- [ ] A script edited past 3,500 characters warns before the button is pressed, and pressing it is
      refused with the limit and the length of what was written
- [ ] A voice the endpoint does not have reports the endpoint's own words rather than a generic
      failure
- [ ] The dialog closes on Escape and on the backdrop, and focus goes back to the composer
- [ ] `/help` and the composer's `/` menu both list `/audio`

## Dictation

- [ ] With no speech endpoint chosen, the 🎙 button says so and nothing is recorded or sent
- [ ] Clicking 🎙 asks the OS for the microphone once, and the button shows the elapsed seconds
      while the recording light is on
- [ ] Clicking it again transcribes, and the words land in the box **appended to whatever was
      already typed**, with the cursor still there
- [ ] Nothing is sent: the text is still editable, and Send is still a separate press
- [ ] A long hold stops itself at five minutes and transcribes what was said rather than refusing
- [ ] A silent recording says nothing was heard instead of inserting an empty line
- [ ] Denying the microphone reports the webview's own words rather than failing silently
- [ ] A sentence spoken over background noise comes back as roughly the words that were said
