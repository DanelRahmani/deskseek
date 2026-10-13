# Reverse-engineering Claude, ChatGPT and Gemini for DeskSeek

A feature teardown of the three reference desktop assistants (Anthropic Claude, OpenAI ChatGPT,
Google Gemini) mapped onto DeskSeek's own code, grouped so that a later agent can plan and build
each group without re-doing the analysis.

Written 2026-10-12. Research is from vendor documentation, vendor release notes and independent
trackers, listed in [Sources](#sources). The three products move weekly; treat every vendor claim
here as "as documented at the research date" and re-check before building.

Companion to [README.md](README.md) (what DeskSeek does), [ARCHITECTURE.md](ARCHITECTURE.md) (how
it is shaped) and [plan.md](plan.md) (the phased roadmap, currently through Phase 7).

---

## How to read this

1. [Method and sources](#method-and-sources)
2. [DeskSeek baseline](#deskseek-baseline) - what already exists, so nothing gets rebuilt
3. [Reference teardowns](#reference-teardowns) - Claude, ChatGPT, Gemini, feature by feature
4. [The eleven convergences](#the-eleven-convergences) - what all three independently decided
5. [Feature groups by DeskSeek code area](#feature-groups-by-deskseek-code-area) - **the part an agent builds from**
6. [Viability tiers and effort](#viability-tiers-and-effort)
7. [Proposed phase plan: DeskSeek 8 to 13](#proposed-phase-plan-deskseek-8-to-13)
8. [Deliberately not doing](#deliberately-not-doing)
9. [Appendix A: parity matrix](#appendix-a-parity-matrix)
10. [Appendix B: the standards worth adopting](#appendix-b-the-standards-worth-adopting)

An agent picking up a group should read [Decisions taken](#decisions-taken) first: it records what
the owner chose on 2026-10-13, and those choices are binding on the plan below.

Each feature in section 5 carries the house shape from `plan.md`: **Why**, **Files**, **Test**,
**Stop**, plus **Viability** (V0 to V3) and **Size** (S/M/L). An agent picking one up should not
need anything in section 3.

---

## Decisions taken

Recorded 2026-10-13, from the owner, at handover. Where a decision contradicts an earlier section,
the decision wins and that earlier section has been revised to match.

1. **Build Phase 8 first.** Interop and daily-habit wins, in this order: 6.1 skill interchange,
   14.2 the skill and memory importer, 5.1 incognito chats, 4.3 memory ergonomics, 7.1 more cadences,
   7.4 scheduled-run history, 7.5 the schedule tool, 11.1 break reminders and quiet hours, 11.2 the
   recap, 11.3 linking notifications, 10.5 composer mentions, 12.4 audit export, and **8.1 edit an
   artifact in place**, pulled forward from Group 8 because it was named as a daily-use feature and
   it is self-contained.

2. **The provider layer is two dialects, not three vendors (2.1, revised below).** Do not build
   vendor-specific clients. Build an **OpenAI-compatible** adapter and an **Anthropic-compatible**
   adapter, each configured by base URL, API key and a model list. DeepSeek is already
   OpenAI-compatible, so the current client becomes the first endpoint rather than being discarded.
   This means **OpenRouter, Google's OpenAI-compatible endpoint, Ollama, LM Studio and vLLM all
   arrive as configuration**, not as separate projects, and the "should we support local models"
   question disappears: a local model is a base URL pointing at localhost.

3. **The cloud boundary is parked, deliberately.** No decision now. The binding constraint that
   follows is negative: build nothing in Phase 8 that requires a server, and nothing that would
   foreclose one later. Concretely, keep session, task and queue state in the database rather than
   only in memory, and keep identifiers stable, so a later local-network bridge would be a new
   surface rather than a migration.

4. **Priority inside the later phases**, because these were named as features he would use daily:
   8.1 artifact edit in place (Phase 8), 10.1 the quick-entry window (Phase 10), 9.2 browser
   console and network (Phase 10), 7.2 event triggers (Phase 11: keep the phase, but do not defer
   the item within it).

Viability vocabulary:

| Tier | Meaning |
| --- | --- |
| **V0** | Interop or polish. Days. Touches one or two files. |
| **V1** | Self-contained. One subsystem, no new external dependency. |
| **V2** | New subsystem, or a cross-cutting change touching several layers. |
| **V3** | Needs a server, a cloud account, or multiple users. Out of scope for a local single-user app. |

---

## Method and sources

What was read to produce the teardowns:

- Anthropic: Claude Help Center collections for Claude Cowork, Claude Desktop, Claude Code and
  Claude in Chrome, plus the Claude release notes page and the Claude Academy desktop guide.
- OpenAI: the ChatGPT and Codex documentation set (What's new, Product/Workflows, Capabilities,
  Codex, Plugins, Dots) and the ChatGPT desktop release notes.
- Google: the Gemini Apps help centre, the Gemini apps release notes, the Workspace Updates blog
  (skills and the Gems transition; Gemini Notebook), the Gemini Spark help material, Google's own
  Spark overview page, and an independent, source-cited NotebookLM release tracker.

What was read to establish the DeskSeek baseline: `README.md`, `ARCHITECTURE.md`, `plan.md`,
`package.json`, and a directory walk of `src/` and `src-tauri/src/`.

Two cautions stated up front:

- **Rollouts are staged.** Every vendor labels features "rolling out", and plan, region and admin
  settings decide what a given account sees. Nothing here should be read as "everyone has this".
- **Vendor marketing and vendor documentation disagree.** Where they did, the help centre and the
  release notes won.

---

## DeskSeek baseline

DeskSeek is a Tauri v2 (Rust core) + React/TypeScript desktop client for the DeepSeek API, with two
sections (Chat and Coding) sharing one core. What it already has, by area, because the fastest way
to waste a week is to build something that exists:

| Area | Already in DeskSeek |
| --- | --- |
| Agent loop | Streaming turns, a collapsible thinking pane, tool rounds, cancel/stop, one live run per chat (`conversation/agent.rs`, `runs.rs`) |
| Modes | Ask / Standard / Plan / Agent, plus Ask-first / Auto / Skip approval modes, cycled with `Shift+Tab` (`tools/mod.rs`, `tools/guard.rs`) |
| Containment | Path sandbox (`tools/sandbox.rs`), catastrophic-command guard (`tools/guard.rs`), Docker / WSL2 / this-machine execution with a Windows job object (`tools/sandbox_exec.rs`, `tools/job.rs`) |
| Tools | `read_file`, `list_dir`, `glob`, `grep`, `search_history`, `write_file`, `edit_file`, `run_command`, `git_*` |
| Internet | `fetch_url`, `browser_read`, `web_search` (SearXNG, self-provisioned on Docker), `browser_open` / `browser_extract` / `browser_eval` / `browser_click` / `browser_type` / `browser_screenshot` (`tools/browse.rs`, `tools/browser.rs`, `searxng.rs`) |
| Desktop control | `ui_snapshot`, `ui_click`, `ui_type`, Windows only, off by default, 25-action cap, 250 ms spacing, `Ctrl+Alt+Esc` kill switch (`tools/desktop.rs`) |
| Memory | `MEMORY.md` profile plus `memory/<category>.md`, keyword lookup, `save_memory`, `/summarise`, a settings viewer (`memory.rs`, `memory_files.rs`) |
| Customise stores | Skills (`skills/<id>/SKILL.md`), design systems (`design/<name>.md`), saved prompts (`prompts/<name>.md`), all over one shared store with an id guard (`store/files.rs`, `skills.rs`, `design.rs`, `prompts.rs`) |
| Instructions | Global scoped instruction blocks (chat / coding / both) and per-project rules fed through one filter (`agent.rs`) |
| Projects | Name, folder, membership, rules, shared project memory, all-time spend (`ProjectsPanel.tsx`, `db::project_spend`) |
| Scheduling | Recurring tasks (daily / weekly / first weekday of month), Run now, Pause, a needs-you park queue, notification on park, missed-window fire-once (`schedule.rs`, `commands.rs`, `ScheduledPanel.tsx`, `db::parked_requests`) |
| Notifications | In-app dot plus an OS toast only when the window is not focused (`notify.rs`) |
| Artifacts | `html` / `svg` / `mermaid` / `chart` / `markdown` / `code`, versioned, with a gallery and a sandboxed frame (`artifact_create`, `src/lib/artifact.ts`, `chart.ts`) |
| Documents | `doc_create` (md/docx/pdf), `sheet_create` (xlsx), `slides_create` (pptx), read back by `read_file` (`tools/docs.rs`) |
| Spend | Per-chat cost, a turn-level `turn_usage` table, a spend panel with pie/bar by day, model and chat, and a per-run spend limit (`db::spend_report`, `SpendPanel.tsx`) |
| Conversation memory of itself | `search_history` scoped to the chat's folder or section, plus sidebar FTS with BM25, paging and snippet highlighting |
| MCP | Client over stdio and streamable HTTP, tool renaming, per-call approval, read-only trust opt-in, logs (`mcp/manager.rs`) |
| Portability | Zip backup/restore of the whole profile, markdown/PDF/JSON export, duplicate, DB pruning |
| Shell | Tabs, split panes, command palette, keyboard map, sidebar search, archive, file tree with real name search, error boundary, network resilience, a11y and motion checks |

Modules an agent will meet constantly: `src-tauri/src/{commands.rs, lib.rs, runs.rs, approvals.rs,
questions.rs, schedule.rs, notify.rs, fs_watch.rs, backup.rs, searxng.rs}`, `src-tauri/src/store/`
(`paths.rs`, `db.rs`, `settings.rs`, `keychain.rs`, `files.rs`), `src/lib/*` with its `*.check.ts`
neighbours, and `src/components/*`.

**The one structural fact that decides everything below:** DeskSeek is single-provider (DeepSeek
only), single-user, and entirely local. Roughly half the vendor feature list assumes a cloud
account with a server behind it; the other half is local and directly reachable. Section 6 sorts
them.

---

## Reference teardowns

### Claude (Anthropic)

Surfaces: Claude Desktop (macOS, Windows, Linux beta), Claude on the web and mobile, Claude in
Chrome, and Claude Code (CLI and desktop). As of the research date, chat and Cowork share one home,
so "ask a question" and "hand over a task" are the same box.

| Feature | What it actually does | Code-relevant shape |
| --- | --- | --- |
| **Quick entry** | A small always-on-top Claude window over any app. Mac: double-tap Option. Windows/Linux: a shortcut set in Settings. Menu bar / system tray entry. | A second Tauri window plus a global shortcut |
| **Dictation** | Mac: hold Caps Lock, talk. Dictation is not on Linux. | Speech-to-text into the composer |
| **Screenshots from quick entry** | Mac only: hover a window to send its screenshot, or drag to capture a region. | Region capture to a multimodal attachment |
| **Folders** | Add a folder, allow editing, and Claude reads, edits and saves finished work back into it. | DeskSeek has the equivalent (workspace sandbox plus coding mode) |
| **Built-in browser** | A browser panel inside the desktop app, its own tabs and history, separate from your Chrome. Per-site allow once / always / deny. | DeskSeek has `tools/browser.rs`, but no per-site memory and no tab model |
| **Claude in Chrome** | A side panel that reads and acts on your own Chrome. Multi-tab via a tab group, console logs and network and DOM state, learns popular sites (Slack, Gmail, Calendar, Docs, GitHub), **Record a workflow** to teach a repeatable browser task, Scheduled tasks, Follow a plan, model selection, notifications when it needs you, screenshot/highlight to point at something. | A browser extension is a genuinely separate product surface |
| **Computer use** (beta, Mac and Windows, Pro/Max) | Claude points, clicks and types in other applications. It prefers a connector, then the browser, and turns to computer use last, because it is the slowest. Asks before using an app; you can stop it any time. | DeskSeek has `ui_snapshot` / `ui_click` / `ui_type`; the missing piece is vision |
| **Connectors and Extensions** | MCP connectors everywhere, plus desktop-only Extensions (Apple Notes, Messages). Controlled from a Customize hub holding skills, plugins and connectors. | DeskSeek has MCP and a Customise hub; it has no first-party connectors |
| **Plugins** | A package bundling **skills, connectors and sub-agents** into one installable unit, saved to the account and usable in chat, Cowork and Claude Code alike. A directory, a developer portal, per-org admin controls, and Enterprise skill and plugin security scanning. | A bundle format over the existing stores |
| **Skills** | `SKILL.md`, declared as the open **Agent Skills** standard so skills move between platforms. Org-wide management, a partner directory, and **Record a skill** (do the task once, narrating, and Claude writes it up). | DeskSeek already uses `skills/<id>/SKILL.md` |
| **Cowork** | Claude Code's agentic architecture for non-coding work: multi-step tasks, sub-agent coordination, **parallel workstreams**, long-running work without conversation timeouts, professional outputs (XLSX with working formulas, PPTX, formatted docs), and **edit drafts in place** (highlight text, click "Edit with Claude", type the change). Deletion protection: explicit permission before permanently deleting files. | Sub-agents and in-place artifact editing are the two real gaps |
| **Modes** | Manual (ask before acting), Auto (Claude reviews each action for safety, blocking exfiltration or prompt injection, and falls back to asking if blocks pile up), Skip (nothing checks). Auto consumes more usage because of the extra review. Optional network egress permissions per session. | DeskSeek's Auto has a string-based guard, not a reviewing pass |
| **Scheduled tasks** | Recurring and on-demand, created by `/schedule`, from a Scheduled page, or conversationally (Claude proposes name, cadence and instructions, you confirm). Fields: name, prompt, **approval mode**, frequency (hourly / daily / weekly / weekdays / manual), optional model, optional folder. Pause, resume, delete, run on demand, review upcoming and past runs. | DeskSeek has most of this; the gaps are hourly, weekdays, per-task model and per-task folder |
| **Cloud sessions** | Since 2026-10-06, new Cowork tasks on Pro/Max run on Anthropic's servers: they survive a closed laptop, scheduled tasks run with no device online, and one session is reachable from desktop, web and mobile. Local files, local connectors, the browser and computer use still reach the machine **through the desktop app**, and only while it is open. | V3 without a server |
| **Dispatch / assign tasks from anywhere** | A persistent agent thread you steer from desktop or phone while the desktop does the work. Needs the computer awake and the app open. | V3 locally; a same-LAN PWA is the honest local analogue |
| **Projects** | Persistent workspaces with their own files, links, instructions and memory, on every surface. Folder-backed projects support Cowork on desktop only. | DeskSeek's projects are already close |
| **Memory** | Individual categorised entries, editable and deletable under **Topics** in Settings, a **sensitive topics** switch, shared between chat and Cowork, incognito chats that stay out of memory, and import/export. | DeskSeek has categories and a viewer; no incognito and no topic review |
| **Chat search** | Prompt Claude to search past conversations. | DeskSeek has `search_history` |
| **Compaction** | Summarise earlier messages as a chat approaches the limit, for effectively unbounded conversations. | DeskSeek has `/summarise` and a manual "Summarise it" prompt at 80% |
| **Artifacts** | Claude Design, Claude Slides and Claude Docs as artifact types, live artifacts that update while Claude works, versions, embed codes, on-canvas editing, design-system import, and editing a draft in place. | DeskSeek has versioned artifacts and a design-system store; it has no live artifact and no selection-level edit |
| **Reflect and Time and focus** | A monthly recap (topics, most active day, peak hour) and optional **break reminders** and **quiet hours**. Requires memory. | Cheap, local, and a direct answer to "create reminders" |
| **Claude for Excel / PowerPoint** | Add-ins that share one conversation context across both apps, support skills, and can reach a model through an LLM gateway (Bedrock, Vertex, Foundry). | Out of scope for a desktop client |
| **Claude Tag in Slack** | Tag Claude in Slack to delegate a task. | V3 |
| **Claude Code desktop** | Scheduled tasks, parallel agents, Git worktrees, drag-and-drop panes, an in-app editor, remote control with Trusted Devices, OpenTelemetry. | DeskSeek's Coding section is the cousin; worktrees and parallel agents are the interesting parts |

### ChatGPT (OpenAI)

Surfaces: one desktop app for macOS, Windows and Linux (preview), which has merged **Chat / Work /
Codex** behind a mode switcher, plus web, mobile, the CLI and the IDE extension.

| Feature | What it actually does | Code-relevant shape |
| --- | --- | --- |
| **Pets (companion window)** | Floating controls over other apps, with an optional pet or a Mini mode. Type or start a voice conversation from them, keep working while you follow progress, then open the full conversation. `Option+Space` on macOS, `Windows+Alt+P` on Windows. `@` adds context, `$` picks a skill. | A second window plus a global shortcut |
| **Appshots** | On Windows, press **both Alt keys** to bring a screenshot and the available text of the frontmost window into ChatGPT, then ask about it. On macOS it can start a new chat in the Pets controls. | Screen capture plus window text, into a turn |
| **Computer History** | Turns activity across apps and websites into a **searchable timeline** and memories that ChatGPT and Codex can use. You choose which apps and websites contribute, pause collection, review and delete. Off by default; Pro/Business/Enterprise; EEA and UK first excluded, later enabled. | A persistent activity log with a privacy surface |
| **ChatGPT Work** | A mode that uses your apps, files, tools, browser and approved business context to produce docs, decks, spreadsheets, sites, reports and analyses. Has a Cloud browser, optional local computer access, its own usage and credits shared with Codex, and syncs a task across desktop, web and mobile. | A "work" task surface, cloud-dependent |
| **Browser** | Built-in browser, or drive your **own** Chrome, Edge, Brave, Opera or Vivaldi. Bring an open tab into a chat, sign in to a site through the cloud browser without connecting your local profile, and **Site tools (WebMCP)** where a website exposes its own actions. | DeskSeek drives its own headless Edge/Chrome; the extension route is the gap |
| **Computer use** | The model drives a computer, with permissions, sandboxing and a review path for approvals. | DeskSeek has text-only desktop control |
| **Plugins and Plugin extensions** | Plugins bundle a skill, an MCP server and optional UI. **Extensions** add sidebar apps, panels, editors and forms to ChatGPT, with composer mentions on desktop. Plus plugin controls, management and migration from custom GPTs. | A UI extension surface, well beyond MCP tools |
| **MCP** | MCP servers plus **MCP events** (webhooks, requiring MCP 2.0 so a user can ask ChatGPT to act on server-side updates), a Secure MCP Tunnel, and Vaults for credentials. Skills, tool search, programmatic and async tool calling. | DeskSeek is an MCP client; events are the interesting addition |
| **Sites** | Publish workspace-private sites with a live database, editable URLs, co-editing and connected data per visitor. | A publishing pipeline, cloud |
| **Space** | Files and **Pages** together, with collaborators you can mention an agent into while editing. | Collaboration surface, V3 locally |
| **Library** | Saved files reusable across conversations, with search inside Library, and pastes over 10,000 characters turned into attachments. | DeskSeek files attachments per chat; there is no vault |
| **Projects and chats** | Projects and chats, pinned threads synced between desktop and iOS. | DeskSeek has projects |
| **Memory** | Memories plus Computer History plus "Personalize ChatGPT" custom instructions. | DeskSeek has profile plus categories; no activity-derived memory |
| **Scheduled tasks** | Cadence-based tasks **and event triggers**: a task can start when a supported event occurs in Gmail, Slack or GitHub, so triage does not poll a fixed cadence. Team Tasks run scheduled or event-triggered work through a service account. | Event triggers are the real gap in DeskSeek's scheduler |
| **Long-running work, notifications and sync** | Long-running tasks, notifications when they need you, and starting a Work task on one device and continuing on another. | Notification exists; sync is V3 |
| **Codex** | Cloud environments you prepare once and reuse, Git worktrees, code review, an IDE extension, CLI, app server, SDK, GitHub / GitLab / Slack / Linear integrations, sandboxing, agent approvals and security, `AGENTS.md`, subagents, hooks, Record and Replay, and site tools. | The deep end of the coding-agent reference |
| **Voice** | Voice conversations that now support uploaded files and Projects, so you can ask about a document out loud. | Needs speech-to-text and text-to-speech |
| **Import from other agents** | Import instructions, settings, skills, plugins, projects and recent work from **Claude Code, Claude Cowork or Cursor**, with optional automatic sync. | A one-off importer, cheap because skills share a format |
| **Dots** | Give a named agent ongoing responsibility; it keeps making progress between conversations and brings results back for review, and you can connect plugins from the phone. | A proactive background agent, cloud |
| **Models and modes** | GPT-6.1 Sol, GPT-6 Astra, Ultrafast token generation on higher plans, a reasoning-effort slider, and model pickers that differ between Chat and Work or Codex. | Model routing and effort control |

### Google Gemini (and Gemini Notebook and Spark)

Surfaces: the Gemini app on web and mobile, a **native desktop app** for Windows and macOS, Gemini
in Chrome, Google Workspace apps, **Gemini Notebook** (NotebookLM renamed on 2026-07-16, now at
`notebook.google.com`), **Gemini Spark**, and the developer-facing Antigravity and Jules.

| Feature | What it actually does | Code-relevant shape |
| --- | --- | --- |
| **Desktop app and `Alt+Space` overlay** | A compact Gemini panel over any Windows app: text, voice or image in, and it remembers the conversation so follow-ups work. Built-in Gemini Live. Screen sharing is opt-in per session. | A second window plus a global shortcut, and voice |
| **Workspace actions from the overlay** | Find a Drive document, summarise a Gmail thread, create a Calendar event from a message, save a note to Keep, without opening a browser. Runs because the app is signed into the Google account at system level. | First-party connectors, which DeskSeek has none of |
| **Gemini Live** | Conversational mode built into the conversation: switch between talking and typing, show Gemini what you see, and transform the scene with Nano Banana. | Realtime audio and vision; V2 at best |
| **Personalisation / Personal context** | The assistant draws on your own Google context. | A profile-plus-RAG problem |
| **Skills** (rolled out October 2026) | Reusable custom instructions that work **inline and stackable** in one prompt, built entirely on the open, **Markdown-native `SKILL.md` standard**, so a skill copied from another platform works. A skill builder in Google Docs. Skills do not sync between the Gemini app and Workspace apps. **Gems are being retired** and auto-migrated to draft skills. | DeskSeek already speaks this format |
| **Scheduled actions** | Schedule an action from a conversation; listed under Settings and help, then Scheduled actions. If the action uses another app such as Workspace, that app must be connected. Holds its own sidebar entry. | The closest analogue to DeskSeek's Scheduled panel |
| **Gemini Spark** | A cloud agent that splits work into three explicit parts: **a Task is what, a Skill is how, a Schedule is when.** Tasks live in a Tasks area, Spark plans steps before acting, works across apps, and can stop to ask you a question. Skills hold purpose, context, process, guardrails and output, are stackable, and can be auto-applied when relevant. Three schedule types: **time-based** (once, or hourly / daily / weekly / monthly / yearly), **Gmail-triggered** (an incoming message matching a filter), and **monitor** (a condition checked periodically). Runs in the cloud even with the device off, and **pauses when it needs a decision, a permission or a sign-in**. Timing is approximate. Needs Pro or Ultra, 18+, in a supported country. | The clearest existing model for the scheduler and trigger work |
| **Gemini Notebook** (formerly NotebookLM) | A persistent, **source-grounded** research workspace. In the standalone app, answers are grounded exclusively in the selected sources, with citations back to them; inside the Gemini app the same notebook may also use web search. Sources: PDFs, Docs, Slides, Sheets, Word, EPUB, URLs, YouTube, audio, images, pasted text. Drive sources auto-sync. Source discovery and Deep Research recommend sources to import. A secure cloud computer runs **native code execution** for charts, spreadsheets and slide decks. Copying a whole notebook transfers sources and Studio artifacts but not chat history or notes. | The single biggest missing surface in DeskSeek |
| **Notebook Studio outputs** | Summary, FAQ, briefing, study guide, timeline, report, flashcards, quizzes (short answer, multiple select, fill in the blank), mind map, infographic, slide deck, data table, Audio Overview with an **Interactive** mode (pause the audio and ask a question; it answers from your sources and resumes), Video Overview (visual styles and a Brief format), Cinematic Video Overview, and **Short Video Overview** (about 60 seconds, 80+ languages). Plus notes, chat history, sharing and limits (roughly 100 notebooks and 50 sources per notebook on the standard tier). | A generator menu over one source set |
| **Study and Classroom** | Study Notebooks in Gemini sync with NotebookLM: adaptive lessons, diagnostic quizzes, progress tracking. Guided Learning, Classroom integration, Schoology via Gemini LTI. | Education-shaped outputs |
| **Workspace Studio** | No-code flows with an **Ask NotebookLM** step and a step that **automatically adds sources** to a notebook, so recurring notebooks stay fresh. | An automation graph over the same primitives as Spark |
| **Gemini in Chrome** | The assistant inside the browser. | Same extension surface as Claude in Chrome |
| **Antigravity, Jules, Mariner, AI Mode in Search** | An agentic development platform, an async coding agent, a browser agent, and notebooks arriving inside Search's AI Mode. | Adjacent products, listed for completeness |
| **Gemini Enterprise and the Gemini agent for work** | Connectors to Microsoft 365, Google Workspace, HubSpot, Jira; a no-code Workflow Builder; bring-your-own agents via ADK; an agent library. | Enterprise integration layer, V3 |

### The eleven convergences

The useful finding is not the feature list, it is what three competitors independently chose. These
are the patterns worth treating as direction rather than fashion:

1. **Chat and task in one box.** Claude merged Chat and Cowork; ChatGPT merged Chat, Work and
   Codex; Gemini answers a question or starts a Task from the same thread. A separate "agent mode"
   is on its way out, replaced by the model deciding.
2. **Markdown skills are the interoperability layer.** Claude publishes Agent Skills as an open
   standard; Gemini's skills are "built entirely on the open, Markdown-native `SKILL.md` standard"
   and explicitly copyable across platforms; ChatGPT documents skills the same way. DeskSeek
   already stores `skills/<id>/SKILL.md`, so it is a shipping-format decision away from
   interchange.
3. **Scheduling is three ideas, not one.** Time-based cadence, event trigger, and monitor. Gemini
   states it cleanly (Task / Skill / Schedule), ChatGPT ships event triggers, Claude ships
   cadence plus on-demand, and DeskSeek ships cadence only.
4. **A background run must be able to park.** Every vendor says a run that needs you stops and
   waits rather than guessing. Gemini: "a Task will pause if Spark needs a decision, a permission,
   a sign-in". DeskSeek already has a park queue and a needs-you list, which is unusual for its
   size and worth protecting.
5. **Approval is a ladder with an auto rung that still reviews.** Manual, Auto, Skip; Claude's Auto
   runs a safety review over each action for exfiltration and prompt injection before doing it.
   DeskSeek's Auto is a string classifier with a catastrophic tier. The upgrade is a reviewing
   pass, not a longer list.
6. **The agent reaches local things through the desktop app.** Cloud sessions exist everywhere, but
   local files, the browser and computer use still route through the client. That is exactly
   DeskSeek's native advantage and the reason it should not try to be a cloud product.
7. **Computer use is the last resort, by explicit policy.** Claude tries a connector, then the
   browser, then the screen. That ordering is a design decision worth copying, because it is also
   the cheapest-first ordering.
8. **Memory is editable, categorised, and switchable.** Topic lists, per-item delete, a sensitive
   topics switch, incognito chats. DeskSeek's category files are already the right storage; the
   gaps are review ergonomics and an opt-out.
9. **Sources with citations is a distinct product mode.** Notebook and Deep Research both mean
   "answer only from these, and show me where". A chat assistant that can be told "from my folder,
   with citations" is a different tool from a chatbot.
10. **One source set, many outputs.** Reports, decks, flashcards, quizzes, mind maps, infographics,
    audio, video, all derived from the same grounded sources. DeskSeek has artifacts and documents
    but no generator menu over a named source set.
11. **A second, smaller window is the adoption trick.** Quick entry, Pets, the `Alt+Space` overlay:
    all three ship an always-available, always-on-top mini surface on a global shortcut. It costs
    little and it changes daily usage more than any agent capability.

---

## Feature groups by DeskSeek code area

Fourteen groups. Each group is a set of features that share owner files, so one agent can hold the
whole group's context without paging in the rest of the app. Within a group, features are ordered
by value per unit of effort.

| # | Group | Primary owner files |
| --- | --- | --- |
| 1 | Agent loop and orchestration | `conversation/agent.rs`, `conversation/assemble.rs`, `runs.rs` |
| 2 | Provider and model layer | `api/client.rs`, `api/models.rs`, `api/stream.rs`, `store/keychain.rs`, `store/settings.rs` |
| 3 | Tools and containment | `tools/mod.rs`, `tools/guard.rs`, `tools/sandbox.rs`, `tools/sandbox_exec.rs` |
| 4 | Persistence and schema | `store/db.rs`, `store/paths.rs` |
| 5 | Memory and context assembly | `memory.rs`, `memory_files.rs`, `conversation/assemble.rs`, `conversation/agent.rs` |
| 6 | Skills, prompts, plugins | `store/files.rs`, `skills.rs`, `prompts.rs`, `design.rs` |
| 7 | Scheduling and triggers | `schedule.rs`, `commands.rs`, `fs_watch.rs`, `notify.rs` |
| 8 | Artifacts, documents, notebooks | `src/lib/artifact.ts`, `src/lib/chart.ts`, `tools/docs.rs`, `ArtifactPanel.tsx` |
| 9 | Desktop and browser control | `tools/desktop.rs`, `tools/browser.rs` |
| 10 | Windows and out-of-window surfaces | `src/App.tsx`, `src/main.tsx`, `src-tauri/src/lib.rs` |
| 11 | Attention and presence | `notify.rs`, `src/lib/time.ts`, `SettingsDialog.tsx` |
| 12 | Trust, safety and audit | `tools/guard.rs`, `approvals.rs`, `tools/sandbox*.rs`, `db::approval_log` |
| 13 | Cost, usage and routing | `api/models.rs`, `db::turn_usage`, `SpendPanel.tsx`, `store/settings.rs` |
| 14 | Portability and sharing | `backup.rs`, `src/lib/changelog.ts`, export/import commands |

---

### Group 1: Agent loop and orchestration

Owner: `conversation/agent.rs` (the loop and the system prompt), `conversation/assemble.rs` (the
messages array), `runs.rs` (the in-flight registry).

Vendor features landing here: Cowork's **sub-agent coordination and parallel workstreams** and
"long-running tasks without timeouts"; ChatGPT Codex **subagents**; Gemini Spark's **planned
steps** and its pause-on-question; Claude's **compaction** for unbounded conversations.

Current DeskSeek state: a single sequential loop, one live run per chat, tool rounds, a park
mechanism (`approvals.rs`, `questions.rs`), and a manual `/summarise`.

#### 1.1 Sub-agents with isolated context
- **Why**: long tasks drown in one context. All three vendors split work into sub-tasks with their
  own context and an aggregated result. This is the highest-leverage change to what DeskSeek can
  finish in one go.
- **Files**: new `conversation/subagent.rs`; `conversation/agent.rs` (spawn and aggregate);
  `runs.rs` (child runs must be cancellable with the parent); `tools/mod.rs` (a `spawn_agent`
  tool, classed as an action); `db.rs` (a `run_children` table or a parent id on `turn_usage`).
- **Shape**: a sub-agent is the existing loop with its own message array, its own system prompt
  slice, a hard iteration and spend cap, and a text result. The parent sees only the result, so the
  child's tool noise never enters the parent's context. Cancellation must cascade: `runs.rs`
  currently guarantees one run per chat, and a child must not be able to outlive its parent or
  escape the job object.
- **Test**: `cargo test` for the context isolation (the parent's assembled messages contain the
  child's summary and none of the child's tool calls), the cap enforcement, and cascade cancel.
- **Stop**: a task that reads twenty files and produces one answer costs one parent context.
- **Viability**: V2. **Size**: L. **Depends on**: nothing, but pairs with 12.1 and 13.2.

#### 1.2 A reviewing safety pass for Auto mode
- **Why**: Claude's Auto mode runs a safety review over each action (data exfiltration, prompt
  injection) before doing it, and falls back to asking when blocks pile up. DeskSeek's `guard.rs`
  is an honest heuristic and says so. A model-shaped review is the natural second layer.
- **Files**: `tools/guard.rs` (add an `Advisory` verdict beside `Allow`/`Refuse`), a new
  `conversation/review.rs`, `conversation/agent.rs` (call it in Auto only).
- **Shape**: before a risky call in Auto, a small, cheap model call with the tool name, a redacted
  argument summary and the last few turns, returning one of allow / refuse / ask. Cost is visible
  and it must be attributed to the turn (see 13.1). Fail closed to "ask", never to "allow".
- **Test**: a golden set of calls (a download-and-execute pipeline, a `rm -rf` variant, an upload
  of a local file, an ordinary write) asserting the verdict, plus that a review failure degrades to
  asking rather than proceeding.
- **Stop**: Auto no longer relies on string matches alone, and the fallback is asking.
- **Viability**: V1. **Size**: M.

#### 1.3 Context compaction as an automatic step
- **Why**: Claude compacts automatically near the limit; DeskSeek offers `/summarise` and a button
  at 80%. Automating it removes a decision from the user.
- **Files**: `conversation/summarize.rs` (exists), `conversation/agent.rs` (trigger), `src/lib/context.ts`.
- **Shape**: at a threshold (say 90%), compact before the next request and mark the transcript so
  the user can see that history was folded rather than lost. Keep the manual command: an automatic
  step with no manual equivalent is hard to debug.
- **Test**: a message array over the threshold compacts once and only once, and the folded range is
  recorded.
- **Stop**: a long chat stops hitting a length error without the user pressing anything.
- **Viability**: V1. **Size**: S.

---

### Group 2: Provider and model layer

Owner: `api/client.rs`, `api/models.rs`, `api/stream.rs`, `api/types.rs`, `store/keychain.rs`,
`store/settings.rs`.

Vendor features landing here: **model choice per surface and per task** (Claude picks a model per
scheduled task, ChatGPT distinguishes Chat / Work / Codex models, Gemini gates by plan);
**reasoning-effort control** (ChatGPT's slider); **Ultrafast** fast-generation modes; and,
implicitly, every feature in this document that needs vision, audio or a second opinion.

Current DeskSeek state: a DeepSeek-only client, SSE parsing, delta accumulation, and a model price
table with peak/off-peak hours.

This is the most strategically important group in the document, because several V2 features
elsewhere are gated on it: computer-use screenshots need a vision model, dictation and audio
overviews need speech models, and the reviewing safety pass benefits from a cheap second model.

#### 2.1 A provider abstraction: two dialects, N endpoints
- **Why**: every competitor lets the model be chosen, and the reason is not brand preference: a
  cheap model for a scheduled triage and an expensive one for a hard refactor is a real cost
  decision. It also unblocks vision and audio.
- **The decision (2026-10-13)**: build **two wire dialects**, not three vendor clients.
  `openai-compatible` and `anthropic-compatible`, each an endpoint record.
- **Files**: new `api/provider.rs` (a trait: `stream(messages, tools, model) -> token stream`),
  `api/openai_compat.rs`, `api/anthropic_compat.rs`, `store/keychain.rs` (a key per endpoint,
  which is what makes a local endpoint with no key work), `store/settings.rs` (the endpoint list
  and the default endpoint), `api/models.rs` (a price table keyed by endpoint and model).
- **Shape**: an endpoint is `{ id, label, dialect, base_url, key_ref, models[], notes }`. DeepSeek
  is endpoint one and already speaks the OpenAI dialect, so the existing `api/client.rs` becomes
  the reference implementation of `openai_compat` rather than being deleted. The trait must keep
  the invariants the current client owns: `reasoning_content` echoing, the thinking-mode rules, and
  transport-versus-refusal error classification (Phase 3.6). The agent loop must not be forked,
  and there must be exactly one place that knows a wire shape, per dialect.
- **What this buys, as configuration rather than code**: DeepSeek, OpenRouter (one key, many
  models), Google's OpenAI-compatible endpoint, Groq, Together, xAI, and local runtimes (Ollama,
  LM Studio, vLLM) on `http://localhost:11434` and friends. Mentioning these in the endpoint
  labels is worth doing so the field is discoverable, but none of them should get its own module.
- **Risk**: the tool-call wire format differs between the two dialects, and this repo has already
  been bitten once by an invisible contract (`the_spend_payload_is_camel_case_on_the_wire`). Each
  dialect needs a wire-shape test, not just a unit test of the mapper.
- **Spend, and the honest limit that follows**: `api/models.rs` prices are per endpoint and model.
  A local or gateway endpoint has no published rate, so the money figure must read as **unknown**
  rather than being guessed at, while tokens are still recorded. Saying "1,204 tokens, cost
  unknown" is honest; inventing a number is not, and it would quietly corrupt the spend view that
  currently agrees with the per-chat totals.
- **Test**: `cargo test` per dialect for stream assembly, tool-call round trip, error
  classification, the base-URL join, and the unknown-price path; plus an E2E checklist line per
  dialect (a real turn against a real endpoint, and one against a local model with no key).
- **Stop**: adding an endpoint is a form and no code, and switching endpoint runs a turn.
- **Viability**: V2. **Size**: M (was L under a three-vendor reading).

#### 2.2 Per-task and per-chat model selection
- **Why**: Claude's scheduled-task form has a model field, and ChatGPT changes models between Chat
  and Work. A nightly triage does not need the frontier model.
- **Files**: `schedule.rs` / `db.rs` (a `model` column on the scheduled row, `CREATE TABLE` plus a
  guarded `ALTER`), `ChatView.tsx` (a picker), `api/models.rs`.
- **Stop**: a scheduled task can be pinned to a cheaper model and its spend rows say which ran.
- **Viability**: V1 (after 2.1). **Size**: S.

#### 2.3 Reasoning-effort control
- **Why**: a slider over how much thought a model puts in. Useful for cost without losing quality
  where it matters.
- **Files**: `store/settings.rs`, `api/*`, `ChatView.tsx`.
- **Viability**: V1. **Size**: S. **Note**: must degrade gracefully per provider; a provider with no
  such parameter should say so rather than silently ignoring it.

---

### Group 3: Tools and containment

Owner: `tools/mod.rs` (the toolset and risk classes), `tools/guard.rs`, `tools/sandbox.rs`,
`tools/sandbox_exec.rs`, `tools/job.rs`.

Vendor features landing here: **per-app and per-site permission memory** (Claude's built-in browser
asks once / always for a site, and asks before using an app in computer use); **network egress
permissions per session** (Claude Cowork); **MCP events** (ChatGPT); **Site tools / WebMCP** (both
ChatGPT and Gemini); **secret handling** (ChatGPT Vaults).

Current state: risk classes, an approval ladder, a path sandbox, a job object that kills children
with the app, and a decision log with hashed arguments.

#### 3.1 Per-target permission memory
- **Why**: today every new write or command asks. Claude remembers an "always for this site" and
  "always for this app". Generalising that to per-target, revocable grants cuts approval fatigue
  without weakening the model.
- **Files**: `tools/mod.rs` (a `target` on each tool: a site, an app, a command prefix, a path
  root), `db.rs` (a `tool_grants` table beside the existing always-allowed set), `SettingsDialog.tsx`.
- **Shape**: a grant is (tool, target, scope, created_at), listed and revocable in one place. The
  existing rule holds: only decisions you can see and revoke may survive a restart.
- **Test**: a grant skips the prompt for exactly its target and for nothing else; revoking it asks
  again; the decision log still records the first grant and the hash.
- **Stop**: a repeated browser action on one site asks once.
- **Viability**: V1. **Size**: M.

#### 3.2 Per-run network egress policy
- **Why**: Claude Cowork respects session egress permissions. DeskSeek's guard already tries to
  catch uploads; a declared policy is stronger and clearer.
- **Files**: `tools/guard.rs`, `tools/sandbox.rs`, `ChatView.tsx` (a per-chat switch).
- **Shape**: per chat or per run: allow all, or read-only, or none. The guard consults the policy
  before the heuristic, so a declared "none" refuses uploads outright rather than pattern-matching.
- **Stop**: a chat set to "no uploads" cannot send a local file out, and says why when refused.
- **Viability**: V1. **Size**: S. **Honest limit**: still not a sandbox; state that in the UI.

#### 3.3 MCP events (server-initiated updates)
- **Why**: ChatGPT lets a user ask the assistant to act on updates from an MCP server, which turns
  a polling schedule into an event. DeskSeek's MCP client is otherwise complete, so this is an
  extension rather than a rewrite.
- **Files**: `mcp/manager.rs`, `commands.rs` (an inbound event command), `db.rs` if subscription
  state is persisted.
- **Shape**: a subscription list per server, an inbound handler that can start a run (reusing the
  scheduler's run path so it parks the same way), and a decision about whether app-closed events
  are replayed on launch. DeskSeek's rule for missed schedules is "fire once on next launch", and
  the same rule is the honest one here.
- **Test**: a synthetic event starts exactly one run and does not double-fire on restart.
- **Stop**: a server can wake a task without a cadence.
- **Viability**: V2 (needs an MCP 2.0-shaped protocol surface). **Size**: M.

---

### Group 4: Persistence and schema

Owner: `store/db.rs` (schema, table and ALTER discipline, all queries), `store/paths.rs` (every path
under the app root), `store/settings.rs`.

Vendor features landing here: **notebooks and sources** (Gemini Notebook), an **activity/computer
history timeline** (ChatGPT Computer History), **memory topics** (Claude), **parked and parked-lost
runs** (already present).

House rules that apply to everything in this group, from `plan.md`: a new column is a `CREATE
TABLE` entry *and* a guarded `ALTER`; any index over a new column is only safe because the ALTERs
run before the schema batch; and a test over a fresh in-memory schema proves nothing about an
upgrade, so anything touching `open()` needs a real temp file.

#### 4.1 A notebooks and sources model
- **Why**: this is the substrate for Group 8.2 and for every "answer only from these sources, with
  citations" feature. It is also the feature Danel would use weekly as a student and a consultant.
- **Files**: `db.rs` (`notebooks`, `sources`, `notebook_messages`, `notebook_outputs`), `paths.rs`
  (a per-notebook source folder), `commands.rs` (CRUD plus an ask command).
- **Shape**: a notebook is a title, a folder, and an instruction block. A source is a file or URL,
  an extract, a content hash and a synced flag. A message is grounded (with citations) or not.
  Reuse `tools/extract.rs` for document text and the FTS machinery for retrieval.
- **Test**: a notebook with three sources lists them, a delete cascades, and a source that changed
  on disk is reported as stale rather than silently reused.
- **Stop**: sources, questions and outputs persist and can be added to later.
- **Viability**: V2. **Size**: L.

#### 4.2 An activity timeline (off by default)
- **Why**: ChatGPT's Computer History turns app and site activity into a searchable timeline and
  memory. DeskSeek already emits `fs-changed` and records runs and tool calls, so a local version
  is mostly a read model over data it has.
- **Files**: `db.rs` (`activity`), `fs_watch.rs` (producer), `conversation/agent.rs` (producer),
  `commands.rs` (search), a new panel.
- **Shape**: an append-only row per meaningful event with a source (a file, a browser page, a tool
  call), a summary and a timestamp, with a per-source switch and a pause, and a hard retention cap
  pruned like `file_checkpoints`. Off by default, because the honest reading of ChatGPT's staged
  rollout and its EEA exclusion is that this one is sensitive.
- **Test**: pruning keeps the newest N; disabling a source stops new rows; a search returns the
  event and its source.
- **Viability**: V2. **Size**: M. **Depends on**: a decision about whether it feeds memory (5.3).

#### 4.3 Memory topic ergonomics in the schema
- **Why**: Claude exposes memory as editable, categorised topics with per-item delete. DeskSeek
  stores exactly that but the read path is narrower than the write path in one place (the project
  memory view is read-only).
- **Files**: `db.rs` if topics are indexed, `MemorySection.tsx`, `ProjectsPanel.tsx`.
- **Viability**: V0. **Size**: S. Fold into whichever group is open.

---

### Group 5: Memory and context assembly

Owner: `memory.rs`, `memory_files.rs`, `conversation/assemble.rs` (the messages array and the
reasoning rule), `conversation/agent.rs` (the assembled system prompt).

Vendor features landing here: Claude's **memory topics**, **sensitive topics** switch and incognito
chats; ChatGPT **Computer History**-derived memories; Gemini **Personal context**; and every
vendor's **Deep Research / notebook grounding**, which is context assembly by another name.

Current state: `MEMORY.md` always sent, `memory/<category>.md` looked up by keyword, project-scoped
memory files, `save_memory`, `/summarise` producing tagged proposals, and a settings viewer.

#### 5.1 Incognito chats
- **Why**: Claude and ChatGPT both offer a conversation that memory does not learn from. It is a
  single switch and it is the difference between using the assistant for something private and not.
- **Files**: `store/db.rs` (a `chats.incognito` column, `CREATE TABLE` plus guarded `ALTER`),
  `memory.rs` (skip writes), `conversation/agent.rs` (skip the memory section), `ChatView.tsx`.
- **Test**: a `save_memory` call from an incognito chat writes nothing anywhere, and the prompt
  carries no memory section.
- **Stop**: nothing learned in an incognito chat can be quoted back in another.
- **Viability**: V0. **Size**: S.

#### 5.2 Sensitive-topic control
- **Why**: Claude keeps health and belief topics out of memory unless a switch is on. DeskSeek's
  categories are free-form, so the equivalent is a category-level exclusion list.
- **Files**: `store/settings.rs`, `memory_files.rs`, `MemorySection.tsx`.
- **Viability**: V1. **Size**: S.

#### 5.3 Semantic memory lookup
- **Why**: the README already names the gap honestly: ranking is by shared words, so "the car" does
  not surface "vehicle". Every vendor's memory retrieves by meaning.
- **Files**: `memory_files.rs` (the lookup), a new `store/embed.rs`, `store/settings.rs` (opt-in).
- **Shape**: an opt-in embedded index, kept local, with the keyword path as the fallback so a
  machine without the model still works. The honest cost is a model download; the README already
  states the trade.
- **Viability**: V2. **Size**: M. **Depends on**: an embeddings provider (local or 2.1).

#### 5.4 Grounded retrieval over a folder
- **Why**: this is the notebook idea without a notebook shell, and it is the cheapest way to make
  "answer from these documents, with citations" real.
- **Files**: `tools/walk.rs` (a chunk walk beside `find_named`), `store/db.rs` (a chunk index),
  `conversation/assemble.rs` (inject retrieved chunks with source labels), `Markdown.tsx` (citation
  rendering).
- **Shape**: chunk by heading, index text, retrieve top-k for the turn, label each chunk with its
  file and heading, and require the model to cite the label. Citations must be checkable: a
  citation naming a file that is not in the retrieved set is a bug, and the test should assert that.
- **Viability**: V2. **Size**: M. This is the sibling of 4.1 and should probably land with it.

---

### Group 6: Skills, prompts, plugins

Owner: `store/files.rs` (the shared one-markdown-file store and the id guard), `skills.rs`,
`prompts.rs`, `design.rs`, `SkillsPanel.tsx`, `PromptsSection.tsx`, `DesignSystemsSection.tsx`.

Vendor features landing here: **the `SKILL.md` open standard** (all three), Claude's **plugins**
bundling skills, connectors and sub-agents, Claude's **Record a skill**, ChatGPT's **import from
another agent**, and every vendor's skill directory.

This is the cheapest high-value group in the document, because DeskSeek already speaks the format
that Gemini and Claude standardised on.

#### 6.1 Skill interchange (import and export)
- **Why**: Gemini's documentation states that skills are built on the open, Markdown-native
  `SKILL.md` standard so that skills created elsewhere can be copied in; Claude calls it Agent
  Skills; ChatGPT documents skills the same way. DeskSeek stores `skills/<id>/SKILL.md`. An import
  and export path is a format-fidelity problem, not a research problem.
- **Files**: `skills.rs` (an importer and an exporter), `store/files.rs` (the existing guard does
  the safety work), `SkillsPanel.tsx` (Import from folder, Import from file, Export all),
  `commands.rs`.
- **Shape**: accept a folder of skill directories, a single `SKILL.md`, or a zip. Map frontmatter
  that exists (`name`, `description`, `default`) and preserve unknown keys verbatim so a round trip
  is lossless. Refuse an id that would climb out of its folder, using the existing rule, and say so
  in the UI rather than silently renaming. Import is additive and never overwrites.
- **Test**: `cargo test` for a round trip through a file with unknown frontmatter keys preserved,
  the id guard on a hostile name, and an empty-prefix case (the prefix has never been what protects
  the folder; the separator rule is).
- **Stop**: a skill downloaded from a Claude or Gemini library works here unmodified, and a
  DeskSeek skill exported round-trips.
- **Viability**: V0 or V1. **Size**: S. **This should be the first thing built.**

#### 6.2 Plugins as bundles
- **Why**: Claude's plugins package skills, connectors and sub-agents into one installable unit and
  save to the account. DeskSeek has all three primitives in separate stores, so a bundle is a
  manifest plus an installer.
- **Files**: new `plugins.rs`, `store/files.rs` (a plugin store beside the shared one),
  `mcp/manager.rs` (a bundled server config), `CustomiseDialog.tsx`.
- **Shape**: a plugin is a folder with a `plugin.json` naming its skills, its `mcp.json` servers,
  its prompts and (later) its sub-agent definitions. Install is additive; uninstall removes only
  what the manifest named. The one-file-owns-each-rule discipline applies: a plugin must not
  duplicate an id guard or a store format.
- **Test**: install a bundled plugin, assert each store gained exactly the declared ids and nothing
  else; uninstall leaves hand-made entries untouched.
- **Stop**: a bundle installs and uninstalls without touching anything the user wrote.
- **Viability**: V1. **Size**: M.

#### 6.3 Record a workflow into a skill
- **Why**: Claude allows a workflow to be recorded rather than described, then turns it into a
  skill, and Claude in Chrome records browser workflows. DeskSeek has the two recording surfaces
  already: `fs_watch` sees file activity and `tools/browser.rs` sees page actions.
- **Files**: a new `recorder.rs`, `tools/mod.rs` (a recording mode), `skills.rs` (write the draft),
  `ToolsPanel` / `SkillsPanel`.
- **Shape**: turn on recording, do the task, and the recorder writes a draft `SKILL.md` from the
  observed steps, with an explicit warning never to type secrets while recording. The draft is a
  proposal the user edits before it is saved, matching the `/summarise` precedent (nothing is
  stored until accepted).
- **Test**: a recorded sequence produces a draft whose steps match the events in order, and a
  recording containing a browser `type` into a password-shaped field is flagged.
- **Stop**: a repeated browser or folder workflow becomes a skill without writing prose.
- **Viability**: V1. **Size**: M.

#### 6.4 Skill and plugin review before install
- **Why**: Claude runs skill and plugin security scanning for Enterprise because third-party
  instructions are executable in the prompt. DeskSeek imports skills now, so an import is a trust
  boundary.
- **Files**: `skills.rs` (an import path hook), `adversarial-verifier`-style review using the model.
- **Shape**: on import, show what the skill asks for: tools it names, whether it mentions
  exfiltration or credential paths, whether it tries to override the guard. Report, do not block.
- **Viability**: V1. **Size**: S.

---

### Group 7: Scheduling and triggers

Owner: `schedule.rs` (next-run arithmetic), `commands.rs` (the ticker, `scheduled-fired`, the park
queue), `notify.rs`, `fs_watch.rs`, `ScheduledPanel.tsx`.

Vendor features landing here: Gemini Spark's **three schedule types** (time, Gmail trigger,
monitor), ChatGPT's **event-triggered tasks** (Gmail, Slack, GitHub), Claude's **hourly, weekdays,
manual, per-task model, per-task folder, approval mode** fields and its **upcoming and past runs**
view.

Current state: daily / weekly / first-weekday-of-month, Run now, Pause, one live run, a needs-you
park queue with a notification, and "a missed window fires once on next launch".

This is the group where DeskSeek is closest to parity and where the remaining gaps are cheapest.

#### 7.1 More cadence shapes
- **Why**: Claude offers hourly, weekdays and manual-on-demand. DeskSeek's three fields are a
  deliberate simplification and should stay, but "every hour" and "weekdays only" are the two
  additions users will reach for.
- **Files**: `schedule.rs` (the pure arithmetic and its tests), `ScheduledPanel.tsx`.
- **Test**: hourly across a day boundary, weekdays across a weekend, and the existing invariant
  that `next_run_at` is always strictly after now and is advanced before a run starts.
- **Viability**: V0. **Size**: S.

#### 7.2 Event triggers
- **Why**: ChatGPT starts a task when an event occurs in Gmail, Slack or GitHub, and Gemini Spark
  has Gmail-triggered schedules. A cadence-only scheduler either fires too often or too late.
- **Files**: new `triggers.rs`, `schedule.rs` (a trigger kind beside the cadence),
  `fs_watch.rs` (a file trigger, almost free because it already emits `fs-changed`), `commands.rs`,
  `ScheduledPanel.tsx`.
- **Shape**: three local trigger kinds to start with, because they need no cloud account:
  a file appears or changes in a folder; a git ref or branch moves; an MCP event arrives (3.3) or a
  fetched page matches a condition. A trigger fires into the same run path as a cadence, so it
  parks, notifies and records identically. Debounce, and cap fires per hour, so a busy folder cannot
  start fifty runs.
- **Test**: a debounced fire produces one run; a fire while the app is closed is not replayed
  hundreds of times; the cap holds.
- **Stop**: "when a new file lands in this folder" starts a task without a cadence.
- **Viability**: V1 for the file case, V2 for the MCP case. **Size**: M.

#### 7.3 Monitors (a condition, not a time)
- **Why**: Gemini Spark's monitor type checks a condition periodically and acts once it is met.
  It is the shape of half the useful automations ("tell me when this changes").
- **Files**: `triggers.rs`, `schedule.rs`, `tools/browse.rs` (the fetch).
- **Shape**: a monitor holds a prompt, an interval, a bounded check (a URL and a predicate, or a
  file stat), and a state. Edge-triggered on a transition, never on every poll, or it becomes a
  spam generator. State persists so a restart does not re-fire.
- **Test**: a value that stays true across polls fires once; a value that flaps fires on each
  transition up to the cap.
- **Viability**: V1. **Size**: M.

#### 7.4 Scheduled-run form parity
- **Why**: Claude's task form carries approval mode, model, folder and a run history. DeskSeek has
  the park machinery but not the fields, and the run history is the thing that makes a schedule
  trustworthy.
- **Files**: `db.rs` (`scheduled_runs` or reuse `turn_usage` with a task id), `schedule.rs`,
  `ScheduledPanel.tsx`.
- **Stop**: a task shows its last N runs with outcome and cost.
- **Viability**: V0. **Size**: S.

#### 7.5 Natural-language schedule authoring as a tool
- **Why**: Claude proposes a name, a cadence and instructions and asks you to confirm; Gemini Spark
  creates a schedule conversationally. DeskSeek has no `schedule_task` tool, so the model cannot
  offer it mid-conversation.
- **Files**: `tools/mod.rs` (a new tool, classed as an action so it asks), `commands.rs`,
  `conversation/agent.rs` (a prompt section), the `# Scheduled` guidance.
- **Shape**: the tool writes a task, and the result names it so the model can confirm in prose. The
  panel remains the place to edit it. Match the `/summarise` rule: the tool proposes, the user
  confirms, and the confirm is an approval like any other.
- **Test**: the tool's argument parsing over malformed cadences (the `tasks.check.ts` lesson: this
  parses model-written arguments and must survive every shape).
- **Viability**: V0. **Size**: S.

---

### Group 8: Artifacts, documents, notebooks

Owner: `src/lib/artifact.ts`, `src/lib/chart.ts`, `tools/docs.rs`, `ArtifactPanel.tsx`,
`ArtifactsGallery.tsx`, `ChartImage.tsx`.

Vendor features landing here: Claude's **live artifacts** and **edit in place**, its design /
slides / docs artifact types; Gemini Notebook's **Studio outputs** (reports, study guides,
flashcards, quizzes, mind maps, infographics, slide decks, data tables, audio and video overviews);
ChatGPT's Sites and visualizations.

Current state: six artifact kinds, versioning with restore, a gallery, a sandboxed frame, a chart
spec that takes data rather than markup, and three document writers.

#### 8.1 Edit an artifact in place
- **Why**: Claude's version is the best UX idea in the group: highlight the text you want changed,
  type the change, and the edit lands where you marked it rather than requiring you to re-describe
  the paragraph in chat.
- **Files**: `ArtifactPanel.tsx` (a selection and an edit affordance), `src/lib/artifact.ts` (a
  patch operation), `conversation/agent.rs` (a revise call that sends only the selection, the
  instruction and a small surrounding window).
- **Shape**: an edit is a new version attributed to the user, matching the existing *Edit* path, so
  nothing is overwritten and the version menu stays meaningful. The prompt for a selection edit is
  small on purpose, and the cost row should show that.
- **Test**: `artifact.check.ts` for the patch application (a selection in the middle, at the start,
  at the end, a selection that appears twice) and that a revision always adds a version.
- **Stop**: a paragraph can be fixed without opening a chat turn or retyping the document.
- **Viability**: V1. **Size**: M.

#### 8.2 A notebooks surface
- **Why**: see 4.1 and 5.4. This is the user-facing half: a notebook holds sources, a grounded
  conversation and a set of outputs, and it is the product shape Gemini committed to with
  NotebookLM.
- **Files**: new `NotebookPanel.tsx` / `NotebookView.tsx`, `commands.rs`, `db.rs`, `tools/extract.rs`.
- **Shape**: a notebook shows its sources with sync state, a grounded chat with citations, and an
  output menu over the existing artifact and document writers. Reuse the artifact panel rather than
  building a second viewer. Copying a notebook should carry sources and outputs but not chat
  history, which is exactly the line Gemini draws and is also the honest one.
- **Test**: source add and remove, a stale source warning, an answer whose citations all resolve to
  a real source, and a copy that carries outputs but not messages.
- **Stop**: one source set supports several questions and several outputs, durably.
- **Viability**: V2. **Size**: L. **Depends on**: 4.1 and 5.4.

#### 8.3 A study and briefing output set
- **Why**: the reason NotebookLM is popular is the generator menu. DeskSeek already has the
  writers and the chart engine, so flashcards, quizzes, a briefing, a timeline and a study guide
  are mostly prompts plus a small schema each.
- **Files**: `tools/docs.rs` (new generators), `src/lib/artifact.ts` (a new kind if a quiz needs
  interactivity), a new `StudyPanel` or a notebook output menu.
- **Shape**: prefer reuse. A flashcard set is a document. A quiz that marks answers needs a small
  interactive artifact kind, with the same sandbox rules as `html`. A mind map is `mermaid`, which
  already exists. An infographic is an `html` artifact. Only draw a line at a new kind when the
  existing ones cannot carry it.
- **Viability**: V1 for the document-shaped ones, V2 for an interactive quiz. **Size**: M.

#### 8.4 Data tables over sources
- **Why**: Gemini's Data Tables synthesise facts across sources into a structured table and export
  to a spreadsheet. DeskSeek has `sheet_create`, so the gap is the extraction schema plus a
  citation per cell.
- **Files**: `tools/docs.rs`, `conversation/agent.rs` guidance, `.xlsx` writer.
- **Shape**: cells carry a source reference in a comment column rather than a live formula, which is
  honest, because the workbook cannot resolve a citation.
- **Viability**: V1. **Size**: S.

#### 8.5 Audio Overview (text to speech)
- **Why**: the single most-loved NotebookLM feature is two voices discussing the sources. It is also
  the one that needs a paid third-party service.
- **Files**: new `tools/audio.rs`, a provider key (2.1), `ArtifactPanel.tsx` (an audio kind).
- **Shape**: a script generator over the notebook, then a TTS call, then saving the clip into the
  chat or notebook folder. Costs money per minute, so it must be an explicit, priced action, and the
  live cost must show before it runs.
- **Viability**: V2. **Size**: M. **Depends on**: 2.1.

---

### Group 9: Desktop and browser control

Owner: `tools/desktop.rs`, `tools/browser.rs`.

Vendor features landing here: Claude's **computer use** and **built-in browser** with per-site
permission memory, **Claude in Chrome** (multi-tab, console logs, network, DOM, recording), Gemini's
opt-in screen sharing, ChatGPT's **Appshots** and its ability to drive your own browser across five
vendors, and **Site tools / WebMCP**.

Current state: headless Edge/Chrome with its own profile on a debugging port, JSON-over-WebSocket
CDP spoken directly, reads separated from actions, actions always asking, screenshots going to the
user's eyes rather than the model, and a documented Windows-only desktop automation with real rails.

The stated ordering policy from Claude is the design to copy: **connector first, then browser, then
computer use**, because that is also cheapest first.

#### 9.1 Vision into tool results
- **Why**: the README says screenshots go to the user's eyes because DeepSeek's API is text-only. A
  vision-capable model (2.1) removes the reason, and this one change upgrades both browser control
  and computer use from "text-only and blind" to "can see what happened".
- **Files**: `tools/browser.rs`, `tools/desktop.rs`, `api/*` (a multimodal tool-result shape),
  `ToolCallCard.tsx` (show the image the model saw).
- **Shape**: downscale once, keep the full-size file, and send the smaller one, matching the
  attachment rule the README already states. A tool result that carries an image must say so in
  text too, for the model's benefit, and the current "returned but not shown" wording should be
  revised rather than left to contradict the new behaviour.
- **Test**: the result-shape test asserts an image part and a text part together, and a provider
  without vision degrades to the text part with an explicit note.
- **Viability**: V2. **Size**: M. **Depends on**: 2.1.

#### 9.2 Multi-tab, console and network in the browser
- **Why**: Claude in Chrome reads console errors, network requests and DOM state, which is what
  makes it useful for debugging a web app. CDP already offers all three.
- **Files**: `tools/browser.rs` (new tools or new fields), `conversation/agent.rs` (prompt
  guidance), `ToolStepGroup.tsx`.
- **Shape**: `browser_console`, `browser_network` and a tab-aware `browser_open`. Console output is
  a read, network headers are a read; both should be classed accordingly and bounded, because a
  network log is unbounded by nature.
- **Viability**: V1. **Size**: S. High value for Danel's client work.

#### 9.3 An extension bridge to the user's own browser
- **Why**: every vendor lets the assistant work in the browser you are already signed into. Neither
  DeskSeek's isolated profile nor a headless browser can do that, and it is the reason people keep
  a second assistant open.
- **Files**: a new `browser-extension/` package, `tools/browser.rs` (a second transport beside
  CDP), `store/settings.rs` (the choice of browser backend).
- **Shape**: a thin MV3 extension that opens a WebSocket to a loopback port the app owns,
  authenticated with a per-run token, and relays read and action commands for the active tab. It
  must be a separate install with its own permission screen, and it must be the user's explicit
  choice which tabs it may see. Treat page content as data, and keep the existing rule that the
  acting tools always ask.
- **Test**: E2E only (it needs a real browser and a real extension), so it lands on
  `docs/E2E-CHECKLIST.md`, plus unit tests for the message framing and the token check.
- **Viability**: V2. **Size**: L.

#### 9.4 Record a browser workflow
- **Why**: Claude in Chrome records a workflow by watching you and then repeating it.
- **Files**: `tools/browser.rs` (a recording channel), `recorder.rs` (6.3).
- **Viability**: V1 once 6.3 exists. **Size**: S.

#### 9.5 Computer use: coordinates, an app inventory, and the ordering policy
- **Why**: DeskSeek's desktop tool reads the accessibility tree, so a canvas or game reports
  nothing, and there is no raw coordinate clicking. Vision (9.1) plus a screenshot-anchored click
  closes that gap the way every vendor's computer-use does.
- **Files**: `tools/desktop.rs`, plus a prompt section that states the connector, then browser, then
  computer-use ordering.
- **Shape**: keep the rails exactly as they are (off by default, ask every time, 25 actions, 250 ms
  spacing, `Ctrl+Alt+Esc`). Add a screenshot-anchored click and an enumerated list of open windows
  so the agent can choose a target rather than guess a name.
- **Test**: the spacing and cap tests stay; add a test that a coordinate click with desktop control
  off is refused.
- **Viability**: V2. **Size**: M. **Depends on**: 9.1.

---

### Group 10: Windows and out-of-window surfaces

Owner: `src-tauri/src/lib.rs` (window setup, `setup()`), `src/App.tsx`, `src/main.tsx`.

Vendor features landing here: **quick entry** (Claude), **Pets** and the companion window
(ChatGPT), the **`Alt+Space` overlay** (Gemini), plus every vendor's system tray / menu bar entry.

This group is the highest ratio of daily-habit change to code in the whole document. All three
competitors shipped it and all three lead their marketing with it.

#### 10.1 A global-shortcut quick-entry window
- **Why**: a small always-on-top window over any app, on a shortcut, is how users stop switching
  windows to ask something. It costs one extra window and one shortcut.
- **Files**: `src-tauri/src/lib.rs` (a second window, `always_on_top`, `skip_taskbar`, no
  decoration), a new `src/components/QuickEntry.tsx`, `@tauri-apps/plugin-global-shortcut` (a new
  Tauri plugin), `store/settings.rs` (the chord), `tauri.conf.json` (window config).
- **Shape**: the window sends into a dedicated chat or a new one, whichever the setting says, and it
  must appear on the monitor with the focused window. The shortcut must be changeable and
  reportable, because a chord that collides with another launcher is the first thing a user hits.
  Claude's Mac-only caveats (screenshots and Caps Lock dictation) are a reminder to be explicit
  about platform differences rather than silently omit them.
- **Test**: mostly E2E (a real global chord and a real focus change); leave a unit test for the
  chord parsing and the settings round trip.
- **Stop**: a question can be asked from inside another application without leaving it.
- **Viability**: V1. **Size**: M.

#### 10.2 Context capture from the focused window (an Appshot)
- **Why**: ChatGPT's Appshots bring a screenshot and the available text of the frontmost window into
  a conversation. DeskSeek already has both halves: `ui_snapshot` reads the focused window as text
  and the browser tool can capture; only the shortcut and the wiring are missing.
- **Files**: `tools/desktop.rs` (a capture that returns a file path), `QuickEntry.tsx`, `lib.rs`.
- **Shape**: a chord (or the quick-entry window's own button) that captures the focused window,
  files it into the chat's folder like any attachment, and prefills a turn. Windows-only, matching
  the existing desktop feature's honesty.
- **Viability**: V1. **Size**: S. **Depends on**: 10.1.

#### 10.3 Dictation
- **Why**: Claude's hold-Caps-Lock dictation and ChatGPT's voice input exist because talking is
  faster than typing for some inputs.
- **Files**: a new `src/lib/voice.ts`, a provider or local speech-to-text (2.1), `ChatView.tsx`.
- **Shape**: push to talk, transcribe, insert into the composer rather than sending (matching the
  prompt-library rule that text goes into the box to edit). Say plainly which platform is supported.
- **Viability**: V2 if it needs a paid API, V2 if it needs a local model. **Size**: M.

#### 10.4 A tray or menu-bar entry
- **Why**: all three ship one. It is also the natural home for "new chat", "open quick entry" and a
  status dot.
- **Files**: `lib.rs` (Tauri tray), `src/App.tsx` (commands).
- **Viability**: V1. **Size**: S.

#### 10.5 Composer mentions for context and skills
- **Why**: ChatGPT's quick controls use `@` for context and `$` for a skill, and plugin composer
  mentions are desktop-only. DeskSeek's composer already parses `/` commands and `/p:<id>` prompts,
  so the parser and the insert counter pattern exist.
- **Files**: `src/lib/commands.ts` (the parser), `ChatView.tsx`, `commands.check.ts`.
- **Shape**: `@` inserts a file, folder or open tab reference; `$` inserts a skill mention. Both are
  text-insertion, never a hidden send, matching the prompt rule. Extend the existing check rather
  than writing a second parser.
- **Viability**: V0. **Size**: S.

---

### Group 11: Attention and presence

Owner: `notify.rs`, `src/lib/time.ts`, `SettingsDialog.tsx`.

Vendor features landing here: Claude's **break reminders** and **quiet hours** under Time and
focus, its **monthly recap** under Reflect, ChatGPT's notifications for long-running work, and every
vendor's "it tells you when it needs you".

Current state: a green dot on a sidebar row for a chat that moved while you were elsewhere, an OS
toast only when the window is not focused, a settings switch, and a notification on a parked run.

This group is small, self-contained, and it is the direct answer to the "create reminders" ask.

#### 11.1 Break reminders and quiet hours
- **Why**: Claude ships both, and they are the least glamorous and most used features in its
  settings. They also fit a solo founder's day better than another agent capability.
- **Files**: `notify.rs` (respect quiet hours), `store/settings.rs` (the schedule),
  `SettingsDialog.tsx`, a small ticker beside the scheduler's.
- **Shape**: break reminders are a cadence with an on/off and an interval; quiet hours are a window
  in which no toast is raised, with a rule for how a queued notification is shown afterwards (the
  honest answer: in-app dot only, and say so).
- **Test**: a toast inside quiet hours is suppressed and the in-app dot still appears, which mirrors
  the existing rule that the dot is not affected by the notification switch.
- **Stop**: notifications respect a window, and it is visible in settings.
- **Viability**: V0. **Size**: S.

#### 11.2 A weekly or monthly recap
- **Why**: Claude's Reflect reports topics, the most active day and the peak hour. DeskSeek records
  turn usage with timestamps and knows every chat, so this is a read model over existing tables.
- **Files**: `db.rs` (a recap query), a new `RecapPanel.tsx` or a section of `SpendPanel.tsx`,
  `notify.rs` if it is delivered.
- **Shape**: draw the numbers rather than describe them: reuse the `chart` engine the spend view
  already uses, in prose plus one chart. Never invent a topic list the data cannot support.
- **Viability**: V0. **Size**: S.

#### 11.3 A notification that links to the thing
- **Why**: a toast that says "a task finished" and cannot take you to it is half a feature.
- **Files**: `notify.rs`, `lib.rs` (focus and route), `App.tsx` (open the chat or the park item).
- **Viability**: V0. **Size**: S.

---

### Group 12: Trust, safety and audit

Owner: `tools/guard.rs`, `approvals.rs`, `questions.rs`, `tools/sandbox.rs`,
`tools/sandbox_exec.rs`, `db::approval_log`, `SettingsDialog.tsx`.

Vendor features landing here: Claude's **Auto mode safety review**, its **network egress
permissions**, its **deletion protection** and its **skill and plugin security scanning**;
ChatGPT's agent approvals and security, its sandboxing model, and its Vaults; and every vendor's
admin and audit surface.

Current state: a documented heuristic guard with a catastrophic tier, three approval modes,
per-folder grants, a decision log with hashed arguments and no foreign key to a chat, and a
workspace path sandbox described as the real containment.

#### 12.1 A safety review before acting (see 1.2)
Listed in Group 1 because the loop calls it; it belongs to this group's philosophy. A review pass is
the honest upgrade path for an Auto mode that currently says out loud that it is a heuristic.

#### 12.2 An egress policy the user declares (see 3.2)
Same reasoning: declared beats inferred.

#### 12.3 Deletion protection
- **Why**: Claude requires an explicit permission before permanently deleting files, separately
  from other writes, because a delete under a pile of approvals is the one that cannot be undone.
- **Files**: `tools/guard.rs` (a delete class), `tools/mod.rs` (a delete class that never
  auto-approves), the rewind/checkpoint path.
- **Shape**: a deletion is a distinct risk class that no mode auto-approves, and the pre-image is
  recorded first so `/rewind` can still put the file back. DeskSeek's undo already only covers file
  tools, which makes a `rm` through `run_command` the real hole; say so where the command card
  already says it.
- **Test**: a recursive delete is refused under Skip, and a file tool delete still rewinds.
- **Viability**: V1. **Size**: S.

#### 12.4 Audit export
- **Why**: Claude ships an Analytics API and OpenTelemetry for Cowork sessions; ChatGPT has an
  Analytics API and audit events. A local equivalent is "export my decision log and run history".
- **Files**: `db::approval_log`, `turn_usage`, `commands.rs`, `lib/changelog.ts` for the shape rule.
- **Shape**: a CSV or JSON export of decisions, runs and spend, so the user can actually look at it.
  The existing hash-not-arguments rule stays: an export is a file that gets mailed around.
- **Viability**: V0. **Size**: S.

---

### Group 13: Cost, usage and routing

Owner: `api/models.rs`, `db::turn_usage`, `db::spend_report`, `SpendPanel.tsx`,
`store/settings.rs`.

Vendor features landing here: ChatGPT's **usage and credits shared between Work and Codex**, its
**Ultrafast** modes and its reasoning-effort slider; Claude's **usage limits** and the explicit note
that Auto mode costs more because of the safety review; Gemini's credit-based limits.

Current state: a genuinely strong spend view for a solo app (per-turn rows, pie of where tokens
went, bar per day, by model, by chat) and a per-run spend limit.

#### 13.1 Cost attribution for new sub-systems
- **Why**: sub-agents (1.1), a safety review (1.2), dictation, TTS and semantic lookup all spend
  money, and the repo's own rule is that the sidebar total and the spend view must be two readings
  of one number rather than two stories.
- **Files**: `db.rs` (`turn_usage` gains a kind or a parent id), `api/models.rs` (prices),
  `SpendPanel.tsx` (a split by kind).
- **Shape**: one row per unit of spend, as now, with a label that says who spent it. A sub-agent's
  spend must roll into its parent turn, and a review call must be visible as an overhead line, so a
  surprising bill is explainable.
- **Test**: the sections sum to the total for a turn that includes a sub-agent and a review call.
- **Stop**: no new feature can spend money invisibly.
- **Viability**: V1. **Size**: S. **Do this before 1.1.**

#### 13.2 Cheap-model routing with a recommendation
- **Why**: every vendor picks a model per surface and every vendor prices it. DeskSeek already has
  a `model-router-cost-estimator` habit in its skill library, so the app-side counterpart is a
  suggestion at the point of a decision.
- **Files**: `api/models.rs`, `store/settings.rs`, `ChatView.tsx` (a suggestion, never a silent
  switch), `ScheduledPanel.tsx`.
- **Shape**: never silently route. A suggestion with the reason and an estimate is honest; a silent
  downgrade that changes an answer's quality is not.
- **Viability**: V1 after 2.1. **Size**: S.

---

### Group 14: Portability and sharing

Owner: `backup.rs`, the export and import commands, `src/lib/changelog.ts`, `docs/`.

Vendor features landing here: Claude's **memory import and export** and **embed codes** for
published artifacts; ChatGPT's **shared thread snapshots** with secret redaction, and its
**import from Claude Code, Cowork or Cursor**; every vendor's account-level portability.

Current state: a zip backup of the whole profile, markdown/PDF/JSON export, a versioned chat bundle
for moving between machines, and an honest statement that the bundle is the transcript and not the
attachments.

#### 14.1 Read-only share snapshots
- **Why**: ChatGPT exports a read-only snapshot of a thread, redacting known secret patterns, and
  says plainly that sensitive content may remain. That is exactly the shape a local app can offer:
  an HTML file, not a service.
- **Files**: a new `export_snapshot` command, `src/lib/artifact.ts` (the render path), `backup.rs`
  (the redaction pass).
- **Shape**: one self-contained HTML file with the transcript, the artifacts and the task list,
  rendered through the same sandboxed pipeline, with a secret-pattern redaction pass and a warning
  that redaction is best-effort. No upload, no server.
- **Test**: a transcript containing a key-shaped string is redacted; the file opens with no network
  access.
- **Stop**: a conversation can be handed to someone without handing over a service.
- **Viability**: V1. **Size**: M.

#### 14.2 A skill and memory importer for other agents
- **Why**: ChatGPT imports instructions, settings, skills, plugins, projects and recent work from
  Claude Code, Claude Cowork and Cursor. DeskSeek should import at least skills and memory, because
  it already speaks the skill format.
- **Files**: `skills.rs` (overlaps 6.1), `memory_files.rs` (accept a memory export),
  `store/settings.rs` (instructions).
- **Viability**: V0 once 6.1 exists. **Size**: S.

---

## Viability tiers and effort

Every feature above, sorted. "Blocks" means another feature should land first.

| Tier | Feature | Group | Size | Blocks |
| --- | --- | --- | --- | --- |
| V0 | Skill interchange (import and export) | 6.1 | S | none. **Build first** |
| V0 | Import skills and memory from other agents | 14.2 | S | 6.1 |
| V0 | Incognito chats | 5.1 | S | none |
| V0 | Natural-language schedule authoring tool | 7.5 | S | none |
| V0 | More cadence shapes (hourly, weekdays, manual) | 7.1 | S | none |
| V0 | Scheduled-run history | 7.4 | S | none |
| V0 | Break reminders and quiet hours | 11.1 | S | none |
| V0 | Weekly or monthly recap | 11.2 | S | none |
| V0 | Notification that links to the thing | 11.3 | S | none |
| V0 | Composer `@` and `$` mentions | 10.5 | S | none |
| V0 | Audit export | 12.4 | S | none |
| V0 | Memory topic ergonomics in the project view | 4.3 | S | none |
| V1 | Context compaction as an automatic step | 1.3 | S | none |
| V1 | A reviewing safety pass for Auto | 1.2 | M | 13.1 |
| V1 | Per-target permission memory | 3.1 | M | none |
| V1 | Per-run network egress policy | 3.2 | S | none |
| V1 | Sensitive-topic control | 5.2 | S | none |
| V1 | Plugin bundles | 6.2 | M | 6.1 |
| V1 | Record a workflow into a skill | 6.3 | M | 6.2 |
| V1 | Skill review before install | 6.4 | S | 6.1 |
| V1 | Event triggers (file, git, page) | 7.2 | M | none |
| V1 | Monitors | 7.3 | M | 7.2 |
| V1 | Edit an artifact in place (in Phase 8, confirmed) | 8.1 | M | none |
| V1 | Study and briefing output set | 8.3 | M | 8.2 for the notebook menu |
| V1 | Data tables over sources | 8.4 | S | 5.4 or 4.1 |
| V1 | Browser console, network and tabs | 9.2 | S | none |
| V1 | Record a browser workflow | 9.4 | S | 6.3 |
| V1 | Quick-entry window | 10.1 | M | none |
| V1 | Appshot from the focused window | 10.2 | S | 10.1 |
| V1 | Tray or menu-bar entry | 10.4 | S | none |
| V1 | Deletion protection | 12.3 | S | none |
| V1 | Cost attribution for new sub-systems | 13.1 | S | none |
| V1 | Cheap-model routing suggestion | 13.2 | S | 2.1 |
| V1 | Read-only share snapshots | 14.1 | M | none |
| V2 | Provider abstraction: two dialects, N endpoints | 2.1 | M | none. **Unblocks the rest of V2** |
| V2 | Sub-agents with isolated context | 1.1 | L | 13.1 |
| V2 | Per-task and per-chat model selection | 2.2 | S | 2.1 |
| V2 | Reasoning-effort control | 2.3 | S | 2.1 |
| V2 | MCP events | 3.3 | M | 7.2 for the run path |
| V2 | Notebooks and sources model | 4.1 | L | none |
| V2 | Activity timeline | 4.2 | M | privacy decision |
| V2 | Semantic memory lookup | 5.3 | M | an embeddings provider |
| V2 | Grounded retrieval over a folder | 5.4 | M | 4.1 to land with it |
| V2 | A notebooks surface | 8.2 | L | 4.1, 5.4 |
| V2 | Audio Overview (text to speech) | 8.5 | M | 2.1 |
| V2 | Vision into tool results | 9.1 | M | 2.1 |
| V2 | Extension bridge to the user's browser | 9.3 | L | none |
| V2 | Computer use: coordinates and ordering | 9.5 | M | 9.1 |
| V2 | Dictation | 10.3 | M | 2.1 or a local model |
| V3 | Cloud sessions, cross-device sync | - | - | a server |
| V3 | Dispatch or remote control from a phone | - | - | a server |
| V3 | First-party Gmail, Calendar, Drive, Slack connectors | - | - | OAuth apps and a service |
| V3 | Sites, Space, Pages, team collaboration | - | - | a server and multiple users |
| V3 | Excel and PowerPoint add-ins, Slack Tag | - | - | other platforms |

---

## Proposed phase plan: DeskSeek 8 to 13

Continuing `plan.md`'s numbering from Phase 7. Each phase is independently shippable and ordered by
value per unit of effort, not by dependency alone. The house shape applies: one file owns each rule,
a new column is a `CREATE TABLE` entry and a guarded `ALTER`, one runnable check per non-trivial
piece of logic, and the gate is `typecheck` then `check` then `build` then `cargo test` then
`npm run tauri dev`.

### Phase 8: Interop and daily-habit wins (all V0 and V1, no new subsystem) - the confirmed first batch
Order: 6.1 skill interchange, 14.2 skill and memory import, 5.1 incognito chats, 4.3 memory
ergonomics, 7.1 cadences, 7.4 run history, 7.5 schedule tool, 11.1 break reminders and quiet
hours, 11.2 recap, 11.3 linking notifications, 10.5 composer mentions, 12.4 audit export, and
**8.1 edit an artifact in place**, pulled forward because it was named as a daily-use feature and
it needs no new infrastructure.

Why this order: it makes DeskSeek immediately more usable and immediately interoperable, it touches
no new infrastructure, and it produces the reference material (an imported skill, an exported
skill) that later agent sessions will want. It is also the batch that is safest to hand to a
sub-agent, because every item is one file plus its check.

The cloud constraint applies here specifically: an item in this phase must not need a server and
must not foreclose one. Keep queue, task and session state in the database rather than only in
memory.

### Phase 9: Containment and trust
1.3 compaction, 3.1 per-target grants, 3.2 egress policy, 12.3 deletion protection, 13.1 cost
attribution, 1.2 the safety review.

Why: 13.1 must precede 1.2 so a review call's cost is visible, and 1.2 must precede any agent work
that runs unattended.

### Phase 10: Quick entry and the desktop surfaces
10.1 quick-entry window, 10.2 Appshot, 10.4 tray, 9.2 browser console and tabs, 9.4 browser
recording.

Why: the biggest habit change for the least architecture, and 9.2 is the debugging tool every later
browser feature will need.

### Phase 11: Triggers
7.2 event triggers, 7.3 monitors, then 3.3 MCP events.

Why: DeskSeek's scheduler is already good; these three turn it from a clock into an automation
engine, and none of them needs a cloud account.

### Phase 12: Providers, then everything they unblock
2.1 the two provider dialects, 2.2 per-task model, 2.3 effort, 13.2 routing suggestion, then 9.1
vision, 9.5 computer use, 8.5 audio, 10.3 dictation.

Why: this is the phase with the widest blast radius and it should be its own, because a
half-migrated provider layer is worse than a single-provider app. It is cheaper than the teardown
first implied, because the decision of 2026-10-13 removed the per-vendor clients: two dialects,
each one wire shape, and every named endpoint is configuration. Everything after 2.1 in this phase
is a capability that only exists because a model can see or speak, and it is also where the
unknown-price rule above starts to matter.

### Phase 13: Grounded research
4.1 notebooks and sources, 5.4 grounded retrieval, 8.2 the notebook surface, 8.3 study outputs,
8.4 data tables, 5.3 semantic lookup.

Why: the largest single product surface, and it is best built on a settled memory and retrieval
path rather than beside them.

### Kept in reserve
1.1 sub-agents, 9.3 the browser extension bridge, 4.2 the activity timeline, 14.1 share snapshots,
6.2 to 6.4 plugins and recording.

Why: each is either large, privacy-heavy, or best justified by a real week of using the phases
above. 1.1 in particular should not be built before there is a task that needs it.

---

## Deliberately not doing

Named because a plan that lists only what it will build is half a plan:

- **A cloud account, cloud sessions and cross-device sync.** Every competitor's headline feature
  requires a server. DeskSeek's differentiator is that the work, the key and the files stay on the
  machine. Anything that embeds an account is a different product.
- **Multi-user sharing, Spaces, Pages with collaborators, Team Tasks.** Same reason.
- **First-party OAuth connectors to Gmail, Drive, Slack and Microsoft 365.** This is an
  app-registration, verification and secrets-hosting project, not a feature. MCP remains the honest
  route, and a third-party MCP server is the user's choice to install.
- **Excel and PowerPoint add-ins, Slack Tag, a browser extension store, a plugin directory.** Being
  a good citizen of someone else's marketplace is a product in itself.
- **Audio and video generation of the Cinematic variety.** Audio Overview is defensible because it
  is a script plus TTS with a clear per-minute price. Video is a separate media pipeline.
- **Per-hunk diff accept and reject.** Still not doing it, for the reason `plan.md` states.
- **Silent model routing.** A suggestion with an estimate, never a downgrade the user did not see.

---

## Appendix A: parity matrix

How each capability maps across the three reference products and DeskSeek. "Partial" means the
primitive exists and the feature does not.

| Capability | Claude | ChatGPT | Gemini | DeskSeek |
| --- | --- | --- | --- | --- |
| Global-shortcut mini window | Quick entry | Pets | `Alt+Space` overlay | No (10.1) |
| Screenshot the focused window | Mac quick entry | Appshots | Opt-in screen share | Partial (10.2) |
| Dictation / voice input | Caps Lock (Mac) | Voice, files in voice | Gemini Live, voice in overlay | No (10.3) |
| Local folder read and write | Yes | Yes (Codex) | Via desktop app | **Yes** |
| Built-in browser, isolated | Yes | Yes | In Chrome | **Yes** (headless) |
| Drive the user's own browser | Claude in Chrome | Chrome, Edge, Brave, Opera, Vivaldi | Gemini in Chrome | No (9.3) |
| Console and network reading | Yes | Via Codex tooling | - | No (9.2) |
| Computer use | Yes (beta) | Yes | Screen share, Spark | Partial (9.5) |
| Connectors via MCP | Yes | Yes | Yes | **Yes** |
| Connector events / webhooks | - | MCP events | Gmail triggers | No (3.3) |
| Skills as `SKILL.md` | Yes (standard) | Yes | Yes (standard) | **Yes** |
| Skill import and export | Directory | Import from other agents | Copy across platforms | No (6.1) |
| Plugin bundles | Yes | Yes (plugins and extensions) | Enterprise workflows | No (6.2) |
| Record a workflow into a skill | Yes | - | - | No (6.3) |
| Projects with rules and memory | Yes | Yes | Notebooks | **Yes** |
| Memory, editable topics | Yes | Memories | Personal context | Partial (5.2) |
| Incognito chat | Yes | Temporary chat | - | No (5.1) |
| Search past conversations | Yes | Yes | Yes | **Yes** |
| Context compaction | Automatic | Automatic | Yes | Partial (1.3) |
| Versioned artifacts | Yes | Canvas | Canvas | **Yes** |
| Edit an artifact in place | Yes | Yes | Yes | No (8.1) |
| Live artifact that updates | Yes | - | - | No |
| Reports, decks, spreadsheets | Yes | Work outputs | Notebook outputs | **Yes** |
| Flashcards, quizzes, study guides | - | Education plugins | Notebook study tools | No (8.3) |
| Mind maps, infographics | Custom visuals | Visualisations | Notebook Studio | Partial (mermaid, html) |
| Audio Overview | - | - | Yes | No (8.5) |
| Video Overview | - | - | Yes | No |
| Source-grounded answers with citations | Yes | File search | Notebooks (core) | No (5.4) |
| Data tables over sources | - | - | Yes | No (8.4) |
| Scheduled cadence tasks | Yes | Yes | Yes | **Yes** |
| Event-triggered tasks | - | Gmail, Slack, GitHub | Gmail triggers | No (7.2) |
| Monitors (condition) | - | - | Yes | No (7.3) |
| Sub-agents, parallel workstreams | Yes | Yes (Codex) | Yes (Spark) | No (1.1) |
| Approval ladder | Manual, Auto, Skip | Approvals and security | Permission prompts | **Yes** (Ask, Standard, Plan, Agent) |
| Reviewed auto mode | Yes | Yes | - | No (1.2) |
| Declared network egress | Yes | Yes | - | No (3.2) |
| Deletion protection | Yes | Permissions | - | Partial (12.3) |
| Sandboxed execution | Isolated cloud VM | Local and cloud sandboxes | Secure cloud computer | **Yes** (Docker, WSL2, path sandbox) |
| Cost and usage view | Usage limits | Usage and credits | Credits | **Yes** (strongest per-turn) |
| Model choice per surface or task | Yes | Yes | By plan | No (2.1, 2.2) |
| Model effort control | Effort levels | Slider | - | No (2.3) |
| Notifications when it needs you | Yes | Yes | Yes | **Yes** |
| Break reminders and quiet hours | Yes | - | - | No (11.1) |
| Activity timeline | - | Computer History | - | No (4.2) |
| Share a read-only thread | Artifact embeds | Thread snapshots | Notebook sharing | No (14.1) |
| Cloud session, cross-device | Yes | Yes | Yes | **No, by design** |

---

## Appendix B: the standards worth adopting

Three pieces of other people's work are worth designing around rather than reinventing:

1. **`SKILL.md` and Agent Skills.** Anthropic published it as an open standard; Google's
   Workspace team states its skills are "built entirely on the open, Markdown-native `SKILL.md`
   standard" and that this is what allows a skill to be copied between platforms. DeskSeek already
   stores `skills/<id>/SKILL.md`. The work is frontmatter fidelity (an explicit comment on Gemini's
   side that the format carries a name, a description and inline custom instructions), round-trip
   preservation of keys it does not understand, and an importer that refuses an id that would walk
   out of its folder. Cheap now, and it makes the whole skill library portable in both directions.

2. **MCP, including the client-side discipline DeskSeek already has.** The protocol is the
   connector substrate for all three products. DeskSeek's additions that are worth keeping strict:
   every call asks, `readOnlyHint` only skips the prompt when trusted, server output is data and
   never instructions, and servers die with the app. The direction of travel in the wider ecosystem
   is events and richer server-side state, which is 3.3, and it should land without weakening any of
   those four rules.

3. **The Task / Skill / Schedule split.** Gemini states it cleanly and it is the right shape for
   DeskSeek's scheduler: a prompt is what, a skill is how, and a cadence or trigger is when. It maps
   one-to-one onto `schedule.rs` plus `skills.rs`, and it is the vocabulary to use in the UI so the
   panel stops being a form full of fields and starts being three ideas.

One more thing worth saying plainly, because it is the honest reading of the whole document:
the three products are converging on "an agent that acts, on sources you choose, on a schedule you
set, with approvals you control, and a window that is always one shortcut away". DeskSeek already
has four of those five. It is missing the shortcut, and it is missing sources.

---

## Sources

Anthropic:

- Claude release notes: <https://support.claude.com/en/articles/12138966-release-notes>
- Get started with Claude Cowork: <https://support.claude.com/en/articles/13345190-get-started-with-claude-cowork>
- Schedule recurring tasks in Claude Cowork: <https://support.claude.com/en/articles/13854387-schedule-recurring-tasks-in-claude-cowork>
- Use Claude Cowork on web, desktop and mobile: <https://support.claude.com/en/articles/15520349-use-claude-cowork-on-web-desktop-and-mobile>
- Navigating the Claude desktop app (Claude Academy): <https://academy.claude.com/tutorials/navigating-the-claude-desktop-app>
- Schedule recurring tasks in Claude Code Desktop: <https://code.claude.com/docs/en/desktop-scheduled-tasks>

OpenAI:

- What's new in ChatGPT and Codex: <https://learn.chatgpt.com/docs/whats-new>
- ChatGPT Work: <https://chatgpt.com/work/>
- ChatGPT desktop app for Windows: <https://learn.chatgpt.com/docs/windows/windows-app>
- ChatGPT agent: <https://help.openai.com/en/articles/11752874-chatgpt-agent>

Google:

- Skills in the Gemini app and Workspace, and the Gems transition: <https://workspaceupdates.googleblog.com/2026/09/skills-gemini-app-workspace.html>
- Schedule actions in Gemini Apps: <https://support.google.com/gemini/answer/16316416>
- Create and manage schedules for tasks in Gemini Spark: <https://support.google.com/gemini/answer/17094710>
- Gemini Spark overview: <https://gemini.google/overview/agent/spark/>
- Gemini Spark tasks, skills and schedules explained (independent, source-cited): <https://www.aiagentslibrary.com/blog/gemini-spark-tasks-skills-schedules/>
- NotebookLM updates tracker (independent, source-cited): <https://notebooklm-guide.com/notebooklm-updates/>
- Gemini Notebook as a research workspace (independent): <https://notebooklm-guide.com/notebooklm-as-research-workspace/>
- Gemini app for Windows guide (independent): <https://google-gems-blog.lovable.app/blog/gemini-app-windows-launch-guide-2026>

Independent trackers and commentary used only for cross-checking, and flagged as such where quoted:

- <https://cashandcache.substack.com/p/claude-just-had-a-crazy-2026-the>
- <https://www.ai-toolbox.co/claude-management-and-productivity/how-to-install-claude-desktop-computer-use-2026>
- <https://www.digitalocean.com/resources/articles/what-is-notebooklm>
