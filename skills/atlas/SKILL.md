---
name: atlas
description: Work inside a project that has an atlas.yaml. Check service status, read and search logs, restart services, run actions, choose service ports, and edit atlas.yaml through the atlas MCP tools or the .logs/ files. Use when debugging a dev environment, when a service is failing, not ready or hits EADDRINUSE, when adding a service or port, when the user mentions atlas, atlas dev, atlas.yaml, .logs, or service ports, or before touching anything that starts dev servers.
metadata:
  version: "3.2.0"
---

# Atlas: working in an Atlas project

Atlas boots a project's dev stack from `atlas.yaml` (services in dependency order, docker infra, health checks, restarts) and writes logs and state to `.logs/`. The developer owns the running `atlas dev` TUI. You observe and steer it; you never replace it.

## Rules

- **NEVER start or stop the stack yourself**: no `atlas dev` (with or without `--detach`), no `bun run dev` or any dev server, no `atlas down`. The TUI needs a terminal and exits `2` without one; a second instance fights the first over `.logs/.tui.lock`; `atlas down` kills the developer's running stack. Ask the user instead. (`atlas dev --status` and `atlas config check` are read-only and always fine.)
- Prefer MCP tools (`atlas_*`). If they are not available, use the fallbacks below.
- A service shown `off` (`disabled`, `"off": true`) was deliberately left out of a partial run (`atlas dev web`, `--profile backend`): not a failure. Start it with `atlas_start_service` if the task needs it.
- Read a failing service's log before restarting it; a restart rotates `<service>.log` to `.prev.log`.
- Never run an action that has a `confirm:` message without the user's explicit yes.
- New service or port? Follow **Ports** in [yaml.md](yaml.md#ports): never default ports (3000, 5173, 5432, 6379, 8080…), always a random per-project block.

## Tools

`atlas mcp` serves the project found from the working directory. Not registered? `atlas setup omp --apply` registers it for OMP (see the `atlas-install` skill).

| Tool | Args | Needs `atlas dev` running |
|---|---|---|
| `atlas_status` | none | no (reports `running: false`); `setup` = last `pre_command` run `{state, command, started_at, finished_at?, reason?}` or null |
| `atlas_config` | none | no |
| `atlas_logs` | `service?`, `lines?` (1-2000, default 100), `since_minutes?`, `level?` (`error` or `warn`) | no |
| `atlas_logs_search` | `query` (substring or `/regex/`), `service?`, `limit?` (1-500, default 50), `include_previous?` | no |
| `atlas_restart_service` / `atlas_stop_service` / `atlas_start_service` | `service` | yes |
| `atlas_run_action` | `action` (key or label), `confirmed?` | yes |
| `atlas_wait_ready` | `service?`, `timeout_s?` (1-600, default 60) | yes (live heartbeat) |

Control tools reply `atlas dev is not running; start it in a terminal` when nothing is up. Use `atlas_config` first to learn valid service names and action keys/labels. `atlas_wait_ready` failures include the service's last error lines.

Slash prompts (omp): `/atlas:status`, `/atlas:diagnose <service>`, `/atlas:add-service <name> <command> [port]`. Resources: `atlas://status`, `atlas://config`, `atlas://logs/{service}`.

## Debugging recipe

1. `atlas_status`: what is `failed` or stuck `starting`? Read `exit_code`, `restarts`, `note` (e.g. `waiting for :24310 (12s)`, `dependency db failed`).
2. `atlas_logs {"level": "error"}`, then `atlas_logs {"service": "<svc>", "lines": 200}` for the failing service.
3. `atlas_logs_search {"query": "/ECONNREFUSED|EADDRINUSE/", "include_previous": true}` if the log was just rotated by a restart.
4. Fix the code or config. If upstream failed first (`dependency X failed`), fix that service first.
5. `atlas_restart_service {"service": "<svc>"}` then `atlas_wait_ready {"service": "<svc>"}`.
6. Re-read errors. Stop after two failed cycles with the same error and report what you saw.

Keep `timeout_s` at 25 or less unless the MCP client timeout was raised (OMP default is 30 s); call again if it times out.

## Actions

`atlas_run_action` runs an `atlas.yaml` action in the running TUI. If the action has a `confirm` text, the call is refused and returns that text. Show it to the user, and retry with `confirmed: true` only after they agree. Output goes to `.logs/action-<slug>.log`; read it with `atlas_logs {"service": "action:<label>"}`.

## Without MCP

| Need | Command |
|---|---|
| Is it running, which states | `atlas dev --status` (exit `0` running, `3` not running) |
| Machine-readable state | `.logs/.tui-status.json` |
| Service log | `.logs/<service>.log`, previous run `.logs/<service>.prev.log` |
| Action output | `.logs/action-<slug>.log` |
| Validate config | `atlas config check` |

You cannot restart services without MCP; tell the user what to press (table below). File formats and failure patterns: [logs.md](logs.md).

## Talking to the developer

The developer is looking at the TUI while you work. When you need them to act, give the exact key and where to press it, one step per line. Don't invent keys: these are the only ones that exist (`?` in the TUI shows the full list).

| They need to | Tell them |
|---|---|
| Start the stack | run `atlas dev` in a terminal (or `atlas dev --detach` to keep it in the background) |
| See one service | press its digit (`1`-`9`) or `Tab`; `0` = All |
| Restart one service | open its tab, press `r` (or `m` → Restart) |
| Restart everything | `r` on the All tab (asks to confirm) |
| Stop / start one service | `s` on its tab / `m` → Start |
| Show only errors | `v` (cycles all → warn+ → error) |
| Search logs | `/` then type; `n`/`N` next/previous; `Esc` clears |
| Jump to newest / resume follow | `G` (or `f`) |
| Copy logs for you | `P` (logs as AI prompt) or `E` (errors only) |
| Run a project action | its key from `atlas.yaml` `actions`, or `:` and type the label |
| Pick up config changes | quit (`q` → `y`) and run `atlas dev` again; there is no hot reload |
| Leave it running in background | `D` (only in an attached TUI); `atlas down` stops it |

Prefer doing it yourself through MCP (`atlas_restart_service`, `atlas_logs`, …) over asking them to press keys.

## Editing atlas.yaml

1. Read the current file, change only what the task needs.
2. Run `atlas config check` (prints `OK: ... is valid` or every error, exit `1`). Unknown keys are errors with a "did you mean".
3. The running TUI does not hot-reload: tell the user to quit (`q` → `y`) and run `atlas dev` again.
4. Machine-specific values go in gitignored `atlas.local.yaml`, not `atlas.yaml`.

Field reference, action keys, reserved keys and common mistakes: [yaml.md](yaml.md). Authoritative: `docs/configuration.md` in the Atlas repo; `atlas config schema` prints the JSON Schema.

## Service states

`starting`, `running`, `failed`, `stopped`, `disabled` (stopped by a user or agent, never auto-restarted; undo with `atlas_start_service`), `suspended` (paused by an action's `pause_services`). A service with no `command` is externally managed (docker) and only port-watched.

## More

Repo docs: `docs/mcp.md` (tool details), `docs/runtime.md` (startup, restarts, `.logs/` files), `docs/cli.md`, `docs/tui.md` (keys).
