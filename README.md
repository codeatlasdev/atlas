<p align="center">
  <img src="assets/icon.svg" width="80" height="80" alt="Atlas">
</p>

<h1 align="center">Atlas</h1>

<p align="center">
  <strong>Development environment orchestrator for macOS</strong><br>
  <em>One command. All your services. Built in Rust.</em>
</p>

<p align="center">
  <a href="#install">Install</a> •
  <a href="#features">Features</a> •
  <a href="#usage">Usage</a> •
  <a href="#architecture">Architecture</a> •
  <a href="#development">Development</a>
</p>

---

## Overview

Atlas is a terminal-first development tool that manages your entire local environment — services, databases, tunnels, and infrastructure — from a single `atlas.yaml` config.

Think **docker-compose for your dev workflow**, but native, fast, and with a beautiful TUI.

```bash
atlas dev
```

That's it. Atlas reads your project config, starts services in dependency order, monitors health, streams logs, and gives you a polished interactive dashboard — all in your terminal.

## Install

**Homebrew** (recommended):
```bash
brew tap codeatlas/tap
brew install atlas
```

**Shell installer**:
```bash
curl -fsSL https://atlas.dev/install.sh | sh
```

**From source** (requires Rust 1.87+):
```bash
git clone https://github.com/codeatlasdev/atlas
cd atlas
cargo build --release
```

## Features

### TUI Dashboard

A full terminal application — not a script. Keyboard-driven with mouse support.

- **Real-time service status** with colored indicators
- **Aggregated log viewer** with per-service filtering
- **Tab navigation** between services (0-6, Tab)
- **Health monitoring** (HTTP, TCP port, process liveness)
- **Toast notifications** with animated spinners
- **Command palette** (`:` key) with fuzzy search
- **Help modal** (`?` key) with contextual shortcuts
- **Responsive layout** — adapts to terminal size

### Intelligent Log Management

Logs are first-class. Designed for the AI-assisted development workflow.

| Key | Action | Description |
|-----|--------|-------------|
| `L` | Copy Logs | Last 3 minutes, formatted with timestamps and service headers |
| `P` | Copy for AI | Pre-formatted prompt for Claude/GPT — includes error context, surrounding lines, and a help request |
| `E` | Copy Errors | Only errors/warnings with service attribution |

The `P` shortcut generates output like:

```
I'm working on a development environment and encountering issues.
Here are the recent logs from my services:

Project: myapp
Services: api, web, worker

>>> [api] Error: Cannot read property 'id' of undefined
>>> [api]     at UserController.get (/app/src/user.ts:42)

Can you help me understand what's going wrong and how to fix it?
```

Paste directly into any AI assistant. Zero effort.

### Headless Mode

For CI, scripts, and automation:

```bash
atlas dev --headless
```

Starts all services, streams logs to stdout with ANSI colors, and shuts down cleanly on Ctrl+C.

### Self-Update

```bash
atlas self-update              # Update to latest
atlas self-update --check      # Check without installing
atlas self-update --channel beta   # Switch channels
```

Detects your installation method (Homebrew, shell installer, or manual) and acts accordingly.

## Usage

### Configuration

Create `atlas.yaml` in your project root:

```yaml
name: myapp

tunnel:
  enabled: true
  local_port: 54320
  remote_host: localhost
  remote_port: 5432
  ssh_host: prod-db

services:
  redis:
    command: docker compose up -d redis
    port: 6379
    critical: true

  api:
    command: bun run dev:api
    port: 3000
    health: http://localhost:3000/health
    depends_on: [redis]

  web:
    command: bun run dev:web
    port: 3001
    depends_on: [api]

  worker:
    command: bun run dev:worker
    depends_on: [redis]

infra:
  compose_file: docker-compose.yml
```

Local overrides (gitignored):

```yaml
# atlas.local.yaml
tunnel:
  enabled: false
services:
  web:
    port: 3002
```

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `q` | Quit (with confirmation) |
| `?` | Help |
| `r` | Restart all services |
| `:` | Command palette |
| `Tab` / `Shift+Tab` | Next/prev tab |
| `0-6` | Jump to tab |
| `j` / `k` | Scroll logs |
| `G` / `g` | Jump to bottom/top |
| `L` | Copy logs to clipboard |
| `P` | Copy logs as AI prompt |
| `E` | Copy errors only |

### CLI Commands

```bash
atlas dev                    # Start dev TUI
atlas dev --headless         # Start without TUI
atlas dev --dir ./my-project # Specify project directory
atlas self-update            # Update Atlas
atlas ping                   # Check daemon connection
atlas server list            # List managed servers
atlas deploy run             # Deploy to production
```

## Architecture

Atlas is a Rust workspace with clean separation of concerns:

```
atlas/
├── crates/
│   ├── atlas-core/       # Domain types, traits, zero dependencies
│   ├── atlas-tui/        # TUI application (ratatui + crossterm)
│   │   ├── config/       # atlas.yaml parser + merge
│   │   ├── runtime/      # Process lifecycle, health, tunnel, docker
│   │   ├── theme/        # Semantic color tokens
│   │   ├── event/        # Async event loop (tokio + crossterm)
│   │   └── tui/          # TEA state machine, views, widgets
│   ├── atlas-cli/        # Binary: `atlas` command
│   ├── atlas-daemon/     # Background daemon (socket server)
│   ├── atlas-db/         # SQLite persistence
│   ├── atlas-ssh/        # SSH client (session management)
│   ├── atlas-ai/         # AI provider abstraction
│   ├── atlas-server/     # Remote server management
│   ├── atlas-agent/      # AI agent lifecycle (ACP protocol)
│   ├── atlas-terminal/   # PTY session management
│   ├── atlas-memory/     # Knowledge base (vector + graph)
│   ├── atlas-plugin/     # Plugin system (TOML manifests)
│   └── atlas-mcp/        # MCP bridge binary
├── app/                  # SwiftUI desktop app (macOS 14+)
├── scripts/              # Install + CI scripts
├── Formula/              # Homebrew formula
└── .github/workflows/    # CI/CD
```

### Design Principles

- **Single binary** — `atlas` does everything (dev, deploy, manage)
- **Config-driven** — `atlas.yaml` is the source of truth
- **Daemon optional** — `atlas dev` works standalone, daemon adds remote features
- **TEA pattern** — Elm Architecture for testable, predictable UI state
- **Async-first** — tokio throughout, channels for communication
- **Zero-cost DX** — instant startup, no runtime dependencies

### Tech Stack

| Component | Technology |
|-----------|-----------|
| Language | Rust (edition 2024, 1.87+) |
| TUI | ratatui 0.30 + crossterm 0.28 |
| Async | tokio (full features) |
| Config | serde_yaml |
| Desktop | SwiftUI (macOS 14+, Apple Silicon) |
| IPC | Unix Domain Socket + JSON-RPC |
| Database | SQLite WAL (sqlx) |
| AI | reqwest → Claude/GPT/Ollama |
| SSH | russh (pure Rust, async) |
| Memory | redb (embedded key-value) |

## Development

### Requirements

- macOS 14+ (Sonoma) or later
- Rust 1.87+ (via `rustup`, handled by `rust-toolchain.toml`)
- Xcode 15+ (for SwiftUI desktop app)

### Build

```bash
# Everything
cargo build

# Just the CLI + TUI
cargo build -p atlas-cli -p atlas-tui

# Run tests
cargo test -p atlas-tui

# Run with clippy
cargo clippy -p atlas-tui -- -D warnings
```

### Test Coverage

```
74 tests (atlas-tui) + 5 tests (atlas-cli) = 79 total

config/         9 tests — parser, merge, find_root, defaults
runtime/       18 tests — health, lock, manager, tunnel, docker, deps, logs
theme/          2 tests — token coverage
event/          3 tests — channel, tick, debug
tui/app        19 tests — TEA messages, layers, scroll, palette, mouse
tui/widgets     3 tests — command palette, spinner
tui/views       2 tests — quit modal rendering
headless        1 test  — config requirement
cli/self_update 5 tests — platform, install method, version compare
```

### Project Status

Atlas is in active development. The CLI/TUI is functional and ready for daily use. The desktop app and remote features (deploy, AI agents) are in progress.

| Component | Status |
|-----------|--------|
| CLI + TUI | ✅ Production-ready |
| Config system | ✅ Complete |
| Process management | ✅ Complete |
| Health monitoring | ✅ Complete |
| Log management | ✅ Complete |
| Self-update | ✅ Complete |
| SSH tunnel | ✅ Complete |
| Headless mode | ✅ Complete |
| Desktop app | 🚧 In progress |
| Remote deploy | 🚧 In progress |
| AI agents | 🚧 In progress |

## License

MIT
