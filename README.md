# DeskSeek

A lightweight desktop client for the DeepSeek API — built with **Tauri v2** (Rust core) and
**React + TypeScript**.

Two sections share one core:

- **Chat** — a general assistant with streaming replies, a collapsible *thinking* pane, and a
  global `memory.md` that is always injected into the system prompt.
- **Coding** — an agent that works inside a folder you trust (planned; see *Status*).

Bring your own API key: it is stored in the **OS keychain** (Credential Manager / Keychain /
Secret Service), never in a config file and never in a log.

## Status

Built so far (milestone 1):

- Streaming chat against `https://api.deepseek.com` with the `deepseek-flash` and
  `deepseek-v4-pro` models
- Per-conversation model, **thinking mode** toggle and effort (`low` / `high` / `max`)
- Reasoning rendered in a collapsible block; Markdown + syntax-highlighted code
- Local conversation history in SQLite; chats scoped per section
- API key in the OS keychain, with a *Test connection* button
- `memory.md` appended to the system prompt when present

Planned, in order: memory proposals at chat end → skills → MCP plugins → the coding agent
(files, shell, git — every write and command approved).

## Requirements

- [Node.js](https://nodejs.org/) 20+ and npm
- [Rust](https://rustup.rs/) (stable) — Tauri needs the platform toolchain (MSVC on Windows)
- Windows 10/11, macOS, or Linux. **Windows is the only officially tested target today**;
  macOS and Linux build in CI but are experimental.

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
cd src-tauri && cargo test
```

## Architecture

```
src/                     React UI (App, Sidebar, ChatView, SettingsDialog, Markdown)
src-tauri/src/
  api/                   DeepSeek client, SSE parsing, delta accumulation, model pricing
  conversation/          message-array assembly (owns the reasoning-content rule)
  store/                 SQLite schema, settings JSON, OS keychain, paths
  commands.rs            the Tauri command surface
```

Notable invariants (covered by tests):

- `reasoning_content` must be echoed back on prior assistant turns **whenever the request
  carries `tools`** — otherwise the API returns 400.
- `tool_choice: required`/named is rejected in thinking mode.
- Model prices are half off-peak; peak is 01:00–04:00 and 06:00–10:00 UTC, Mon–Fri.

## License

MIT — see [LICENSE](LICENSE).
