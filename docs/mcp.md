---
title: "MCP"
description: "The atlas MCP server: tools, resources, prompts and client setup."
---

# MCP server for AI agents

`atlas mcp` is a [Model Context Protocol](https://modelcontextprotocol.io) server over stdio. It lets an agent (OMP, Claude Code, Codex, Cursor, …) see your dev environment the way you do in the TUI: service states, logs, config. It can also restart, stop and start services and run your `atlas.yaml` actions.

- Protocol: JSON-RPC 2.0, newline-delimited, MCP revision `2025-11-25` (also accepts `2025-06-18`, `2025-03-26`, `2024-11-05`). Capabilities: tools, resources (`atlas://status`, `atlas://config`, template `atlas://logs/{service}`), prompts (`diagnose <service>`, `add-service <name> <command> [port]`, `status`; OMP shows them as `/atlas:<name>`).
- Zero setup beyond registering the command: `atlas mcp` finds the project from the directory the agent starts it in.
- The server sends usage `instructions` in its `initialize` reply; clients such as OMP put them in the agent's system prompt, so the agent already knows to call `atlas_status` first.

## How it works

```text
agent ──stdio──▶ atlas mcp ──files──▶ .logs/*.log, .logs/.tui-status.json, atlas.yaml     (always works)
                      └──unix socket──▶ .logs/atlas.sock ──▶ running `atlas dev`          (control tools)
```

| Capability | Source | Needs `atlas dev` running? |
|---|---|---|
| status | `.logs/.tui-status.json` (heartbeat refreshed about every second) | no, reports `running: false` |
| logs, log search | `.logs/<service>.log` (and `<service>.prev.log`) | no, shows the last run |
| config | `atlas.yaml` + `atlas.local.yaml` | no |
| restart / stop / start service, run action | control socket `.logs/atlas.sock` | **yes** |
| wait for ready | polls the status file | needs a live heartbeat |

The control socket is served by the TUI, the background runner (`atlas dev --detach`) and `atlas dev --headless` (not `--ci`); see [Runtime › Runner and attach](runtime.md#runner-and-attach). It is a Unix domain socket with mode `0600` (only your user can use it), created while holding the project's singleton lock and removed on exit. Requests go through the same code paths as the keyboard, so `confirm`, `pause_services` and "already running" guards behave identically.

If `atlas dev` is not running, control tools return an error result: `atlas dev is not running; start it in a terminal`. Ask the user to start it, or run `atlas dev --detach` yourself: it returns immediately and leaves the services running for the user to `atlas dev` attach to later (`atlas down` stops them).

## Tools

All tools return text. Failures set `isError: true` with a message the agent can act on; unknown tool names and malformed requests are JSON-RPC errors.

| Tool | Arguments | Needs `atlas dev` |
|---|---|---|
| [`atlas_status`](#atlas_status) | none | no |
| [`atlas_logs`](#atlas_logs) | `service?`, `lines?`, `since_minutes?`, `level?` | no |
| [`atlas_logs_search`](#atlas_logs_search) | `query`, `service?`, `limit?`, `include_previous?` | no |
| [`atlas_config`](#atlas_config) | none | no |
| [`atlas_restart_service`](#atlas_restart_service--atlas_stop_service--atlas_start_service) | `service` | yes |
| [`atlas_stop_service`](#atlas_restart_service--atlas_stop_service--atlas_start_service) | `service` | yes |
| [`atlas_start_service`](#atlas_restart_service--atlas_stop_service--atlas_start_service) | `service` | yes |
| [`atlas_run_action`](#atlas_run_action) | `action`, `confirmed?` | yes |
| [`atlas_wait_ready`](#atlas_wait_ready) | `service?`, `timeout_s?` | live heartbeat |

### `atlas_status`

Is `atlas dev` running for this project, and what is each service's state? Call this first.

```json
{
  "running": true,
  "pid": 4242,
  "heartbeat_age_s": 1,
  "services": [
    { "name": "api", "state": "running", "port": 3001, "health": "http://localhost:3001/health",
      "exit_code": null, "restarts": 0, "uptime_s": 412, "started_at": 1760000000, "note": null, "off": false },
    { "name": "web", "state": "failed",  "port": 3002, "health": null,
      "exit_code": 1, "restarts": 5, "uptime_s": null, "started_at": 1760000412, "note": "gave up after 5 restarts" }
  ],
  "tunnel": { "state": "running", "local_port": 54320, "ssh_host": "prod-db" }
}
```

Per service: `exit_code` is the last exit status of the process (`null` while it runs, never exited, or died on a signal); `restarts` the restart attempts in the current streak; `uptime_s` seconds `running` (`null` otherwise); `started_at` unix seconds of the last spawn; `note` the sidebar annotation (`waiting for :3001 (12s)`, `waiting for ready_log (12s)`, `start timeout: still waiting for :3001 (75s)`, `restarting in 4s (2/5)`, `dependency db failed`, …).

`tunnel` is `null` when no [SSH tunnel](configuration.md#tunnel) is running; otherwise its `state` uses the same values as services (`failed` while reconnecting). Tunnel ssh output is readable with `atlas_logs` (`service: "tunnel"`).

`state` is one of `starting`, `running`, `failed`, `stopped`, `disabled` (stopped by a user/agent, or left out of the run), `suspended` (paused by an action). `off: true` marks a service left out of this run by `atlas dev SERVICE…` / `--profile` (state `disabled`): start it with `atlas_start_service`, which clears the flag. When nothing is running: `{"running": false, "reason": "no status file", "hint": "atlas dev is not running; start it in a terminal"}`. Other reasons: the pid is dead, or the heartbeat is older than 120 s.

### `atlas_logs`

Tail logs from `.logs/`. ANSI is already stripped. Each returned line is `[service:line_no] HH:MM:SS [stdout|stderr] text`.

| Argument | Type | Default | Notes |
|---|---|---|---|
| `service` | string | all merged by timestamp | Must have a log file; otherwise the error lists services that do. Project [action](runtime.md#actions) output: `action:<label>` (e.g. `action:Run checks`) or the file name `action-run-checks`. |
| `lines` | integer 1–2000 | 100 | Trailing lines to return. |
| `since_minutes` | integer ≥ 0 | none | Only lines from the last N minutes (by the `HH:MM:SS` prefix, local time, capped at 24 h). Lines without a timestamp (stack-trace continuations) are kept. |
| `level` | `error` \| `warn` | none | `error` = errors only; `warn` = warnings and errors. Detected from text (`error`, `fatal`, `panic`, `warn`, color codes). |

Individual lines longer than 2,000 characters are clipped with `… [clipped]`. Empty result: `(no matching log lines)`. A log file larger than 8 MiB is read from its end only (the line numbers in `[service:line_no]` stay the true line numbers of the file); the disk log itself rotates to `<service>.prev.log` at 50 MiB. Lines on disk are the assembled lines (partial lines joined, progress-bar `\r` updates collapsed, lines over 16 KiB cut with ` … [truncated]`).

```text
[api:212] 10:00:05 [stderr] ERROR db down
[web:88] 10:00:07 [stdout] warn: slow
```

### `atlas_logs_search`

Search logs. `query` is a literal substring, or a regex when wrapped in slashes (`/ECONNREFUSED|timeout/`). Matching is case-insensitive. Files are streamed line by line (CRLF and invalid UTF-8 are handled) and only the newest `limit` hits are kept, so searching a large log does not load it into memory.

| Argument | Type | Default | Notes |
|---|---|---|---|
| `query` | string | **required** | Non-empty. Invalid regex is an error. |
| `service` | string | all | Restrict to one service. Errors with the list of known services if it has neither a current nor a previous log. |
| `limit` | integer 1–500 | 50 | The **most recent** `limit` matches are returned. |
| `include_previous` | boolean | `false` | Also search the previous run's logs (`<service>.prev.log`); those lines are tagged `(prev run)`. |

Empty result: `(no matches)`.

### `atlas_config`

Summary of `atlas.yaml` merged with `atlas.local.yaml`, so the agent learns valid service names and action keys/labels. Fails with the validation message if the config is invalid.

```json
{
  "project": "shop",
  "services": [
    { "name": "api", "port": 3001,
      "depends_on": [{ "service": "database", "condition": "healthy" }],
      "command": "bun run dev:api", "enabled": true }
  ],
  "actions": [
    { "key": "R", "label": "Reset local DB", "confirm": "This wipes the local database. Continue?",
      "pause_services": ["api"], "session": false }
  ]
}
```

### `atlas_restart_service` / `atlas_stop_service` / `atlas_start_service`

Argument: `service` (name exactly as in `atlas.yaml`; unknown names return an error listing the known ones).

- **restart**: stop, then start. Also works to start a service that is stopped or failed.
- **stop**: stop the service and mark it `disabled`: health checks skip it and it is never auto-restarted.
- **start**: bring a stopped, disabled or failed service back (same mechanism as restart).

All three return as soon as the request is accepted, not when the service is up:

```json
{ "service": "api", "queued": true }
```

Follow with [`atlas_wait_ready`](#atlas_wait_ready).

### `atlas_run_action`

Run an `atlas.yaml` action by `action` (its key character or its label, case-insensitive; key wins) in the running `atlas dev`.

| Argument | Type | Default | Notes |
|---|---|---|---|
| `action` | string | **required** | Key or label, see `atlas_config`. |
| `confirmed` | boolean | `false` | Set `true` **only after the user approved** the action's confirm text. |

**Confirm gate.** An action with a `confirm:` message is destructive by declaration. Without `confirmed: true` the call is refused and the text is returned:

```text
action 'Reset local DB' requires confirmation: "This wipes the local database. Continue?". Ask the user, then retry with confirmed: true.
```

The agent must show that text to the user and retry only if they agree. Other errors: unknown action (lists known ones), the action is already running, or (runner / headless) the action has a `platform` (needs the TUI device picker) or is a `session: true` action (needs the terminal that owns it), both owner-TUI only. On success: `{ "action": "Reset local DB", "queued": true }`. The command runs in the background (paused services are restarted when it ends); watch progress with `atlas_status` and `atlas_logs`. A `session: true` action opens its tab in the owner TUI.

### `atlas_wait_ready`

Block until a service (or every service) is `running`, polling the live status twice a second.

| Argument | Type | Default | Notes |
|---|---|---|---|
| `service` | string | all | When omitted, `disabled` and `suspended` services are not waited for. A targeted service that is disabled keeps waiting. |
| `timeout_s` | integer 1–**600** | 60 | Values above 600 are clamped. |

Results:

- ready: `ready after 3.2s` followed by `api=running web=running` (never reported in under 1.5 s, so a call right after a restart cannot see the old `running`);
- a service entered `failed`: error `service 'api' failed` plus all states (fails fast);
- timeout: error `timed out after 60s waiting for readiness` plus all states;
- atlas not running: error `atlas dev is not running; start it in a terminal`.

> **Client timeouts.** MCP clients cancel a call that takes too long. OMP's default is 30 s per request, so a `timeout_s` above about 25 will be cut off by the client. Either keep `timeout_s` ≤ 25 and call again, or raise the client timeout (OMP: `"timeout": 120000` in the server entry, up to cover the 600 s maximum if you need it). Atlas itself answers every control request within 30 s.

### Typical agent workflow

1. `atlas_status`: is it up, what is failing?
2. `atlas_logs {"level": "error"}` or `atlas_logs {"service": "api", "lines": 200}`; `atlas_logs_search {"query": "/ECONN|timeout/"}`.
3. Fix the code.
4. `atlas_restart_service {"service": "api"}` then `atlas_wait_ready {"service": "api"}`.
5. Re-check logs.

## Set up your client

`atlas` must be on `PATH` (see [Installation](installation.md)). With no `--dir`, `atlas mcp` serves the nearest ancestor of its working directory that contains `atlas.yaml`; clients launch servers from the project directory, so a bare `atlas mcp` is usually all you need. Pass `--dir <path>` (or set the client's `cwd`) to pin a project.

### OMP (primary)

`atlas setup omp --apply` does this for you: it writes the plugin `~/.atlas/omp-plugin/` (its `.mcp.json` registers the server) and runs `omp plugin link`. Manual alternative, project-level, committed with the repo, `.omp/mcp.json`:

```json
{
  "mcpServers": {
    "atlas": {
      "type": "stdio",
      "command": "atlas",
      "args": ["mcp"],
      "timeout": 120000
    }
  }
}
```

User-level (all projects), `~/.omp/agent/mcp.json`: same shape. Add `"cwd": "/path/to/project"` there if you want one fixed project, otherwise OMP's working directory is used and Atlas finds the project from it.

Notes for OMP:

- `timeout` is in milliseconds. OMP's default request timeout is 30 s (also `OMP_MCP_TIMEOUT_MS`); `120000` leaves room for `atlas_wait_ready` calls with a larger `timeout_s` (see [Client timeouts](#atlas_wait_ready)).
- OMP puts the server's `instructions` into the system prompt. To disable that for this server, set `"instructions": false` in the entry.
- OMP may send string JSON-RPC ids; Atlas echoes ids back unchanged, so that works.
- Discovery waits briefly and then continues in the background; the tools may appear a moment after startup.
- Commands: `/mcp list` (servers and tools), `/mcp test atlas` (connect and list), `/mcp reload` (re-read config), `/mcp reconnect` (restart the server process, e.g. after `atlas update`).
- OMP also imports MCP configs from Claude Code, Codex and Cursor, so any of the setups below is picked up by OMP as well.

`atlas setup claude-code --apply` / `atlas setup codex --apply` write these entries for you ([Agent setup](agents.md)); the manual forms:

### Claude Code

```bash
claude mcp add --scope user atlas -- atlas mcp
```

or `--scope project` / a `.mcp.json` at the project root (user scope lives in `~/.claude.json`):

```json
{ "mcpServers": { "atlas": { "command": "atlas", "args": ["mcp"] } } }
```

### Codex

`~/.codex/config.toml`:

```toml
[mcp_servers.atlas]
command = "atlas"
args = ["mcp"]
```

### Cursor

`.cursor/mcp.json` in the project:

```json
{ "mcpServers": { "atlas": { "command": "atlas", "args": ["mcp"] } } }
```

### Any other client

Run `atlas mcp [--dir <project>]` as a stdio server. Quick manual check:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"atlas_status"}}' \
  | atlas mcp
```

## Protocol notes

- `initialize` returns the requested protocol version if Atlas supports it, otherwise `2025-11-25`; `serverInfo` is `{ "name": "atlas", "version": <atlas version> }`; capabilities: `tools` with `listChanged: false`.
- `ping`, `tools/list` and `tools/call` are served even if the client skipped the handshake. Notifications never get a reply.
- Requests run concurrently, so a long `atlas_wait_ready` never blocks `ping` or other calls.
- JSON-RPC batches are accepted only when the negotiated version is `2025-03-26` (the only revision that has them); otherwise they get error `-32600`. Request ids must be strings or numbers.
- stdout is protocol only. Diagnostics go to stderr; `atlas mcp` prints `atlas mcp: serving <dir> on stdio` there.
- The server exits when the client closes stdin, after answering in-flight calls.

## Troubleshooting

| Symptom | Cause / fix |
|---|---|
| Tools missing in the client | `atlas` not on the client's `PATH` (GUI apps often lack your shell `PATH`: use the absolute path, e.g. `~/.atlas/bin/atlas`). In OMP run `/mcp test atlas`. |
| `atlas_config` fails | `atlas.yaml` not found from the working directory (set `--dir` or `cwd`) or invalid (`atlas config check`). |
| `atlas_status` says `running: false` but the TUI is open | The agent is looking at a different project directory than the TUI. Compare the `--dir`/`cwd`. |
| `atlas dev is not running; start it in a terminal` | Start `atlas dev` (or `atlas dev --detach` / `--headless`) for that project. `--ci` does not serve control. |
| Control call: `timed out waiting for atlas dev` | The run loop did not answer within 30 s (it is stuck or shutting down). |
| `atlas_wait_ready` cut off after 30 s | Client timeout shorter than `timeout_s`; see [Client timeouts](#atlas_wait_ready). |
| `unknown service 'x'. Known services: …` | Use names from `atlas_config`. |
| `no log file for service 'x'` | The service has not produced a log yet. Command-less services never have one. |
| Stale `.logs/atlas.sock` after a crash | Harmless: the next `atlas dev` replaces it. |
