# AgentDeck

[![License](https://img.shields.io/github/license/lepfinder/AgentDeck?style=flat-square)](LICENSE)
[![Release](https://img.shields.io/github/v/release/lepfinder/AgentDeck?style=flat-square)](https://github.com/lepfinder/AgentDeck/releases/latest)
[![Platform](https://img.shields.io/badge/platform-macOS%2014%2B-007AFF?style=flat-square)](https://github.com/lepfinder/AgentDeck/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/lepfinder/AgentDeck/total?style=flat-square)](https://github.com/lepfinder/AgentDeck/releases)
[![Stars](https://img.shields.io/github/stars/lepfinder/AgentDeck?style=flat-square)](https://github.com/lepfinder/AgentDeck/stargazers)
[![CI](https://img.shields.io/github/actions/workflow/status/lepfinder/AgentDeck/ci.yml?branch=main&label=CI&style=flat-square)](https://github.com/lepfinder/AgentDeck/actions/workflows/ci.yml)

[中文文档](./README_CN.md)

Collect every coding-agent session and message on your machine into one place — manage, search, and analyze them from a single native desktop app. Built with **Tauri 2 + Rust + React**.

Cursor, Antigravity, Claude Code, Codex, Hermes, and WorkBuddy each keep their own logs in separate directories. AgentDeck syncs them read-only into SQLite, mirrors media locally, and exposes a loopback REST API so other agents can query your history too.

![AgentDeck dashboard — stats, activity charts, and contribution heatmap](imgs/dashboard.png)

---

[Features](#features) | [AI Agent Friendly](#ai-agent-friendly) | [Download](#download) | [Supported Agents](#supported-agents) | [Quick Start](#quick-start) | [Local API](#local-api) | [Data](#data)

---

## Features

- **Panoramic dashboard** -- session counts, user prompts, message totals, tool-call stats; breakdowns by source, time of day, and workspace.
- **Three-pane session browser** -- workspace sidebar, conversation cards, Markdown message stream; starred favorites and quick filters.
- **Spotlight search (⌘K)** -- full-text search across conversations with role filters and jump-to-match navigation.
- **Prompt library** -- collect external prompts independently (categories, tags, source notes); not tied to a single session.
- **Project analysis** -- per-workspace retrospective reports and feature-module extraction powered by configurable LLM providers.
- **Open in AI IDE** -- launch Cursor / Antigravity for the selected workspace, or start Claude Code / Codex in Terminal.
- **Loopback REST API** -- `127.0.0.1:8788`, no token, localhost only; interactive docs at `/docs`.
- **Backup & media archive** -- SQLite hot snapshots, media mirrored to `~/.agentdeck/media/`.
- **i18n & themes** -- English + Chinese UI; dark / light theme toggle.

![Spotlight search across all local AI sessions](imgs/search.png)

---

## AI Agent Friendly

AgentDeck is not just a viewer -- it exposes a **standard localhost REST API** so other AI agents can plug in and analyze your coding history in real time.

- **Loopback-only, zero friction** -- `127.0.0.1:8788`, no token, no cloud; any local agent, script, or MCP tool can call it directly.
- **Read + write where it matters** -- query stats, workspaces, conversations, full-text search, and flat user-message exports; push prompts into the library via `POST /api/prompts`.
- **Live data, not a static export** -- trigger `POST /api/sync` to refresh the archive, then pull the latest sessions for dynamic retrospectives and cross-project summaries.
- **Self-describing docs** -- interactive playground at `/docs`; export the full Markdown spec via `GET /api/docs/markdown`.

Wire the endpoints into your assistant as HTTP tools or MCP functions -- for example, list today's active workspaces, search recent prompts, and synthesize a daily development review across Cursor, Antigravity, and Claude Code sessions.

![An external AI agent calling AgentDeck APIs to produce a real-time daily development retrospective](imgs/ai-agent.png)

Typical agent workflows:

| Goal | Endpoints |
|---|---|
| Global overview | `GET /api/stats` |
| List / filter projects | `GET /api/workspaces`, `GET /api/workspaces/detail` |
| Pull user prompts for analysis | `GET /api/user-messages`, `GET /api/search` |
| Refresh before analysis | `POST /api/sync` |
| Save reusable prompts | `POST /api/prompts`, `GET /api/prompts` |

See [Local API](#local-api) below for endpoint details.

---

## Supported Agents

AgentDeck watches local agent data directories and syncs them into its own SQLite archive. All parsing is read-only against source files.

| Agent | Role in AgentDeck |
|---|---|
| Cursor | Full session + message sync; image attachments archived locally |
| Antigravity | Full session + message sync; image attachments archived locally |
| Claude Code | Session history sync |
| Codex | Session history sync |
| Hermes | Session history sync |
| WorkBuddy | Session history sync |

Data stays on your machine under `~/.agentdeck/`. AgentDeck does not upload sessions to the cloud.

---

## Download

Prebuilt installers are published on [GitHub Releases](https://github.com/lepfinder/AgentDeck/releases/latest).

### macOS

1. Download **`AgentDeck-<version>-macos-aarch64.zip`** (Apple Silicon) or **`-macos-x64.zip`** (Intel)
2. Unzip and drag **AgentDeck.app** to Applications

The app is ad-hoc signed. macOS Gatekeeper may block the first launch: **right-click → Open → Open**, or run:

```bash
xattr -d com.apple.quarantine /Applications/AgentDeck.app
```

**Requirements:** macOS 14+ (Apple Silicon or Intel build below)

Prefer building locally? See [Quick Start](#quick-start).

---

## Quick Start

### Prerequisites

| Tool | Notes |
|---|---|
| macOS | 14+ (Apple Silicon or Intel) |
| Node.js | 18+ recommended |
| Rust | Stable toolchain + Tauri 2 system deps |
| Xcode | Command Line Tools |

### Run from source

```bash
git clone https://github.com/lepfinder/AgentDeck.git
cd AgentDeck
npm install
npm run tauri dev
```

The REST API starts with the app:

- Health check: `http://127.0.0.1:8788/health`
- Interactive docs: `http://127.0.0.1:8788/docs`

### Build a release

```bash
npm run tauri build
```

Artifacts land in `src-tauri/target/release/bundle/`. Before releasing, bump the version in all three places:

1. `package.json`
2. `src-tauri/Cargo.toml`
3. `src-tauri/tauri.conf.json`

Also update `CHANGELOG.md`. The build runs `npm run generate:api-docs` to refresh `src-tauri/src/api_docs.generated.md`.

### Publish a release (maintainers)

Pushing a **`v*`** tag triggers [`.github/workflows/release.yml`](.github/workflows/release.yml) to build macOS packages (Apple Silicon + Intel) and create a GitHub Release with install notes (including the macOS `xattr` command).

```bash
# 1. Bump version in package.json, src-tauri/Cargo.toml, src-tauri/tauri.conf.json
# 2. Update CHANGELOG.md
git add -A && git commit -m "chore: release v0.2.7"
git tag v0.2.7
git push origin main --tags
```

Every push to `main` and every PR also runs [`.github/workflows/ci.yml`](.github/workflows/ci.yml) (lint, frontend build, `cargo check`).

---

## Tech Stack

- **Desktop:** Tauri 2 (Rust)
- **Frontend:** React + TypeScript + Vite + Tailwind CSS
- **Storage:** SQLite (rusqlite) + FTS5 full-text search

---

## Local API

The HTTP server listens on **`127.0.0.1:8788` only** -- no auth token, no external network exposure.

**Prompt library workflow (typical for agents):**

1. `GET /api/prompts/categories` -- category candidates (`category` must use returned `value`)
2. `GET /api/prompts` -- search / dedupe (list returns `content_preview` only)
3. `POST /api/prompts` -- insert (`title` required)
4. `DELETE /api/prompts/:id` -- delete

Read-only session endpoints include `/api/stats`, `/api/workspaces`, `/api/conversations`, `/api/user-messages`, and `/api/search`.

Full reference: open `/docs` in the running app, or `GET /api/docs/markdown` for the Markdown spec.

---

## Data

| Path | Contents |
|---|---|
| `~/.agentdeck/agentdeck.db` | SQLite archive (sessions, messages, FTS index, prompts) |
| `~/.agentdeck/media/` | Mirrored conversation images |
| `~/.agentdeck/config.json` | App config (backup paths, sync settings) |

Do not commit local databases, backup archives, or absolute user paths into the repository.

---

## Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| `⌘K` / `Ctrl+K` | Open Spotlight search |
| `⌘R` / `Ctrl+R` | Trigger sync |
| `⌘,` / `Ctrl+,` | Open settings |

## Star History

[![Star History Chart](https://api.star-history.com/chart?repos=lepfinder/AgentDeck&type=date&legend=top-left)](https://www.star-history.com/?repos=lepfinder%2FAgentDeck&type=date&legend=top-left)

## License

[MIT](LICENSE)
