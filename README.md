# DeskSeek

A lightweight desktop client for the DeepSeek API — built with **Tauri v2** (Rust core) and
**React + TypeScript**.

Two sections share one core:

- **Chat** — a general assistant with streaming replies, a collapsible *thinking* pane, and a
  `MEMORY.md` profile that is always injected into the system prompt, backed by a folder of
  per-category memory files.
- **Coding** — an agent that works inside a folder you trust.

Bring your own API key: it is stored in the **OS keychain** (Credential Manager / Keychain /
Secret Service), never in a config file and never in a log.

## Status

Both sections are functional:

**Chat**

- Streaming replies with a collapsible *thinking* pane, Markdown and highlighted code — and every
  fenced block, and every answer, carries a quiet **copy** button, so a snippet does not have to be
  drag-selected out of the page
- **Tool steps fold away** — a turn that reads twelve files is one collapsible *N steps* line
  rather than twelve cards. Open it to see each call; a failed step, or a screenshot, opens the
  group by itself
- **A long conversation stays quick** — the transcript draws its newest 60 turns and offers a
  *Show N earlier messages* line above them, every row is skipped unless its own content changed,
  and streamed text is committed once a frame rather than once a token. It used to do the
  opposite of all three: a 500-turn chat re-parsed every answer's Markdown on every token, which
  measured at ~2.1 ms a row on this machine — about 640 ms per token for a 300-row transcript,
  against a 16.7 ms frame budget. Honest limit: 60 rows is a plain window, not a virtualiser, so
  a chat of thousands of turns reveals in 60-row steps instead of scrolling seamlessly
- **It can ask you a question** — when it genuinely cannot proceed without a decision only you can
  make, it calls `ask_user` and the run waits for your answer in the composer instead of guessing;
  every question offers a few options and always lets you type your own
- **Attachments** — drop a file anywhere on the window (an overlay appears), or paste or pick
  one. **Every file is copied into this chat's own folder**, so the conversation really owns it —
  a picture as much as a document. A picture is *also* downscaled and sent to the model as a
  multimodal content part, so the folder keeps the image the user chose while the model is shown
  a smaller one. A text file is sent as its own text, a Word, Excel, PowerPoint or PDF file has
  the words lifted out of it, and a document too long to paste is left in the folder with the
  model told to read it when it needs it. Nothing is captured twice — a file already in the
  folder is used where it is, and a name that is taken becomes `name-2` rather than replacing the
  file already there. Two limits, stated rather than discovered: a picture over 25 MB and a picked
  file over 32 MB are refused with a message that says so, because a picked file has to travel
  through the window as bytes (a *dropped* one does not, so the ceiling is the picker's); and a
  file attached to a brand-new conversation before its first message is read but not filed, since
  the first message is what creates the folder
- **Agentic by default** — the assistant can read, search, write and edit files in **its own
  folder** — created for the chat the first time it needs one, under `…/documents`
  (configurable), and the 📁 in the header shows you where — so it can produce a document for you
  and save it rather than only printing it, and read one you drop in. The **Tools** button in the
  header turns that off for a chat: the next request carries no tools at all, so it becomes a
  plain conversation with no file access, no browsing and no MCP. The choice is per chat and
  remembered
- **Memory** — two kinds of file, because one file has to be either short or cheap and it cannot
  be both. Both are plain markdown you can read and edit by hand: the files *are* the store.
  - `MEMORY.md` is the **profile**: who the user is and how they want replies — name, language,
    standing preferences. It is short on purpose, sent with *every* request, and editable in-app.
  - Everything else lives in `memory/<category>.md`, one file per part of life — `work`,
    `education`, `projects`, `interests`, `people`, and `other` — looked up by keyword against
    what you just asked. Only the matching entries are sent, so the store can grow without
    slowing anything down. Inside a project folder, memory is filed with that project instead
    (`project-<name>.md`), so a note about one client does not turn up in another's chat.
  - Summarising is a command, not a panel: `/summarise` reads the open conversation, proposes
    entries, and tags each one with the file it would go to. Nothing is stored until you accept
    it in *Settings → Memory*, and a proposal only reaches `MEMORY.md` when it says so.
  - When the assistant remembers something itself — with `save_memory` — it does the same: only an
    explicit `profile` category goes to `MEMORY.md`, anything else goes to the category it names,
    and a name it invents falls back to `other` rather than creating a file. Memory saved
    mid-run therefore cannot quietly grow the always-sent file.
  - *Settings → Memory* (`Ctrl+M`, or `/memory`) shows every category with its entries, lets you
    forget any single line, add one by hand into a category you choose, and warns when `MEMORY.md`
    has grown past what is worth sending every time. *Show folder* opens the files themselves.
  - Honest limits: the lookup ranks by shared words, not meaning, so asking about "the car" will
    not surface a memory stored as "vehicle" — embeddings would fix that, at the cost of a model
    download and network at query time. Every category file is also scanned in full on a lookup,
    which is fine for thousands of lines and not for millions.
- **Chat log** — each summary is also written to `CHATLOG/YYYYMMDD-topic.md`
- **Skills** — instruction files at `skills/<id>/SKILL.md`, toggled per chat. Easier to build by
  talking: *Build one by talking it through* opens a chat, the assistant asks what it should
  cover, and `create_skill` writes it straight into your skills folder switched on for that chat
  (`replace: true` revises one it wrote). You can also paste a skill's text in, install one from a
  file, or put back the bundled examples you deleted — and remove one from the same list (the
  panel also opens the folder)
  - The prompt carries the catalogue with each skill's id, so the assistant can reach for one
    itself: `use_skill` switches it on and hands over its instructions mid-conversation, and
    `list_skills` asks for the list again. `/skill` lists them from the composer, `/skill <id>`
    toggles one, and the panel's toggle does the same thing
  - **Nothing is sent until it is used.** The list is always in the prompt — a skill that is off
    costs one line — and only the skills marked `[on]` have their text in there too. So a store of
    twelve skills costs twelve lines until one is actually wanted, which is the point of having
    them
  - **Two switches, and they answer different questions.** The checkbox is *this chat*: the skill's
    text joins this conversation's prompt. **always on for new chats** writes `default: on` into the
    skill's own `SKILL.md`, so every chat you start from then on begins with it switched on — the
    right setting for a standing preference, and the one to be deliberate about, because that text
    then goes with every request in those chats. Neither switch touches a chat that already exists
  - Skills live in **Customise** (the sidebar's *Customise* entry, or `/skills`), together with
    connectors, instructions, design systems and prompts
- **Design systems** — a stored style reference the assistant applies when it builds or writes UI:
  the type scale, the palette, the spacing, and the rules that hold them together. They live in
  **their own folder**, one markdown file per system (`design/<name>.md`), so they are never a
  skill with a prefix and the two lists cannot disagree about what they are
  - **The assistant works with them directly.** They are listed in its prompt with each id and
    whether this chat has one switched on, so *"use our house style"* needs no explanation:
    `use_skill` switches one on and hands over its text, `create_design_system` writes or revises
    one from the conversation, and `draft_design_system` starts one from a project folder that
    already has a look. Like skills, only the switched-on ones cost anything in the prompt
  - *Draft from a folder* in **Customise → Design systems** does the same thing from the panel;
    or write one from scratch and revise it by saving again. It joins the prompt while it is
    switched on for a chat, so a page or a document comes out looking like the rest
  - Two switches, on purpose: the checkbox is *this chat*; there is no "always on" for a design
    system, because applying one is a decision rather than a standing preference
- **Prompts** — the requests you retype, kept as text. A prompt is a *one-shot* where a skill is
  a *rule*: `/p:<name>` in the composer's slash menu drops its text into the box to edit before
  you send it (it is never sent on your behalf, and the model is never told the list exists, so a
  shelf of them costs nothing). Ctrl+K offers them too. One markdown file each in
  `prompts/<name>.md` — **Customise → Prompts** is a convenience over that folder, not the only
  way in, so a prompt can be added or fixed in any editor
- **Projects** — a named container with a folder, the conversations that belong to it, and the
  three things every chat in it shares. *New project…* picks the folder; a chat started inside it
  works there, and an existing chat can be moved in or out from the panel. Deleting a project
  keeps its chats — they are only unassigned, which is exactly why it needs no confirmation
  - **Rules** — the project's own instruction blocks, written once and inherited by every chat in
    it: a client's tone, a house format, a set of terms. They go through the same scope filter as
    the global instructions in *Settings → Instructions* — a rule can be for `chat`, `coding` or
    `both` — so one rule set covers the whole project without being retyped into each chat, and an
    edit reaches every turn in it from then on. A chat moved out of the project stops seeing them,
    which is the point of holding the rules on the project
  - **Memory** — a project's chats share its folder, and memory learned in that folder is filed
    with the project (`project-<folder name>.md`). *Memory* in the panel shows exactly those
    entries, so what the assistant has learned about a client is in one place. A project without a
    folder has no shared file — its chats each keep their own memory — and the panel says so
    rather than showing another project's notes
  - **Spend** — the project's all-time cost, summed over the turns in its chats. It is the same
    number the sidebar shows per chat, added together; the *Spend* panel is still the windowed
    view, for "this month". Honest limit: the memory view is read-only, so forgetting a single
    entry is still done from *Settings → Memory*
- **Scheduled tasks** (*Scheduled* in the sidebar, or `/scheduled`) — a prompt that runs itself on
  a calendar: every day at 09:00, every Wednesday at 17:00, or **the first Wednesday of the month**
  at 08:00. There is no cron string to learn; the three fields that describe the schedule *are*
  the schedule. Each run goes through the ordinary turn path in a fresh chat, so a daily task does
  not become one conversation that grows forever, and the chat appears in the sidebar under the
  task's title. *Run now* fires it without moving when it next runs on its own; *Pause* leaves it
  in the list and stops the clock
  - Three things are deliberate rather than missing. A run starts in **auto** — nobody is watching
    at 09:00 — so it never waits for an ordinary approval; the catastrophic guard still applies,
    and a few tools are never auto-approved at all. One of *those*, or a question the model asks,
    stops the run and puts it in the panel's **Needs you** list, showing the same prompt the chat
    does: answer it there and the run carries on, so a decision never has to be hunted for in a
    chat nobody has open. It also raises a notification the moment it parks, so nothing waits in
    silence. **Honest limit:** a park only means something while the app is running — one left
    behind by a close or a crash is listed as *lost* with the reason, because the run it belonged
    to is gone, and re-running the task is the way to pick it up. And a window missed while the
    app was closed fires **once** on the next launch rather than catching up on every missed
    occurrence
  - A task can also be woken by **something changing** rather than by a clock, which is the other
    kinds in the same form. Point it at a **folder** (optionally only files matching a pattern like
    `*.csv`), at a **git repository** and a ref, or at a **page** — with a word to wait for, or with
    nothing and it fires on any change at all. The fourth kind is the one DeskSeek does not check
    for you: pick an **MCP server**, and if you like the notification it should react to, and the
    server waking it starts the run. Two rules make them usable rather than noisy. A **look is not a
    run**: it starts something only when what it sees *differs* from what it saw last time, so a
    folder that stays busy starts one run and not fifty, and a page that merely contains a word is
    not the same as a page that has just come to say it. And **six runs an hour** is the cap, which
    delays rather than drops — a change it will not fire on yet is still pending, and the row says
    so in words rather than going quiet. **Honest limits:** the check is a poll on the interval you
    choose, so a change is noticed up to one interval late, and it only looks while the app is open.
    A change that happened and was undone while DeskSeek was closed is never seen, and an MCP event
    that arrived then is not replayed — unlike a missed cadence, which fires once on the next launch
- **It tells you when it is done** — a conversation that finishes while you are reading another one
  gets a green dot on its sidebar row, cleared the moment you open it; a scheduled task starting,
  or the assistant calling `notify_user` at the end of a long job, raises an **OS notification** as
  well. It only pops up when the window is not already in front of you — if it is, the answer is on
  screen and a second popup over it is noise — and **Settings → Tools** turns the whole thing off
  or loosens that rule. The dot is not affected by either switch: `on` or `off`, you can still see
  which conversation moved while you were elsewhere
- **Scoped instructions** — named instruction blocks that apply only in chat, only in coding,
  or both (the bundled *Document design* one is an example)
- **The system prompt is assembled, not written once** — the base prompt says what you have
  (memory, tools, skills, MCP servers); the app appends the guidance that matches what is actually
  switched on for that chat: planning, artifacts and documents, where a command really runs, the
  internet, the desktop, MCP, the approval mode, the project's own context files and its rules,
  your scoped instructions, then memory — and the skills this chat has on, whose text joins the
  prompt as its own section. **Settings → Models & prompt** shows the base prompt; *Context* in a
  chat shows the assembled result part by part
- `/compact` and friends — `/help` lists the built-in commands, including `/summarise`, `/mcp`,
  `/projects`, `/artifacts`, `/scheduled` and `/export`
- **`@` a file and `$` a skill from the composer** — `@` lists the workspace, searched by name
  wherever the file is, and `$` lists the installed skills; either one **fills the box** rather
  than sending, so the mention is text you are mid-sentence in. A marker that means nothing to the
  app stays ordinary text
- **Command palette** (`Ctrl+K`) — type to jump to a conversation in either section, most
  recently used first, or to run the same action a panel button does; every entry is a state
  change the UI already makes
- **Quick entry** — a global chord (`Ctrl+Alt+Space` by default, changed in **Settings →
  Commands**) raises a small always-on-top window over whatever you are doing, centred on the
  monitor your pointer is on, and pressing it again puts the window away. The question goes to
  your most recent conversation or to one of its own, whichever you choose; it is an ordinary
  turn underneath, so the answer streams into the main window and the chat it landed in is
  brought to the front. Honest limit: a chord another application already owns cannot be
  registered, and that is reported where you set it rather than swallowed
- **Appshot** — the 📷 button in that window photographs the window you were in a moment ago,
  *before* the quick window took the focus, and attaches both the picture and the window's
  readable text to the question. It is the same two halves a dropped file gives: the image as an
  image, the words as text, filed into the chat's own folder. Windows only, matching `ui_snapshot`,
  and it refuses a window that is minimised or that will not draw itself rather than filing a
  blank picture
- **Tray** — DeskSeek sits in the system tray with *New chat*, *Quick entry*, *Show DeskSeek* and
  *Quit*; a left click on the icon brings the window back. **Closing the window puts it away
  rather than ending the session**, which is what a tray icon is for. Quit from the menu (or the
  icon's own right-click) really ends it. If the tray cannot be created — the shell has it
  switched off — the window closes the ordinary way instead, because a hidden window with no way
  back is worse than quitting
- **Sidebar search** — filter the conversation list by title or folder as you type, and search
  the *text* of every conversation: matching messages are listed with the words in context, from
  both sections, so a chat can be found by what was said in it rather than by its name. The
  results are **ranked by relevance, not recency** — a message that is *about* the word comes
  above one that merely mentions it — the matched word is emphasised in the snippet, and a long
  list is **paged**: *Show more matches* adds the next screenful underneath, so a word in
  hundreds of messages is browsable rather than truncated at 40. Honest limit: ranking is BM25
  over the words themselves, so it finds what you typed and not what you meant — asking for
  "invoice" will not surface a message that only ever says "bill"
- **Archive** — a conversation worth keeping but not worth looking at can be put away instead of
  deleted: the arrow on its sidebar row files it under *Archived (n)* at the bottom, and `↑`
  brings it back. Nothing is removed — the messages, the spend and the folder stay — which is
  exactly why it needs no confirmation. Deleting is still the only thing that destroys anything
- `/export` — write the whole conversation to a Markdown file in the chat's own folder; the
  palette also offers **Export this chat as a PDF**, printed by the browser on this machine
  through the same pipeline `doc_create` uses rather than a second implementation of its own
- **Duplicate a conversation** — *Duplicate* in a sidebar row's right-click menu. The copy gets
  its own transcript and, for a chat-mode conversation, its own copy of the folder; a coding
  chat shares the project's folder, because duplicating a conversation is not a reason to copy a
  client's working tree. It starts at **no spend**: nothing was paid for the copy, and a second
  chat claiming the same dollars would double-count in the *Spend* view
- **Move a conversation between machines** — *Export this chat as JSON* writes a versioned
  `name.chat.json` beside the conversation's own files, and *Import a conversation from JSON…*
  reads one back as a new chat. Importing is additive, so a file that turns out to be the wrong
  one costs nothing. **Honest limit: it is the transcript, not the conversation.** Attachments
  are referenced by name and live as files in the folder the export came from, so the imported
  copy remembers them but does not have them; take the folder too, or re-attach them
- **Task list** — for multi-step work the model publishes its plan with the `update_tasks` tool
  and the checklist is pinned above the composer, ticking over as steps start and finish. It is
  collapsible, and a step in progress pulses so a long job is visible at a glance
- **Artifacts** — a document the model publishes with `artifact_create` opens in a panel beside
  the chat, rendered live and versioned, instead of scrolling away as a giant code block
- **Documents** — `doc_create`, `sheet_create` and `slides_create` write real `.md`/`.docx`/`.pdf`,
  `.xlsx` and `.pptx` files into the chat's own folder, with an *Open* button on the result. The
  same formats are read back: `read_file` opens a `.docx`, `.xlsx`, `.pptx` or `.pdf` as the text
  inside it rather than refusing it as binary, and `write_file` refuses to write one as text, so a
  file that merely calls itself a `.docx` can never be produced
  - Honest limits: what comes back is what a reader would see — formatting, images and comments
    are left out, and a document whose words are a picture (a scan) says so rather than coming
    back empty. Revising a document means reading it and creating it again, not editing the
    original in place
- Approximate **context-window meter** in the header — and once a conversation passes 80% of the
  model's window, the same fact appears above the composer with a *Summarise it* button, so the
  fix is next to the warning rather than a command you have to remember. Past 90% a run folds the
  conversation itself: the transcript is replaced by its summary, with a note saying how many
  messages went into it, so a chat that would have run out of room carries on instead — and a
  scheduled run, which has nobody watching the meter, fits the same rule
- **Per-chat cost** — the sidebar shows each conversation's accumulated spend, summed from
  real usage as every turn ends
- **Spend** (*Spend* in the sidebar, or `/spend`) — the same figures over a window: the total,
  then a pie of **where the tokens went** per section, then a bar per day, then the same money by
  model and by conversation. One row is written per **call** — a turn, a summary, a fold, an
  in-place rewrite — added to as the run goes, so a turn you stopped half-way still appears for
  what it really spent, and a call nobody typed appears too rather than hiding inside a total —
  which is also what makes this
  agree with the per-chat totals rather than telling a second, quieter story. The split is the
  half of the page that is not a bill: a coding chat that reads a lot of files burns **input** and
  a conversation burns **output**, same total, different problem, and *By what spent it* names the
  calls nobody typed — a summary, a fold, an in-place rewrite — each priced onto the same rows a
  turn is on. A model with no published price has its tokens counted and its money marked
  unknown, never zero: the figure is then a floor, and the panel says so. Honest limit: prompt and
  completion are billed at different rates and cached input at a fraction of fresh, so the pies
  are a picture of where the tokens went rather than what each part cost. The prices are the
  published rates for the model and the hour, so treat the money as an estimate of your bill
- **A spend limit** — *Stop a run after spending* in **Settings → Models & prompt** (0 is off)
  ends a run between steps once its cost crosses the figure, and says so in the chat rather than
  going quiet. It is checked between rounds, so a long loop of tool calls cannot slip past it
- **The model can look back** — `search_history` lets it search what was said in earlier
  conversations here, so "what did we decide about X" is a lookup rather than a guess or a re-ask.
  It searches the messages, not the files (`grep` is for those), and it is **narrower than the
  sidebar's search on purpose**: a coding chat reads its own folder and a chat-mode conversation
  reads that section, so one folder's agent can never quote another's
- **Regenerate, and edit-and-resend** — the copy icon under the newest answer sits next to
  **Retry**, which asks the same question again, and *Edit* under your own last message changes it
  and asks again. Either way everything after that turn is dropped first, so the model is never
  shown the answer it is replacing — but **dropped is not deleted**: the answer that was there is
  kept, and a ↺ button on that turn swaps it back, keeping the one you are leaving. It is a
  toggle between two answers, not a history to wade through, and neither answer costs a second run

**Coding**

- Point a task at a **folder** it may work in
- Tools: `read_file`, `list_dir`, `glob`, `grep`, `search_history`, `write_file`, `edit_file`,
  `run_command`, `git_status`, `git_diff`, `git_log`, `git_commit`, `git_push`
- **Every write and every command asks first** — allow once, allow for the session, allow
  always for that folder, or deny. Reads run freely. An *always* answer names the **target** it
  covers rather than the whole tool: the program a command line runs (`cargo`, not
  `run_command`), the folder a write lands in, the app a desktop click presses in, the site a
  browser action is about to act on. So the next `cargo` command in that folder runs without a
  prompt while an `npm` one still asks, and a command that chains (`cd x && …`) gets no target
  at all: the grant on offer stays the whole tool rather than a prefix that would cover the
  rest of the line. The grants live in **Settings → Commands → Always allowed, per folder**,
  each with the thing it covers, where each one can be taken back — the only decisions that
  survive a restart are the ones you can see and revoke
- A **path sandbox** refuses anything outside the workspace, including `..` traversal and
  symlink escapes; the API key is stripped from every child process environment
- **Commands can run in a container** instead of on this machine — see *Where commands run* below
- A **live file tree** (`/files`, or the *Files* button) — the folder is watched, so files
  the agent creates or edits appear as it works. Click any file to read it without
  interrupting the run; if the agent writes to the file you are reading, it re-reads itself.
  The box above the tree **searches the whole folder by file name** — it reads the folder rather
  than filtering what you have already opened, so a file three levels down is found without
  opening its parents. It skips the same noise directories the agent's own tools skip
  (`node_modules`, `target`, `.git`…), needs two characters before it looks, and caps what it
  returns, so a large tree cannot hang the window. Contents are not searched: that is `grep`,
  and it is the model's tool
- Project context from `AGENTS.md`, else `.deskseek/context.md` — and, in any chat that belongs
  to a project, the project's own rules, which apply to every chat in it (see *Projects*)
- A **task list** the model keeps up to date as it works — see the chat section above
- Tool cards **open themselves when a call fails**, so a broken build is readable without a
  click
- **Undo a run** — `/rewind` (or the palette) puts back the files the agent's last run changed:
  before `write_file` or `edit_file` touches a file, its previous content is recorded, so an edit
  is restored and a file the run created is removed again. It undoes **one run at a time, newest
  first**, because an older run's pre-images are only correct once the runs after it have been
  undone. Two honest limits: a shell command that rewrites files is invisible to it (`npm run
  build` is not undone) — which is why every `run_command` card says so before it runs — and it
  says how many files it put back rather than claiming success
- **An interrupted turn is reported, not left to rot.** A run writes a marker before it starts and
  clears it however it ends — finished, stopped or errored — so a marker still there at launch
  means the app was killed mid-turn. That chat opens with a banner: *Ask again* re-runs the turn
  (which is also the repair, because the history ends on a tool call with no result and the next
  request cannot be built from that), *Put its files back* offers the rewind above when the run
  had changed something, and *Dismiss* just clears it. Notices like this are read once at
  startup; a turn running right now is never reported as interrupted

**Approvals, permission modes and stopping a run**

Each conversation carries its own **mode**, chosen beside the message box and cycled with `Shift+Tab`
(or `/mode <name>`). The modes are a ladder, and each rung is about what is *allowed*, not tone:

- **Ask** — no tools at all. The model answers from what it knows and says plainly when something
  would need a file, the web or a command, instead of implying it looked
- **Standard** — every tool the section offers, with the approval mode below deciding what is
  asked about
- **Plan** — reads only: files and searches, plus lookups on the web (`fetch_url`, `web_search`,
  and the browser's *reading* tools for pages JavaScript builds or pages you have to page
  through). It cannot write, run or click anything — `browser_click`, `browser_type` and
  `browser_eval` are all actions on someone else's site, so they wait until the chat is allowed
  to act. Useful for "tell me what you would do" on a codebase you have not handed over yet
- **Agent** — every tool, and nothing asks: it works until the job is done. The guard still
  refuses the catastrophic tier, so autonomous is not unchecked — and a delete is still put to
you, because that is the one change no rewind can undo

In *Standard* the mode is followed by the chat's **approval mode**, which only exists there
because it is the only rung with something to approve:

- **Ask first** (default) — pause for approval before every write and every command
- **Auto** — run without asking. A safety guard refuses known-catastrophic commands (disk
  formatting, `git reset --hard`, `git push --force`, publishing to a registry, `rm -rf /`,
  download-and-execute pipelines) and commands that upload local data to the network. Three
  refusals in one run drop the chat back to *Ask first* automatically. What the guard lets
  through is then read by a **small, cheap reviewer** — the tool, its redacted arguments and the
  last few turns — which answers *allow*, *refuse* or *ask*: a refusal stops the call and says so,
  and an *ask* becomes the approval prompt, so a reviewer that cannot tell hands the decision to
  you rather than waving it through. It is one extra cheap call per action, and it appears in the
  spend view under *By what spent it* like every other call
- **Skip** — never ask. Only the catastrophic tier is still refused

The one thing every mode stops for is a **delete**. `write_file` and `edit_file` record a file's
previous contents, which is what `/rewind` walks, but a file an `rm` removes was never the app's to
remember — so a command that deletes (`rm`, `del`, `rd`, `Remove-Item`, `git clean`, `git rm`) is
put to you whatever the mode says, *Skip* included. Answering "always" still works, and it is a
narrow answer now: the grant names the program (`Always allow rm for this folder`), not
`run_command`

One honest caveat, stated plainly in the app too: **the guard is a heuristic, not a security
boundary.** It catches the patterns a confused or prompt-injected model reaches for. It is not a
sandbox, and a missing match is not proof of safety. The reviewer above is a *second opinion* for
the same reason and has the same limit — it is a cheap model reading a summary, not a proof. The
real containment is the workspace path sandbox plus the API key being stripped from every child
process.

**Every decision is written down.** *Settings → Tools* lists the last hundred, newest first: what
you were asked, what you said, and the refusals the guard made without asking. It is there because
of a gap — a call you *allowed* leaves a tool result in the transcript, but one you *denied*, or
one the guard blocked, leaves nothing at all, so the only record of it would otherwise be your
memory. The arguments are stored as a short hash rather than as themselves, because an approval can
cover a command carrying a secret and a log you read back later is not the place to keep one; the
hash is enough to tell two approvals of the same tool apart. Unlike every other table, the log has
**no foreign key to the chat**: deleting a conversation must not erase what it was allowed to do,
so a decision about a chat you deleted stays, unnamed.

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
That path has a gap — a grandchild whose parent has already exited is reparented and escapes
`taskkill` — but nothing escapes the app *ending*. Every process DeskSeek starts joins a Windows
job object that is killed when the app's last handle to it closes, so a crash, a power cut or a
kill from Task Manager takes the whole tree with it instead of leaving a build running with
nobody left to stop it.

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
the pane you clicked, not the other one. Right-clicking a conversation in the sidebar offers
**Open in split view**, which puts it in the other pane — splitting first if there is only one —
rather than replacing what is already on screen. Closing a tab refills its pane from another open tab
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
| `F2` | Rename the open conversation, in the sidebar |
| `Ctrl+B` | Show or hide the file tree |
| `Ctrl+M` | Memory settings |
| `Ctrl+K` | Command palette |
| `?` | This list, over whatever is on screen |
| `Ctrl+,` | Settings |
| `Ctrl+\` | Split into two panes, or back to one |
| `Ctrl+1` / `Ctrl+2` | Move focus to the left / right pane |
| `Ctrl+G` | Jump to the newest message |
| `Escape` | Close the open panel or dialog |

`Cmd` works in place of `Ctrl` on macOS. `Escape` belongs to whatever field has
focus: in the composer it dismisses the slash-command menu and leaves the panel
alone, and `F2` is the same — a bare key, so it acts when no field has focus. The
map is a pure function (`src/lib/shortcuts.ts`) with its own checks, so a chord
can never fire for the wrong key.

Two chords are global rather than in-window, and so live outside that map.
`Ctrl+Alt+Space` raises **quick entry** over whatever application you are in; its
combination is set in **Settings → Commands**, and a combination another
application owns is refused there when you save — the shortcut you had keeps
working. `Ctrl+Alt+Esc` is the kill switch: it stops every run and pauses
computer control, and it is fixed rather than configurable, because a hotkey you
cannot find is not a means of stopping something.

**Internet access** (both sections)

- `fetch_url` reads a page as text, `browser_read` renders it in a real browser first (so
  JavaScript-built pages work), and `web_search` queries a **SearXNG** instance — the setup
  screen will stand one up for you on Docker
- All three are read-only, so they never prompt; turn them off in **Settings → Browsing**
- `browser_open` and `browser_extract` are reads too — the same lookup, spread over two calls
  when a page needs poking at — so they prompt for nothing either
- `browser_read` drives the Chromium-family browser already on your machine — nothing to
  download. Point it at Chrome or Edge explicitly if auto-detection picks the wrong one
- A chat can **declare what it may send out** — *Read only* (pages and searches are fine, an
  upload or a form-submitting click is not) or *No network* (nothing reaches out at all) — set
  in the composer's row beside *Incognito*. It is a **refusal in every mode** rather than one
  more thing to approve, so it holds in *Skip* too, and the refusal says which policy refused it.
  Said plainly beside the switch: it is a policy and not a sandbox, because a program the agent
  runs can open a socket by itself. What it refuses is what *this app* sends out

**Driving a browser** (both sections)

`browser_read` reads one page. When a job needs more than that — filling in a form, clicking
through to a result, checking what a site really does — the agent can drive the browser instead:

| Tool | What it does |
| --- | --- |
| `browser_open` | Open a URL and read the page. A read, so it does not prompt. The browser keeps its state, so open once and work on it. With `new_tab` it opens a second tab instead and drives that one |
| `browser_extract` | Read the page that is already open — also a read |
| `browser_eval` | Run a JavaScript expression in the page and get the value back |
| `browser_click` / `browser_type` | Click an element or fill a field by CSS selector, then read the page back |
| `browser_screenshot` | Show **you** a picture of the page, in the chat |
| `browser_console` | What the page has logged since it was opened: console output, errors, deprecations and failed requests — usually the page saying why it is broken |
| `browser_network` | Every request the page made, with its status or the reason it failed |
| `browser_tabs` / `browser_tab` | List the open tabs, switch to one, or close one. Switching drops the console and the network log, because those belonged to the page you left |

It is Edge or Chrome, launched headless with its **own profile directory** under DeskSeek's data
folder on an auto-assigned debugging port. It never touches the browser you are signed into: it
cannot see your tabs, your cookies or your logins, and nothing outside this machine can reach it.
There is no CDP crate and no bundled download — the protocol is JSON over a WebSocket, and the
module speaks it directly.

Four things are worth saying plainly:

- **The four that act always ask, even in *Auto* mode.** `browser_eval`, `browser_click`,
  `browser_type` and `browser_screenshot` — no string rule can tell a harmless click from a
  purchase, so the app does not pretend to. Only *Skip* runs them unattended. Opening and reading
  a page is a lookup and is classed as a read, like `browser_read`. `browser_tab` asks too: closing
  a tab throws away whatever that page was holding. The console, the network log and the tab list
  are reads — they report what the page already did
- **The log starts when DeskSeek attaches.** Console output and requests from before that are not
  replayed, because the browser only reports events as they happen. `browser_console` says so
  rather than showing an empty list, and both buffers are **bounded** — the last 200 console lines
  and the last 300 requests, oldest dropped — because a network log is unbounded by nature
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
chat. The document opens in a panel beside the conversation and is kept, not scrolled away.
Every one of them is also in the **Artifacts** list in the sidebar (`/artifacts`), newest first
across every conversation — a document is usually wanted long after the chat that made it is
forgotten:

| Kind | How it is shown |
| --- | --- |
| `html` | A sandboxed frame, rendered live |
| `svg` / `mermaid` | The diagram, drawn and centred on white |
| `chart` | A bar, line or pie chart, drawn from the numbers you send |
| `markdown` | The normal Markdown renderer |
| `code` | Monospace, read-only |

Each artifact is **versioned**. Calling `artifact_create` again with the same title publishes a
new version rather than a second document; a version menu goes back to any earlier one, and
*Restore* makes an old version current again — by saving it as a new version, so nothing is ever
overwritten. *Edit* opens the document as text; saving it adds a version attributed to you. The
tool card in the chat names what was published, and the live panel is where the substance lives.

A `chart` artifact takes **data, not markup**: the content is a small JSON spec —
`{"type":"bar","labels":["Jan","Feb"],"series":[{"name":"Sales","values":[10,40]}]}` — and the
app draws the bars, the line or the pie. That is deliberate. A model asked to hand-write SVG for
a plot produces a picture that is subtly wrong and impossible to check by looking; a spec either
has one value per label or it says it does not. `type` is `bar`, `line` or `pie`, up to eight
series, and the tool result names what to fix when a shape is wrong. A **pie** is one series
divided between the labels — a breakdown rather than a series over time — so it takes exactly one
series, no negative values, and at most eight slices; its arcs are computed from the same sum the
legend prints its percentages from, so the numbers and the picture cannot disagree. A chart that
still cannot be drawn shows the reason and the JSON in the panel rather than an empty frame. A
chart is also **drawn in the conversation itself**, inside the tool card that published it — the
card has the numbers it sent, so the picture is there without a click and without the panel
having to be open.

**Honest limit of the drawing code**: it is bars, lines and pies on a linear axis, with no
stacking, no log scale and no smoothing. The legend spaces its entries by an estimated character
width, so a very long name can overrun the next swatch.

**Maths** renders as LaTeX wherever Markdown does — in the chat and in a `markdown` artifact.
Write `$…$` for inline and `$$…$$` for a displayed equation. It is the model's own LaTeX, drawn by
KaTeX from fonts bundled with the app, so nothing is fetched at read time. The renderer is a lazy
chunk that loads the first time an answer contains a `$`, and a formula it cannot parse is drawn
in red rather than taken as a reason to fail the whole answer.

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

**When something in the window breaks**

A component that throws would leave a blank window and nothing to go on, so the app renders on top
of an error boundary: what failed, a stack you can copy, and two ways back — *Try again*, which
re-renders, and *Reload the window*. It is honest about being a report rather than a fix. The
composer is the exception it protects: a half-typed message is mirrored into `sessionStorage`, so
it comes back after *Try again* and after a reload. You can produce a crash on purpose to see the
screen — `localStorage.setItem("deskseek.crash", "1")`, then reload.

**When the connection drops mid-answer**

A network failure is not the model refusing, and the two are told apart rather than collapsed into
"something went wrong":

- **Before the first token, nothing has been said** — so the request is sent again, up to three
  times with a growing wait, and only when the endpoint was busy or briefly broken (`408`, `429`,
  `5xx`). A `400` or a `401` is final, because retrying it just costs you time.
- **Once tokens have arrived, a retry would say everything twice** — so the run stops there. The
  part that arrived is written into the chat *before* the error is raised, so a dropped connection
  costs you nothing that was already on screen: the text stays, and you can say "carry on".

The message names which one it was — *"Could not reach DeepSeek (3 tries)"* or *"DeepSeek refused
the request (401)"* — because they call for different things from you.

**Connectors (MCP servers)**

Managed in **Customise → Connectors** (`/mcp`). DeskSeek is an MCP *client*. Servers you configure
are launched as child processes and their tools are offered to the model in **both** sections:

- Config lives in `mcp.json` next to your settings, and can be edited in the app under
  **MCP servers** or by hand (the panel has a *Re-read file* button for that)
- Tools are exposed as `mcp__<server>__<tool>`, so two servers offering `search` never collide
- **Every MCP call asks first.** A server's own `readOnlyHint` does not skip the prompt unless
  you tick *trust read-only* for that server — a server describes its own tools, so that
  shortcut is opt-in
- Server output is treated as data, not instructions
- Servers start in the background, so a slow `npx` download never blocks the window
- **A tool result is text.** When a server returns an image or an audio clip, the model is told
  exactly that — returned, and not shown, because a tool result cannot carry one — rather than
  being handed a placeholder that reads like the picture is on its way. The multimodal channel is
  your own attachments, not this one
- Servers are stopped when the app exits, **and when it is killed** — they join the same job
  object as every other child, so nothing of DeskSeek's survives a hard kill
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

**Backups** (both sections)

*Back up the whole profile* and *Restore a backup over this profile…* are in the command palette
(`Ctrl+K`). A backup is **one zip** in your documents folder holding the database, `MEMORY.md` and
`memory/`, the skills, design systems and saved prompts, the `documents/` folder conversations
filed into, the chat logs, the settings and the `mcp.json` server list. It **never contains the API
key** — that is in the OS keychain, which is not a file this app can hand out, and a backup is a
file that gets mailed around. The automation browser's own profile is left out too: it is
re-creatable and never your data.

Two honest points. The database is checkpointed before it is copied, because in WAL mode the newest
writes sit in a side file until something flushes them. And restoring replaces the database the
window is already reading, so the app **reloads itself** afterwards — every open chat, transcript
and stored shell belonged to the profile that was just swapped out. Restoring is all-or-nothing: the
archive is unpacked into a scratch folder first, so a file that is not a backup leaves the profile
as it was rather than half-replaced. A coding chat's attachments live in *your* workspace, not in
the profile, so they are not in the archive.

**Keeping the database small**

Two tables grow for ever if nothing sweeps them, and neither is a log. `file_checkpoints` keeps a
pre-image per changed file per run, and a rewind can only ever walk back **one** run — so the older
runs' pre-images are unreachable by construction. Artifact versions keep every revision a document
ever had, and nobody opens the twentieth-from-last. Both are pruned to a fixed depth (**10 runs**,
**20 versions each**) every time the app starts, and *Maintain the database* in the palette
(`Ctrl+K`) does it on demand and tells you the size. The prune is by reachability rather than age,
so a run from a year ago that is still the newest in its chat is still there. A `VACUUM` — which
rewrites the whole file — only runs when a prune freed a few hundred rows, so an ordinary launch
does not pay for it.

**Taking the record with you**

*Export the decision log and the spend* in the palette writes one JSON file to your documents
folder: the same decisions **Settings → Commands** lists, and the same bill the spend panel shows
for the same window — the export calls those reads rather than querying again, so the file cannot
disagree with the screen. A call's **arguments are not in it**, only the digest the log itself
keeps, because an approval can cover a command carrying a credential and a file that gets mailed
around is the last place to undo that.

## First run

On first launch DeskSeek shows a short setup screen — reopen it any time with `/setup`. It
checks what is already on the machine and asks about the two optional pieces rather than
quietly leaving them broken:

| Piece | Needed for | Required? |
| --- | --- | --- |
| DeepSeek API key | everything | **Yes** — stored in the OS keychain |
| SearXNG instance | `web_search` | No — the setup screen can build one on Docker, and the other browsing tools work without it |
| Docker or WSL2 | running commands in a container instead of directly | No |
| *The user's screen* toggle | reading and operating the desktop (`ui_snapshot`, `ui_click`, `ui_type`) | No — off by default |

The same screen also chooses where commands run, and shows which rung that setting currently
resolves to.

DeskSeek **never installs Docker or WSL2 for you**. It detects what is present, links to the
official install guides, and uses whatever it finds. A machine without Docker is not broken —
it just runs commands directly, with the guard as the only protection. SearXNG is the one thing
it will stand up on request, and even then it is a container on the engine you already have
rather than software added to the machine: *Set it up for me* — in the setup screen, and in
*Settings → Browsing* beside the URL field — runs `docker run`, writes the config the instance
needs, and points the app at it.

It also distinguishes *installed* from *running*, because those are different questions with
different answers. `docker --version` is a client-only string that succeeds whenever the CLI is
on your PATH; `docker info` needs the engine's named pipe and fails with exit 1 when Docker
Desktop is merely stopped. Asking the second question to answer the first reports a perfectly
good Docker install as "not found" — so both are asked, and a stopped engine reads as
*Docker installed, engine stopped* with a nudge to start Docker Desktop and press Re-check.

The SearXNG check is honest about the common failure: an instance only answers JSON once
`json` is in the `formats` list in its `settings.yml`, so a plain HTML instance is reported as
*not configured* rather than silently returning nothing. The instance DeskSeek builds writes
that file itself, which is most of why the button exists.

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
artifact and document readers; the chart contract in `src/lib/chart.ts`, so a bar's height cannot
silently stop matching its value, a pie's slice cannot stop matching its share, and a label cannot
become markup; and the shortcut map in `src/lib/shortcuts.ts`, so a chord cannot
silently fire for the wrong key or stop firing for the right one. Two more rules
are the checks' own: the palette's scopes and ranking — and, in the same file, which menu the
composer puts up for `/`, `@` and `$` — in `src/lib/commands.ts`,
and the animation inventory in `src/lib/motion.check.ts` — nothing may move
without naming the state it explains, and the reduced-motion rules have to keep
neutralising the movement while holding the streaming caret visible.

The last two are the accessibility pair. `src/lib/tree.check.ts` drives the pure
module the file tree's keys go through — what is on screen, and where each arrow
moves you, including the two keys the tree hands back to the panel so it can open
and close folders. `src/lib/a11y.check.ts` reads the components for the structure
a screen reader depends on: a full-screen overlay has to be a named dialog, the
tree and the transcript have to say what they are, `outline-none` is banned in
favour of `outline-hidden` (the Tailwind v4 spelling that survives Windows High
Contrast Mode), and a button made of one icon has to have a name.

One check spans the two languages: `src/lib/tray.check.ts` reads the tray's menu table out of
`src-tauri/src/tray.rs` and holds the half the window runs to the command palette's own ids, so a
menu item cannot be renamed on one side and quietly do nothing on the other.

One check is not about the code at all. `src/lib/changelog.check.ts` holds
[`CHANGELOG.md`](CHANGELOG.md) to the shape the release workflow reads it by —
newest first, one dated section per version, a section marked unreleased only while
there is unreleased work — and pins the reader that turns a tag into release notes, because a
release that quietly ships with no notes is the failure that replaced retyping them
at tag time.

Before a release, `npm run tauri build` produces the installers (Windows NSIS + MSI, macOS
`.app`/`.dmg`, Linux `.deb`/`.rpm`/AppImage). The behaviour that cannot be tested automatically
is written down as a tick-list in [`docs/E2E-CHECKLIST.md`](docs/E2E-CHECKLIST.md) — run it once
against the built app before publishing.

Every push to `main` runs the gate in CI — `typecheck`, `check`, `build` and `cargo test`, on the
Node this project is developed on. A tag is the publish button: pushing a `v*` tag runs the same
build and drafts the release, taking the release body from that version's section of
[`CHANGELOG.md`](CHANGELOG.md), so the notes are written once, in the file, rather than retyped at
tag time.

**Windows: `cargo test` dies with `0xc0000139`?** That is a loader error, not a test failure: the
test harness links `TaskDialogIndirect`, which only `comctl32` **v6** exports, but a test binary
gets no application manifest, so the loader binds the v5 DLL and kills the process before `main`.
`build.rs` adds the v6 dependency explicitly. Binary targets get it from Tauri's own link args,
which is why only the tests are affected.

## Architecture

A fuller map — the layers, where the events flow, and which file owns each rule — is in
[`ARCHITECTURE.md`](ARCHITECTURE.md). This is the short version:

```
src/                     React UI (App, Sidebar, ChatView, SettingsDialog, Markdown)
src-tauri/src/
  api/                   DeepSeek client, SSE parsing, delta accumulation, model pricing
  conversation/          message-array assembly (owns the reasoning-content rule), agent loop
  store/                 SQLite schema, settings JSON, OS keychain, paths
  tools/                 built-in tools, path sandbox, process runner, permission guard
  mcp/                   MCP client (rmcp) and server config
  runs.rs                in-flight run registry + cancel handles (the durable
                         marker a killed run leaves behind is db::in_flight_runs)
  backup.rs              one zip for the whole profile: what it carries, and how a
                         restore puts it back (writes: db's WAL is flushed first)
  schedule.rs            when a recurring task next runs (pure local-time arithmetic)
  triggers.rs            a task that starts because something changed: what each kind
                         fingerprints, the edge rule that makes a poll not a run, and
                         where a server-initiated notification lands
  notify.rs              one place that decides whether a ping becomes an OS toast
  fs_watch.rs            workspace watcher, refcounted roots, coalesced events
  commands.rs            the Tauri command surface
```

Notable invariants (covered by tests):

- `reasoning_content` must be echoed back on prior assistant turns **whenever the request
  carries `tools`** — otherwise the API returns 400.
- `tool_choice: required`/named is rejected in thinking mode.
- Model prices are half off-peak; peak is 01:00–04:00 and 06:00–10:00 UTC, Mon–Fri.
- A chat has at most one live run; starting another cancels the first.
- `next_run_at` is always **strictly after** now, and the scheduler advances it *before* the run
  starts — those two together are what stop a task firing twice.

## License

MIT — see [LICENSE](LICENSE).
