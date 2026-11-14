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
| `atlas-audit.log` | audit trail of control calls, one JSON object per line (`ts`, `caller` human/agent, `client`, `session`, `tool`, `target`, `result` allowed/denied/error); previous run in `atlas-audit.prev.log`. Read it with `atlas_logs {"service": "audit"}` |
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

- `state`: `starting`, `running`, `degraded`, `unhealthy`, `failed`, `stopped`, `disabled`, `suspended`. A `disabled` service with `"deselected": true` (MCP: `"off": true`) was left out of this run by `atlas dev SERVICE…` / `--profile`; start it with `atlas_start_service`.
- `exit_code`: last exit of the process; `null` while running, never exited, or killed by a signal.
- `restarts`: attempts in the current streak. `uptime_s`: only while `running`. `started_at`: unix seconds of the last spawn.
- `note`: scheduler text: `waiting for :3000 (12s)`, `waiting for ready_log (12s)`, `start timeout: still waiting for ...`, `restarting in 2s (1/5)`, `dependency X failed`, `gave up after N restarts`.
- Health facts on every row: `ready` (`true` while `running`, `false` while `degraded` by a failing `ready:` probe, `null` without a probe), `probe_error` (why it is `degraded`: the probe's message or the repeating log line), `unhealthy_since` (unix seconds), `unhealthy_misses`, `unhealthy_lines` (last output lines when it hung; `[]` otherwise). MCP `atlas_status` returns the same keys.
- Memory facts on every row (`docs/runtime.md#status-file-memory-facts`): `memory_rss` (bytes of the process tree, whole MB; `null` when not running), `memory_trend` (`rising`/`flat`/`falling`) with `memory_trend_mb_min`, `budget_state` (`ok`/`warn`/`over` against `memory_warn`/`max_memory`) and `memory_leak` (`leak suspected: +600 MB over 12 reloads`, else `null`). The series behind them is `.logs/.tui-metrics.json` (`services.<name>.samples` = `[unix_s, bytes]`, one per 5 s, last hour); prefer MCP `atlas_metrics`.
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
- `EADDRINUSE` / `port N held by PID P (…)`: something else holds the port. `atlas_status` names it in `port_conflict` `{pid, cmd, cwd}`; Atlas never kills by port. Tell the user what holds it and let them stop it themselves or change the service port (on macOS with OrbStack that can be the docker port forwarder, never to be killed). Do not kill the PID yourself.

**`unhealthy` (hung, not failed).** The process is alive but missed `failure_threshold` liveness probes in a row. `unhealthy_lines` is its last output at that moment: look for the stall (a pending DB call, a lock, an exception loop). `unhealthy_since` says how long. `atlas dev --status` prints the same under the row. It clears when the service answers again; with `on_unhealthy: restart` Atlas restarts it once per episode within the `max_restarts` budget. A restart rotates the log, so use `include_previous` to see the run that hung.

**`degraded` (answers, not usable).** `probe_error` is the reason: `ready probe failed: GET /ready → 503`, `connection refused`, `timed out after 5s`, `` `cmd` exited 1: <stderr> ``, or `repeating error: <line>` (the same error 3 times in 60 s). Fix what it names (usually a dependency), not the service's port. `atlas dev --status` shows a READY column (`ok`/`fail`/`-`) when any service has a `ready:` probe.

**Starts, then fails.** `ECONNREFUSED 127.0.0.1:5432` means the database was not ready (check `depends_on` and infra). A missing env var points to `env_files` or `env:`. `dependency X failed` means fix X first. `gave up after N restarts` means the `restart:` budget is spent; read the log of the last attempt, then `atlas_restart_service` resets it.

**Docker/infra.** Compose output is not written to a file. A failure shows as a `docker` critical alert in the TUI (stderr under `--headless`). Run `docker compose -f <compose_file> up -d` by hand to see the real error. Quitting atlas never runs `docker compose down`.

## Which source for which question

| Question | Source |
|---|---|
| Everything running? | `atlas_status`, or `atlas dev --status` |
| Why did X fail? | `atlas_logs {"service": "X"}`, then the previous run via `atlas_logs_search` with `include_previous` |
| Did the action work? | `.logs/action-<slug>.log` |
| Leftover processes after a crash | `.logs/.pids` (cleaned automatically on next start) |
