---
title: "FAQ"
description: "Short answers to the questions people ask first."
---

# FAQ

## What is Atlas, in one sentence?

A dev-environment orchestrator: one `atlas.yaml` starts your infra and services in dependency order, supervises them, and exposes them to a TUI, a desktop app and AI agents.

## How is it different from `docker compose`, `foreman` or `process-compose`?

`docker compose` runs containers; Atlas runs your *processes* (and calls `docker compose up -d` for the infrastructure). Compared with Procfile runners it adds dependency conditions (`healthy`, `completed_successfully`, `log_ready`), restart backoff, health probes, log search, config validation, an MCP server and a desktop app. It does not do file-watch restarts, replica scaling or Windows (yet).

## Does it run on Linux or Windows?

Prebuilt releases are macOS on Apple silicon. The CLI, TUI, runner and MCP server build from source on Linux and their tests run on Ubuntu in CI, but there are no prebuilt Linux binaries and the runtime is mostly exercised on macOS. Windows is not supported. Atlas Desktop is macOS only (its terminal renders with Metal).

## Does Atlas send my data anywhere?

No telemetry. The CLI talks to GitHub to check for and download releases, and `gh` (if you use pull-request features in the desktop app) talks to GitHub as you. Agent hooks post to a loopback server inside the desktop app, token-protected and 127.0.0.1 only. Atlas never ships or reads your env files except to inject them into your own services.

## Where are my logs?

`<project>/.logs/<service>.log`, ANSI stripped. See [Logs](logs.md).

## Why does quitting not run `docker compose down`?

Stopping and starting containers on every session is slow, and on some Macs restarting the container runtime has crashed it. Infra containers are left running (`restart: unless-stopped` containers survive between sessions). Run `docker compose down` when you want them gone.

## Why did Atlas not kill the process on my port?

Atlas only ever signals PIDs it spawned (recorded in `.logs/.pids`) and their descendants. Killing "whatever listens on a port" can hit a container runtime's port forwarder or an unrelated tool, so it is deliberately not done.

## Can services outlive my terminal?

Yes: `atlas dev --detach` starts a background runner. `atlas dev` attaches a TUI, `D` detaches, `atlas down` stops everything.

## Can two worktrees run the same stack?

Yes, in Atlas Desktop. Each worktree gets a port offset (`ATLAS_PORT_OFFSET`, 100, 200, …) that is added to every service port, tunnel port and matching URL. Try it: `ATLAS_PORT_OFFSET=100 atlas config check`.

## Which agents are supported?

omp, Claude Code and Codex have one-command setup (`atlas setup <agent> --apply`). Any MCP client, such as Cursor, can use `atlas mcp` with a manual entry. See [Agent setup](agents.md) and [MCP](mcp.md).

## Why does `atlas dev` refuse to run when an agent starts it?

The hooks that `atlas setup` installs stop agents from launching the interactive TUI, which would hang their shell. They are pointed at `atlas dev --status` and the MCP tools instead. `atlas dev --detach` or `--headless` is fine for an agent.

## Is `atlas.yaml` compatible between versions?

Within a major version, yes. New fields are additive; unknown fields are errors, so regenerate the JSON Schema after updating (`atlas config schema > .atlas.schema.json`). See the [changelog](https://github.com/codeatlasdev/atlas/blob/main/CHANGELOG.md).

## Can I use my own terminal theme and shortcuts in the desktop app?

Yes. Themes use the Zed theme format (drop files in `~/.atlas/desktop/themes/`), and every shortcut is a named command you can rebind in Settings → Keyboard shortcuts or under `keymap` in `~/.atlas/desktop/settings.json`.

## How do I contribute?

Read [Contributing](contributing.md). Bugs and ideas go to the [issue tracker](https://github.com/codeatlasdev/atlas/issues).
