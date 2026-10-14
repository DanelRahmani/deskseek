# Changelog

One line per user-visible change, newest first. The release workflow puts the
section for a tag into the GitHub release body — it reads it out of this file with
`scripts/release-notes.mjs`, so the notes are written here once rather than
retyped at tag time.

The heading is `## <version> — <date>`, and the version is the tag without its
`v`. Add to the top section as you go; at release time the word `unreleased`
becomes the date and nothing else changes. `src/lib/changelog.check.ts` holds the
file to that shape — including that a section is marked `unreleased` only while
there is unreleased work, so a released top section is as valid as an unreleased
one.

## 0.6.0 — unreleased

- A document the app writes looks like one: every Word file and PDF is set in **Times New Roman 11** with bold headings, an exam's questions are **bold at 14** with their options at 11 underneath, and a code block stays monospace. The model's `**bold**` becomes a bold word rather than two asterisks, and headings no longer point at Word styles the file never defined — which is what left a generated paper looking like one flat wall of text
- An **exam preset**: one press gives a paper of 40 multiple-choice questions as a `.docx` or `.pdf` and, as a **separate document**, its answer key — the paper carries no answers anywhere, no asterisk and no hint, because it is the thing you hand out. How many questions, how many open questions (none by default) and which format are all yours to set, and the open questions are answered in the key beside the multiple-choice ones
- A study set is **dealt fresh every time it is taken**: the questions come in a new order and each question's options do too, so "the right answer is the first option" is not a strategy and taking the same quiz twice is worth doing. The answer rides on its option's text, so the marking is untouched by the shuffle
- *Quiz* and *Flashcards* in a notebook's *Make* column make a **set you take** rather than a file you print: they name no format because they write no file, and they appear under *Study* beside the questions. The number the form asks for is enforced, not hoped for — a set that comes back the wrong size is refused with both numbers, so the model fixes its own call instead of the reader counting what arrived
- Every entry in a notebook's *Make* column opens a form first: how many (each entry declares its own count and ceiling), which of the notebook's sources to read, and a line of your own to steer it with — and the sources you untick are out of scope for the question *and* for the tools it may reach for, not just left out of the prompt
- A notebook can be asked for a **quiz or a set of flashcards and get one it can take**: the model writes the items through a *tool call* rather than into its answer, so the window runs them — a quiz is marked a question at a time with the line explaining each answer and a score at the end naming what to look at again; flashcards turn over one by one. Every item names the source it came from, and an item whose source has since been removed says so rather than naming something that is not there

- A source's description can be read in full: *Read* on a card opens the whole thing beside the facts the rail has no room for — where the file came from, the copy the notebook keeps, how much text was read out of it, and when it was described

## 0.5.0 — 2026-10-14

- A notebook can build a **fact table** across its sources and export it as a spreadsheet: one row per fact with the source it came from in the last column, and nothing is written at all unless every row names a source the notebook holds
- A notebook can be asked for the things a source set is actually for: flashcards, a quiz with an answer key, a briefing, a timeline, a study guide and a mind map — each written from its sources, cited like any other answer, and saved as a document in the notebook's folder
- A notebook opens as its own surface — the sources on the left, the questions in the middle, what it produced on the right — and sits in a pane beside a chat, coming back with it on the next launch. Copying one carries its sources and its outputs and deliberately not its conversation
- Asking about something in a chat's folder now answers from the file itself: the passages that match the question are read into the prompt and cited by file and heading, so a document does not have to be attached to every question about it
- A **notebook** holds its own sources instead of a chat holding a folder: add a file or a page, switch any of them on or off, and each is listed with a line saying what it contains. Questions are answered from those sources and cite them, so an answer names the document it came from rather than describing one you cannot find
- Skills move in and out: import a folder of `SKILL.md` directories written for Claude or Gemini, or export the whole library in that same open shape
- Another assistant's memory file or prompt folder imports into editable categories and saved prompts — additive, with nothing already written overwritten
- A chat can be set to not feed memory: nothing said in it is remembered, and no memory is shown to it
- A project's memory can be edited from the panel that shows it — forget one line, or add one, without leaving for Settings → Memory
- A scheduled task can run every hour (on a chosen minute) or on weekdays only
- A scheduled task lists its recent runs with what each cost and the chat it ran in, read from the same rows the spend view sums
- Ask in chat for something to happen on a schedule and the assistant sets it up, after an approval, with no form
- A passage of an artifact can be rewritten in place: select it in the editor, say how it should change, and it lands as a new version
- Break reminders on an interval you choose, and quiet hours that hold popups back without hiding the in-app notice
- The spend panel gained *The shape of it*: turns per day drawn as a chart, the busiest day and the peak hour, from the same rows as the bill
- Clicking a notification opens the conversation it was about
- The composer's `@` mentions a file from the workspace and `$` a skill, both filled into the box to edit rather than sent
- The decision log and the spend export to one file in your documents folder — the same rows the panels show, with a call's arguments left out in favour of their digests
- A chat that has nearly filled the model's context window folds itself into a summary between turns rather than waiting for you to notice, and the note says how many messages went into it
- A permanent "always allow" now names what it covers — the program, folder, app or site — so allowing `cargo` in a folder no longer allows every command in it, and the grants list shows each one's target
- A chat can declare what it may send out — *Read only* (read the web, send nothing) or *No network* — and the policy is a refusal in every mode rather than one more thing to approve
- A command that deletes files is always put to you, in every mode including *Skip*: an `rm` removes a file that `/rewind` cannot put back, because the app never saw what the shell was about to remove
- The spend view gained *By what spent it*: the summariser and the in-place rewrite are priced onto the same rows as a turn, and a model with no published price has its tokens counted and its money marked unknown rather than guessed
- In *Auto* mode a cheap reviewer reads each action before it runs — the tool, its redacted arguments and the last few turns — and can refuse it or hand it to you; anything it cannot read clearly is handed to you rather than waved through
- Ask a question over whatever you are doing: a global chord (`Ctrl+Alt+Space` by default, changeable in Settings) raises a small always-on-top window, and the answer lands in your most recent chat or one of its own — the main window comes back to it
- An **Appshot**: one button in that window photographs the window you were just in and attaches both the picture and its readable text to the question, so an error on screen can be asked about without retyping it
- DeskSeek lives in the **system tray** — *New chat*, *Quick entry*, *Show DeskSeek* and *Quit*, with a left click on the icon bringing the window back. Closing the window puts it away instead of ending the session, as long as there is a tray to come back from
- The browser gained the three tools that explain a broken page: `browser_console` reads what it logged, `browser_network` lists every request with its status or why it failed, and `browser_open` with `new_tab` opens a second tab alongside the one you are on — with `browser_tabs` and `browser_tab` to see the list, switch between them or close one
- A scheduled task can wait for something to change instead of a clock: point one at a folder (optionally only files matching `*.csv`) or at a git repository and a ref, and it starts a run when what it sees differs from the last look — never every time it looks, at most six runs an hour, and the panel says in words why it did not fire
- A task can also watch a **page**: give it a URL and it starts a run when the page comes to say a word you name, or — with no word given — when the page changes at all
- A task can be woken by an **MCP server** instead of by anything DeskSeek checks: pick a server and optionally the notification it should react to, and the server waking it starts a run — the third kind of trigger, and the only one that does not look at anything on its own
- The app is no longer wired to one vendor: an **endpoint** is a base URL, a dialect, a key and the models it serves, so OpenRouter, Groq, Together, xAI, Google and local runtimes (Ollama, LM Studio, vLLM) are configuration in **Settings → Access** rather than code — with one keychain account per endpoint, no key at all for a model on this machine, and a rate nobody published reported as *unknown* instead of guessed at
- A scheduled task can run on a model of its own — a cheap one for a nightly triage — instead of the chat's; the run's spend row names the model that really ran
- A model with no thinking to budget can be marked as such, and the chat then shows no thinking controls at all rather than a switch that quietly does nothing
- A screenshot now goes to the model as well as to you, when the model you are running on can see: `ui_screenshot` photographs the window in front — for the canvases and custom-drawn apps whose controls report nothing — and `browser_screenshot` sends the page it captured, both scaled to 1280px on the longest side. A text-only endpoint is sent the words instead, and a desktop screenshot keeps its full-size file
- A window with no controls to press can be driven by **clicking a pixel in that picture** — `ui_click_at` aims at the screenshot you were last shown, so the coordinates mean what you can see rather than a place on the screen — and `ui_windows` lists what is open, and which window has focus, so the agent names a window instead of guessing at one
- When a cheaper model of the same endpoint would have done the same work for noticeably less, the chat says what the turn would have cost there and offers the switch — a number and a suggestion, never a downgrade you did not ask for. The task form's model list shows each model's published rate
- `/audio` turns a conversation into an **audio overview**: a spoken script read out loud by a voice endpoint, saved as `audio-overview.mp3` in the chat's own folder and played in place. It is quoted before it runs — the script, the voice, about how long it is and what it costs, with the script editable and the price following it — and it refuses rather than guessing when no speech endpoint is set in **Settings → Access → Voice**
- **Dictation**: the 🎙 button in the composer records and lands the words in the box to edit — never sent, capped at five minutes with the recorder stopping itself, and the same **Settings → Access → Voice** endpoint answers it

## 0.4.0 — 2026-10-12

- Web search sets itself up: one button in the setup screen builds a local SearXNG container, writes the config it needs, and points the app at it
- Every empty panel now names the action that fills it, instead of only saying there is nothing there
- The focus ring survives Windows High Contrast Mode — `outline-hidden` in place of `outline-none`, which removed it
- The four overlays are named dialogs that put focus back on the control that opened them, and Escape closes them
- The file tree answers the arrow keys: Up/Down walk the rows, Right opens a folder then steps into it, Left closes it or steps out to the parent, Home/End jump
- `F2` renames the open chat in place; `Ctrl+G` scrolls the focused pane to its newest message
- The command palette's `>` and `@` scope the list to actions or to conversations
- Panels animate in as they open, and the reduced-motion setting switches that off
- The sidebar's message search pages its results, and emphasises the words that matched
- The file tree searches the whole folder by name, folders deep, without opening anything
- A long conversation draws only its newest turns and reveals the rest a screenful at a time; streamed text is committed once a frame
- A dropped connection keeps the part of the answer that had already arrived, and says so once
- Every approval decision — allowed, denied or blocked — is listed in *Settings → Tools → Recent decisions*
- The database prunes the file pre-images and artifact versions nothing can reach any more, and *Maintain the database* reports what it reclaimed
- The whole profile backs up and restores as one zip; the API key stays out of it
- A turn the last session left in flight is reported on the next launch, offering *Ask again* or *Put its files back*
- The composer keeps its draft when a render throws
- *Where the tokens went* draws a pie per section, from the same numbers as the totals above it
- A scheduled run that needs an answer can be answered from the Scheduled panel, not only from its chat
- Projects gained their own rules, a memory view and a spend total
- A chart can be drawn from the numbers, and LaTeX renders in answers
- A conversation can be duplicated, and carried to another machine as JSON
- *Retry* asks again without losing the first answer, and ↺ toggles between the two answers
- Saved prompts (`/p:` in the composer), and one store behind every folder of markdown files
- History search the model can use, inside its own folder only
- A spend view, and a record of what each turn actually cost
- Attachments always land: every file is copied into the chat's own folder, and a picture is downscaled before the model sees it
- Design systems are their own store, and the assistant can build and apply them
- Word, Excel, PowerPoint and PDF files are read for their text, and every chat gets its own folder

## 0.3.0 — 2026-10-12

- `ask_user`: the model can ask a question mid-run, and the run waits for the answer in the composer
- Customise: one hub for skills, connectors, instructions, design systems and prompts
- Projects: a folder of chats that share a workspace folder and can be given rules
- A scheduler: recurring tasks with a panel, *Run now*, *Pause*, and a *Needs you* list
- A notification when a run finishes while the window is not in front of you
- Design systems, and skills the model can reach for by itself
- A skill can be marked *always on for new chats*
- The artifacts gallery, and right-click menus in the sidebar

## 0.2.0 — 2026-10-11

- Four usage modes, and a plan mode that can look things up
- Memory: one markdown file per category instead of a fact table, with *Settings → Memory* to edit it
- `/summarise` proposes memory entries and waits for you to accept them
- Archiving a conversation, with `↑` to bring it back
- Skills you build by talking, paste in, or install from a file
- Nicer chrome, and a prompt that names what actually exists

## 0.1.0 — 2026-10-11

First release.

- Streaming replies with a collapsible thinking pane, Markdown, highlighted code and copy buttons
- The API key kept in the OS keychain, never in a config file
- Conversations, memory, skills, scoped instructions and multimodal attachments
- `MEMORY.md` as a profile injected into every request, plus `CHATLOG/` summaries
- A coding agent that works inside a folder you trust, with a live file tree
- Tabs and a draggable two-pane split
- Permission modes, cancellation, and a permission guard over the tools
- Built-in tools: files, `run_command`, and browsing (`fetch_url`, `browser_read`, `web_search`)
- Read the desktop as text, and operate it with rails
- MCP client support, including streamable HTTP
- Artifacts: publish a document and keep every version
- Documents: `.md`, `.docx`, `.xlsx`, `.pdf`, `.pptx`
- A sandbox ladder over Docker, WSL2 and this machine
- A task list, with failures that say what went wrong
- A first-run setup screen that asks about SearXNG and Docker, and tells *installed* apart from *running*
- Three themes built from tokens, with a contrast gate over them
- Settings: a rail, search, appearance, tools and commands
- The command palette, sidebar search, Markdown export, and per-chat cost
- Keyboard shortcuts, and `?` to list them
- Undo a run (`/rewind`), and a spend limit that stops one
- Regenerate an answer, or edit a question and ask again
- Export a conversation as PDF or Markdown, and back up settings and the MCP server list
- The "always allow" grants, listed and revocable
- Chat mode as a plain conversation, saying when it is nearly full
- Attachments sent as whatever the model can read, stored as what you chose
- The syntax highlighter fetched only when there is code to highlight
