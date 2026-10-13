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

## 0.5.0 — unreleased

- Skills move in and out: import a folder of `SKILL.md` directories written for Claude or Gemini, or export the whole library in that same open shape
- Another assistant's memory file or prompt folder imports into editable categories and saved prompts — additive, with nothing already written overwritten
- A chat can be set to not feed memory: nothing said in it is remembered, and no memory is shown to it
- A project's memory can be edited from the panel that shows it — forget one line, or add one, without leaving for Settings → Memory
- A scheduled task can run every hour (on a chosen minute) or on weekdays only
- A scheduled task lists its recent runs with what each cost and the chat it ran in, read from the same rows the spend view sums
- Ask in chat for something to happen on a schedule and the assistant sets it up, after an approval, with no form
- A passage of an artifact can be rewritten in place: select it in the editor, say how it should change, and it lands as a new version
- Break reminders on an interval you choose, and quiet hours that hold popups back without hiding the in-app notice

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
