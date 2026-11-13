---
title: "Configuration"
description: "Every atlas.yaml field, local overrides, validation and the JSON Schema."
---

# Configuration: `atlas.yaml`

`atlas.yaml` is optional. Atlas looks for it in the directory you run it from (`atlas dev --dir <path>` to point elsewhere); `atlas.local.yaml` next to it is merged on top for per-machine overrides. Without one, see [No atlas.yaml](#no-atlasyaml-detected-config).

Contents: [Top level](#top-level-fields) · [services](#services) · [depends_on](#depends_on) · [restart policy](#restart-policy) · [profiles](#profiles) · [infra](#infra) · [tunnel](#tunnel) · [actions](#actions) · [mobile](#mobile) · [env_files](#env_files) · [local overrides](#local-overrides-atlaslocalyaml) · [validation](#validation) · [JSON Schema](#json-schema-editor-autocomplete) · [full example](#full-example)

Unknown keys are errors at every depth (with a "did you mean" hint), so a typo never silently does nothing.

## No atlas.yaml: detected config

`atlas dev` (TUI, `--detach`, `--headless`, `--ci`), `atlas down`, `atlas mcp` and Atlas Desktop work in a folder that has no `atlas.yaml`. Atlas builds the config in memory from what it finds, the same one `atlas init` would write:

| Found | Services |
|---|---|
| `package.json` | every script named `dev`, `dev:*`, `start`, `start:*` or containing `serve` (command = the script's body, port from `--port N` / `PORT=N` when present, else from the `4010–4099` block) |
| `pyproject.toml` / `requirements.txt`, `Cargo.toml`, `go.mod` | the language's usual run command, when one is detected |
| nothing | no services: the stack starts empty, and `atlas -- <cmd>` adds [ad-hoc services](cli.md#atlas----cmd) to it |

Every run on a detected config prints one line, e.g. `no atlas.yaml: using detected config (services: dev); save it with 'atlas init'`. **Nothing is written implicitly**; `atlas init` (or "Save as atlas.yaml" in Atlas Desktop) writes exactly what was detected, after which the file is the source of truth. `atlas.local.yaml` still layers on top of a detected config, and an `atlas.yaml` that exists is never replaced by detection (a broken one is an error, not a fallback). `atlas config check` and `atlas deploy` still need a real file.

## Top-level fields

| Field | Type | Default | Description |
|---|---|---|---|
| `name` | string | **required** | Project name (shown in the TUI, used in copied logs, returned by MCP). |
| `services` | map | **required** | Service name → [service definition](#services). Order in the file does not matter; Atlas sorts by `depends_on`. |
| `pre_command` | string | auto-detect | Shell command run in the background before the dashboard needs dependencies. See [pre_command](#pre_command). |
| `pre_command_when` | `always` \| `lockfile` | `always` with a `pre_command`; `lockfile` when auto-detecting | When `pre_command` runs. See [pre_command](#pre_command). |
| `profiles` | map | `{}` | Named service groups for `atlas dev --profile NAME`. See [profiles](#profiles). |
| `infra` | object | none | [Docker compose](#infra) file started before app services. |
| `tunnel` | object | none | [SSH tunnel](#tunnel) opened at boot and supervised. |
| `actions` | list | `[]` | [Custom key-bound commands](#actions). |
| `env_files` | string list | `[]` | [Dotenv files](#env_files) injected into every service and action. |
| `mobile` | object | `{}` | [iOS/Android settings](#mobile) for `platform:` actions (normally in `atlas.local.yaml`). |
| `deploy` | object | none | Release engine for `atlas deploy`. See [deploy.md](deploy.md). |

### pre_command

Atlas runs it in the project root (through `sh -c`). Whether it runs on a given start depends on [`pre_command_when`](#pre_command_when) (below). Same logic in every mode:

- **TUI** (`atlas dev`): in the background while the dashboard opens; app services wait for it to finish before they start (infra still comes up meanwhile). A "Dependencies installed" toast on success; on failure an error toast `pre_command failed: exit N: <last stderr line>` and the services start anyway (they fail with their own errors if dependencies are really missing).
- **`--headless`**: runs **before** any service starts; success prints `dependencies installed`, failure is logged (`pre_command failed: …`) and startup continues.
- **`--ci`**: runs **before** any service starts; a failure aborts the run with exit code `1` and JSON `{"ok": false, "reason": "pre_command failed: exit N: …", "services": []}`.

- **Omitted**: auto-detect, JS projects only (a `package.json` must exist; Rust/Go projects never run it). If `node_modules` is missing, or a lockfile (`bun.lock`, `pnpm-lock.yaml`, `yarn.lock`, `package-lock.json`) is newer than `node_modules/.atlas-installed` (or `node_modules` itself), Atlas runs `bun install`, `pnpm install --frozen-lockfile`, `yarn install --frozen-lockfile` or `npm ci`, picked by which lockfile exists (bun, pnpm, yarn, else npm).
- **Non-empty string**: run that command instead of the auto-detected one. By default (`pre_command_when: always`) it runs on **every** start, so `pre_command: bun install` does what you expect even when `node_modules` is fresh. With `pre_command_when: lockfile` it runs only under the staleness check above (your command must then create `node_modules`, or it re-runs on every start).
- **`""`** (empty string): never install. Typical in `atlas.local.yaml` when you manage dependencies yourself. Not even "Run now" runs it.

#### pre_command_when

- `always` (default when `pre_command` is a non-empty string): run on every start.
- `lockfile`: run only when `node_modules` is missing or a lockfile is newer than `node_modules/.atlas-installed`.
- The auto-detected installer (no `pre_command`) **always** uses `lockfile`, whatever this field says: `npm ci` deletes `node_modules`, so it is never run unconditionally.
- May be overridden in `atlas.local.yaml` (replaces the base value).

#### Output and "Run now"

Every run that actually executes writes its combined stdout+stderr to `.logs/setup.log` (first line `$ <command>`, last line `exit status: N`; truncated per run) and its state to `.logs/setup.json` (`{"state": "running" | "ok" | "failed", "command", "started_at", "finished_at", "reason"}`). A run skipped by the gate leaves both files untouched. Atlas Desktop shows the command, schedule, last status and the output in a **Setup** section of the project overview, with a **Run now** button that runs the command once regardless of `pre_command_when`.

## services

```yaml
services:
  database:                 # no command: externally managed
    port: 5432

  api:
    command: cd apps/api && air
    dir: apps/api           # optional working directory (relative to project root)
    port: 3001
    health: http://localhost:3001/health
    depends_on: [database]
    env: { LOG_LEVEL: debug }
    open_key: a
    critical: true
    critical_help: "Run `bun run db:up` and retry"
```

| Field | Type | Default | Description |
|---|---|---|---|
| `command` | string | none | Shell command, run as `sh -c "<command>"` in the project root (or `dir`), so `cd`, `&&`, pipes and redirects work. **Omitted = externally managed** (docker, a system service): Atlas never spawns it and only watches its `port`. |
| `dir` | string | project root | Working directory, relative to the project root. |
| `port` | integer (u16) | none | TCP port. Used for readiness/health probing, shown in the sidebar, injected into the child as `PORT`, and default target of `open_key`. |
| `health` | string | none | HTTP(S) URL used as the health probe instead of the plain port check. Atlas sends a real `GET` (2 s timeout, redirects not followed): **2xx and 3xx are healthy**, 4xx/5xx, timeouts and connection errors are not. `https://` works. |
| `ready_log` | regex | none | A stdout/stderr line (ANSI stripped) matching this Rust regex marks the service ready. It replaces the port/health probe for that service and satisfies `log_ready` dependents. Must be a valid regex. |
| `depends_on` | list or map | none | Start gate, see [depends_on](#depends_on). |
| `restart` | `no` \| `on_failure` \| `always` | `no` | See [restart policy](#restart-policy). |
| `max_restarts` | integer | `5` | Consecutive restarts before giving up (`Failed`). `0` = unlimited. |
| `oneshot` | bool | `false` | The command is a task that exits (migration, seed). Exit `0` is success (`stopped`, shown as `done`), not a failure. Implied when any dependent uses `completed_successfully`. |
| `start_timeout` | seconds | `60` | How long the service may stay `starting` (port, `health` or `ready_log` not satisfied). After that Atlas adds a one-time warning note (`still starting after 60s: waiting for :3001`) and, when `critical: true`, fires the critical alert. The service is **not** killed. Until then the sidebar shows `waiting for :3001 (12s)` / `waiting for ready_log (12s)`. `0` disables the timeout. |
| `autostart` | bool | `true` | `false` = not started at boot; the service sits `stopped` until you start it from the service menu (`m`), the command palette or MCP (`atlas_start_service`), or name it on the command line (`atlas dev lab`, or through `--profile`; a lazy service pulled in only as a dependency still waits). Start does not wait for `depends_on`. `atlas config check` warns when an autostarting service depends on one that is not. |
| `critical` | bool | `false` | When the service is `Failed`, the sidebar shows a blocking **ISSUES** entry with the service's last log line. |
| `critical_help` | string | none | Hint shown under that alert (only meaningful with `critical: true`). |
| `enabled` | bool | `true` | `false` skips the service entirely. Mostly used from `atlas.local.yaml`. |
| `env` | map string→string | `{}` | Extra environment variables for this service only. |
| `open_key` | single character | none | Dashboard key that opens the service in the browser. |
| `open_url` | string | `http://localhost:<port>` | URL opened by `open_key` instead of the default. |
| `hide_logs` | bool | `false` | No dedicated tab for this service, and its lines are left out of the **All** tab. Use for quiet infra like databases. Logs are still written to `.logs/<name>.log`. |
| `lock_file` | string | none | Path (relative to project root) to a JSON file holding `{"pid": N}`, e.g. `apps/ai/.mastra/dev.lock`. Atlas cleans stale locks before the service starts, see [lock files](runtime.md#lock-files-lock_file). |
| `max_memory` | size (`512M`, `4G`) | none | Hard memory ceiling for the service's whole process tree. Atlas samples memory and CPU every 5 s (on macOS the physical footprint, compressed pages included, as in Activity Monitor); over the ceiling for two samples in a row, `on_memory_max` decides: `restart` (default) restarts it with an alert and, after 3 restarts in a run, stops it and marks it failed; `notify` only alerts. A guard for leaking dev servers. See [Memory](#memory). Usage is shown per service in Atlas Desktop and in `atlas_status` (`mem_mb`, `cpu`). |
| `memory_warn` | size (`512M`, `1.5GB`) | none | Soft budget, below `max_memory`: over it for two samples in a row the service turns `degraded` with the reason (`using 2.1 GB, over memory_warn 1.5 GB`) and nothing is killed. See [Memory](#memory). |
| `on_memory_max` | `restart` \| `notify` | `restart` | What Atlas does when the service stays over `max_memory`: `restart` (bounded to 3 per run, then `failed`) or `notify` (one alert, left running and `degraded`). |
| `health_timeout` | seconds | `2` | How long one liveness probe (the `health` GET, or the TCP connect to `port`) may take before it counts as a miss. See [Health and readiness](#health-and-readiness). |
| `failure_threshold` | integer | `3` | Liveness misses in a row, while the process is alive and after it answered once, before the service turns `unhealthy`. `0` is treated as `1`. |
| `ready` | object | none | Readiness probe, separate from liveness: `http` (path or URL) with an optional `status`, or a `command`, plus `timeout`. While liveness passes but this fails, the service is `degraded`. See [Health and readiness](#health-and-readiness). |
| `on_unhealthy` | `notify` \| `restart` | `notify` | What Atlas does when the service turns `unhealthy`: `notify` shows the state, raises an alert and keeps the log context; `restart` also restarts it (same budget and delays as `max_restarts`). |

Notes:

- **Command-less services** ("infra" services) with a `port` are probed after `docker compose up -d`: Atlas waits up to 30 s for the port to accept connections, then marks the service `running` or `failed`. A command-less service without a port is never probed.
- **Environment** of a spawned service, lowest to highest precedence: inherited environment < [`env_files`](#env_files) < the service's `env:` < variables Atlas injects (`PORT` when `port` is set, `FORCE_COLOR=1`, `BROWSER=none`; `NO_COLOR` is removed). Stdin is `/dev/null`.
- **Readiness** without `ready_log`: a successful `GET` of the `health` URL, else a TCP connect to `port`, else the process being alive. A command with no port, no health and no `ready_log` is `running` once it has stayed alive for **2 seconds** (a process that crashes inside that window, e.g. in a restart loop, never turns green and never satisfies a `healthy` dependent). `atlas config check` prints a non-fatal `warning:` (exit code stays `0`) when a `healthy` dependency targets such a service.
- Commands starting with `docker` get a `DOCKER_HOST` pointed at OrbStack's socket when present (unless `DOCKER_HOST` is already set).
- `open_key` and action `key` values must not collide with [built-in keys](tui.md#key-bindings) or each other; `atlas config check` rejects clashes.

### Health and readiness

Two separate questions, two separate settings:

- **Liveness** (is it answering?) is the `health` URL or `port` probe. Each probe waits `health_timeout` seconds (default `2`). Once the service has answered at least once, `failure_threshold` misses in a row (default `3`) while the process is still alive turn it `unhealthy`. A single answer resets the count, and the misses before the first answer never count (startup is [`start_timeout`](#services)'s business). A service with a `ready_log` is not port-probed.
- **Readiness** (is it usable?) is the optional `ready:` probe, run on every health tick **only while liveness passes** (a hung service's endpoints would time out too, and `unhealthy` already says so). A service without `ready:` behaves as before.

```yaml
services:
  api:
    command: bun run dev:api
    port: 24310
    health: http://localhost:24310/health     # liveness
    health_timeout: 2                         # seconds per probe (default 2)
    failure_threshold: 3                      # misses in a row before `unhealthy` (default 3)
    on_unhealthy: restart                     # notify (default) | restart
    ready:                                    # readiness
      http: /ready                            # path on the service `port`, or a full http(s):// URL
      status: 200                             # optional; default: any 2xx
      timeout: 5                              # seconds per probe (default 5)
  worker:
    command: bun run dev:worker
    ready:
      command: ./scripts/queue-ready.sh       # sh -c in the service `dir`; exit 0 = ready
```

| `ready` field | Type | Default | Description |
|---|---|---|---|
| `http` | string | none | A path (`/ready`), resolved against the service `port` on `127.0.0.1`, or a full `http://` / `https://` URL. A bare path without a `port` on the service reports `ready http '/ready' needs the service port`. |
| `status` | integer | any `2xx` | Exact status code expected from `http`. Any other code is a failure (`GET /ready → 503 (expected 200)`). |
| `command` | string | none | Shell command run as `sh -c` in the service's `dir` (project root when unset), stdin `/dev/null`; exit `0` = ready. A failure reports the exit code and the last non-empty stderr line. |
| `timeout` | seconds | `5` | How long one readiness probe may take (`` `cmd` timed out after 5s `` for a command). |

Set **exactly one** of `http` or `command`; `atlas config check` rejects both or neither. Unknown keys under `ready` are errors like everywhere else.

What the verdict is for a service with a live process:

| Liveness | `ready` / log | State |
|---|---|---|
| answers | passes (or none configured), no repeating error | `running` |
| answers | `ready` probe fails | `degraded`, reason `ready probe failed: <error>` |
| answers | the same error line 3 times within 60 s (numbers, timestamps, UUIDs and hex ids normalised, so `timeout after 31ms` and `timeout after 87ms` count as one) | `degraded`, reason `repeating error: <line>` |
| misses, fewer than `failure_threshold` | | unchanged (a dev server's hot restart closes its port for a moment) |
| misses `failure_threshold` in a row after it answered once | | `unhealthy` |

`unhealthy` beats `degraded`, and a failing `ready` probe beats a repeating log line (it carries the endpoint's own error). `degraded` clears by itself when the probe passes again or the error stops repeating; `unhealthy` clears when the service answers again. `unhealthy` is for a process that is **alive**: a process that exits is `failed`, as before, and a service that was never healthy is still `starting`. Concepts: [Service states](concepts.md#service). Runtime details: [Health states](runtime.md#unhealthy-and-degraded).

### Memory

Atlas samples the memory of each service's whole process tree every 5 s (on macOS the physical footprint, compressed pages included, as in Activity Monitor) and keeps the last hour (720 samples) per service. Two optional budgets act on it, and a leak watch needs no setting:

```yaml
services:
  web:
    command: bun run dev:web
    memory_warn: 1.5GB          # soft: over it for two samples in a row → `degraded`, nothing is killed
    max_memory: 4GB             # hard: over it for two samples in a row → on_memory_max
    on_memory_max: restart      # restart (default) | notify
```

| Setting | What happens |
|---|---|
| `memory_warn` | Over the budget for **two samples in a row** (~10 s; a build spike is not a state) the service turns `degraded` with the reason `using 2.1 GB, over memory_warn 1.5 GB`. Under it for two samples in a row it goes back to `running` by itself. Must be below `max_memory` (`atlas config check` rejects the reverse). Sizes parse like `max_memory`: `512M`, `1.5GB`, `2048`. |
| `max_memory` + `on_memory_max: restart` (default) | Over the ceiling for two samples in a row, the service is restarted with an alert (`using 5.0 GB (max_memory 4.0 GB), restarting (1/3)`); after 3 such restarts in a run it is stopped and marked `failed`. |
| `max_memory` + `on_memory_max: notify` | Over the ceiling for two samples in a row, Atlas raises one alert and leaves the service running, `degraded` with `using 5.0 GB, over max_memory 4.0 GB` until it drops under it. For services you would rather restart yourself (a database, a long build). |

Precedence with the other `degraded` causes: `unhealthy` beats everything; a failing `ready:` probe beats a repeating error, which beats the memory overrun (the least specific finding: the others say what is broken, this says it is big). While a probe error is the reason, memory dropping never flips the service to `running`; once the probe passes and memory is still over budget, the memory reason is what remains. A service that is not `running` (starting, failed, stopped) is never degraded by memory.

**Trend and leaks.** From the series Atlas derives a `memory_trend`: `rising`, `flat` or `falling` (a least-squares slope over the last 5 minutes, needs ~30 s of data; under ±2 MB/min is `flat`). It also watches the size at the end of every **reload** (a `[bun] reloaded`, `[vite] hmr update`, `✓ Compiled`, `[nodemon] restarting`, … line in the service's output; lines of one change count once) and every **restart**: a healthy dev server returns to about the same size after a reload, a leaking one grows every time. Four cycles in a row without a drop of more than 3 % and at least 64 MB of growth in total raise one alert, `leak suspected: +600 MB over 12 reloads` (again only after 4 more cycles of growth), and the text rides along in the service's note. A suspected leak is a warning, never a state of its own.

Where it shows: the sidebar row (`memory 2.1 GB · rising +42 MB/min`, `budget warn`, the leak), `atlas dev --status` (one `↳` line under a service with something to say), the Atlas Desktop card, and `memory_rss`, `memory_trend`, `memory_trend_mb_min`, `budget_state` and `memory_leak` in the status file and MCP [`atlas_status`](mcp.md#atlas_status); the whole series is MCP [`atlas_metrics`](mcp.md#atlas_metrics).

### depends_on

Two forms. The list form waits until each dependency is `healthy`:

```yaml
depends_on: [database, redis]
# identical to
depends_on: { database: healthy, redis: healthy }
```

The map form picks a condition per dependency:

| Condition | Satisfied when |
|---|---|
| `started` | the dependency's process has been spawned (for command-less services: its port wait has finished, either way) |
| `healthy` (default) | the dependency is `running`: port/`health` probe passed, or its `ready_log` matched |
| `completed_successfully` | the dependency's command exited `0`. The target needs a `command`; it is treated as `oneshot`. |
| `log_ready` | a line matching the dependency's `ready_log` appeared. The target must define `ready_log`. |

```yaml
services:
  migrate:
    command: bun run db:migrate         # exits 0 when done
    depends_on: { database: healthy }
  api:
    command: bun run dev:api
    port: 3001
    ready_log: 'listening on'
    depends_on: { migrate: completed_successfully }
  web:
    command: bun run dev:web
    depends_on: { api: log_ready }
```

Behavior:

- A service spawns only when **all** its conditions hold. Until then the sidebar shows `↳ waiting for migrate (completed_successfully)`.
- If a condition can never be met (the dependency failed, exited non-zero, or gave up restarting), the dependent becomes `failed` with reason `dependency <name> failed`, and the failure cascades down the chain. It never waits forever.
- A dependency that is merely restarting under its [restart policy](#restart-policy) is not a failure; dependents keep waiting.
- A `oneshot` that exits `0` ends as `stopped` (`done`) and counts as OK in [`atlas dev --ci`](cli.md#atlas-dev).

### Restart policy

```yaml
services:
  worker:
    command: bun run worker
    restart: on_failure   # no (default) | on_failure | always
    max_restarts: 5       # default 5, 0 = unlimited
```

| `restart` | Relaunches when the child exits… |
|---|---|
| `no` | never |
| `on_failure` | non-zero or killed; a clean exit `0` just finishes (`exited cleanly`) |
| `always` | on any exit |

- Only exits the child makes **on its own** trigger it. Stopping or restarting from the TUI/MCP, quitting, and services paused by an action's `pause_services` are never auto-restarted.
- Backoff before restart *n* (starting at 0): 1 s, 2 s, 4 s, 8 s, 16 s, then 30 s cap.
- The counter resets after the service stays `running` for 60 s.
- While waiting, the sidebar shows `↳ restarting in 4s (2/5)`; afterwards `↳ restarted ×N`. When the budget is spent the service is `failed` with `gave up after N restarts`.
- Avoid `restart: always` on a one-shot task: it is relaunched after every success.

## profiles

Named groups of services, selected with `atlas dev --profile NAME` (repeatable; profiles and positional `SERVICE`s are unioned):

```yaml
profiles:
  backend: [server, worker]
  front: [web]
```

```text
atlas dev --profile backend                    # server, worker + their depends_on (migrate, database, redis…)
atlas dev --profile backend --profile front    # union
atlas dev --profile front --no-deps            # web only
```

- Members must be names under `services:`; anything else is a config error with a "did you mean" hint (`profiles.backend[0]: unknown service 'servr' (did you mean 'server'?)`).
- A selection always includes the transitive [`depends_on`](#depends_on) of its members (infra such as `database` only when something selected needs it); `--no-deps` drops that. Services left out are shown `off` and can be started by hand. Details and exit codes: [`atlas dev` › Selecting services](cli.md#selecting-services).
- Without `--profile` / `SERVICE`, every service starts: profiles never change the default run.
- `atlas.local.yaml` can add or replace profiles by name (see [Local overrides](#local-overrides-atlaslocalyaml)).

## infra

```yaml
infra:
  compose_file: packages/infra/dev/compose.yml   # relative to the project root
```

| Field | Type | Description |
|---|---|---|
| `compose_file` | string, **required** | Compose file. Atlas runs `docker compose -f <file> up -d` (waiting up to 15 s for the docker daemon, one retry on failure) before starting app services. |

If compose fails, Atlas raises a critical alert (`docker`), marks command-less services with a port as `failed`, and still starts the app services. Quitting Atlas does **not** run `docker compose down`; see [Runtime](runtime.md#stopping).

## tunnel

```yaml
tunnel:
  enabled: false
  local_port: 54320
  remote_host: localhost
  remote_port: 5432
  ssh_host: prod-db          # entry in ~/.ssh/config
  keepalive_interval: 15
  keepalive_max: 3
  reconnect_cooldown: 30
```

| Field | Type | Default | Description |
|---|---|---|---|
| `enabled` | bool | `true` | `false` keeps the block but turns the tunnel off. |
| `local_port` | integer | **required** | Local port forwarded. |
| `remote_host` | string | **required** | Host on the remote side. |
| `remote_port` | integer | **required** | Port on the remote side. |
| `ssh_host` | string | **required** | SSH host alias. |
| `keepalive_interval` | integer | `15` | `ServerAliveInterval` (seconds). |
| `keepalive_max` | integer | `3` | `ServerAliveCountMax`. |
| `reconnect_cooldown` | integer | `30` | Seconds between reconnect attempts. |

When the block is present and `enabled` is not `false`, Atlas opens the tunnel **at boot, in the background** (the UI never waits for it):

```
ssh -N -o ExitOnForwardFailure=yes -o ServerAliveInterval=<keepalive_interval> \
    -o ServerAliveCountMax=<keepalive_max> -o BatchMode=yes \
    -L <local_port>:<remote_host>:<remote_port> <ssh_host>
```

- `BatchMode=yes`: ssh never prompts, so a missing key or unknown host key fails fast instead of corrupting the TUI. Make sure `ssh <ssh_host>` works non-interactively (agent loaded, host key accepted) before enabling.
- The tunnel is **ready** when `local_port` accepts a TCP connection (up to 15 s).
- ssh runs in its own session (`setsid`, stdin `/dev/null`); its stderr goes to the `tunnel` log tab and `.logs/tunnel.log`.
- Health runs every second. If ssh dies or the local port stops answering, Atlas reconnects with exponential backoff: 5 s, 10 s, 20 s, … capped at `reconnect_cooldown` seconds. A change of default gateway (switching Wi-Fi/VPN, macOS) reconnects immediately.
- If the tunnel cannot come up, Atlas raises a **critical alert** named `tunnel` whose message ends with ssh's last stderr line (once per failure episode, not on every retry).
- If something already accepts connections on `local_port` and Atlas did not start it, Atlas uses it as is and never kills it. Atlas never kills by port.
- State appears in the sidebar **TUNNEL** block, `atlas dev --status`, `.logs/.tui-status.json` and MCP `atlas_status` (`tunnel: {state, local_port, ssh_host}`, `null` when no tunnel is running). See [Runtime](runtime.md#ssh-tunnel).

With `enabled: false` the block stays configured but nothing starts at boot; an action with [`open_tunnel: true`](#actions) starts it on demand.

On exit the tunnel is closed (SIGTERM to the ssh Atlas started) unless you turned off **Kill tunnel on exit** with `t` in the TUI; then it is left running and recorded in `.logs/.tunnel.pid` so the next start can reuse it. `--headless` and `--ci` always close it.

## actions

Custom commands bound to a key, shown in the help view (`?`) and the command palette (`:`).

```yaml
actions:
  - key: c
    label: Run check
    command: bun run check

  - key: u
    label: DB refresh local
    command: bun run db:refresh-local
    pause_services: [server, worker]

  - key: R
    label: Reset local DB
    command: bun run db:reset
    confirm: "This wipes the local database. Continue?"
```

| Field | Type | Default | Description |
|---|---|---|---|
| `key` | single character | **required** | Case-sensitive. Must not be a [built-in key](tui.md#key-bindings), a service `open_key`, or used by another action. |
| `label` | string | **required** | Name in help, palette, sidebar, toasts, and what [MCP](mcp.md) matches (case-insensitively). |
| `command` | string | **required** | Run with `sh -c` in the project root, with `env_files` applied. |
| `confirm` | string | none | Show a confirmation modal (title = `label`, body = this text) before running, from any trigger. `Enter`/`y` runs, `Esc`/`n` cancels. With `pause_services` the modal adds a `Pauses: web, api` line. Over MCP the action is refused until the call passes `confirmed: true`. |
| `pause_services` | string list | `[]` | Services stopped (state `suspended`) before the command runs and restarted when it finishes. Only services that are `running` or `starting` when the action starts are paused, and only those are restarted; one you had stopped stays stopped. |
| `session` | bool | `false` | Open a dedicated interactive tab streaming stdout/stderr, with stdin attach. See [sessions](tui.md#sessions-and-attach). |
| `input_map` | map string→string | `{}` | Session only: when attached, pressing a key sends the mapped string to stdin instead of the key itself. |
| `platform` | `ios` \| `android` | none | Session only: show a device picker first and inject [mobile settings](#mobile). |
| `device_id` | string | none | Pin a simulator name or adb serial; skips the picker. E.g. `"iPhone 16 Pro"`, `"emulator-5554"`. |
| `host_mode` | `auto` \| `lan` \| `simulator` | `auto` | Session actions with a `platform`: which host the dev server advertises to the device. Atlas sets `TAURI_DEV_HOST` and `ATLAS_DEV_HOST` in the session's environment (a variable you already set via `env_files`, a service `env:` or your shell is kept). `simulator` → `127.0.0.1`. `lan` → the LAN IPv4 of the default-route interface (falls back to `127.0.0.1` if it cannot be determined). `auto` → `lan` when the selected device is physical (iOS: a `devicectl` device; Android: any serial not starting with `emulator-`), else `simulator`. See [host_mode](#host_mode). |
| `group` | string | none | Label that groups actions: the help overlay (`?`) lists ungrouped actions first, then each group in order of first appearance under a header; the command palette (`:`) prefixes grouped actions with `Group ›` (and also matches the group name when you search). An empty string counts as ungrouped. |
| `open_tunnel` | bool | `false` | Before the command runs, make sure the [`tunnel`](#tunnel) is up: it is started on demand even with `tunnel.enabled: false`, waiting up to 15 s (the sidebar operation shows `opening tunnel…`). If it cannot come up, the action **fails with the reason and the command does not run**. A tunnel the action had to open on demand is **closed when the action finishes**, success or failure (if several `open_tunnel` actions overlap, the last one closes it; a tunnel that was already up, e.g. `enabled: true`, is left alone). Requires a `tunnel:` block, otherwise the config is rejected (`actions[N].open_tunnel`). |

Behavior of a non-session action:

- While it runs, the sidebar shows an **OPERATIONS** block: spinner, label, elapsed `mm:ss`, last output line. Success clears it; failure keeps `✗` and the last line until the next run or 30 s.
- Triggering the same action while it is still running is ignored ("already running").
- When it finishes, paused services are restarted (success or failure).
- `atlas dev --headless` can run actions via MCP but not actions with a `platform` (they need the TUI device picker).

### host_mode

For session actions with a `platform`, Atlas decides which host address the dev server should advertise and exports it as `TAURI_DEV_HOST` and `ATLAS_DEV_HOST` (use whichever your tooling reads, e.g. `vite --host $ATLAS_DEV_HOST`):

| `host_mode` | Value |
|---|---|
| `simulator` | `127.0.0.1` |
| `lan` | LAN IPv4 of the default-route interface. macOS: `route -n get default` → interface → `ipconfig getifaddr <if>`. Linux: the `src` of `ip route get 1.1.1.1`. Falls back to `127.0.0.1` if it cannot be determined. |
| `auto` (default) | `lan` when the selected device is physical, otherwise `simulator`. iOS: a device listed by `xcrun devicectl` is physical, a `simctl` simulator is not. Android: a serial starting with `emulator-` is an emulator, anything else is hardware. A device Atlas cannot classify (or none selected) is treated as a simulator. |

The lookup runs off the async runtime. A variable already set by you (`env_files`, a service `env:`, or the environment Atlas was started from) is never overridden; the other one is still set.

## mobile

Settings for `platform:` actions. They are secrets-adjacent, so keep them in `atlas.local.yaml`.

```yaml
mobile:
  ios:
    team_id: A6FCTW4KP6        # exported to the action as APPLE_DEVELOPMENT_TEAM
    bundle_id: com.example.app # optional override
  android:
    keystore: ~/.android/debug.keystore
```

| Field | Type | Description |
|---|---|---|
| `mobile.ios.team_id` | string | Apple Developer Team ID. Exported as `APPLE_DEVELOPMENT_TEAM` to iOS session actions. |
| `mobile.ios.bundle_id` | string | Bundle identifier override. |
| `mobile.android.keystore` | string | Keystore path for signing. |

Only `team_id` is read by Atlas today (to set `APPLE_DEVELOPMENT_TEAM`); the other fields are validated and kept in the config.

## env_files

```yaml
env_files:
  - .env
```

Dotenv-style files (paths relative to the project root), loaded in order (later wins), injected into every service and every action/session. Missing files are skipped silently. Supported syntax: `KEY=VALUE`, `export KEY=VALUE`, single or double quotes, `#` comments (full-line, or trailing after a space on unquoted values), blank lines, `=` inside values.

`env_files` is only valid in `atlas.yaml`; `atlas.local.yaml` cannot set it.

Atlas does **not** scrub the environment it inherits (there is no allowlist): every variable of the shell that started Atlas reaches services and actions. `env_files` rank **above** the inherited environment, so a key defined in both comes from the file. Variables you want to force off (e.g. `MASTRA_SKIP_DOTENV`) must be set explicitly in a service `env:`.

## worktree

```yaml
worktree:
  shared: [.env, node_modules]   # dirs symlinked, files copied from the main checkout
  setup: bun install && bun run db:migrate
  archive: docker compose -p "$ATLAS_WORKSPACE_NAME" down
  setup_timeout: 600             # seconds (default 600)
```

Used by Atlas Desktop when it creates or deletes a git worktree of this repo. `setup` runs with `sh -c` in the new worktree, in a terminal tab named "Setup"; `archive` runs before deletion. Both get `ATLAS_ROOT_PATH` (main checkout), `ATLAS_WORKTREE_PATH`, `ATLAS_WORKSPACE_NAME` and `ATLAS_PORT_OFFSET`.

### Port offset

Each worktree gets a port offset (100, 200, …). When `ATLAS_PORT_OFFSET=N` is set, atlas adds `N` to every service `port`, to `tunnel.local_port`, to every service `env` value equal to some service's port, and to `:<port>` in `open_url`/`health`. Try it: `ATLAS_PORT_OFFSET=100 atlas config check`. The TUI title shows `ports +N`.

## Local overrides (`atlas.local.yaml`)

`atlas.local.yaml` is merged over `atlas.yaml` at load time. Commit `atlas.yaml`; gitignore `atlas.local.yaml`.

```yaml
name: myapp-local
pre_command: ""                  # skip dependency install on this machine

services:
  web:
    port: 3002
  redis:
    enabled: false

tunnel:
  ssh_host: my-personal-alias

actions:
  - key: S                       # replaces the action with key S, or adds it
    label: Seed (local fixtures)
    command: cd apps/api && bun run db:seed:local

mobile:
  ios:
    team_id: A6FCTW4KP6
```

What can be overridden:

| Key | Merge rule |
|---|---|
| `name` | replaces |
| `pre_command` | non-empty replaces; `""` disables; omitted keeps the base |
| `pre_command_when` | replaces |
| `services.<name>` | field-by-field for `command`, `port`, `health`, `critical`, `depends_on`, `enabled`, `restart`, `max_restarts`, `ready_log`, `oneshot`, `start_timeout`, `autostart`. Entries for services not in `atlas.yaml` are ignored. Other service fields (`dir`, `env`, `open_key`, `open_url`, `hide_logs`, `lock_file`, `critical_help`, `health_timeout`, `failure_threshold`, `ready`, `on_unhealthy`) cannot be overridden and are rejected as unknown keys. |
| `tunnel` | field-by-field; ignored if `atlas.yaml` has no `tunnel` |
| `infra` | replaces the whole block |
| `profiles` | merged by name: same name replaces that profile's list, a new name is added. Members are validated against the merged `services` |
| `actions` | merged by `key`: same key replaces the action, new key is appended |
| `mobile.ios`, `mobile.android` | each replaces the base section |

`env_files` is not overridable.

## Validation

Atlas validates both files before doing anything (`atlas dev`, `--headless`, `--ci`, and `atlas mcp`'s config tool all fail with the same messages):

```text
$ atlas config check
3 config errors:
  atlas.yaml: services.web.helth: unknown field (did you mean 'health'?)
  atlas.yaml: services.web.depends_on.dbb: unknown service 'dbb' (did you mean 'db'?)
  atlas.yaml: services: dependency cycle: api -> web -> api
```

`atlas config check [--dir <path>]` prints `OK: <path>/atlas.yaml is valid` and exits `0`, or prints every error and exits `1`. Checks:

- unknown field at any depth, with a suggestion by edit distance
- wrong types or missing required fields (reported as a single parse error)
- `depends_on` targets exist, no self-dependency, no cycles
- `log_ready` needs `ready_log` on the target; `completed_successfully` needs a `command` on the target
- `ready_log` is a valid regex
- `ready` sets exactly one of `http` or `command`
- enabled services have unique `port`s (disabled services hold no port)
- every `profiles` member is a known service
- action `key`s are unique and do not clash with built-in keys or a service `open_key`; an `open_key` must not be a built-in key either

Built-in keys (never usable as `open_key` or action `key`): `0-9 j k g G f v E M L P q r s S t m i ? : / n N W T J z -`.

## JSON Schema (editor autocomplete)

```bash
atlas config schema > .atlas.schema.json
```

Add this as the first line of `atlas.yaml` (works with the YAML language server and the VS Code YAML extension):

```yaml
# yaml-language-server: $schema=.atlas.schema.json
```

Regenerate after `atlas update` to pick up new fields. Commit or gitignore the schema file as you prefer.

## Full example

```yaml
# yaml-language-server: $schema=.atlas.schema.json
name: shop
pre_command: bun install
env_files: [.env]

infra:
  compose_file: packages/infra/dev/compose.yml

services:
  database:
    port: 54321
    hide_logs: true
    critical: true
    critical_help: "Start OrbStack, then retry"

  redis:
    port: 63790
    hide_logs: true

  migrate:
    command: bun run db:migrate
    oneshot: true
    depends_on: { database: healthy }

  server:
    command: bun run dev:server
    port: 3070
    health: http://localhost:3070/health/live
    ready: { http: /health/ready }
    on_unhealthy: restart
    depends_on: { migrate: completed_successfully, redis: healthy }
    restart: on_failure
    open_key: a

  web:
    command: bun run dev:web
    port: 3071
    depends_on: { server: healthy }
    open_key: w

  worker:
    command: bun run dev:worker
    restart: always
    max_restarts: 0

profiles:
  backend: [server, worker]
  front: [web]

actions:
  - key: c
    label: Run check
    command: bun run check
  - key: u
    label: DB refresh local
    command: bun run db:refresh-local
    pause_services: [server, worker]
    confirm: "Drops and recreates the local database. Continue?"
```
