# Manual end-to-end checklist

Run this before cutting a release. Everything here needs a real window, a real
API key and (for the folder items) a real folder, so it is deliberately manual —
the automated checks live in `cargo test` and `npm run check`.

Tick a box only when the result matches. If something fails, note the step number.

## Setup

- [ ] `npm run typecheck` passes
- [ ] `npm run check` passes (shell / tasks / artifact / docs / shortcuts / commands / theme)
- [ ] `cd src-tauri; cargo test` passes
- [ ] `npm run tauri build` produces an installer and the app launches from it

## First run

- [ ] A fresh profile (delete the app data folder) shows the setup screen
- [ ] An invalid API key is rejected with a readable message, not a stack trace
- [ ] Entering a valid key and closing setup lands on an empty chat
- [ ] `/setup` reopens the same screen with the saved values

## Chat

- [ ] A message streams in; the *thinking* pane appears and collapses
- [ ] Markdown, a table and a fenced code block all render
- [ ] Dropping an image attaches it and the reply accounts for it
- [ ] `Stop` mid-stream keeps the partial reply and ends the turn
- [ ] `/compact` summarises and the context meter drops
- [ ] `/summarise` proposes entries and opens *Settings → Memory* with them

## Conversations

- [ ] The arrow on a sidebar row moves a chat into *Archived (n)* at the bottom
- [ ] An archived chat keeps its messages, its spend and its folder; `↑` restores it
- [ ] `Ctrl+K` offers *Archive this chat*, and *Take out of the archive* once it is archived
- [ ] Restoring does not reorder the list: the chat comes back where it was

## Two panes

- [ ] *Split* shows two conversations; the divider drags and resizes
- [ ] Closing the app and reopening restores the tabs, the split and the divider
- [ ] Starting a run in the left pane, then clicking into the right pane, leaves
      the left run streaming with its pulse dot still lit
- [ ] `Stop` in one pane does not touch the other pane's run

## Keyboard

- [ ] `Ctrl+N` starts a chat; `Ctrl+W` closes the focused tab
- [ ] `Ctrl+B` toggles the file tree; `Ctrl+M` opens the memory settings
- [ ] `Ctrl+,` opens settings; `Ctrl+\` toggles the split
- [ ] `Ctrl+1` / `Ctrl+2` move focus between the panes
- [ ] `Escape` closes the open panel or dialog; while typing in the composer it
      only dismisses the slash menu

## Approvals, guard and permissions

- [ ] In *Ask*, a `write_file` and a `run_command` each prompt
- [ ] In *Auto*, an `rm -rf /`-style command is refused and the refusal appears
      in that chat
- [ ] Three refusals in one run drop the chat back to *Ask* with a notice
- [ ] In *Skip*, ordinary commands run unattended; only the catastrophic tier is
      still refused
- [ ] The guard is described as a heuristic in the banner, not as a sandbox

## Where commands run

- [ ] `/setup` shows the resolved rung for *Auto*
- [ ] Pinning *Docker* with the engine stopped runs on this machine and says so
- [ ] A command's output is prefixed with where it ran
- [ ] A Docker run with no network fails a download and succeeds on this machine

## Files

- [ ] The file tree loads a folder lazily and reflects a file the agent creates
- [ ] Clicking a file previews it; a binary file reads as "binary", not garbage
- [ ] A file written while it is open re-reads itself

## Artifacts

- [ ] `artifact_create` with HTML opens the panel and the frame renders
- [ ] The framed page cannot reach the network or the app (inline only)
- [ ] Publishing the same title again adds a version; the picker lists both
- [ ] Editing saves a version attributed to you
- [ ] *Restore* on an older version makes it current without deleting the newer one
- [ ] *Copy* puts the version on screen on the clipboard

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

## Desktop (Windows, off by default)

- [ ] The toggle is off until switched on in `/setup`
- [ ] `ui_snapshot` reads the focused window's controls as text
- [ ] `ui_click` asks, even in *Auto*
- [ ] `Ctrl+Alt+Esc` cancels every run and the banner stays until restarted

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
- [ ] Stopping the app leaves no orphaned child process
