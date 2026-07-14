# Atlas

Native macOS app for developers. Manage servers, deploy services, and chat with AI agents — all from one place.

## Architecture

```
┌─────────────────────────────────────────────────────┐
│  SwiftUI App (app/)                                 │
│  • Native macOS 14+ / Apple Silicon                 │
│  • Sidebar + Detail navigation                      │
│  • Menu bar status indicator                        │
│  • Terminal, AI chat, server dashboard              │
└─────────────────────┬───────────────────────────────┘
                      │ Unix Domain Socket (~/.atlas/atlas.sock)
                      │ Protocol: newline-delimited JSON-RPC
┌─────────────────────▼───────────────────────────────┐
│  Rust Daemon (crates/atlas-daemon)                  │
│  • Always-on background process (launchd agent)     │
│  • SSH connections to remote servers                │
│  • systemd service management                       │
│  • AI provider routing                              │
│  • SQLite state (WAL mode)                          │
└─────────────────────────────────────────────────────┘
│  CLI (crates/atlas-cli)                             │
│  • Same Unix socket protocol                        │
│  • Terminal-first workflows                         │
└─────────────────────────────────────────────────────┘
```

## Project Structure

```
atlas/
├── crates/
│   ├── atlas-core/       # Domain types, error types, port traits
│   ├── atlas-db/         # SQLite (sqlx), migrations, repositories
│   ├── atlas-ssh/        # SSH client (russh), session management
│   ├── atlas-ai/         # AI provider abstraction (Claude, GPT, Ollama)
│   ├── atlas-server/     # Server/service management (systemd, caddy)
│   ├── atlas-daemon/     # Main daemon binary (socket server)
│   └── atlas-cli/        # CLI binary (thin socket client)
├── app/
│   ├── Package.swift     # SwiftUI app (macOS 14+)
│   └── Sources/Atlas/    # Views, Models, Services, Theme
├── migrations/           # SQL migrations (applied by atlas-db)
├── Cargo.toml            # Rust workspace root
└── rust-toolchain.toml
```

## Design Principles

- **Clean Architecture**: domain types and port traits in `atlas-core`, adapters in specific crates
- **Single Responsibility**: each crate has one job
- **Dependency Rule**: `atlas-core` depends on nothing; `atlas-daemon` depends on everything
- **Thin CLI**: just a socket client; all logic lives in the daemon
- **No god files**: every module is small, focused, and testable
- **Durable facts, derived status**: store minimal state in SQLite, compute the rest

## Requirements

- macOS 14+ (Sonoma)
- Apple Silicon (arm64)
- Rust 1.87+ (`rustup` will handle this via `rust-toolchain.toml`)
- Xcode 15+ (for SwiftUI app)

## Development

### Rust daemon + CLI

```bash
# Build everything
cargo build

# Run the daemon
cargo run --bin atlas-daemon

# Run CLI commands
cargo run --bin atlas-cli -- servers list
```

### SwiftUI app

```bash
cd app

# Build
swift build

# Open in Xcode
open Package.swift

# Run tests
swift test
```

### Full workflow

1. Start the daemon: `cargo run --bin atlas-daemon`
2. Open the app in Xcode (or `swift build && .build/debug/Atlas`)
3. The app connects to `~/.atlas/atlas.sock` automatically

## Communication Protocol

Newline-delimited JSON-RPC over Unix Domain Socket.

**Request:**
```json
{"method": "servers.list", "params": {}, "id": "req-1"}
```

**Response:**
```json
{"id": "req-1", "result": [...], "error": null}
```

**Methods:**
- `servers.list` / `servers.add` / `servers.remove` / `servers.status`
- `services.list` / `services.restart`
- `sessions.list`
- `ai.chat`

## Tech Stack

| Layer | Technology |
|-------|-----------|
| App UI | SwiftUI (macOS 14+, @Observable) |
| Daemon | Rust (tokio, axum patterns) |
| SSH | russh (pure Rust, async) |
| Database | SQLite WAL (sqlx) |
| AI | reqwest → Claude/GPT/Ollama APIs |
| IPC | Unix Domain Socket + JSON-RPC |
| CLI | clap (thin socket client) |

## License

MIT
