---
title: "Architecture"
description: "The crates, the processes and the sockets that connect them."
---

# Architecture

Atlas is a Rust workspace plus a SvelteKit frontend. This page is for people reading or changing the code; for using Atlas see the [Introduction](introduction.md).

## Crates

Seven crates, all `publish = false`, in one Cargo workspace (`cargo metadata --no-deps` is the source of truth).

| Path | Crate | Role | Depends on (workspace) |
|---|---|---|---|
| `crates/atlas-core` | `atlas-core` | shared domain types, paths (`~/.atlas`), GitHub release helpers | none |
| `crates/atlas-host` | `atlas-host` | `atlas hostd`: PTYs, one-shot exec, file access and scrollback behind a unix socket, plus the client and the live upgrade | none |
| `crates/atlas-term` | `atlas-term` | the GPU terminal without Tauri: VT state (on `alacritty_terminal`), the wgpu/Metal renderer, fonts, inline images, key encoding | none |
| `crates/atlas-tui` | `atlas-tui` | a library: config loading and validation, the runtime (manager, scheduler, health, process supervision, logs), the TUI (ratatui), the runner, the control socket and the MCP server | `atlas-core` |
| `crates/atlas-skills` | `atlas-skills` | the agent skills compiled into the binary and their installer | `atlas-core` |
| `crates/atlas-cli` | `atlas-cli` | the `atlas` binary: argument parsing (clap), every subcommand, self-update, agent setup | `atlas-core`, `atlas-host`, `atlas-skills`, `atlas-tui` |
| `crates/atlas-desktop` | `atlas-desktop` | Atlas Desktop: the Tauri 2 shell, worktrees, the agent hook server, the embedded browser, the tray, the debug bridge | `atlas-core`, `atlas-host`, `atlas-tui`, and `atlas-term` on macOS only |

```text
atlas-cli ──▶ atlas-tui ──▶ atlas-core
   ├────────▶ atlas-skills ─▶ atlas-core
   ├────────▶ atlas-host
   └────────▶ atlas-core
atlas-desktop ──▶ atlas-tui, atlas-core, atlas-host
              └─▶ atlas-term   (macOS only)
```

Other top-level directories:

| Path | What |
|---|---|
| `apps/desktop` | the desktop frontend (SvelteKit 3, using `@codeatlasdev/ui`) |
| `skills/` | markdown skills, embedded by `atlas-skills/build.rs` |
| `docs/` | these docs; the website renders them directly |
| `site/` | the website (Astro); see [The website](site.md) |
| `scripts/` | setup, release, smoke and CI helper scripts |
| `demo/` | the demo stack (`atlas.yaml`, `demo.tape`) used by `scripts/smoke.sh` and the website footage |
| `assets/` | logos and app icons (sources for `scripts/icons.sh`) |
| `testdata/` | fixtures shared by the Rust and TypeScript tests (log levels) |

`atlas-cli` is the only crate that produces the `atlas` binary. Building `atlas-tui` alone gives a library. `atlas-desktop` and `atlas-term` are macOS-only: the workspace `default-members` leave them out, so a plain `cargo build`/`cargo test` works on Linux, and `--workspace` builds everything on a Mac.

## Processes and sockets

```text
                           ┌──────────────────────────────┐
  you ── atlas dev ───────▶│ runner (services, scheduler, │◀── atlas mcp ◀── AI agent
  (TUI attaches)           │ health, disk logs)           │
  Atlas Desktop ──────────▶│ .logs/atlas.sock             │
                           └──────────────────────────────┘
                                          │ spawns (sh -c, setsid)
                                          ▼
                                   your services

  Atlas Desktop ──▶ atlas hostd ──▶ PTYs (terminals, agents), exec, fs, scrollback
                    ~/.atlas/hostd.sock
  atlas wt|term|browser|file ──▶ ~/.atlas/desktop.sock ──▶ Atlas Desktop
  agent hooks ──▶ loopback HTTP server in Atlas Desktop (token per run)
```

| Endpoint | Served by | Used by |
|---|---|---|
| `<project>/.logs/atlas.sock` (0600) | the runner or TUI that owns the project | attached TUIs, Atlas Desktop, `atlas mcp`, `atlas down` |
| `~/.atlas/hostd.sock` + `hostd.token` | `atlas hostd` | Atlas Desktop, `atlas term` without the app |
| `~/.atlas/desktop.sock` | Atlas Desktop | `atlas wt`, `atlas term`, `atlas browser`, `atlas file` |
| `~/.atlas/desktop-debug.sock` | debug builds of Atlas Desktop only | `atlas desktop debug …` |
| loopback HTTP, `~/.atlas/desktop/hook-endpoint.env` | Atlas Desktop | `atlas hook`, the omp `atlas-status` extension |

### The runner

`atlas-tui/src/runtime/manager` holds the `ServiceManager`: the scheduler (dependency graph and conditions), the spawner, health checks, restart policy, actions, the tunnel and the disk log writer. The same manager drives the TUI, the background runner (`--detach`), `--headless` and `--ci`. An attached TUI runs a *mirror* of the manager that is fed by the runner's event stream instead of spawning anything. The wire protocol is documented in [Runtime › Runner and attach](runtime.md#runner-and-attach).

### MCP

`atlas-tui/src/mcp` is a hand-rolled stdio MCP server (JSON-RPC 2.0). Read tools work from files in `.logs/` and `atlas.yaml`; control tools go through the control socket. See [MCP](mcp.md).

### hostd

`atlas hostd` outlives the app so terminals survive quits, crashes and updates. After an app update the daemon is older than the app; protocol 2+ daemons are replaced live (the new process receives every PTY master over `SCM_RIGHTS`, along with the session state and the lock). Details in [Desktop › Architecture](desktop.md#architecture).

### The GPU terminal

```text
program ─pty─▶ atlas hostd ──raw bytes──▶ reader task ─▶ Vt (alacritty_terminal + atlas scanner)
                                                              │ snapshot (Frame)
                                  native view over the pane ◀─┴─ render thread (wgpu / Metal)
```

See [GPU terminal](terminal.md#architecture).

## The desktop app

`crates/atlas-desktop` is a Tauri 2 shell with child webviews (the `unstable` feature), which makes the main window multi-webview: window operations use `get_window("main")` and the app's own webview is `get_webview("main")`. The frontend in `apps/desktop` is a SvelteKit SPA; its stores (`workspace`, `worktrees`, `agents`, `browser`…) are the single source of UI state, and the Rust side exposes commands (`projects_list`, `stack_start`, `term_*`, …) over Tauri IPC.

## The log pipeline

Service output flows through three places that must agree on what a line means:

1. **Rust runtime** (`atlas-tui/src/runtime/logs.rs`, `logindex.rs`, `loglinks.rs`): the ring buffer, `LogLevel::detect` (whole-word `error|err|fatal|panic|warn|warning|trace|debug` tokens, not after `/ . - _`), `strip_ansi`, links, search and level jumps. Drives the TUI and the on-disk logs.
2. **Desktop IPC** (`atlas-desktop/src/live.rs`): streams entries, with the runner's level, to the webview.
3. **Frontend** (`apps/desktop/src/lib/logs.ts`, `ansi.ts`, `Logs.svelte`): buffers, filters and renders. `levelOf` is the same word-token rule, used only when an older runner sends no level.

The level rule exists in Rust and TypeScript, so both are tested against one table, `testdata/log-levels.json`. Moving the shared text logic into one crate is on the [roadmap](https://github.com/codeatlasdev/atlas/blob/main/ROADMAP.md).

## Data on disk

See [Where Atlas keeps things](concepts.md#where-atlas-keeps-things) and [Files in `.logs/`](runtime.md#files-in-logs).

## Safety rules the code relies on

These come from real incidents and are enforced by tests; keep them when you change the runtime:

- Orphan cleanup uses only `.logs/.pids` and **exact** PIDs. It never kills by port (`lsof`) and never uses negative PIDs (process-group kills).
- Atlas never runs `docker compose down` or restarts the docker daemon.
- Children run with `setsid`, `stdin(null)` and `sh -c`.
- The TUI reads input from an event stream, renders only when dirty and never blocks startup.
- Nothing secret is loaded into the embedded browser; Design Mode results are read back with `eval_with_callback`.
