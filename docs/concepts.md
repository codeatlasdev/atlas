---
title: "Concepts"
description: "Projects, spaces, worktrees, services, actions, runners and hostd, in one page."
---

# Concepts

## Project

A directory with an `atlas.yaml`. The file lists the services to run and everything around them. `atlas.local.yaml` next to it overrides a subset per machine. Atlas keeps its runtime files in the project's `.logs/` directory (add it to `.gitignore`).

## Service

One thing the stack runs. A service with a `command` is spawned by Atlas (`sh -c`, in its own session, stdin `/dev/null`). A service without a `command` is **externally managed** (typically a docker container from your compose file): Atlas never spawns it and only watches its `port`.

A service is always in one of these states:

| State | Meaning |
|---|---|
| `starting` | spawned, not ready yet |
| `running` | ready: port or `health` probe passed, or its `ready_log` matched |
| `failed` | exited abnormally, a dependency failed, or the restart budget is spent |
| `stopped` | not running; also a finished one-shot |
| `disabled` | stopped by you, or left out of this run by `atlas dev SERVICE…` |
| `suspended` | paused by an action's `pause_services` |

Services start when every [`depends_on`](configuration.md#depends_on) condition holds (`started`, `healthy`, `completed_successfully`, `log_ready`) and restart according to their [restart policy](configuration.md#restart-policy). Details in [Runtime](runtime.md).

## Profile

A named group of services (`profiles:` in `atlas.yaml`) you start with `atlas dev --profile backend`. The selection always includes the transitive dependencies.

## Action

A command bound to a key (`actions:`), for the things you do next to the stack: reset the database, run the checks, launch an iOS simulator. An action can `confirm` first, `pause_services` while it runs, open an interactive `session`, or target a mobile `platform`. Actions show up in the TUI help, the command palette, the desktop **Actions** menu and over MCP, where `confirm` becomes a gate that needs `confirmed: true`.

## Runner, TUI and attach

The **runner** is the engine that owns the services: scheduler, health checks, control socket, status heartbeat and disk logs. A TUI can own it (plain `atlas dev`), or the runner can live on its own as a daemon (`atlas dev --detach`, or `--headless` in the foreground). A TUI is then just a client that attaches and detaches without touching the services. The desktop app, the TUI and MCP all talk to the same runner through `.logs/atlas.sock`.

```text
atlas dev --detach   start the runner and return
atlas dev            attach a TUI (D detaches; services keep running)
atlas down           stop everything gracefully
```

## Atlas Desktop vocabulary

**Project.** A main git checkout you added to the app. Stored in `~/.atlas/desktop/projects.json`.

**Space.** The app's view of a project. It keeps its own worktrees, tabs, panes and docks, and has its own accent color so you always know which project you are in. Move between spaces with ⌥⌘← / ⌥⌘→ or ⌃1…⌃9.

**Worktree.** A `git worktree` of the project, created by Atlas under `~/.atlas/worktrees/<repo>/<name>` on its own branch. One worktree is one task: it has its own terminals, agent sessions, diff and (optionally) its own copy of the stack. Each gets a **port offset** (100, 200, …) so its services run next to the main checkout's without port clashes (`ATLAS_PORT_OFFSET`).

**Panel.** The first tab of every worktree: stack health, live CPU and memory per service, the setup strip and service cards.

**Agent.** A coding agent CLI (omp, Claude Code, Codex) running in a worktree terminal. Hooks report its state to the app: working, needs you, done.

**Dock.** A side or bottom region around the panes: the left dock holds the explorer, search and source control; the right dock holds agent history and listening ports; the bottom dock is a terminal tab group.

## hostd

`atlas hostd` is a per-user daemon that Atlas Desktop starts. It owns the PTYs, one-shot command execution, file access and terminal scrollback, which is why terminals and agents keep running when you quit, crash or update the app. It listens on `~/.atlas/hostd.sock` (token in `~/.atlas/hostd.token`) and can run on an SSH host too, so a remote worktree behaves like a local one. See [Architecture](architecture.md#hostd).

## Skill

Instructions for AI agents, compiled into the `atlas` binary so they always match the version you run. `atlas setup` or `atlas install` copies them to `~/.agents/skills/` (and to `~/.claude/skills/` for Claude Code). The skills are `atlas` (the dev environment), `atlas-desktop` (driving the app with `atlas wt|term|browser|file`) and `atlas-install`.

## Where Atlas keeps things

| Where | What |
|---|---|
| `<project>/.logs/` | per-service logs, status file, control socket, locks. See [Files in `.logs/`](runtime.md#files-in-logs) |
| `~/.atlas/bin/atlas` | the installed CLI |
| `~/.atlas/desktop/` | desktop settings (`settings.json`), projects, workspace layout, themes |
| `~/.atlas/worktrees/` | worktrees created by the desktop app |
| `~/.atlas/hostd.*`, `~/.atlas/hostd/` | the terminal host: socket, token, pid, log, scrollback |
| `~/.atlas/desktop.sock` | control socket of the running desktop app (`atlas wt`, `atlas term`…) |
| `~/.atlas/omp-plugin/` | the omp plugin written by `atlas setup omp` |
| `~/.agents/skills/` | bundled skills for omp and Codex |
