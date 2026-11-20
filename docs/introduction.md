---
title: "Introduction"
description: "What Atlas is: AI agents doing real work in their own worktrees and crews, with a person keeping the final say. Where to start."
---

# Introduction

Atlas is a free, open-source workspace for working with AI agents. It runs your projects, gives each agent its own copy of the code (a git worktree, with its own ports and terminal), and keeps a crew of agents working on a Linux server you own, day and night. Anything important waits for a person's approval, and every step is written down.

The picture we use: crossing an ocean takes a **map**, a **ship** and a **crew**, with a **captain** who answers for the result.

| | In Atlas | Read |
|---|---|---|
| The map | How each project works, written once in `atlas.yaml` (or detected without one), readable by people and agents | [Configuration](configuration.md) |
| The ships | Your computers and servers: Atlas Desktop and the CLI on your machine, a crew server on Linux, the phone app | [Desktop app](desktop.md), [Atlas Crew](crew.md), [Mobile](mobile.md) |
| The crew | The agents you already use (Claude Code, Codex, omp, OpenCode), each in its own worktree, or as named crewmates with a role, a queue and memory | [Agent setup](agents.md), [Atlas Crew](crew.md) |
| The captain | You: permission modes, approval cards, an audit log of what each agent did | [Atlas Crew](crew.md#tasks-and-approvals) |

Where it is going is called **Grand Line**: every project an island, every computer a ship, every crew with a captain, across a whole company. What exists today is below; plans are tracked on [GitHub](https://github.com/codeatlasdev/atlas/issues).

You drive the same live state from three places:

- **Atlas Desktop**: a native app for macOS, Windows and Linux with projects, terminals (GPU-drawn on macOS), git worktrees, coding agents, Atlas Crew, diff review and an embedded browser.
- **`atlas dev`**: a terminal dashboard (the [TUI](tui.md)), and `--headless` / `--ci` for scripts.
- **`atlas mcp`**: a [Model Context Protocol](mcp.md) server, so an agent can read status and logs, restart a service and wait until it is healthy.

> **Status.** Atlas is open source (MIT or Apache-2.0) and in alpha. Available now: macOS on Apple silicon, Windows 10/11 x64 and Linux x86_64 (the CLI, the TUI, the MCP server and Atlas Desktop; [download](https://atlas.codeatlas.com.br/download/)); crew servers on Linux x86_64 and aarch64. Coming soon: Intel Macs, Windows on Arm and Linux on Arm desktops.

## The problem it solves

A modern monorepo needs Postgres, Redis, a migration, an API, a web app and a worker, started in the right order, with the right ports, and restarted when they crash. The usual answers are a pile of `sleep 10 && bun dev`, a `Procfile` with no health checks, or a README nobody reads.

With Atlas you write the graph once:

```yaml
name: myapp

infra:
  compose_file: docker-compose.yml

services:
  migrate:
    command: bun run db:migrate
    oneshot: true
  api:
    command: bun run dev:api
    port: 3000
    health: http://localhost:3000/health
    depends_on: { migrate: completed_successfully }
    restart: on_failure
  web:
    command: bun run dev:web
    port: 3001
    depends_on: { api: healthy }
```

and run `atlas dev`. Postgres and Redis come up, the migration runs first, the API waits until `/health` answers, the web app waits for the API, and crashy services restart with exponential backoff. Every log line goes to the dashboard and to `.logs/` on disk.

## What is in the box

| Piece | What it does | Read |
|---|---|---|
| Runtime | Dependency conditions (`started`, `healthy`, `completed_successfully`, `log_ready`), restart policy, exact-PID orphan cleanup, SSH tunnel, actions with confirm gates | [Runtime](runtime.md) |
| TUI | Tabs per service, log search with regex, level and time filters, JSON log compaction, mouse support, command palette | [TUI](tui.md) |
| Logs | Line assembly that survives bursts, bounded buffers, error/warning jumps, clickable URLs and `path:line` | [Logs](logs.md) |
| Atlas Crew | Crewmates on your own Linux server: permission modes, approvals, automations (schedules, GitHub, webhooks), Telegram, voice | [Atlas Crew](crew.md) |
| Desktop | Spaces per project, a worktree per task, persistent terminals, agent status and notifications, review, PRs, browser | [Desktop app](desktop.md) |
| GPU terminal | Metal renderer at 120 Hz, Kitty graphics and sixel, ligatures, shell integration | [GPU terminal](terminal.md) |
| MCP | Nine tools, three resources, three prompts | [MCP](mcp.md) |
| Agent setup | `atlas setup omp\|claude-code\|codex` wires MCP, skills and hooks | [Agent setup](agents.md) |
| Deploy | Atomic, health-gated releases with automatic rollback | [Deploy](deploy.md) |

## Where to start

1. [Install Atlas](installation.md), from a release or from source.
2. Follow the [Quickstart](quickstart.md): `atlas init`, edit, `atlas dev`.
3. Skim [Concepts](concepts.md) to learn the vocabulary (projects, spaces, worktrees, services, actions, hostd).
4. Look things up in [Configuration](configuration.md) and the [CLI reference](cli.md).

Something broken? See [Troubleshooting](troubleshooting.md) and the [FAQ](faq.md). Want to hack on Atlas itself? See [Architecture](architecture.md) and [Contributing](contributing.md).
