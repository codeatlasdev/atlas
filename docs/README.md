# Atlas documentation

Atlas boots a project's whole dev environment from one `atlas.yaml`: infra, services, dependency order, restarts and logs. You drive it from a TUI, a native desktop app or an AI agent over MCP. These pages are also published at [atlas.codeatlas.com.br/docs](https://atlas.codeatlas.com.br/docs/).

## Start here

| Page | Read it when you want to… |
|---|---|
| [Introduction](introduction.md) | know what Atlas is and what is in the box |
| [Getting started](getting-started.md) | install, write an `atlas.yaml` and run it, in three steps |
| [Installation](installation.md) | install from a release, update, build from source, uninstall |
| [Quickstart](quickstart.md) | the same flow with more detail, plus agents and the desktop app |
| [Concepts](concepts.md) | learn the vocabulary: projects, spaces, worktrees, services, actions, runner, hostd |

## Guides

| Page | Read it when you want to… |
|---|---|
| [Desktop app](desktop.md) | work with spaces, worktrees, agents, review, the browser and SSH hosts |
| [GPU terminal](terminal.md) | know how the native terminal renders, what it supports and how it performs |
| [TUI](tui.md) | learn the dashboard, every key binding and the log viewer |
| [Logs](logs.md) | understand how logs are collected, stored, searched and exposed |
| [Agent setup](agents.md) | run `atlas setup omp\|claude-code\|codex` |
| [MCP](mcp.md) | connect any MCP client to your running environment |
| [Deploy](deploy.md) | ship releases to a server with atomic swap and automatic rollback |

## Reference

| Page | Read it when you want to… |
|---|---|
| [CLI](cli.md) | look up a command, a flag or an exit code |
| [Configuration](configuration.md) | look up any `atlas.yaml` field, local overrides, validation, JSON Schema |
| [Desktop settings](desktop-settings.md) | edit `~/.atlas/desktop/settings.json` by hand |
| [Runtime](runtime.md) | understand startup order, restarts, health, shutdown and `.logs/` files |

## Help

| Page | Read it when… |
|---|---|
| [Troubleshooting](troubleshooting.md) | something does not work |
| [FAQ](faq.md) | you have a quick question |

## Project

| Page | Read it when you want to… |
|---|---|
| [Architecture](architecture.md) | read or change the code: crates, processes, sockets |
| [Contributing](contributing.md) | set up a dev environment, run the checks, send a pull request |
| [Releasing](release.md) | (maintainers) cut and publish a release |

The agent skills that ship with Atlas live in [`skills/`](../skills/atlas/SKILL.md); they summarize these docs for LLMs. The sidebar order of the website is defined in [`_nav.json`](_nav.json).
