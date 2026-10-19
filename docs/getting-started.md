---
title: "Getting started"
description: "Install Atlas, write an atlas.yaml and run your stack in five minutes."
---

# Getting started

Three steps. Each links to the full page when you want the details.

## 1. Install

```bash
curl -fsSL https://atlas.codeatlas.com.br/install.sh | sh
atlas version
```

The installer verifies the release checksum, puts `atlas` in `~/.atlas/bin` and wires your `PATH` (open a new shell afterwards). Prebuilt releases cover macOS on Apple silicon, Linux x86_64 and Windows x64; Linux users can also install a [.deb, .rpm, Arch package or AppImage](installation.md#linux-packages), and on Windows you run `irm https://atlas.codeatlas.com.br/install.ps1 | iex` in PowerShell instead ([Windows](installation.md#windows)). Manual install, updates and uninstalling: [Installation](installation.md).

## 2. Describe your stack

In your project root:

```bash
atlas init                              # scaffold atlas.yaml
atlas config schema > .atlas.schema.json   # editor autocompletion
```

```yaml
# yaml-language-server: $schema=.atlas.schema.json
name: myapp

services:
  database:
    port: 5432            # no command: externally managed (docker), port-watched

  api:
    command: bun run dev:api
    port: 3000
    health: http://localhost:3000/health
    depends_on: [database]
    open_key: a

  web:
    command: bun run dev:web
    port: 3001
    depends_on: [api]

infra:
  compose_file: docker-compose.yml
```

`atlas config check` validates it and prints every error at once. Every field is documented in [Configuration](configuration.md).

## 3. Run it

```bash
atlas dev
```

A tab per service, merged logs on **All**, service states in the sidebar. `?` shows the keys, `/` searches the logs, `q` quits and stops everything. See the [TUI guide](tui.md).

To leave the stack running in the background: `atlas dev --detach`, then `atlas dev` to attach and `atlas down` to stop.

## What next

| I want to… | Read |
|---|---|
| walk through the whole flow with more detail | [Quickstart](quickstart.md) |
| understand the vocabulary | [Concepts](concepts.md) |
| use the native app | [Desktop app](desktop.md) |
| let an AI agent read logs and restart services | [Agent setup](agents.md), [MCP](mcp.md) |
| find a flag or exit code | [CLI reference](cli.md) |
| fix something that broke | [Troubleshooting](troubleshooting.md) |
