---
title: "Quickstart"
description: "From zero to a running stack in five minutes."
---

# Quickstart

You need Atlas on your `PATH` ([Installation](installation.md)) and a project with something to run.

## 1. Scaffold `atlas.yaml`

In your project root:

```bash
atlas init
```

Atlas detects the project type and writes a minimal `atlas.yaml`. It never overwrites an existing one. Edit it to describe what you run:

```yaml
# yaml-language-server: $schema=.atlas.schema.json
name: myapp

services:
  database:             # no command: externally managed (docker), only port-watched
    port: 5432

  api:
    command: bun run dev:api
    port: 3000
    health: http://localhost:3000/health
    depends_on: [database]
    open_key: a         # press `a` in the dashboard to open http://localhost:3000

  web:
    command: bun run dev:web
    port: 3001
    depends_on: [api]
    open_key: w

infra:
  compose_file: docker-compose.yml   # `docker compose up -d` runs before app services
```

Validate it and get editor autocompletion:

```bash
atlas config check                       # prints OK, or every error at once
atlas config schema > .atlas.schema.json
```

Typos are caught before anything starts: `services.web.helth: unknown field (did you mean 'health'?)`.

## 2. Run it

```bash
atlas dev
```

You get a dashboard with a tab per service, merged logs on **All** and service states in the sidebar. Press `?` for the key bindings, `/` to search the logs, `q` to quit (all services are stopped). See the [TUI guide](tui.md).

Other ways to run the same environment:

| Command | Use |
|---|---|
| `atlas dev web` | start `web` and what it `depends_on`; everything else shows `off` |
| `atlas dev --detach` | start in a background runner and return; run `atlas dev` to attach, `atlas down` to stop |
| `atlas dev --headless` | no TUI, logs to stdout, still controllable over MCP |
| `atlas dev --ci` | start everything, wait for health, print JSON, exit `0` or `1` |
| `atlas dev --status` | print the state of a running stack (exit `0` running, `3` not) |

## 3. Per-machine tweaks

Create `atlas.local.yaml` next to `atlas.yaml` and gitignore it. It overrides ports, disables services or skips the dependency install on one machine. See [Local overrides](configuration.md#local-overrides-atlaslocalyaml).

## 4. Plug in an AI agent

```bash
atlas setup                        # detects omp, Claude Code and Codex, shows what to run
atlas setup claude-code --apply    # MCP server + skills + hooks
```

Restart the agent, open it in the project and ask *"why is api failing?"* or *"restart web and wait until it is ready"*. See [Agent setup](agents.md) and [MCP](mcp.md).

## 5. Try the desktop app

```bash
atlas desktop
```

Add your project, press **Start**, and open a terminal with ⌘T. A git worktree per task, an agent in each, and your stack running next to the main checkout: see the [Desktop app](desktop.md).

## Next

- [Concepts](concepts.md): the vocabulary in one page.
- [Configuration](configuration.md): dependency conditions, restart policy, actions, profiles.
- [Runtime](runtime.md): what happens on start and on quit.
