---
title: "Introduction"
description: "What Atlas is, what it replaces and where to start."
---

# Introduction

Atlas is a developer platform for everyone: the paved road from `git clone` to a running, observable stack, without needing a platform team. It boots your project's whole development environment from one `atlas.yaml`: docker infrastructure, app services in dependency order, restarts with backoff, health checks and searchable logs. It gives you three ways to drive it, and they all see the same live state:

- **`atlas dev`**: a terminal dashboard (the [TUI](tui.md)).
- **Atlas Desktop**: a native macOS app with a GPU terminal, git worktrees, coding agents, diff review and an embedded browser.
- **`atlas mcp`**: a [Model Context Protocol](mcp.md) server, so an AI agent can read status and logs, restart a service and wait until it is healthy while you keep working.

> **Status.** Atlas is open source (MIT or Apache-2.0). Prebuilt releases target macOS on Apple silicon (the CLI, the TUI, the MCP server and Atlas Desktop). The CLI builds from source on Linux and its tests run on Ubuntu in CI, but Linux is not a release target yet.

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
| Logs | Robust line assembly, bounded buffers, error/warning jumps, clickable URLs and `path:line` | [Logs](logs.md) |
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
