<p align="center">
  <img src="assets/icon.svg" width="80" height="80" alt="Atlas">
</p>

<h1 align="center">Atlas</h1>

<p align="center">
  <strong>TUI dev environment orchestrator for macOS</strong><br>
  <em>One command. All your services. Built in Rust.</em>
</p>

<p align="center">
  <a href="#install">Install</a> •
  <a href="#usage">Usage</a> •
  <a href="#configuration">Configuration</a> •
  <a href="#development">Development</a>
</p>

---

Atlas reads `atlas.yaml`, starts your services in dependency order, monitors health, streams logs, and gives you an interactive terminal dashboard — all in a single binary.

```bash
atlas dev
```

## Install

**Homebrew** (recommended):
```bash
brew tap codeatlasdev/tap
brew install atlas
```

**Shell installer**:
```bash
curl -fsSL https://atlas.codeatlas.com.br/install.sh | sh
```

**From source** (requires Rust 1.87+):
```bash
git clone https://github.com/codeatlasdev/atlas
cd atlas
cargo build --release -p atlas-cli
# binary: target/release/atlas
```

**Self-update** (if already installed):
```bash
atlas self-update
```

## Usage

```bash
atlas dev                        # Start TUI dashboard
atlas dev --headless             # Headless mode (CI / scripts)
atlas dev --dir ./my-project     # Specify project root
atlas init                       # Scaffold atlas.yaml for current directory
atlas self-update                # Update to latest stable
atlas self-update --check        # Check without installing
atlas self-update --channel beta # Switch to beta channel
```

### TUI keyboard shortcuts

| Key | Action |
|-----|--------|
| `q` | Quit (with confirmation) |
| `?` | Help |
| `r` | Restart all services |
| `:` | Command palette |
| `Tab` / `Shift+Tab` | Next / prev service tab |
| `0`–`9` | Jump to tab by index |
| `j` / `k` | Scroll logs down / up |
| `G` / `g` | Jump to bottom / top |
| `L` | Copy last 3 min of logs to clipboard |
| `P` | Copy logs formatted as an AI prompt |
| `E` | Copy only errors to clipboard |

### Headless mode

For CI pipelines and shell scripts — no TUI, just structured log output:

```bash
atlas dev --headless
```

Starts all services, streams stdout/stderr to terminal, exits cleanly on `Ctrl+C` or when all services stop.

## Configuration

Create `atlas.yaml` in your project root (or run `atlas init` to scaffold one):

```yaml
name: myapp

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
```

### Service fields

| Field | Type | Description |
|-------|------|-------------|
| `command` | string | Shell command to run |
| `port` | number | Port for TCP health probe |
| `health` | string | HTTP URL for health checks |
| `depends_on` | list | Services that must start first |
| `critical` | bool | Failure stops all services |
| `enabled` | bool | Set `false` to skip (default `true`) |

### Local overrides

Create `atlas.local.yaml` (add to `.gitignore`) to override settings per-machine without touching the shared config:

```yaml
# atlas.local.yaml
services:
  web:
    port: 3002
  redis:
    enabled: false
```

### SSH tunnel

Forward a remote port locally (useful for staging databases):

```yaml
tunnel:
  enabled: true
  local_port: 54320
  remote_host: localhost
  remote_port: 5432
  ssh_host: prod-db        # entry in ~/.ssh/config
```

### Infrastructure

```yaml
infra:
  compose_file: docker-compose.yml   # started before services
```

## Log management

Logs are first-class. Three clipboard shortcuts cover the main workflows:

| Key | Output |
|-----|--------|
| `L` | Last 3 minutes, all services, with timestamps and service headers |
| `P` | AI-ready prompt — includes project name, services, and error context |
| `E` | Errors and stderr only, with service attribution |

The `P` output pastes directly into Claude, ChatGPT, or any AI assistant.

## Architecture

```
atlas/
├── crates/
│   ├── atlas-core/   # Domain types (config, error, project)
│   ├── atlas-tui/    # TUI + runtime
│   │   ├── config/   # atlas.yaml loader, local overrides, defaults
│   │   ├── runtime/  # Process lifecycle, health probes, tunnel, docker, logs
│   │   ├── theme/    # Semantic color tokens (CodeAtlas palette)
│   │   ├── event/    # Async event loop (tokio + crossterm)
│   │   └── tui/      # TEA state machine, views, widgets
│   └── atlas-cli/    # `atlas` binary (clap)
├── scripts/          # install.sh, generate-manifest.py
├── Formula/          # Homebrew formula
└── .github/          # CI/CD (test → build → universal → release → brew)
```

**Design:**
- Single binary — `atlas` does everything
- Config-driven — `atlas.yaml` is the source of truth
- TEA pattern — Elm Architecture for predictable UI state
- Async-first — tokio throughout, channels between runtime and UI
- Zero runtime deps — no daemon, no background process

**Tech stack:**

| Component | Technology |
|-----------|-----------|
| Language | Rust (edition 2024, 1.87+) |
| TUI | ratatui 0.30 + crossterm 0.28 |
| Async | tokio (full features) |
| Config | serde_yaml |
| Build | Cargo workspace |

## Development

**Requirements:** macOS, Rust 1.87+ (managed by `rust-toolchain.toml`)

```bash
# Build
cargo build -p atlas-cli

# Run tests
cargo test -p atlas-tui -p atlas-cli -p atlas-core

# Lint (strict)
cargo clippy -p atlas-tui -p atlas-cli -p atlas-core --all-targets -- -D warnings

# Format
cargo fmt --all
```

**Test coverage (84 tests):**

```
atlas-tui (77):
  config/    9   — loader, merge, find_root, defaults
  runtime/  21   — health, lock, manager (crash/dedup/restart), tunnel, docker, deps, logs
  theme/     2   — token coverage
  event/     3   — channel, tick, debug
  tui/app   27   — TEA messages, layers, scroll, palette, mouse, stderr stream
  widgets/   3   — command palette, spinner
  views/     2   — quit modal rendering
  headless/  1   — config requirement

atlas-cli  (6):
  self_update  — platform detection, install method, version compare

atlas-core (1):
  domain       — project types
```

## License

MIT
