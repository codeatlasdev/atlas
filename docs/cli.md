---
title: "CLI reference"
description: "Every atlas command, flag and exit code."
---

# CLI reference

```text
atlas <command> [options]
```

Global: `-h/--help` on every command, `-V/--version` on `atlas`.

Two forms need no subcommand:

```text
atlas <path>            # open a folder or file in Atlas Desktop
atlas -- <cmd...>       # run a command as a service of this folder's stack
```

### `atlas <path>`

`atlas .`, `atlas ~/code/api`, `atlas src/main.rs`. The path opens in Atlas Desktop: a folder is opened as a project, a file opens **its** project (the nearest folder with an `atlas.yaml`, else the nearest with `.git` or a `package.json` / `Cargo.toml` / `pyproject.toml` / `go.mod`, else the file's own folder) and then the file in the editor. No `atlas.yaml` is needed. If Atlas Desktop is running the path goes to it; otherwise the app is started with the path and opens it on launch (install it first with `atlas desktop`).

**A subcommand always wins.** `atlas dev` runs the subcommand even when a folder named `dev` exists; open that folder as `atlas ./dev`. Any other first word is a path only if it is `.`, `..`, contains a path separator, or exists on disk; otherwise it is a typo and fails as it always did (`atlas devv` → `error: unrecognized subcommand 'devv'` + `tip: a similar subcommand exists: 'dev'`, exit `2`). `atlas` alone prints the help.

### `atlas -- <cmd...>`

```text
atlas -- bun dev
atlas -- node -e "require('http').createServer((q,s)=>s.end('ok')).listen(4123)"
```

Runs the command as an **ad-hoc service** of the stack of the current folder (its [project root](#atlas-path)): supervised like any service (logs in `.logs/<name>.log`, port probe when the command names one with `--port N`, `PORT=N` or `.listen(N)`, restart/stop from the TUI, Desktop and MCP). It is **not** saved to `atlas.yaml`. The name comes from the command (`bun dev` → `bun-dev`; flags and quoted code are left out; a taken name gets `-2`, `-3`…). One argument is taken as a ready command line (`atlas -- "bun dev"`); several are quoted for the platform shell (`cmd.exe` on Windows). If a background runner already owns the root the service is added to it live; if nothing runs, the stack is started in the background (like `atlas dev --detach`) with the ad-hoc service included. If a TUI owns the root (it cannot take services live) the command fails and says so: `atlas down`, then run it again. A TUI attached to a runner shows ad-hoc services that were there when it attached.

| Command | Purpose |
|---|---|
| [`dev`](#atlas-dev) | start the dev environment (TUI, background runner, headless, CI, or status) |
| [`down`](#atlas-down) | stop the running atlas (background runner or TUI) and its services |
| [`init`](#atlas-init) | scaffold `atlas.yaml` |
| [`init`](#atlas-init) | scaffold `atlas.yaml` (also: save what `atlas dev` detected) |
| [`deploy`](deploy.md) | ship a release to a server: atomic swap, health-gated restarts, auto-rollback |
| [`mcp`](#atlas-mcp) | MCP server for AI agents |
| [`update`](#atlas-update) (alias `up`) | self-update |
| [`install`](#atlas-install-hidden) | install the binary into `~/.atlas/bin` |
| [`setup`](#atlas-setup) | wire atlas into your AI agent (OMP, Claude Code, Codex) |
| [`skills`](#atlas-skills) | manage AI skills |
| [`version`](#atlas-version) | print version, OS, arch |
| [`uninstall`](#atlas-uninstall) | remove Atlas from this machine |
| [`desktop`](#atlas-desktop) | open Atlas Desktop, installing it on first run |
| [`wt`](#atlas-wt) | Atlas Desktop worktrees: list, create, remove |
| [`term`](#atlas-term) | Atlas Desktop terminals: list, read, send, wait, kill |
| [`browser`](#atlas-browser) / [`file`](#atlas-file) | drive the embedded browser, open files and diffs |
| [`link` / `unlink`](#atlas-link--unlink) | point the installed `atlas` (and `Atlas.app`) at a local debug build |

On any command except `update`, `self-update` and `mcp`, Atlas checks for a new release in the background (with `curl`, no GitHub API calls; for the [release channel](#atlas-update) you follow; at most every 6 hours, cached in `~/.atlas/.update_check` together with the channel it was computed for, so switching channel discards it; long `atlas dev` sessions keep re-checking). Only a *newer* release is ever announced, never a downgrade. When a newer release exists:

- **Before the TUI starts** (`atlas dev` only, see [Update prompt](#update-prompt-atlas-dev)) Atlas asks whether to update now;
- **Inside a running TUI** a modal asks `Atlas v<new> available — update when you quit?` (Enter = yes, Esc = not now; once per version per session). Accepting shows `⬆ on exit` in the sidebar brand row; after the TUI exits, the services are stopped and the terminal is restored, Atlas runs the update and prints the result (it does not restart). Attached to a runner (`atlas dev` against `--detach`) the runner keeps the old version until `atlas down && atlas dev --detach`. A plain green `⬆` marks an available update you have not accepted;
- after the command finishes (including when you quit the TUI without accepting), if stderr is a terminal, a banner is printed:

```text
────────────────────────────────────────────────
 ⬆ Update available
 atlas v0.3.27 → v0.3.28   Run: atlas update
────────────────────────────────────────────────
```

Errors are printed as `error: <message>` (plus `caused by:` lines) and exit `1`.

## `atlas dev`

```text
atlas dev [SERVICE...] [--profile <NAME>]... [--no-deps] [-d, --dir <DIR>]
          [-D, --detach] [--headless] [--ci] [--timeout <SECONDS>] [--status]
```

| Flag | Default | Meaning |
|---|---|---|
| `SERVICE...` | all | Start only these services plus everything they transitively `depends_on`. See [Selecting services](#selecting-services). |
| `--profile <NAME>` | none | Start the services of a [`profiles:`](configuration.md#profiles) entry (plus their dependencies). Repeatable; profiles and `SERVICE`s are unioned. |
| `--no-deps` | off | With `SERVICE`/`--profile`: start exactly those, do not pull in dependencies. |
| `-d, --dir <DIR>` | `.` | Project directory (`~` is expanded). It does not need an `atlas.yaml`: without one Atlas [detects](configuration.md#no-atlasyaml-detected-config) the services in memory (from `package.json`, `Cargo.toml`, `pyproject.toml`, `go.mod`) and prints `no atlas.yaml: using detected config (…); save it with 'atlas init'`. Nothing is written. |
| `-D, --detach` | off | Start the services in a **background runner** and return: the runner is fully daemonized (own session, stdin `/dev/null`, stdout/stderr in `.logs/runner.log`), holds the project lock and serves the control socket and status heartbeat like `--headless`. Prints `atlas runner started (pid N) · attach: atlas dev · stop: atlas down` and exits `0` once the socket answers. A bad `atlas.yaml` or selection is reported on your terminal and nothing is started. Works with `SERVICE…` / `--profile` / `--no-deps`. If a runner is already up it prints `atlas runner already running (pid N) …` and exits `0` (the selection is not applied to it); if a TUI owns the project it exits `1`. See [Runtime › Runner and attach](runtime.md#runner-and-attach). |
| `--headless` | off | No TUI. Starts services, streams `[name ] line` to stdout and state changes to stderr, stops everything on Ctrl-C / SIGTERM / SIGHUP. Serves the [control socket](mcp.md#how-it-works) so MCP control tools work. |
| `--ci` | off | Start everything, wait until stable, print a JSON report to stdout, stop everything, exit. |
| `--timeout <SECONDS>` | `60` | Maximum wait in `--ci` mode. |
| `--status` | off | Print the state of an already-running `atlas dev` and exit. Reads only; starts nothing. |

Without `--detach`/`--headless`/`--ci`/`--status`, `atlas dev` runs the [TUI](tui.md). **If a runner (`--detach` or `--headless`) already owns the project it attaches to it instead** (same dashboard, services keep running when you leave; see [Attached mode](tui.md#attached-mode)); `SERVICE…` / `--profile` are ignored then, with a toast. If stdin or stdout is not a terminal it prints `atlas dev needs an interactive terminal. Agents/scripts: use 'atlas dev --status' or 'atlas dev --ci'.` and exits **2**.

Precedence if flags are combined: `--status`, then `--detach`, then `--ci`, then `--headless`.

### Selecting services

```text
atlas dev web                      # web + everything web depends_on (e.g. api, database)
atlas dev web worker               # union of both, plus their dependencies
atlas dev --profile backend        # members of profiles.backend, plus dependencies
atlas dev --profile backend --profile front   # union of two profiles
atlas dev api --no-deps            # api only; dependencies are NOT started
```

- The selection is the named services (and profile members) plus their **transitive `depends_on`**. Infra services (command-less, port-probed ones such as `database`) are included only when a selected service depends on them; one nothing selected depends on stays off. With no `SERVICE` and no `--profile`, everything starts as before. When `infra:` (docker compose) is configured but no command-less service is modelled in `atlas.yaml`, Atlas cannot tell who needs it, so `docker compose up -d` still runs (old behavior).
- Services outside the selection are shown in the sidebar as `⊘ … off` (state `disabled`), are **not started at boot**, and can be started by hand from the service menu (`m`), the command palette or MCP `atlas_start_service` (a manual start does not wait for `depends_on`, like [`autostart: false`](configuration.md#services)). Once started they are part of the run like any other service.
- Validation happens before anything starts (exit `1`, nothing touched): an unknown service or profile name fails with `unknown service 'wev' (did you mean 'web'?); available: …`; `--no-deps` without a `SERVICE`/`--profile` is an error. A profile that lists an unknown service is a config error (`atlas config check`).
- Works in the TUI, `--headless` and `--ci`. `--ci` only judges the selected services: the others are reported as `"state": "disabled", "off": true` and never fail the run.
- `--status`, the status file and MCP `atlas_status` mark them: `STATE` shows `off`, the status file has `"deselected": true`, MCP rows have `"off": true`. `atlas_wait_ready` without a service ignores `disabled` services, so it waits only for the selection.

### `--status`

```text
$ atlas dev --status
atlas dev running (pid 4242, updated 1s ago)
mode: runner · 1 attached
SERVICE              STATE     PORT   HEALTH
api                  running   3001   http://localhost:3001/health
web                  starting  3002   -
worker               off       -      -
tunnel               running   54320  prod-db -> localhost:5432
```

`mode:` says who owns the run: `runner` (`--detach` or `--headless`; shows how many TUI clients are attached) or `tui` (a terminal TUI owns it and cannot be attached to). `off` marks a service left out of this run by `SERVICE…` / `--profile` (see [Selecting services](#selecting-services)).

The `tunnel` row appears only while an [SSH tunnel](configuration.md#tunnel) exists: `STATE` is its state, `PORT` the local port, then `<ssh_host> -> <remote_host>:<remote_port>`.

| Exit | Meaning |
|---|---|
| `0` | a live `atlas dev` owns this project (status file fresh, pid alive) |
| `3` | not running; prints `atlas dev not running (<reason>)`, e.g. `no status file`, `process 123 is not alive`, `status is stale (last heartbeat 500s ago)` |

A status file is stale after 120 s without a heartbeat (Atlas refreshes it about once a second). State names: `starting`, `running`, `failed`, `stopped`, `disabled`, `suspended`.

### `--ci`

Starts all selected services (everything unless narrowed by `SERVICE…` / `--profile`; infra first, then app services in dependency order), waits at least 10 s and until nothing is `starting`, no dependency gate or restart is pending, and app services have been started, or `--timeout` expires. Then prints:

```json
{
  "ok": true,
  "services": [
    { "name": "database", "state": "running", "port": 5432 },
    { "name": "migrate",  "state": "stopped", "port": null, "completed": true },
    { "name": "api",      "state": "running", "port": 3001 },
    { "name": "web",      "state": "failed",  "port": 3002, "reason": "dependency api failed" }
  ]
}
```

`ok` is `true` when every service is `running`, `stopped` or `disabled`. Services left out by `SERVICE…` / `--profile` appear as `{ "name": "worker", "state": "disabled", "port": null, "off": true }` and never affect `ok`. A service that exits non-zero, gave up restarting, or whose dependency failed is `failed` (with `reason`) and makes `ok` false. A `oneshot` that exited `0` is `stopped` with `"completed": true` and counts as OK. Progress goes to stderr.

| Exit | Meaning |
|---|---|
| `0` | `ok: true` |
| `1` | `ok: false`, config error, or another atlas already owns the project |
| `130` | interrupted by Ctrl-C / SIGTERM / SIGHUP (services are stopped first) |

`--ci` does not serve the control socket. Both `--ci` and `--headless` run `pre_command` before starting services (see [pre_command](configuration.md#pre_command)).

### One instance per project

`--ci`, `--headless` and `--detach` take the same singleton lock as the TUI. If another `atlas dev` already runs for that directory `--ci` and `--headless` print `atlas dev is already running (pid N); use 'atlas dev --status'` and exit `1`, without touching any process. The plain TUI **attaches** to a runner, and only when the lock holder is another TUI (which cannot be attached to) does it ask `Kill it and restart? [y/N]`; see [Runtime › Singleton](runtime.md#singleton-and-the-control-socket).

### Update prompt (`atlas dev`)

Before the TUI takes the terminal, plain `atlas dev` offers a pending update:

```text
╭──────────────────────────────────────────────────────────────────────╮
│ ⬆ Atlas update available: v0.4.1 → v0.5.0                            │
│ Changelog: https://github.com/codeatlasdev/atlas/releases/tag/v0.5.0 │
╰──────────────────────────────────────────────────────────────────────╯
Update now? [Y/n]
```

- The newest version comes from the cache (`~/.atlas/.update_check`) when it is younger than 6 hours; otherwise Atlas checks live with a hard **2 s** timeout and silently falls back to the cache on timeout or error. It never delays startup longer than that.
- **Enter / `y`** runs the same logic as [`atlas update`](#atlas-update), then re-executes the new binary with the exact same arguments, so `atlas dev …` continues on the new version. If the update fails, the error is printed and Atlas asks `Continue with v<current>? [Y/n]`.
- **`n`** continues with the current version and silences that version for **24 hours** (stored in the cache; older caches without the field still load).
- Never asked for `--status`, `--ci`, `--headless`, `--detach` or the runner, when stdin/stdout is not a terminal, or when `ATLAS_NO_UPDATE_PROMPT=1`. On a linked or debug build (`atlas link`) Atlas only prints a one-line hint and never replaces the binary.

## `atlas down`

```text
atlas down [-d, --dir <DIR>]
```

Stops the atlas that owns the project: asks it over the control socket to stop every service and exit (the same graceful path as Ctrl-C: whole task trees, no `docker compose down`, tunnel exit policy applied), then waits up to **15 s** for it to be gone. Works on a background runner, a `--headless` run and a terminal TUI.

| Output | Exit |
|---|---|
| `atlas stopped (pid N)` | `0` |
| `atlas is not running` | `0` (nothing to do) |
| `atlas (pid N) is still stopping after 15s; check: atlas dev --status` | `1` |

## `atlas init`

```text
atlas init [-d, --dir <DIR>]      # default .
```

Writes a minimal `atlas.yaml` (`name:` from the directory plus detected services, or a commented example) and prints what it detected. Fails if `atlas.yaml` already exists. This is also how you **save** what `atlas dev` [detected](configuration.md#no-atlasyaml-detected-config) in a folder without one: the file is exactly the config that run used.

## `atlas config`

```text
atlas config check [-d, --dir <DIR>]   # default .
atlas config schema
```

- `check` validates `atlas.yaml` plus `atlas.local.yaml`. Prints `OK: <path> is valid` and exits `0`, or prints every error and exits `1`. A missing or unparsable file is an `error:` and exits `1`. Details: [Validation](configuration.md#validation).
- `schema` prints the JSON Schema (draft-07) for `atlas.yaml` to stdout.

## `atlas deploy`

Build locally, upload, and let a driver on the host do lock → swap → health gate → rollback. Subcommands: `rollback`, `status`, `logs`, `history`. Exit codes 0/1/2/3/4. Full reference: [deploy.md](deploy.md).

## `atlas mcp`

```text
atlas mcp [-d, --dir <DIR>]
```

Runs the MCP server over stdio. `--dir` is the project directory; when omitted, Atlas uses the [project root](#atlas-path) of the current directory (nearest `atlas.yaml`, else nearest `.git` or package manifest, else the current directory itself), so the tools also work in a folder without `atlas.yaml` (`atlas_config` then reports the [detected](configuration.md#no-atlasyaml-detected-config) services). stdout carries protocol messages only; a one-line `atlas mcp: serving <dir> on stdio` goes to stderr. Exits when the client closes stdin. Full guide: [MCP](mcp.md).

## `atlas update`

```text
atlas update [--check] [--channel <stable|beta|alpha>] [--json] [--downgrade] [-y, --yes]
atlas update channel [<stable|beta|alpha>] [--json]
# alias: atlas up; also hidden alias: atlas self-update
```

Looks up the newest release of your [release channel](versioning.md#channels) (with `curl`, no login and no GitHub API calls: the public `releases.atom` feed for every channel: Stable is the newest tag without a prerelease label, never the `releases/latest` redirect alone, which is the newest alpha while Alpha is the default) and, if it is newer than the running version, downloads `atlas-<version>-<platform>.tar.gz` and its `.sha256` from the release, verifies the checksum, replaces the running binary in place (keeping a backup that is restored if the app cannot follow), and then refreshes skills and any OMP, Claude Code or Codex setup it finds, using the new binary (failures are non-fatal). An installed [Atlas Desktop](desktop.md) is brought to the same version (`atlas desktop --sync`) and the terminal host is reconciled. `--check` only reports. Homebrew installs are told to run `brew upgrade atlas`. Refuses while `atlas link` is active (it would overwrite the dev build); run `atlas unlink` first.

**Network failures.** Every request has a 10 s connect timeout and retries transient errors up to 3 times; a download has no total time limit (the desktop bundle is large) but is aborted when it stays under 1 KB/s for 30 s, and curl resumes it on retry. A failed lookup is an error, never a different answer: if the release feed cannot be read, or the newest beta/alpha build cannot be confirmed as published (only an explicit 404 means "not published yet"), `atlas update` reports the failure instead of offering Stable or an older build, and the cached check result is kept.

**Channels.** `stable`, `beta` and `alpha` (the default while Atlas is in alpha; Stable again once a real Stable exists) describe how far a release has been soaked, not how finished Atlas is; see [Versioning and release channels](versioning.md). The channel is one word in `~/.atlas/update-channel`, shared with Atlas Desktop; with no file (or an unreadable one) you follow the default. An install that never chose a channel therefore sees the newest alpha as an update (never a downgrade). `ATLAS_UPDATE_CHANNEL=<name>` overrides it for one process (scripts, CI); an invalid value is ignored.

- `--channel <name>` **saves** that channel, then updates from it. Without the flag the saved channel is used. An unknown name is an error that lists the valid ones (exit code 2).
- `atlas update channel` lists every channel with its promise, risk, cadence, audience and what it offers *right now* for your install, and marks the one you follow. In a terminal it ends with a numbered picker (`1`-`3` or the name; Enter keeps the current one); piped, it only prints. `atlas update channel <name>` saves the channel and prints what switching means plus what it offers now.
- `--check --json` prints a single object and never installs:

  ```json
  { "installed": "1.3.0", "channel": "beta", "offered": "1.4.0-beta.1", "tag": "v1.4.0-beta.1", "action": "update" }
  ```

  `action` is `update` (a newer build is offered), `downgrade` (only an older build is offered) or `none`; `offered` and `tag` are `null` when the channel has nothing published. `atlas update channel --json` prints `{ "selected", "installed", "channels": [...] }`, one entry per channel with the same copy the command prints plus `selected`, `tag`, `offered` and `action`.

**Downgrades are never automatic.** When the channel offers something *older* than the build you run (for example you run an alpha and switch to Stable), `atlas update` installs nothing: it says which version you run, which one the channel offers, that you get updates again once the channel passes your build, and how to go back. `atlas update --downgrade` installs that older release anyway, after a data-compatibility warning and a `[y/N]` confirmation (default No; `--yes` skips it, and outside a terminal it is required). The download goes through the same checksum verification, backup and rollback as any update. It is not available for Homebrew or system-package installs (their package manager owns the version).

**Installs another tool owns.** A binary installed by Homebrew, or by a Linux system package, is never overwritten by `atlas update` itself.

- *Homebrew:* `atlas update` prints `brew upgrade atlas` and exits 0 (the desktop app sees `@@atlas managed homebrew` and does not report "up to date").
- *System package (Arch, Debian/Ubuntu, Fedora/openSUSE):* the owner is found with `pacman -Qo`, `dpkg -S` or `rpm -qf` (the `/usr/bin` path rule is the fallback). `atlas update --check` reports current and latest. `atlas update` downloads the matching asset and its `.sha256`, verifies it **before any privilege escalation**, prints the command, confirms (`--yes` skips; required without a terminal) and installs it via `sudo` (`pkexec` without a terminal), holding `~/.atlas/.update.lock`. Then it refreshes skills and agent setups with the installed binary. See [Linux packages](installation.md#linux-packages). If no package manager is found, it prints where to get the package and emits `@@atlas managed package`.
- `--check --json` adds `"managed": true`, `"manager"` (`pacman`, `apt`, `dnf`, `zypper` or `null`) and `"command"` (the exact line, only when an upgrade is offered), and `action` is `package` (Linux package) or `managed` (Homebrew) instead of `update` when an upgrade is offered; `none` and `downgrade` stay as they are.

**What it refuses.** A debug build or a binary reached through an `atlas link` symlink is never replaced (the same rule `atlas dev` uses before it offers an update; `atlas unlink` goes back to the release build). Before downloading, it checks that the CLI's directory and the installed app can be written and names the artifact that cannot, so it never replaces the CLI and then rolls it back because the app is read-only. Skills and agent hooks are refreshed only after the app followed; when the app cannot, the CLI is rolled back and the previous binary refreshes them, so they are never newer than the CLI that stays.

**Cache.** The daily-notice cache `~/.atlas/.update_check` is written atomically (a `.tmp` file and a rename); an unreadable file keeps its "not now" dismissal; and a failed check (offline) is stamped, so nothing retries for 30 minutes (an offline `atlas dev` start no longer waits for the network). The exit banner prints the package manager's command for package installs.

## `atlas desktop`

```text
atlas desktop                          # open Atlas.app; first run installs the release matching this CLI
atlas desktop --install [--latest]     # (re)install: this CLI's version, or the newest release
atlas desktop --from <PATH> [--no-open]  # install a local Atlas.app or Atlas-*.app.tar.gz
atlas desktop --status                 # installed path, version, linked or not
atlas desktop --dev                    # inside the atlas repo: bun run desktop:dev (hot reload)
```

Installs into `/Applications/Atlas.app` (or `~/Applications` when `/Applications` is not writable). Downloads `Atlas-<version>-<platform>.app.tar.gz` and its `.sha256` from the public GitHub release (with `curl`, no login) and verifies the checksum. The new bundle is copied next to the old one and swapped in only once complete, so a failed download never breaks the installed app. Refuses while the app is linked (`atlas link --desktop`). Full guide: [Desktop app](desktop.md).

### `atlas desktop --agent`

```text
atlas desktop --agent [--name NAME] [--scratch]   # start an isolated instance, print its ATLAS_DEBUG_SOCK
atlas desktop --agent --stop [--name NAME]        # stop one instance, or all
```

Debug builds only (inside the atlas repo or from `target/debug/atlas`). The instance has its own `HOME`, hostd dir and debug socket under `/tmp/atlas-agent-<name>/`, no Dock icon or tray, never takes focus and lives on a private virtual display, so it never appears on your desktop. `--scratch` seeds its `HOME` with your `~/.atlas/desktop` settings only. `bun run desktop:agent` wraps it and builds the bundle when missing. Details: [Desktop app](desktop.md#agent-instances-bun-run-desktopagent).

### `atlas desktop debug`

```text
atlas desktop debug info | state | a11y [SELECTOR] | perf [--seconds N]
atlas desktop debug logs [--level log|info|warn|error|debug|csp] [--since SEQ]
atlas desktop debug eval <JS>
atlas desktop debug shot [PATH] [--selector CSS | --rect x,y,w,h | --full | --window]
atlas desktop debug record PATH (--seconds N | --until SELECTOR) [--fps 24|30|60]
atlas desktop debug wait TARGET [--expr] [--timeout MS]
atlas desktop debug click SELECTOR | type SELECTOR TEXT [--delay MS]
```

Drives a running **debug** build through `$ATLAS_DEBUG_SOCK` (default `~/.atlas/desktop-debug.sock`, which is YOUR app: agents use `atlas desktop --agent` and its socket instead) without touching the mouse or keyboard. `eval` takes an expression or a body with `return` and awaits promises. `shot` is a page snapshot (retina, no permission); `--window` is the compositor's picture of the whole window. `click`/`type` are DOM events in the webview, never OS input. Release builds do not contain the bridge.

## `atlas wt`

```text
atlas wt ls [--repo NAME] [--json]
atlas wt create --repo <name|id|path> [--name N] [--base REF] [--agent omp|claude|codex|blank] [--prompt TEXT] [--pr N] [--no-setup] [--json]
atlas wt rm <id|name|path> [--force]
atlas wt set <id> [--name N] [--comment C] [--unread true|false]
atlas wt current [PATH]
```

Needs Atlas Desktop running (control socket `~/.atlas/desktop.sock`). `create` waits until the worktree exists and returns its terminals (`pty` ids). `rm` keeps an unmerged branch unless `--force`.

## `atlas term`

```text
atlas term ls | read <pty> [--lines N] | send <pty> <text> [--enter] | kill <pty> | wait <pty> --for exit|idle [--timeout S]   [--json]
```

Terminals of Atlas Desktop. With the app open it routes through the app (SSH hosts, agent state: `wait --for idle` returns when an agent's turn ends); otherwise it talks to the local `atlas hostd`. `send --enter` to an agent tab submits the text as a prompt. `wait` exits 2 on timeout.

## `atlas browser`

```text
atlas browser goto <url> [--tab T] [--worktree W] | snapshot | click @eN | fill @eN <text> | screenshot   [--json]
```

The embedded browser. `snapshot` lists visible interactive elements with refs (`@e1`…); refs are valid until the next snapshot.

## `atlas file`

```text
atlas file open <path> [--worktree W]
atlas file diff [--worktree W]
```

## `atlas hostd` (hidden)

```text
atlas hostd [--dir PATH] [--status | --stop | --upgrade | --check | --restart | --reconcile [--choice ask|idle|later]]
```

The terminal/exec daemon behind Atlas Desktop, started by the app. `--status` prints its info (exit 1 when down); `--stop` ends it and its terminals; `--upgrade` replaces the running daemon with this binary while keeping every terminal alive. `--check` prints whether it is older than this binary (version, uptime, running terminals, what is off, the fix commands; exit 0 current, 3 stale, 1 not running); `--reconcile` brings it to this binary (live handoff when it can, silent restart when no terminal runs, otherwise it asks, or follows `--choice`; exit 0 also when it is not running); `--restart` ends its terminals, starts a fresh daemon and verifies it. A daemon with no running terminal and no client for 10 minutes (`ATLAS_HOSTD_IDLE_SECS`) exits by itself. `--dir` (or `ATLAS_HOSTD_DIR`) moves its socket, token and scrollback away from `~/.atlas`, which is also how it runs on SSH hosts.

## `atlas install` (hidden)

```text
atlas install [--dir <DIR>]   # default ~/.atlas/bin
```

Copies the running binary to `<dir>/atlas`, configures `PATH` for fish, zsh and bash (skipped when the directory is already in `PATH`), and installs the bundled skills. Used by the manual install; see [Installation](installation.md#manual-install).

## `atlas setup`

```text
atlas setup                                   # detect agents on PATH, print status + next commands
atlas setup omp|claude-code|codex [--scope user|project] [--apply] [--force] [--uninstall]
```

One-command agent integration for [OMP, Claude Code and Codex](agents.md). Each variant installs the `atlas` MCP server entry, makes the bundled skills available to that agent (Codex shares OMP's `~/.agents/skills`; Claude Code gets `~/.claude/skills`) and registers hooks that inject `atlas dev --status` at session start and refuse to start the interactive TUI. It is a **dry run** unless `--apply` is given; it merges into existing config without touching other entries (comments and key order kept), is idempotent, and only replaces or removes artifacts Atlas owns (`--force` overrides a foreign `atlas` entry). `--scope project` writes into the project (nearest `atlas.yaml` ancestor): `.omp/`, `.mcp.json` + `.claude/`, or `.codex/`. Overrides: `PI_CODING_AGENT_DIR`, `CLAUDE_CONFIG_DIR`, `CODEX_HOME` and `HOME`. Full details: [Agent setup](agents.md).

### `atlas hook` (hidden)

```text
atlas hook claude-code|codex <event>   # event JSON on stdin, response JSON on stdout
```

The command Claude Code and Codex run for the hooks that `atlas setup` registers. `session-start` and `pre-tool-use` answer the agent; `user-prompt-submit`, `post-tool-use`, `stop`, `stop-failure`, `session-end`, `notification` (Claude) and `permission-request` (Codex) only report the agent's state to Atlas Desktop when it runs in one of its terminals. Not meant to be run by hand (it is omitted from `--help`); it always exits 0 and prints nothing when it has no opinion. See [Agent setup](agents.md#what-the-hooks-do).

## `atlas skills`

```text
atlas skills list
atlas skills install [NAMES...] [--local]
atlas skills update [--local]
atlas skills status [--local]
```

Skills are agent instructions compiled into the `atlas` binary, so they always match its version (no network). `install` with no names installs all (`atlas`, `atlas-desktop`, `atlas-install`); `update` rewrites installed skills whose content differs from the bundled copy. By default they go to `~/.agents/skills/` (tracked in `~/.atlas/.atlas-skills.json`), which OMP and Codex read (Claude Code reads `~/.claude/skills` instead; `atlas setup claude-code --apply` installs them there); `--local` uses `.agents/` in the current directory instead. `atlas setup omp|codex --apply` installs them too.

## `atlas version`

Prints `atlas <version>`, OS and architecture.

If the CLI panics it writes a crash report (`crash-<timestamp>.toml`) to its data directory and prints the issue tracker URL. Reports stay on your machine; review one for private data before attaching it to an issue.

## `atlas uninstall`

```text
atlas uninstall [-y, --yes]
```

Lists what it will remove, then deletes the whole `~/.atlas/` directory (binary, desktop settings and projects, worktrees, hostd state, omp plugin) and the `.atlas` PATH lines from your shell config files, after confirmation (skip with `--yes`). It does not touch `/Applications/Atlas.app` or the skills in `~/.agents/skills`.

## `atlas link` / `unlink`

For working on Atlas itself.

```text
atlas link [--dir <WORKSPACE>] [--no-build] [--desktop]
atlas unlink
```

`link` builds `atlas-cli` (unless `--no-build`), backs up `~/.atlas/bin/atlas` to `atlas.backup`, and replaces it with a symlink to `<workspace>/target/debug/atlas`. Run it inside the Atlas repo or pass `--dir`. The TUI shows a `⚡ linked` badge in this mode. `unlink` restores the backup.

`--desktop` also builds a debug `Atlas.app` (`cargo tauri build --debug`) and makes `/Applications/Atlas.app` a symlink to it. A real install is moved to `~/.atlas/desktop/Atlas.app.backup` first. The app shows a **Dev build** badge, and its updates are paused. `unlink` restores the app as well.
