# Atlas logs and state files

Reference for the `atlas` skill. MCP tools read exactly these files, so prefer `atlas_status`, `atlas_logs` and `atlas_logs_search`; use the files directly when MCP is unavailable. Authoritative: `docs/runtime.md` (Files in `.logs/`).

## Files in `.logs/` (project root)

| File | Content |
|---|---|
| `<service>.log` | current run, stdout and stderr of a spawned service |
| `<service>.prev.log` | previous run (rotated at every (re)spawn, or past 50 MB) |
| `action-<slug>.log` | output of a non-session action; slug = label lowercased, non-alphanumerics collapsed to `-` (`Run checks` becomes `action-run-checks.log`); `.prev.log` rotation too |
| `tunnel.log` | ssh stderr and reconnect notes when a `tunnel:` is configured |
| `.tui-status.json` | status snapshot, rewritten atomically every second |
| `.tui.lock` | singleton lock, holds the atlas PID |
| `.pids` | PIDs of live children; removed on clean quit, cleaned on next start after a crash |
| `atlas.sock` | control socket used by `atlas mcp` |
| `.atlas-ui.json` | TUI memory (tab, zoom, filters); ignore it |

Command-less services (docker-backed) and interactive sessions have no log file. `.logs/` should be gitignored.

## Line format

```
12:01:04 [stderr] Error: listen EADDRINUSE :::3001
```

`HH:MM:SS [stdout|stderr] text`, local time, ANSI stripped, no date. MCP prefixes each returned line with `[service:line_no]`.

## `.tui-status.json`

```json
{
  "pid": 4242,
  "session": "4242-1767225600",
  "started_at": 1767225600,
  "updated_at": 1767225660,
  "log_dir": ".logs",
  "services": {
    "api": {
      "state": "failed", "port": 3001, "health": "http://localhost:3001/health",
      "exit_code": 1, "restarts": 5, "uptime_s": null, "started_at": 1767225620,
      "note": "gave up after 5 restarts", "autostart": true, "deselected": false
    }
  },
  "tunnel": { "state": "running", "local_port": 54320, "remote": "localhost:5432", "ssh_host": "prod-db" }
}
```

- `state`: `starting`, `running`, `failed`, `stopped`, `disabled`, `suspended`. A `disabled` service with `"deselected": true` (MCP: `"off": true`) was left out of this run by `atlas dev SERVICE…` / `--profile`; start it with `atlas_start_service`.
- `exit_code`: last exit of the process; `null` while running, never exited, or killed by a signal.
- `restarts`: attempts in the current streak. `uptime_s`: only while `running`. `started_at`: unix seconds of the last spawn.
- `note`: scheduler text: `waiting for :3000 (12s)`, `waiting for ready_log (12s)`, `start timeout: still waiting for ...`, `restarting in 2s (1/5)`, `dependency X failed`, `gave up after N restarts`.
- `tunnel` is `null` when no tunnel is running.
- The run is alive only if `pid` exists and `updated_at` is under 120 s old. A file left by a crash is correctly reported as not running by `atlas dev --status` (exit `3`) and `atlas_status` (`running: false`).

## Reading logs

```bash
atlas dev --status                       # states, no TUI (exit 0 running / 3 not)
tail -100 .logs/api.log                  # last 100 lines
grep -iE 'error|fatal|panic' .logs/*.log
```

MCP equivalents: `atlas_logs {"service": "api", "lines": 100}`, `{"level": "error"}` (errors only) or `{"level": "warn"}` (warnings and errors), `{"since_minutes": 10}`, `atlas_logs_search {"query": "/ECONN|timeout/", "include_previous": true}`. Action output: `{"service": "action:Run checks"}` or `{"service": "action-run-checks"}`.

## Reading the patterns

**Stuck in `starting`.** A service with a `port`/`health` stays `starting` until that probe passes; a `ready_log` service until a matching line appears. Check `note`:
- `waiting for X (condition)` (sidebar `↳`): a `depends_on` gate has not opened; look at X.
- `waiting for :3001 (12s)` / `waiting for ready_log (12s)`: its own probe. After `start_timeout` (default 60 s) it becomes `start timeout: still waiting ...`; nothing is killed and it keeps trying.
- `EADDRINUSE`: something else holds the port. Atlas never kills by port; find the owner with `lsof -nP -iTCP:<port> -sTCP:LISTEN` and tell the user rather than killing it (on macOS with OrbStack that can be the docker port forwarder).

**Starts, then fails.** `ECONNREFUSED 127.0.0.1:5432` means the database was not ready (check `depends_on` and infra). A missing env var points to `env_files` or `env:`. `dependency X failed` means fix X first. `gave up after N restarts` means the `restart:` budget is spent; read the log of the last attempt, then `atlas_restart_service` resets it.

**Docker/infra.** Compose output is not written to a file. A failure shows as a `docker` critical alert in the TUI (stderr under `--headless`). Run `docker compose -f <compose_file> up -d` by hand to see the real error. Quitting atlas never runs `docker compose down`.

## Which source for which question

| Question | Source |
|---|---|
| Everything running? | `atlas_status`, or `atlas dev --status` |
| Why did X fail? | `atlas_logs {"service": "X"}`, then the previous run via `atlas_logs_search` with `include_previous` |
| Did the action work? | `.logs/action-<slug>.log` |
| Leftover processes after a crash | `.logs/.pids` (cleaned automatically on next start) |
