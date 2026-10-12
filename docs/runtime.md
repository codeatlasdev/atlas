---
title: "Runtime"
description: "Startup order, health, restarts, shutdown, orphan cleanup and the .logs files."
---

# Runtime behavior

How `atlas dev` starts, supervises and stops your environment, and what it leaves in `.logs/`. The same manager drives the TUI, the background runner (`--detach`), `--headless` and `--ci`.

## Startup order

1. **Config** is loaded and validated (`atlas.yaml` + `atlas.local.yaml`). Errors abort before anything starts. A `SERVICE…` / `--profile` / `--no-deps` selection is resolved here too (unknown names abort, see [Selecting services](cli.md#selecting-services)).
2. **Singleton lock** `.logs/.tui.lock` is taken, see [below](#singleton-and-the-control-socket). Then the control socket `.logs/atlas.sock` is bound.
3. **`pre_command` / dependency install** (same staleness logic as [configuration](configuration.md#pre_command)). **TUI**: the dashboard draws immediately (splash first) and the install runs in the background, but app services are held back until it finishes (success or failure; a failure shows an error toast and the services start anyway). **`--headless` / `--ci`**: it runs here, before anything is started; a failure aborts `--ci` (exit `1`, JSON `reason`) and is only logged by `--headless`.
4. **Orphan cleanup**: leftovers from a crashed previous run are killed, see [Orphan cleanup](#orphan-cleanup).
5. **Infra** (when there is an `infra:` block or command-less services with a port), in the background so the UI stays responsive:
   1. `docker compose -f <compose_file> up -d` after waiting up to 15 s for the docker daemon (one retry after 2 s);
   2. every command-less service with a `port` is polled (500 ms) until the port accepts connections, up to 30 s, all in parallel. Reachable means `running`, otherwise `failed`.
6. **App services** (those with a `command`) are queued once infra is done, whether it succeeded or not (a compose failure raises a `docker` critical alert and marks the infra services `failed`, but apps still start).
7. The **scheduler** spawns each queued service as soon as all its [`depends_on` conditions](configuration.md#depends_on) hold; services without dependencies start immediately and in parallel. A dependency that can never be satisfied fails its dependents.
8. Atlas writes `.logs/.tui-status.json` and then refreshes it every second.

In parallel with 4–7, when a `tunnel:` block is present and enabled, the [SSH tunnel](#ssh-tunnel) is started on its own background task; services never wait for it.

With no infra to wait for, steps 5–6 collapse and app services start right away.

## How a service is spawned

- `sh -c "<command>"` in the project root (or the service's `dir`), stdin is `/dev/null`, stdout and stderr are piped line by line.
- The child gets its own **process group**. Stopping (stop, restart, quit) also kills its **task tree**: Atlas snapshots the process table once (`ps -ax -o pid=,ppid=,pgid=`), collects every descendant of the child, and sends SIGTERM to the child's group plus each descendant's own pid and group, then SIGKILL to whatever is left after the grace period. This is what ends tools like `bun run dev` → `turbo`, which start their tasks in *new* process groups (a plain group kill would leave them running and holding ports). Atlas never signals PID 1, itself or its ancestors, and never selects processes by port. <a id="service-processes"></a>
- Environment: inherited < `env_files` < service `env:` < `PORT` (when `port` is set), `FORCE_COLOR=1`, `BROWSER=none`, with `NO_COLOR` removed. The inherited environment is passed through **unscrubbed** (no allowlist), so a variable exported in your shell reaches every service; because `env_files` rank above it, a key present in both is taken from the file.
- Output is logged to the TUI buffer and to `.logs/<service>.log`, and checked against `ready_log` when set.

## Service states and health

| State | Meaning |
|---|---|
| `starting` | spawned (or infra port not up yet), not ready |
| `running` | ready (see below) |
| `failed` | exited abnormally, probe failed, dependency failed, or restart budget spent |
| `stopped` | not running; also a finished one-shot (`done`) |
| `disabled` | stopped by you (`s`, MCP stop), or left out of this run by `SERVICE…` / `--profile` (shown `off`); skipped by health checks, never auto-restarted |
| `suspended` | paused by an action's `pause_services` |

Readiness and health, per service:

- With `ready_log`: the service stays `starting` (a live child is **not** enough) until the first matching output line makes it `running`; afterwards a live child counts as healthy and no port probe is made.
- Otherwise, a **real HTTP `GET`** of the `health` URL (2 s timeout, redirects not followed; 2xx/3xx = healthy, 4xx/5xx/timeout/refused = not; `https://` supported), or a TCP connect to `port`, or (with neither) the child process being alive. A service with a command but no port, no health and no `ready_log` is `running` after it has been alive for 2 seconds (minimum uptime), so a crash loop never flashes green. The same probe is used by the per-service startup watcher and the one-second health tick.
- A watcher polls every 500 ms while `starting` and every 3 s once `running`; it only flips a service to `failed` after it was healthy at least once, so slow starts are not reported as failures. A one-second health tick also reaps exited children and refreshes the status file.
- A service whose spawned child died is `failed` even if something else answers on its port (an orphan or another tool holding it). The exception is a launcher that exits `0` (`docker compose up -d`) on a service with a port/health: there the probe decides.
- Command-less services are only port-watched.

### Start timeout

A spawned service that waits on a port, `health` URL or `ready_log` shows the sidebar note `waiting for :3001 (12s)` / `waiting for ready_log (12s)`. After `start_timeout` seconds (default 60) it logs one `still starting after Ns: waiting for …` note, the note becomes `start timeout: still waiting for …`, and a `critical` service raises the critical alert. Nothing is killed; the service keeps trying and turns `running` as soon as the probe passes.

### Per-service facts

Per service Atlas tracks the last **exit code** (none while running or when killed by a signal), the **restart count** of the current streak, the **uptime** (while `running`) and the spawn time. They are in `.logs/.tui-status.json` (`exit_code`, `restarts`, `uptime_s`, `started_at`, `note`, `autostart`, `deselected`), in the MCP `atlas_status` rows and (`exit_code`, `restarts`) in `atlas dev --ci` JSON.

### Selected runs (`SERVICE…`, `--profile`)

`atlas dev web` / `--profile backend` start the selection and its transitive `depends_on` (see [Selecting services](cli.md#selecting-services)). Everything else is tracked from the beginning as `disabled` with `deselected: true` in `.logs/.tui-status.json` (`off` in `atlas dev --status`, the sidebar and the MCP rows; `"off": true` in `--ci` JSON): never started at boot or on *restart all*, ignored by health checks and by `--ci`'s verdict, and it never gates a dependent (with `--no-deps` a selected service starts without waiting for its unselected dependencies). Starting one by hand (service menu, palette, `atlas_start_service`) clears the mark and it behaves like any other service from then on. Infra: a command-less service nothing selected depends on is not probed, and when anything is deselected and `atlas.yaml` models infra as services, `docker compose up -d` runs only if a selected service needs one; a project that does not model its compose services as atlas services keeps running compose as before.

### Lazy start (`autostart: false`)

Such a service is skipped when the stack boots (and on *restart all*) and stays `stopped` until started from the service menu, the command palette or `atlas_start_service`.

## Restarts

See [Restart policy](configuration.md#restart-policy) for the config. At runtime: the exit is decided in exactly one place (the health tick that reaps the child), so a crash can never double-spawn; backoff is 1 s, 2 s, 4 s … capped at 30 s; the counter resets after 60 s of continuous `running`; user stop, restart and quit cancel any scheduled restart and reset the history. While a restart is pending the service shows as `starting`, with no critical alert.

## Actions

Non-session actions run `sh -c` in the project root, one at a time per action. Their stdout/stderr is **persisted**: written to `.logs/action-<slug>.log` (`<slug>` = the label lowercased, non-alphanumerics collapsed to `-`, e.g. `Run checks` → `action-run-checks.log`; the previous run is rotated to `.prev.log`) and pushed into the log buffer under the service name `action:<label>`, so the lines show up in the **All** tab and survive the operation row disappearing. MCP `atlas_logs` / `atlas_logs_search` read them with `service: "action:Run checks"` (or `action-run-checks`), and without `service` they are merged with the service logs. Under `--headless`, action output is printed as `[action <label>] <line>` and completion as `✓ action <label>: done` / `✗ … failed` on stderr. `pause_services` that are `running` or `starting` are stopped first (state `suspended`, no auto-restart) and exactly those are restarted when the command ends, even if it failed; a listed service that was already stopped, failed or disabled is left alone and stays that way. An `open_tunnel` action that had to open the tunnel itself closes it again when it ends (see [SSH tunnel](#ssh-tunnel)). Session actions additionally wait up to 5 s for the paused services' ports to free before spawning. Sessions and their children are stopped on quit. Session actions with a `platform` also get `TAURI_DEV_HOST` / `ATLAS_DEV_HOST` in their environment, resolved from [`host_mode`](configuration.md#host_mode) off the async runtime (never overriding a value you set).

## SSH tunnel

Configured by [`tunnel:`](configuration.md#tunnel). One `ssh -N -L local:remote_host:remote_port ssh_host` child (with `ExitOnForwardFailure`, the configured keepalive options and `BatchMode=yes`), spawned in its own session with stdin `/dev/null`. A dedicated task owns it, so spawning, the readiness wait (local port accepts TCP, ≤ 15 s) and reconnects never block the UI or the 1 s health tick.

| Situation | Behavior |
|---|---|
| `enabled` (default) | started at boot; sidebar **TUNNEL** shows `starting` → `running` |
| `enabled: false` | not started; an action with `open_tunnel: true` starts it on demand (state shown the same way) and waits up to 15 s before running the command. When that action ends, success or failure, the tunnel is **closed again** (SIGTERM to Atlas's own ssh, no `.tunnel.pid` left). If several `open_tunnel` actions overlap they share the tunnel and the last one to finish closes it. A tunnel that was already up (boot-enabled, or opened earlier and still in use by another running action) is never closed by an action |
| ssh exits or the port stops answering | state `failed`, reconnect with backoff 5 s, 10 s, 20 s … capped at `reconnect_cooldown`; resets after a success |
| default gateway changes (macOS) | immediate reconnect, ignoring the backoff |
| start fails | one critical alert `tunnel` per failure episode, message ends with ssh's last stderr line; ssh stderr also in the `tunnel` log tab and `.logs/tunnel.log` |
| port already served by a process Atlas did not start | used as is (state `running`), never killed |

Atlas only ever signals a pid it spawned itself, or a pid from `.logs/.tunnel.pid` that passes the checks below. It never kills by port or by command-line pattern.

**On exit** (`tunnel_kill_on_exit`, default on, `t` in the TUI): on → SIGTERM to the ssh Atlas owns. Off → the ssh keeps running and its pid, `ssh_host` and `local_port` are written to `.logs/.tunnel.pid`; it is not part of `.pids` orphan cleanup. `--headless` and `--ci` always kill on exit.

**Reuse on next start.** `.logs/.tunnel.pid` is honored only if the `ssh_host` and `local_port` match the current config, the pid is alive, the process is `ssh`, and the local port accepts connections. Otherwise the record is deleted (nothing is killed) and a fresh tunnel starts.

**Testing without a real host.** `ATLAS_SSH_BIN=/path/to/fake-ssh` replaces the `ssh` executable (the integration tests use a script that parses `-L` and listens on the local port). It is for tests only; leave it unset normally.

## Stopping

| Trigger | What happens |
|---|---|
| `s` / MCP stop / menu Stop | SIGTERM to that service's whole [task tree](#service-processes), polled every 50 ms; SIGKILL after 2 s if anything is left. State becomes `disabled`. Restart history is cleared. |
| restart (`S`, `r` on a tab, MCP restart/start) | stop (as above, state `stopped`), then spawn again. |
| `r` on **All** | stop everything, then run the whole startup sequence again (orphan cleanup, infra, scheduler). |
| quit (`q`+confirm, `Ctrl+C`, SIGTERM, SIGHUP; `--headless`: Ctrl-C/SIGTERM/SIGHUP) | open sessions are stopped; all restarts and queued starts are cancelled; every service's whole [task tree](#service-processes) gets SIGTERM at once, a shared 800 ms grace period, then SIGKILL to survivors; `.logs/.pids` is removed. Then the [tunnel](#ssh-tunnel) exit policy applies. The control socket is removed on exit. |

Quitting **never runs `docker compose down`** (and never restarts the docker daemon): infra containers are left running, which is intended (`restart: unless-stopped` containers survive between sessions). Stop them yourself with `docker compose down` when you want them gone.

## Orphan cleanup

If Atlas is killed hard (SIGKILL, crash, power loss) its children can survive. On the next start Atlas cleans up in two ways, and only these two:

1. **`.logs/.pids`** lists the PIDs of all children Atlas spawned (rewritten after every spawn/stop). On start, each PID is killed **by exact PID** (never the root's process group, never by port), with SIGTERM then SIGKILL after 300 ms, but only if:
   - it is not Atlas itself or its parent, and not PID 1;
   - its executable name is not on a blocklist of things a recycled PID could be: `fish`, `zsh`, `dash`, `orb*`, `docker`, `containerd`, `launchd`, `systemd`, `atlas`.
   For each verified root, its **descendants** (found by walking the parent/child process tree from one `ps` snapshot) are killed as well, so a `turbo` task that moved into its own process group does not survive holding its port. Descendants are only collected below a verified root; nothing is ever selected by port.
   Then the file is deleted.
2. **`lock_file`**, see below.

Atlas deliberately never kills "whatever is listening on the configured port": on macOS with OrbStack that would hit the container runtime's port forwarder.

### Lock files (`lock_file`)

Some dev servers keep a JSON lock `{"pid": N}` (e.g. `apps/ai/.mastra/dev.lock`). Before the service starts (and during orphan cleanup) Atlas reads it and:

| Lock owner | Action |
|---|---|
| unparsable, or process dead | delete the lock file |
| alive, working directory inside the project, executable not blocklisted, **and** started no later than the lock file was written | SIGTERM, wait up to 2 s, SIGKILL, delete the file |
| alive but outside the project, or Atlas itself / its parent | leave everything alone |
| alive, inside the project, but looks like a recycled PID (blocklisted name, or started after the lock was written) | delete the stale lock only; the process is not touched |

## Singleton and the control socket

One `atlas dev` per project directory, enforced with an exclusive `flock` on `.logs/.tui.lock` (the file holds the owner's PID).

| Mode | Another instance is running |
|---|---|
| TUI, other instance is a **runner** (`--detach`, `--headless`) | **attaches** to it, see [Runner and attach](#runner-and-attach); nothing is killed |
| TUI, other instance is a **TUI** | prompts `atlas dev already running (PID N). Kill it and restart? [y/N]` *before* entering raw mode. `y`: SIGTERM, up to 3 s, then SIGKILL, and take over. Anything else: exit, keeping the existing one. The PID is only killed if it really is an `atlas` executable. |
| `--headless`, `--ci` | print `atlas dev is already running (pid N); use 'atlas dev --status'` and exit `1`; nothing is started and nothing is killed |
| `--detach` | runner already up: prints `atlas runner already running (pid N) …`, exit `0`. A TUI owns the project: exit `1` |

The lock is also what makes orphan cleanup safe: only the lock holder ever runs it, so a second run (CI, headless) can never kill a live instance's services.

The **control socket** `.logs/atlas.sock` (mode `0600`) is bound only after the lock is held, replacing any stale file, and removed on exit. The TUI, the runner and `--headless` serve it; `--ci` does not. Protocol: one JSON object per line, `{"cmd": "...", "args": {...}}` → `{"ok": bool, "data"?, "error"?}`, with `restart_service`, `start_service`, `stop_service` (arg `service`) and `run_action` (args `action`, `confirmed`) for [`atlas mcp`](mcp.md) and attached TUIs, plus `info`, `shutdown` and `subscribe` (see [Runner and attach](#runner-and-attach)). A request longer than 64 KiB is rejected, and one not answered within 30 s gets a timeout error.

`--headless` additionally watches the docker daemon every 5 s and prints/raises a `docker` critical alert when it becomes unreachable and again when it recovers. It does not try to restart docker.

## Runner and attach

Services can outlive the terminal. The **runner** is the headless engine (services, scheduler, health checks, control socket, status heartbeat, disk logs) running as a daemon; a TUI is then just a client of it.

```text
atlas dev --detach     start the runner and return        (prints pid, exit 0)
atlas dev              attach a TUI to the runner          (same dashboard)
  D / q → Detach       leave; services keep running
atlas dev              ...reattach any time, from any terminal
atlas down             stop everything gracefully         (waits up to 15 s)
```

**Starting.** `--detach` loads and validates `atlas.yaml` and the selection in the foreground (errors print on your terminal and nothing starts), then re-executes `atlas dev --runner` in a new session (`setsid`: no controlling terminal, so closing your shell can never SIGHUP it or stop it with SIGTTIN/SIGTTOU) with stdin `/dev/null` and stdout/stderr in `.logs/runner.log` (truncated on every start; it holds lifecycle messages such as `● api: Running`, not service output, which is in the per-service disk logs). `--detach` returns once the runner answers on the socket, after the lock is held and before dependency install and infra are done (those proceed in the runner). If the runner exits during startup the last lines of `runner.log` are printed and the exit code is `1`. `--detach` never goes through the kill-prior path: it is refused or a no-op when an instance exists.

**Who is a runner.** `--detach` and a foreground `--headless` run are the same engine; both report `mode: runner` in the status file and on `info`, and both can be attached to and stopped with `atlas down`. The only difference is stdout: `--headless` echoes service output, the daemon does not. An owner TUI reports `mode: tui` and refuses `subscribe`.

**Attaching.** Plain `atlas dev` first asks the lock holder who it is (`info`). A runner is attached to: the client keeps its own terminal, never takes the lock, never runs `kill_orphans`, never writes the status file or `.pids`, never stops services when it exits and loads no services of its own. An owner TUI keeps the kill/keep prompt. The client opens a `subscribe` connection (read-only stream) and sends its actions (`restart_service`, `stop_service`, `run_action` with `confirmed: true` after its own confirm modal) on separate short connections, the same requests MCP uses, so every guard (`already running`, `pause_services` resume by the runner) applies.

**Protocol (`subscribe`).** The reply is `{ok: true, data: <snapshot>}`; after it the connection pushes one JSON line per event until either side closes. The runner builds the snapshot and registers the stream in the same loop turn, so no event is lost or repeated between them, and events keep arrival order.

| Snapshot field | Meaning |
|---|---|
| `project`, `pid`, `mode`, `version`, `started` | who and whether app services were queued |
| `services` | one row per service: `name`, `state`, `exit_code`, `restarts`, `note`, `completed_ok`, `paused_by`, `deselected` |
| `tunnel` | tunnel object or `null` |
| `logs` | the runner's in-memory tail, up to 1000 lines **per service**, oldest first (`name`, `line`, `stream`, `ts`) |
| `events` | the last 50 side events (`state`, `note`, `alert`, `tunnel` lines as below), replayed to rebuild the EVENTS block |
| `operation` | the action in flight, if any |

| Event (`t`) | Payload |
|---|---|
| `log` | `name`, `line`, `stream` |
| `state` | `name`, `state` + the same facts as a service row, as of the transition |
| `rows` | all service rows + `tunnel`; sent when something a row shows changed (at most once a second, e.g. the changing sidebar note `waiting for :3001 (12s)`) |
| `note`, `alert` | scheduler narration / critical alert |
| `tunnel` | `state` + tunnel object |
| `action_progress`, `action_done` | non-session action output line / completion (the runner resumes paused services itself) |
| `started` | all app services queued |
| `bye` | the runner is shutting down; the stream then closes |

A client that disconnects (clean or not) is dropped on the next write or one-second upkeep and never affects the runner or other clients; `atlas dev --status` shows the live count. The same `LogBuffer`/`apply_manager_event` code that renders an owner TUI renders an attached one: events from the stream are fed into the App as ordinary manager events and the manager is a *mirror* (config-derived data real, state and facts from the runner, nothing spawned).

**Limits.** Interactive sessions (`session: true`) and the mobile device picker are owner-only (the runner refuses them with a clear error; the TUI shows a toast). The runner always kills the tunnel on exit. A TUI that owns its services cannot hand them to a runner while running: start the runner first (`atlas dev --detach`).

**Stopping.** `atlas down`, the **Stop everything** choice in an attached TUI, SIGTERM/SIGHUP on the runner (and Ctrl-C of a foreground `--headless`) all run the normal [stop sequence](#stopping); clients receive `bye`.

## Files in `.logs/`

`.logs/` is created in the project root. Add it to `.gitignore`.

| File | Written by | Content |
|---|---|---|
| `<service>.log` | every spawned service | `HH:MM:SS [stdout\|stderr] text`, ANSI stripped, one line per entry |
| `<service>.prev.log` | rotation | the previous run's log (or the previous 50 MB segment) |
| `.tui-status.json` | manager, every second | status snapshot (below). Written atomically (temp file + rename). |
| `.tui.lock` | the running atlas | singleton `flock`, holds the PID |
| `.pids` | manager | PIDs of live children, one per line; removed on clean quit |
| `tunnel.log` | tunnel task | ssh stderr and reconnect notes (same line format; rotated to `tunnel.prev.log` per run) |
| `.tunnel.pid` | tunnel task | only when a tunnel is left running on exit: pid, `ssh_host`, `local_port` (one per line) |
| `atlas.sock` | TUI / runner / `--headless` | control socket |
| `runner.log` | `atlas dev --detach` | the daemonized runner's stdout/stderr (lifecycle messages, not service output); truncated on every start |
| `.atlas-ui.json` | TUI | per-project UI memory: active tab (by name), zoom, wrap/timestamps/JSON toggles, level filter, recent palette commands. Best-effort; a missing or corrupt file is a fresh start |

**Rotation.** When a service is (re)spawned, an existing `<service>.log` is renamed to `<service>.prev.log` (replacing the old one) and a fresh file is started. While running, a log that grows past 50 MB is rotated the same way. There is no further history: only the current and previous file exist. Command-less services and sessions do not write log files (non-session actions do: `.logs/action-<slug>.log`, see [Actions](#actions)).

**Status file** (`.tui-status.json`):

```json
{
  "pid": 4242,
  "mode": "runner",
  "clients": 1,
  "session": "4242-1767225600",
  "started_at": 1767225600,
  "updated_at": 1767225660,
  "log_dir": ".logs",
  "services": {
    "api": { "state": "running", "port": 3001, "health": "http://localhost:3001/health" },
    "web": { "state": "starting", "port": 3002, "health": null }
  },
  "tunnel": { "state": "running", "local_port": 54320, "remote": "localhost:5432", "ssh_host": "prod-db" }
}
```

`mode` is `runner` (`--detach` / `--headless`, attachable) or `tui`; `clients` is the number of attached TUIs. `tunnel` is `null` when no tunnel is running (none configured, or `enabled: false` and not yet opened on demand).

`state` is lowercase (`starting`, `running`, `failed`, `stopped`, `disabled`, `suspended`). `updated_at` is a Unix timestamp, the heartbeat: [`atlas dev --status`](cli.md#--status) and MCP treat the run as alive only if `pid` exists and the heartbeat is less than 120 s old, so a file left by a crash is correctly reported as "not running".

## Exit behavior summary

| Command | Exit |
|---|---|
| `atlas dev` quit normally / detached | `0` |
| `atlas dev --detach` | `0` started or already running / `1` failed |
| `atlas down` | `0` stopped or not running / `1` still alive after 15 s |
| `atlas dev` without a terminal | `2` |
| `atlas dev --status` | `0` running / `3` not running |
| `atlas dev --ci` | `0` ok / `1` not ok / `130` interrupted |
| `--headless` / `--ci` with another instance | `1` |
