---
title: "Contributing"
description: "Set up a development environment, run the checks and send a pull request."
---

# Contributing

Thank you for helping with Atlas. This page is the long version of [CONTRIBUTING.md](https://github.com/codeatlasdev/atlas/blob/main/CONTRIBUTING.md). By taking part you agree to the [Code of Conduct](https://github.com/codeatlasdev/atlas/blob/main/CODE_OF_CONDUCT.md). Atlas is released under the [MIT](https://github.com/codeatlasdev/atlas/blob/main/LICENSE-MIT) or [Apache-2.0](https://github.com/codeatlasdev/atlas/blob/main/LICENSE-APACHE) license, at your option, and your contributions are too.

## Ways to help

- **Report a bug** or ask for a feature in the [issue tracker](https://github.com/codeatlasdev/atlas/issues). Include `atlas version`, your OS, the steps and the relevant lines of `.logs/<service>.log` (redact secrets).
- **Improve the docs.** Every page on the website has an *Edit on GitHub* link; the source is `docs/*.md`.
- **Send a fix or a feature.** For anything bigger than a small fix, open an issue first so we can agree on the shape before you spend time on it.
- **Security issues** do not go in public issues. See [SECURITY.md](https://github.com/codeatlasdev/atlas/blob/main/SECURITY.md).

> **Sign your commits.** Atlas uses the [DCO](https://developercertificate.org/): commit with `git commit -s`. AI-assisted changes follow the policy in [CONTRIBUTING.md](https://github.com/codeatlasdev/atlas/blob/main/CONTRIBUTING.md#ai-assisted-contributions).

## Set up

You need `rustup`, [`bun`](https://bun.sh) and Python 3 with PyYAML (the release checks in `bun run check`) on every OS, plus the per-OS pieces below. `gh` is optional.

```bash
git clone https://github.com/codeatlasdev/atlas.git
cd atlas
bun run setup           # idempotent: checks tools, installs frontend deps, cargo-tauri, cargo-deny, cargo-nextest, actionlint and zizmor
```

The Rust toolchain is pinned in `rust-toolchain.toml`; `rustup` installs it on the first `cargo` command. Workspace lints apply to every crate: fix warnings, do not add global `allow`s. Build profiles, the `cargo lint`/`cargo t` aliases, `bun run sweep` and the measured build times: [Build performance](build-perf.md).

### macOS

The Xcode command line tools (`xcode-select --install`). Everything in this page works, including agent instances, Atlas Lab and the native GPU terminal.

### Linux

WebKitGTK 4.1 and its build dependencies for Atlas Desktop (`bun run setup` names them when they are missing; the CLI alone needs none):

```bash
sudo pacman -S --needed webkit2gtk-4.1 base-devel curl wget file openssl appmenu-gtk-module libappindicator-gtk3 librsvg xdotool   # Arch
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev   # Debian/Ubuntu
sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file libappindicator-gtk3-devel librsvg2-devel libxdo-devel   # Fedora
```

Terminals use xterm.js. Agent instances (`bun run desktop:agent`) run on their own headless sway and D-Bus session bus, so they need `sway`, `grim` and `dbus-daemon` (Arch: `pacman -S sway grim`; Debian/Ubuntu: `apt install sway grim dbus`); recording also needs `ffmpeg` (with libx264). Atlas Lab, the mirror, `debug shot` and `debug record` work on them (see "Linux and Windows gaps" below). The Linux roadmap is #289.

### Windows

Install [Git for Windows](https://git-scm.com/download/win) (it brings Git Bash), [rustup](https://rustup.rs) with the MSVC toolchain (Visual Studio Build Tools, "Desktop development with C++"), [Bun](https://bun.sh) and [Python 3](https://www.python.org/downloads/) from python.org, not the Microsoft Store alias that `python3` points to by default. WebView2 ships with Windows 10 and 11.

`bun run` scripts call repo shell scripts through `bun scripts/sh.ts`, which runs them with Git Bash: on Windows the `bash` first on `PATH` is often WSL's (`WindowsApps\bash.exe`), which would run them inside Linux where your Windows Rust and Bun do not exist. Call a script by hand the same way: `bun scripts/sh.ts scripts/heavy.sh cargo build`. `ATLAS_BASH` points it at another bash. Checked on Windows 11: `bun run setup`, `bun run check:changed`, `bun run lint:workflows` and `bun run check` (rc 0 at 40e8b67, Git `usr\bin` on `PATH`). Run the gate from Git Bash, or put `C:\Program Files\Git\usr\bin` on `PATH` in PowerShell: some Rust tests spawn `sh` and `sleep`, and from a plain PowerShell `PATH` they fail with `program not found`. Not yet checked on Windows: `bun run desktop:dev` and `desktop:build`.

Terminals use xterm.js on Windows; agent instances, Atlas Lab and the mirror are not there (see below). Tests of macOS-only or POSIX-only scripts (`dev-signing-identity`, `install.sh`, the load gate of `heavy.sh`) are skipped on Windows; CI runs the repo scripts and their tests on Linux and Windows (job "Repo scripts").

## The gate

```bash
bun run check
```

`bun run check` is the local gate: every check CI's required jobs run that can run on one machine. The list lives in one place, `scripts/checks/map.ts` (areas with path globs, and the checks of each area); `bun run check` runs all of it, `bun run check:changed` the areas your change touches, and CI's plan job (`scripts/ci-plan.ts`) turns the same areas into the `area=true|false` outputs that gate the jobs of `ci.yml`. `bun run check --json` prints the list, and the checks only CI can run (the Linux desktop launch, the root tests, the arm64 build, the Windows installer) with the reason. In order:

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets --locked -- -D warnings`
3. `cargo nextest run --workspace --locked`, then `cargo test --workspace --locked --doc` (nextest does not run doctests)
4. `cargo deny check` (licenses, advisories, duplicate and banned crates)
5. `scripts/check-skills.sh` (validates `skills/*/SKILL.md` against the Agent Skills spec)
6. `scripts/lint-workflows.sh` (actionlint and zizmor over `.github/`; also `bun run lint:workflows`)
7. `bun run desktop:check` (svelte-check, prettier and the frontend tests, `bun run test` in `apps/desktop`: `bun test --conditions browser --parallel --timeout 60000 src`; a bare `bun test` there stops at once, since without the `browser` condition Svelte resolves its server build and no component can mount). `--parallel` runs every test file in a fresh global and module registry (it implies `--isolate`): module mocks such as `test/uiKit.ts` and stubbed globals are process-wide, so without it a file's mocks leaked into whichever file ran after it and tests passed or failed with the file order. `bun run test` at the root runs those tests plus `release:test`. `--timeout 60000` is a hang detector, not a guard: a performance test never asserts a wall-clock budget (the suite failed under load with nothing regressed); it counts operations (matcher calls, rows rendered, elements read), runs on a virtual clock, awaits the real event (`test/until.ts`), or bounds how the cost grows with the input (`test/scaling.ts`: CPU time at 8x the size over the cost at 1x, ~8 for a linear pass and ~64 for a quadratic one).
8. `bun run i18n:check` (translations: missing, outdated, unused, duplicated or mismatched strings, hardcoded user-facing text, docs translations; offline, seconds; see [Translating Atlas](#translating-atlas))
9. `scripts/mobile-check.sh` (the native crate's fmt and clippy, then Flutter analyze and test for `apps/mobile`; skipped with a notice when Flutter is not installed)
10. `scripts/push-relay-check.sh` (the Go push relay in `services/push-relay`; skipped with a notice when Go is not installed)
11. `bun run release:test` (the repo scripts' own tests, the release manifest tests in Python, the bootstrap channel vectors and the label checks); CI runs it on Linux and Windows in the "Repo scripts" job
12. `bun run dup` (jscpd, at most 1.5% duplication)
13. `bun run site:check` and `bun run site:build` (astro check, type-check and build of `site/`, which also renders `docs/`)

Steps 5, 6, 9 and 10 run through `bun scripts/sh.ts`, so they use Git Bash on Windows (see [Windows](#windows)).

**Adding a check or teaching it a path.** Edit `scripts/checks/map.ts` only: a check goes under its area with the `ci:` jobs that run it; a path goes into the globs of the area it affects. A path no area claims runs every check (an unknown path never skips one), so a new top-level directory costs a full run until it is mapped. `scripts/checks/map.test.ts` fails, naming the line to add, when `ci.yml` gains a job, a gated area or a check command the map does not list, or when the map lists one CI does not run. Docs-only changes (top-level `*.md`, `LICENSE*`, assets, issue templates) skip everything; `docs/` still rebuilds the site.

**The diff base.** `check:changed` and the CI plan job share one resolver, `scripts/lib/base.ts`: `--base <ref>` (or `ATLAS_BASE`) if given, else in CI the pull request's merge-ref first parent (the main tip GitHub merged with, not the base sha the event recorded) or the commit a push replaced, else locally the merge base with `origin/main` (then `main`). Committed, staged, unstaged and untracked changes all count. No usable base (`workflow_dispatch`, a new branch, shallow history) runs everything: "no base" never means "nothing changed". `git fetch origin main` first when checking locally; a stale `origin/main` makes main's newer files look like yours. Other tools that judge a change against a base (the i18n ratchets) import `resolveBase` from there.

Run it on its own and read the exit code before you push. A pull request should pass it.

### Commands

Every command is a root `package.json` script, so `bun run <name>` from the repo root is the one entry point (`scripts/setup.sh` first, once). This is the one command table; other pages link here.

| Command | Use |
|---|---|
| `bun run setup` | the one preflight: rustup, bun, frontend deps, `cargo-tauri`, `cargo-deny`, `cargo-nextest`, `actionlint`, `zizmor` (idempotent; same as `scripts/setup.sh`) |
| `bun run check` | **the gate**, above |
| `bun run sweep` | free `target/build` (intermediate artifacts; the next build recreates them), see [Build performance](build-perf.md) |
| `bun run fmt` | `cargo fmt` and prettier for the desktop frontend |
| `bun run dup` | jscpd duplication over Rust, TypeScript, Svelte, Astro and Dart (the phone app's `lib/`, generated bindings left out); keep it at or below 1.5% (config: `.jscpd.json`) |
| `bun run lint:workflows` | actionlint and zizmor over `.github/` (also part of `bun run check`) |
| `bun run pr:merge <pr> [--subject s] [--dry-run] [--json]` | maintainer squash-merge: refuses a stale, red or mis-titled PR, writes `Closes #N` from the PR's linked issues (`CONTRIBUTING.md#pull-request-flow-and-who-merges`) |
| `bun run desktop:dev` / `desktop:build` / `desktop:build:app` / `desktop:check` / `desktop:link` | run the app with hot reload, build the bundle, build a debug `.app` only, check the frontend, link the debug bundle |
| `bun run mobile:check` | `scripts/mobile-check.sh`: Flutter analyze and test for `apps/mobile` (also part of `bun run check`; skipped when Flutter is absent) |
| `bun run mobile:lab` | validate a branch on your phone beside your real Atlas: a shadow hostd plus the Atlas Dev app, never touching your install (see [Atlas Mobile](mobile.md#lab-lane-validate-a-branch-without-touching-your-install)) |
| `bun run desktop:lab` | the desktop frontend in your browser with HMR, backed by an invisible agent instance (see Atlas Lab below) |
| `bun run desktop:agent` | start your own agent instance of the desktop app (see [AGENTS.md](https://github.com/codeatlasdev/atlas/blob/main/AGENTS.md)) |
| `bun run site:dev` / `site:build` / `site:check` / `site:deploy` | the website: dev server, static build, `astro check` plus script/function type-checks, Cloudflare deploy |
| `bun run studio` / `studio:check` | the film studio CLI, and its checks (`studio/` is not part of `bun run check`) |
| `bun run icons` / `icons:check` | regenerate the app icons from `assets/*.svg`; fail when anything derived is stale |
| `bun run i18n:extract <path-or-glob>` | move the hardcoded text of Svelte components into the catalog (`--dry-run`, `--json`, `--area`; `--plan` prints the migration batches of [`i18n/MIGRATION.md`](https://github.com/codeatlasdev/atlas/blob/main/i18n/MIGRATION.md); a `.rs` path prints a worklist and never writes) |
| `bun run i18n:check` / `i18n:translate` / `i18n:fix` / `i18n:add-locale` | translations: check every catalog (also part of `bun run check`), draft missing or outdated keys with a model, sort and relock, add a language (see [Translating Atlas](#translating-atlas)) |
| `bun run deps:update` | update JS dependencies everywhere (see [Updating JS dependencies](#updating-js-dependencies)) |
| `bun run release <alpha\|beta\|rc\|stable\|patch>` / `release:dry` / `release:publish vX.Y.Z` / `release:test` | maintainers only: cut a release, rehearse it, (re)publish an existing tag, run the release tooling's tests (see [Releasing](release.md)) |
| `cargo test -p atlas-cli --test mcp` | the MCP integration suite (real binary, live headless run; needs `python3`) |
| `cargo test -p atlas-tui --release -- --ignored --nocapture bench_one_million` | the log pipeline benchmark |
| `cargo test -p atlas-term --release -- --ignored --nocapture screen_bench` | the terminal renderer benchmark |

Tests listen on loopback only (`127.0.0.1`, `[::1]`), never on `0.0.0.0` or `[::]`: with the application firewall on, macOS asks "Do you want the application … to accept incoming network connections?" for every new test binary that listens on the network, and each build is a new binary. The Atlas Mobile tests set `ServeOpts::mobile_loopback` (`ATLAS_MOBILE_LOOPBACK=1` for a spawned `atlas hostd`) and bind their phones with `mobile::loopback_only`; they assert the bound sockets are loopback.

### Tests that need root (Linux only)

The memory-cap tests (`crew/spawn/cgroup.rs`: a leaf gets its limits under the helper unit's own cgroup settings, and an agent over its cap is OOM-killed alone) re-run the test binary in a transient `systemd-run` unit with `Delegate=memory pids`, `ProtectControlGroups=yes` and the one writable cgroup path. They need root and systemd as PID 1 (a GitHub runner has both); elsewhere they print why and pass. Two other tests create a system user: `crates/atlas-host/src/crew/spawn/exec.rs` (`a_real_spawn_runs_as_the_crewmates_user_and_hostd_cannot_touch_it`: the child runs as that uid and another user cannot read its directory) and `crew/spawn/provision.rs` (`a_real_provision_creates_the_user_and_modes_and_a_real_deprovision_removes_them`: real `useradd`, `chown`, modes, `userdel`). Both are `#[ignore]`d with their reason, so `bun run check` and the normal CI jobs skip them. The CI job **Rust (Linux, root tests)** runs them on a throwaway runner (part of the `CI` aggregate):

```sh
sudo -E env "PATH=$PATH" cargo test -p atlas-host --lib crew::spawn -- --ignored
```

To run them yourself use a disposable Linux machine or VM, never your Mac (they are compiled out of the run there) and not a container that shares the host's user database. They add `atlas-cm-ffffffffffff` and `atlas-cm-eeeeeeeeeeee` and remove them when they finish.

The egress rules (`nft`) are a pure string tested for order and content here; applying them, the engine installers and the rest of a crew server on real systemd are covered by the crew end-to-end test ([Atlas Crew, "End-to-end test on real systemd"](crew.md#end-to-end-test-on-real-systemd)).

### Updating JS dependencies

`bun run deps:update` runs `npm-check-updates -u` in the root, `apps/desktop`, `site` and `studio` (config: `.ncurc.json`) and then reinstalls. `typescript` is rejected on purpose: it stays on 6.x because TypeScript 7's native compiler drops the JS API that `svelte-kit sync`, `svelte-check` and `astro check` rely on. Move it by hand once those tools support 7. `studio` pins `hyperframes` exactly; after an update run `bun studio check` and keep the old pin if it fails.

## Build and run your changes

```bash
cargo build -p atlas-cli                   # builds the `atlas` binary (atlas-tui alone is only a library)
codesign -f -s - target/debug/atlas        # macOS: ad-hoc sign or Gatekeeper kills it
atlas link                                 # symlink ~/.atlas/bin/atlas to the debug build
atlas unlink                               # restore the release binary
```

To install the build instead of linking it: `cargo build -p atlas-cli && codesign -f -s - target/debug/atlas && cp target/debug/atlas ~/.atlas/bin/atlas`. Verify with `strings ~/.atlas/bin/atlas | grep <version>`. For the desktop app use `bun run desktop:build:app` (debug `.app` only); never `desktop:build -- --debug`, whose DMG step mounts an image and opens Finder on your screen.

### Develop the desktop app

```bash
bun run desktop:dev        # hot reload: Vite + the Tauri shell; Rust changes rebuild
atlas link --desktop       # make /Applications/Atlas.app your debug bundle
```

While linked, the app shows a **Dev build** badge and refuses to update over your build.

Debug builds run a bridge on `~/.atlas/desktop-debug.sock`, so you (or an AI agent) can inspect the app without the mouse:

```bash
atlas desktop debug info
atlas desktop debug logs --level csp        # console, errors, CSP violations
atlas desktop debug eval 'return await window.__TAURI_INTERNALS__.invoke("projects_list")'
atlas desktop debug shot /tmp/x.png
```

Debug builds also expose the live stores as `window.__atlas` (`ws`, `store`, `agents`, `worktrees`, `api`, `host`); use them from `debug eval` instead of `import()`, which creates separate module instances. Check CSP and IPC on the **bundled** build (`tauri://localhost`), not only on Vite. After a hot reload, reload the webview before trusting socket behavior.

### Develop Atlas with Atlas

The repo has its own `atlas.yaml` (ports 13450-13459). `atlas dev` gives you the Lab (`l` opens http://localhost:13450) and the docs site (`w`, port 13451), both started on demand, plus actions for the gate (`c`), format (`F`), desktop tests (`d`), the debug app build (`b`), an agent instance (`A`), the flicker sweep (`x`), the selection and gesture benches (`B`, `H`), the dev icons (`I`) and a release dry run (`Y`). Agents reach the same things through MCP (`atlas_start_service lab`, `atlas_run_action c`, `atlas_logs_search`), so nobody has to remember which script does what.

CI does the same per job: the plan job maps the changed paths to areas with `scripts/checks/map.ts` (see [The gate](#the-gate)), so a commit that only touches top-level markdown runs no job (`docs/` still rebuilds the site), a change under `.github/workflows` or `.github/actions` reruns every job, and any path the map does not know runs everything. System packages on a Linux runner (WebKitGTK, ffmpeg, sway) are installed by `.github/actions/apt-packages` (`scripts/ci-apt.sh`): the `.deb` files are cached by runner image, architecture and package list (saved from `main` only), and a cold cache downloads from `azure.archive.ubuntu.com` (in the runner's own network; the image's default mirror once gave 7 kB/s) and, if that fails, from the image's own sources, with a time limit on every call, so a slow mirror fails the step in minutes instead of using the job's budget (#604); a step that gave up that way says so in its annotation ("ci-apt: the apt mirror is too slow or down") and the job is rerun like a lost runner (the `mirror` kind of `scripts/ci-lost-runner.ts`). A rerun must not hide a hang: each rerun of a lost, stalled or mirror job comments on the pull request (job, kind, the tests running at the Windows watchdog's last sample, the tail of the log when it exists), adds the label `ci: rerun-after-stall` and appends a line to #520 (`scripts/ci-stall-report.ts`); a test that keeps appearing there is a hang, not the platform.

**A lost or missing runner is rerun, not debugged.** GitHub's hosted runners sometimes fail a job by themselves (issue #520), with one annotation and no log: a Windows runner dies mid-step and ~45 min later the job ends as a failure with "The hosted runner lost communication with the server"; or no runner takes the job (macOS arm64 capacity, #547) and it ends cancelled with "The job was not acquired by Runner of type hosted even after multiple attempts". `.github/workflows/ci-rerun-lost.yml` (on `workflow_run`, CI completed with failure) reruns the failed jobs when every job that failed or was cancelled in the run (the aggregate `CI` job aside) carries only one of those annotations, once per attempt up to attempt 3, and only while the run's head is still the tip of its branch (`scripts/ci-lost-runner.ts` has the rules and `ci-lost-runner.test.ts` the table). A real failure, or a job cancelled without such an annotation (by hand, by a newer push), means no rerun. `bun run pr:merge` says "runner lost, rerun pending" or "no runner was available, rerun pending" instead of "fix it and push" while that rerun is due, and tells you to `gh run rerun --failed` by hand when all three attempts ended that way. The Windows Rust job (`rust-windows`) has a time limit on each step and one of 30 min on the job (a normal run is 5-12 min): when the VM freezes no step timeout ever fires and GitHub only notices ~45-50 min later, so a job that ends by its own limit ("The job has exceeded the maximum execution time", Windows Rust job only: any other job at its limit is a hang or a slow download, a real failure) is also rerun, as "runner stalled until the job's time limit".

While editing, `bun run check:changed` (action `u`) runs only what your changes can affect, from the same map as the gate: the changed crates and every crate that depends on them, plus the desktop, mobile, scripts, skills, site, duplication or workflow checks their paths touch (`--base <ref>`, default the merge base with `origin/main`). `--dry-run` lists the commands and why each runs without running them; `--json` prints the plan as JSON (`bun run check:changed --dry-run --json`): the base, the changed files, which areas are on and why, the commands, and the checks only CI can run. `bun scripts/check-changed.ts --json --stdin` reads the changed files from stdin instead of git: it is the `affected:` plugin of this repo's `atlas.yaml`, so agents get the plan through the MCP tool `atlas_affected` (`tasks`, `tests`, `routes` and `builds` are its contract). `bun run check` is still the gate before a push.

**Windows lint from Linux or macOS.** With [`cargo-xwin`](https://github.com/rust-cross/cargo-xwin) installed, `check:changed` also runs clippy for `x86_64-pc-windows-msvc` on the same crates (in `target/xwin`, so the host build cache stays), catching `cfg(windows)` breaks and code that is dead only on Windows before CI does (#326 shipped one). Set up once: Arch `sudo pacman -S --needed clang lld`, Debian/Ubuntu `sudo apt install clang lld`, Fedora `sudo dnf install clang lld`, then `cargo install cargo-xwin --locked` and `rustup target add x86_64-pc-windows-msvc`. The first run downloads the MSVC CRT and Windows SDK (Microsoft's license applies). `--no-windows` skips it; without cargo-xwin the loop says so and goes on.

Heavy actions call `scripts/heavy.sh <cmd>`, which goes through the machine-wide queue (`atlas run --heavy -- <cmd>`, [CLI reference](cli.md#atlas-run---heavy)): at most `min(cores / 2, GB of memory / 8)` heavy jobs at once (3 on a 12-core, 24 GB Mac) across every worktree and agent on the machine, in order, started only when the 5-minute load is below `0.75 * cores` (`ATLAS_HEAVY_MAXLOAD`) and at least 20% of memory is free (`ATLAS_HEAVY_MINMEM`), under `nice`, with `CARGO_BUILD_JOBS` capped to `max(2, cores / slots)`; `atlas heavy status` shows who runs and who waits. With an installed atlas that predates the queue the script falls back to its previous per-user lock, load and (macOS, Linux) memory wait, so this repo's `atlas.yaml` keeps working on any version. Other projects can use `heavy: true` on an action instead, which needs an atlas with the queue (an older one rejects the unknown key). Use `scripts/heavy.sh` (or `atlas run --heavy`) for anything that builds or benchmarks outside `atlas dev` too.

The app-id migration (`crates/atlas-desktop/src/id_migrate.rs`) has an end-to-end check with real WebKit: `scripts/migration-e2e.sh` builds the app under two throwaway bundle ids, writes `localStorage` in the "old" one, starts the "new" one and reads it back, then deletes both stores and rebuilds the normal dev app (macOS, about two minutes).

### Three ways to look at the app you are changing

| Mode | Start | You see | Use it for |
|---|---|---|---|
| Lab | `atlas dev lab` / `bun run desktop:lab` | the frontend in your browser, HMR | most UI work; agents work here |
| Mirror | `bun run desktop:mirror` | the whole real window of an invisible instance, live, click + type | following an agent, native parts without a window |
| Desktop | `atlas dev desktop` / `bun run desktop:dev` | the real **Atlas Dev** window on your screen (orange icon, own id), HMR + Rust rebuilds | native layout: title bar, toolbar buttons, traffic lights, terminal, gestures, menus |

Atlas Dev runs beside your installed Atlas without touching it (own bundle id and WebKit store). An agent starts the Desktop mode only when you ask it to (`atlas_start_service desktop`, `atlas_stop_service desktop` to go back to headless); on its own it stays on Lab and the mirror.

### Atlas Lab: the frontend in your browser, no Atlas window

Most frontend work does not need the app on screen. Pick the lightest level that covers the change:

| Level | Run | Covers |
|---|---|---|
| 0 | `bun run test`, `cargo test` | logic, stores, protocol, contracts between Rust and TS |
| 1 | `bun run desktop:lab` | any Svelte screen, with HMR, against the real backend |
| 2 | `bun run desktop:agent` | native surfaces: GPU terminal, page stage, gestures, flicker, CSP/IPC of the bundle |

`bun run desktop:lab [--name lab] [--port 1421]` starts (or reuses) an invisible agent instance and serves the frontend with Vite on `http://localhost:1421`. Open that URL in any browser: every `invoke()` and event listener is forwarded through the instance's debug bridge (`apps/desktop/scripts/lab-bridge.ts`, `src/lib/debug/labShim.ts`), so the backend, hostd and data are real and isolated from your own Atlas. You and an agent can share the same URL: the agent drives it headless, you watch or click when you want, and no window ever appears on its own. `bun run desktop:lab --stop` stops the instance.

**See the native parts too:** `bun run desktop:mirror [--name lab]` serves a live view of an agent instance's whole window (GPU terminal, page stage, embedded browser included: frames from ScreenCaptureKit, or `grim` on Linux, not the page snapshot; on Linux the picture is the page area and the GPU terminal does not exist, the terminal is xterm.js) at the URL it prints. Click on it to click the app, and type: with the terminal focused your keys reach the shell the way the terminal maps them (Option-as-Alt, Ctrl+C), elsewhere they go to the focused field or the app's shortcuts. All of it goes through the debug bridge, never OS input, and nothing appears on your screen. The app streams its window with ScreenCaptureKit at up to 30 fps into memory (debug bridge `frame`), only while someone watches. The URL carries a per-run token: without it every route answers 403. It pauses while `scripts/heavy.sh` holds its lock, so benches measure an unwatched app.

Agent instances keep a WebKit store each (WKWebView ignores `HOME`); `--stop` deletes it, and `atlas desktop --agent --gc` deletes the stores of instances that are gone (crashes, killed processes).

**Linux: startup and idle cost (#284).** `scripts/linux-startup-bench.ts [--runs 5] [--idle-settle 30] [--idle-cpu 60] [--save docs/perf/linux-startup.json] <atlas-desktop | .AppImage>` launches the app on the same private headless sway as the launch check, N times (the first on a fresh HOME = "cold", the rest "warm"; the page cache is not dropped, so "cold" is a first launch, not a cold disk), reads `ATLAS_BOOT_TRACE` for `app_ready` (`js:ready:gate`) and which of `show:ready` / `show:deadline` showed the window, watches `/proc` for the spawn, the first `WebKitWebProcess` and the mapped window, then holds one more launch idle: PSS/RSS per process from `smaps_rollup` (app, each `WebKitWebProcess`, `WebKitNetworkProcess`, hostd; `SwapPss` counts, so a loaded machine cannot make a process look smaller by having it swapped out) and CPU over the window. It counts the interactive login shells each launch starts (the rc files of its throwaway HOME log who started them). It exits 1 when `app_ready` never arrives, the page is blank, or a number is over `BUDGET`; `--judge <report.json>` re-judges a saved report with no app, `--no-check` only reports. Idle is measured after the first launch has finished onboarding: on a fresh HOME the welcome deck animates at 60 fps and, with software GL, costs ~50% of a core. The committed baseline is `docs/perf/linux-startup.json` (machine, GPU, driver and WebKitGTK version inside; the process that dominates RSS is named in its `idle.dominant`), and CI runs the bench after the launch check in "Desktop opens (Linux)" and uploads the report as an artifact. The budget is set from that baseline and from the CI runner's own numbers (2x the worst sample for time, 1.6x for memory); a bench number is only comparable to one from the same machine and load (`machine.loadAvg1` is recorded).

**Linux: open the app with no window on your screen.** `scripts/linux-shot.sh` runs the app on a private headless sway (own Wayland socket, own D-Bus session, throwaway HOME; Mesa software rendering, no GPU) and screenshots it with `grim`. `scripts/linux-launch-check.sh <atlas-desktop | .AppImage>` does the same and **fails** on what a first user would hit: the app exits or crashes, no window maps, the page stays one flat colour, `WebKitWebProcess` dies, a crash/EGL/CSP marker on stderr, or any error line in the app's own `~/.atlas/desktop/app.log` (exit codes in its header; evidence in `launch-check-out/`). `scripts/linux-first-run.sh --cli target/debug/atlas --desktop target/debug/atlas-desktop` walks the first-user path with your own builds (`atlas install`, `atlas doctor`, `atlas desktop --from`, then the launch check) in a throwaway HOME, so nothing of it touches yours or downloads a published build. Needs `sway`, `grim` and `dbus-run-session` (`pacman -S sway grim`, `apt install sway grim dbus`). The debug binary must embed the frontend, as `cargo tauri build` does: `(cd apps/desktop && bun run build) && cargo build -p atlas-desktop --features tauri/custom-protocol` (without the feature a debug build waits for the Vite dev server on :1420 and shows nothing). CI runs the check on every change that touches Rust or the frontend (job "Desktop opens (Linux)", screenshot attached as an artifact), and the release runs it on the AppImage and each distro package before publishing ([Releasing](release.md)).

**Linux and Windows gaps.** Windows has no agent instance, so no Lab, mirror or recording there; use `bun run desktop:dev` and level 0 tests. What works in an agent instance (`bun run desktop:agent`, `atlas desktop --agent`):

| | macOS | Linux | Windows |
|---|---|---|---|
| Isolated instance (own HOME, hostd, bridge; `--stop`, `--gc`) | yes (private virtual display) | yes: own headless sway + own D-Bus session under `/tmp/atlas-agent-<name>/run`; never your Wayland/X11/D-Bus session | no |
| `debug info\|state\|eval\|logs\|wait\|click\|type\|a11y\|perf\|resize` | yes | yes | no |
| `debug shot` page, `--selector`, `--rect`, `--window` | yes (WKWebView snapshot, ScreenCaptureKit) | yes: `grim` reads the instance's own compositor (needs `sway`, `grim`); `--full` too; rects are the page's CSS px | no |
| `debug record` (`--seconds N` or `--until <selector>`, `--fps`) | yes (ScreenCaptureKit, `.mov`) | yes: `grim` frames of the instance's compositor piped to `ffmpeg` (needs `ffmpeg` with libx264), constant-rate H.264 in `.mov`/`.mp4`; the result says the `frames` really grabbed and `max_gap_ms`, so a slow machine shows as fewer frames, not a faster video | no |
| `desktop:mirror` (live view + click + type, tokened URL) | yes (ScreenCaptureKit, ~30 fps) | yes: the page area (the webview) through `grim`, JPEG, up to ~30 fps, only while someone watches; clicks and keys go through the bridge, never OS input | no |
| Atlas Lab (`desktop:lab`) | yes | yes: the same Vite/HMR page, every `invoke()` through the instance's bridge (WebKitGTK instead of WKWebView) | no |
| Flicker sweep, selection bench, `debug hide`, gesture record/replay | yes | no (they read ScreenCaptureKit, the macOS swipe monitor and window server) | no |

An instance refuses to start (exit 3) when its environment points at anything but its own display: it never uses `/run/user/<uid>`, has no `DISPLAY`, and turns off the accessibility bridge. On Linux the app is the debug binary with the bundled frontend: `bun run desktop:build:debug` builds it (`desktop:agent` does it the first time). It skips the single-instance guard, so it runs beside your installed Atlas and `desktop:dev` (dev builds use id `br.com.codeatlas.atlas.dev`; a bare `cargo build` binary started WITHOUT `ATLAS_AGENT=1` still uses the installed app's id and would hand off to it: use `desktop:dev`). On Windows use `bun run desktop:dev` (a normal window) and level 0 tests; `check:changed`, `heavy.sh` (the load gate is off on Windows) and the repo `atlas.yaml` work everywhere.

Limits: `Channel` streams (terminals, gestures) and `convertFileSrc` assets are not forwarded, and the browser is not WKWebView; use level 2 for those and for the CSP/IPC check before shipping. The bridge answers only its own page (per-run token, same Origin, JSON content type) and only when `ATLAS_DEBUG_SOCK` is an agent instance; it never talks to your own app.

## Code layout

See [Architecture](architecture.md) for the crates and how the processes talk to each other. Where things go:

| Change | Where |
|---|---|
| a new `atlas` subcommand | `crates/atlas-cli/src/commands/` |
| config fields, validation, the JSON Schema | `crates/atlas-tui/src/config/` |
| scheduling, health, restarts, actions | `crates/atlas-tui/src/runtime/manager/` |
| TUI widgets and keys | `crates/atlas-tui/src/tui/` (the key table is `keymap.rs`) |
| MCP tools, resources, prompts | `crates/atlas-tui/src/mcp/` |
| terminals, exec, files | `crates/atlas-host/` |
| terminal rendering | `crates/atlas-term/` |
| desktop backend | `crates/atlas-desktop/src/` |
| desktop UI | `apps/desktop/src/` |
| agent skills | `skills/<name>/SKILL.md` (embedded at build time) |

## Conventions

- **Tests.** Prefer behavior tests: boundaries, failures and state transitions over getters. Manager tests live in `runtime/manager/tests.rs` and are wired by `#[cfg(test)] mod tests;` in `mod.rs`; when you split modules, check the test count does not drop. Integration tests drive the real binary.
- **Docs move with code.** A change that alters a flag, a key, a config field or a behavior updates the page that documents it, in the same pull request. The TUI help and footer are generated from one keymap table; the desktop shortcuts are `DEFAULT_KEYMAP` in `apps/desktop/src/lib/keys.ts`.
- **Changelog.** User-visible changes get a line under `## [Unreleased]` in `CHANGELOG.md`. Release notes are built from it.
- **Agent instructions.** `AGENTS.md` is a map under 8 KB: the rules that cause damage if broken and a table "before you touch X, read Y". Detail goes into a skill in `.agents/skills/<name>/SKILL.md` (short, with sibling files for the rest) or a nested `AGENTS.md` next to the code it covers; never back into the root file. `.claude/skills` is a symlink to `.agents/skills` because Claude Code reads only that path (Codex and omp read `.agents/skills` directly, and omp de-duplicates by realpath). `bun test scripts/agents-md.test.ts` (part of `bun run check`) enforces the size, the references and the link; on Windows without symlink support the link is still committed as one.
- **Formatting.** `.editorconfig` applies: tabs in TypeScript, Svelte and shell; 4 spaces in Rust; 2 spaces in YAML, Markdown and JSON.
- **Commits.** Short imperative subject, conventional prefixes (`feat:`, `fix:`, `docs:`, `chore:`) as in the history. Explain *why* in the body when it is not obvious.
- **Dependencies.** Prefer the standard library and what is already in the tree. A new crate or package needs a reason in the pull request; `cargo deny check` must pass.

### Adding a built-in coding agent

The agents Atlas launches (omp, Claude Code, Codex, OpenCode) are listed once, in `crates/atlas-core/data/agents.json`: id, label, binary, the `atlas setup <setup>` name that wires its hooks, launch and resume flags, and what it can do (`one_shot`: how it answers one prompt with no tools, which AI commit messages need, `null` when none is verified; `sessions_dir`; `can_ask`; the vendor `install` line; the `key_env` of its API key). Everything else reads that file:

- Rust: `atlas_core::agents::all()` / `get(id)` (CLI, hostd, Atlas Desktop).
- Atlas Desktop frontend: `BUILTIN_AGENTS` in `apps/desktop/src/lib/agentRegistry.ts` imports the same file.
- Atlas Mobile: `apps/mobile/lib/core/protocol/agent_registry_data.dart` is generated by `bun scripts/gen-agents-dart.ts`; `bun test scripts` fails when it is stale, and `bun scripts/gen-agents-dart.ts --check --json` says so for a script.

A new agent is one entry there, then the generator, then what is genuinely per-agent: a logo (`agentLogos.ts`, `agent_logo.dart`), an `atlas setup` module, a sign-in probe (`crew/engine_login.rs`), the check script (`atlas-desktop/src/crew.rs`) and an engine (`crew/engine/`). Tests name each of those that is missing. `scripts/agent-list.test.ts` fails when a list of agent ids is typed anywhere else.

### Runtime rules that came from real incidents

Keep these when you touch the runtime ([Architecture](architecture.md#safety-rules-the-code-relies-on)):

- Orphan cleanup uses only the `.pids` file and exact PIDs; never `lsof` port-kill or negative PIDs.
- Never run `docker compose down` or restart the docker daemon from Atlas.
- Children: `setsid`, `stdin(null)`, `sh -c` for commands.
- TUI: event-stream input, dirty-flag rendering, non-blocking startup.
- Desktop: child webviews make the main window multi-webview; use `get_window("main")` for window operations and `get_webview("main")` for the app's own webview. Never put a secret in content loaded into the embedded browser.

## Brand assets

The Atlas mark has **one source**: [`assets/logo.svg`](https://github.com/codeatlasdev/atlas/blob/main/assets/logo.svg) (36 hand-cut tiles and the cobalt gradient). The app icon, menu-bar icon and animated logo are designed in `assets/templates/*.svg` (plate, shadow and bevel around the tiles; `{{TILES}}` marks where the mark goes). **Everything else is generated, never edited by hand**:

| Generated | Used by |
| --- | --- |
| `assets/app-icon.svg`, `assets/app-icon-1024.png` | README, the Tauri icon set |
| `crates/atlas-desktop/icons/{32x32,128x128,128x128@2x,icon}.png`, `icon.icns`, `icon.ico` | the app bundle: Dock, Cmd-Tab, Finder, **notifications** |
| `crates/atlas-desktop/icons/tray.png`, `assets/tray-template.svg` | menu bar (monochrome template; macOS tints it) |
| `crates/atlas-desktop/icons/tray-linux.png` | Linux tray (the same glyph in white: AppIndicator hosts do not tint) |
| `crates/atlas-desktop/icons/32x32.png` (also the app bundle icon) | Windows tray: the full-colour mark, because the notification area never tints and a black template is a solid disc there (`tray.rs` `icon()`) |
| `assets/logo-animated.svg` | README/docs animation |
| `site/public/{favicon.svg,favicon.ico,apple-touch-icon.png}` | the website |
| `studio/assets/logo/logo.svg` | studio films |
| `apps/desktop/src/lib/onboarding/intro/mark.ts` | the welcome globe (tile paths and boxes) |

```bash
bun run icons          # regenerate all of the above (needs librsvg, ImageMagick, cargo-tauri)
bun run icons:check    # fail when anything derived is stale; re-renders the rasters and compares decoded pixels
```

`assets/icons.lock.json` records the hash of every input (including `scripts/icons.ts`) and the pixel hash of every raster. `bun run check` runs `scripts/icons.test.ts`, which goes red when the mark, a template or the generator changed without `bun run icons`, when a copy was edited by hand, or (with the raster tools installed) when a raster no longer matches a fresh render. To change the brand: edit `assets/logo.svg` or a template, run `bun run icons`, commit the lot.

A notification, the Dock and Cmd-Tab all show the **bundle's** `icon.icns` (Tauri's notification plugin posts under the bundle id `br.com.codeatlas.atlas`; dev builds use `br.com.codeatlas.atlas.dev` and the orange icon in `icons-dev/`, regenerated by `bun run icons:dev`; in `tauri dev` macOS attributes it to Terminal). There is no separate notification icon to maintain. macOS caches icons per app: an installed Atlas picks up a new icon with the next release/update, and a stale Dock or notification icon on a dev machine is the system's icon cache, not the repo. Do not clear system caches to "fix" it; update or reinstall the app.

## Making videos

Every published video (the film on the website, its vertical cut, release videos) is built from code in [`studio/`](https://github.com/codeatlasdev/atlas/tree/main/studio). A film is typed data in `studio/films/<id>/film.ts`; [HyperFrames](https://github.com/heygen-com/hyperframes) (Apache-2.0) renders it and `ffmpeg` mixes the audio, so a rebuild is deterministic and works on a laptop or in CI. The engine choice, with the licensing reasoning, is in [`studio/DECISION.md`](https://github.com/codeatlasdev/atlas/blob/main/studio/DECISION.md); agent-oriented notes are in [`studio/AGENTS.md`](https://github.com/codeatlasdev/atlas/blob/main/studio/AGENTS.md).

You need `bun`, `ffmpeg`/`ffprobe` and, for the TUI take, [`vhs`](https://github.com/charmbracelet/vhs). Recordings, takes, voice and music are **not in git**: they live in `studio/.cache/` (see its manifest with `bun studio assets`) and the published files are fetched with `scripts/fetch-media.sh`.

```bash
bun install                                    # once, at the repo root (workspace)
bun studio check                               # typecheck, brand tokens, film data, 1 s-per-scene smoke render, determinism
bun studio preview atlas-1-0                   # live preview; rebuilds when you save film.ts or a scene
bun studio render atlas-1-0 [--aspect 9:16]    # full render -> studio/out/atlas-1-0/ (mp4, webm, vtt, poster, scrub sprite, report)
bun studio publish atlas-1-0                   # copy into site/public/media with content hashes + film-chapters.json
bun studio release unreleased --as 1.1 --draft # "What's new" video from the CHANGELOG.md section (16:9 + 9:16)
```

Capture and narration:

- `bun studio capture app <scene>` records the **real** Atlas Desktop debug build in a separate instance (own `HOME` and `ATLAS_HOSTD_DIR`, scratch projects, window capture by id; it never sends input to your desktop). Build the debug bundle first (`atlas link --desktop` or `bun run desktop:build:app`). `bun studio capture tui <take>` runs `vhs` on `demo/demo.tape` with its output redirected into the cache.
- `bun studio voice <film>` synthesizes the narration with ElevenLabs and caches each line by a hash of its text, voice and model. Secrets come only from the environment (`ELEVENLABS_API_KEY`, `MAGNIFIC_API_KEY`), never from the repo.
- The final mix is measured: the render fails when the integrated loudness is not within the film's tolerance of -14 LUFS.

Narration is a synthetic voice and the music bed is generated; keep the disclosure and licence notes in `site/public/media/README.md` accurate when you change either. A change to `studio/` is checked with `bun studio check`, not `bun run check`.

## Translating Atlas

English is the source language and Brazilian Portuguese (pt-BR) is the first translation. Translations are plain files in the repository, reviewed in the pull request like code; there is no account to create anywhere. You do not need to write Rust or Svelte to translate. Before you write or translate any user-facing text, read [`i18n/style/README.md`](https://github.com/codeatlasdev/atlas/blob/main/i18n/style/README.md) (voice, punctuation, no em dashes) and the file for your language.

Where the text lives: **one tree for the CLI, the TUI, the desktop app and the website**, `i18n/messages/<area>/<lang>.json`, one small folder per area (`settings`, `crew_onboarding`, `cli_config`, ...). Each area has `en.json`, one file per language, and `meta.json` (what each message is and where it appears). Words every screen shares (Cancel, Delete, key names, "your Mac" or "this PC") live once in the `common` areas. each area's `area.json` says which surface uses it; the files the apps read are generated (`bun run i18n:generate`), so you never edit them.

|What|Where|
|---|---|
|Interface text|`i18n/messages/<area>/<lang>.json`|
|Documentation pages|`docs/<lang>/<page>.md`, with `source_sha` in the front matter|

**Fixing or translating a string.** Edit the file of your language in the area folder, keep every `{placeholder}` exactly as in English, and run `bun run i18n:check`. It is offline and takes seconds; each finding names the file, the line and the key. If a message is marked outdated, the English changed after it was translated: read the English, update yours, run `bun run i18n:fix` to refresh the area's lock file. Use `bun run i18n:find <text>` to see whether a word already exists in `common`. The tone and the terms to keep in English are in `i18n/style/` and `i18n/glossary.yaml`. Plurals need care: in pt-BR zero reads as singular, so give "nenhum" its own variant where "0 arquivo" would sound wrong.

**Drafting with a model.** `bun run i18n:translate` translates only the keys that are missing or outdated, with the glossary and each key's description in the prompt, using your own `ANTHROPIC_API_KEY` or any OpenAI-compatible endpoint (`I18N_BASE_URL`, Ollama works). A native speaker still reviews the result in the pull request.

**Moving existing hardcoded text.** `bun run i18n:extract <path-or-glob> --dry-run --json` shows what would move; without `--dry-run` it writes the English keys (with a description derived from the component), replaces the text with `m.<key>()`, reuses `common*` messages when the English is the same, and leaves pt-BR to `i18n:translate`. It never touches what it cannot move safely (plurals, text joined to an expression or split by inline markup, shortcuts, `.ts` modules, `<script>` strings): the report lists each with `file:line` and the reason, and you migrate those by hand. A second run changes nothing. Rust files get a worklist, not a rewrite. The batches, their order and the quality loop are in [`i18n/MIGRATION.md`](https://github.com/codeatlasdev/atlas/blob/main/i18n/MIGRATION.md).

**Adding a language, in one pull request** (one language per pull request):

1. `bun run i18n:add-locale <tag> --owner @you` (a BCP-47 tag such as `es` or `fr`). It adds the language to `i18n/locales.json`, `atlas_core::lang::SUPPORTED`, the desktop `Info.plist`, and the owner lines in `.github/CODEOWNERS`, and creates `<tag>.json` in every area folder; the generators pick the language up for every surface.
2. Translate with `bun run i18n:translate --locale <tag>` and by hand until `bun run i18n:check` is clean for your language. A new language starts as `community`: a missing key falls back to English instead of failing the gate; pt-BR is the only required one.
3. Open the pull request. The `CODEOWNERS` owner of your language reviews it, and a maintainer merges it.

What is never translated: `--json` and other machine output, MCP, skills, logs, exit codes, the install scripts and the changelog. `ATLAS_LANG=pt-BR atlas ...` forces a language for one command, and `atlas config lang <tag>` saves it; the desktop has Settings > General > Display language.

## Pull requests

The flow is defined once, in [CONTRIBUTING.md](https://github.com/codeatlasdev/atlas/blob/main/CONTRIBUTING.md#pull-request-flow-and-who-merges): branch `type/short-desc` from `main`, a draft pull request, green CI, and two approvals from people other than the author for external contributors (maintainers, the `atlas-maintainers` team, may merge their own pull requests without approvals). A maintainer merges (squash) external pull requests, and only maintainers cut releases. Governance and the reason for the rule are in [GOVERNANCE.md](https://github.com/codeatlasdev/atlas/blob/main/GOVERNANCE.md).

Before you open it: make the change with its tests and doc updates, run `bun run check`, and fill in the template, linking the issue it closes. Small, focused pull requests get reviewed faster than large ones.

## Agent guards

AI agents work in this repo, so the rules about where they write and what they may push are enforced by tools, not by prose. One decision function (`scripts/lib/agent-guard/decide.ts`, table-tested in `scripts/agent-guard.test.ts`) answers "allow or refuse, and what to do instead" for every agent, through four entry points:

| Agent | Wiring (committed) | How it blocks |
|---|---|---|
| Claude Code | `.claude/settings.json`: `PreToolUse` on `Write\|Edit\|MultiEdit\|NotebookEdit\|Bash` runs `bun scripts/agent-guard.ts hook` | exit 2, the reason on stderr is shown to the agent |
| Codex | `.codex/hooks.json`: `PreToolUse` on `Bash\|apply_patch` (hooks must be reviewed once in `/hooks`; the project `.codex/` layer must be trusted) | same, exit 2 |
| omp | `.omp/extensions/agent-guard.ts` (loaded from the session's cwd) calls the same function in-process | `tool_call` returns `{ block: true, reason }` |
| any tool, a human too | `scripts/git-hooks/pre-push` through `git config core.hooksPath scripts/git-hooks` (`scripts/setup.sh` sets it, idempotently, and never replaces a hooks path you chose) | git aborts the push |

What is refused (each message names the fix):

- **Writes outside your worktree.** A file tool or a shell redirect (`>`, `>>`, `tee`, `cp`, `mv`, `rm`, `mkdir`, `touch`, `dd of=`, `cd <main checkout>`, `git -C <main> commit`…) whose target really lands (symlinks and `..` resolved) outside the git worktree of the session's cwd. Temp dirs and the agents' own state (`~/.claude`, `~/.codex`, `~/.omp`, `~/.atlas`, `~/.agents`) are allowed; the main checkout, other agents' worktrees and git's own data (`.git/`) are not. A session that starts in the **primary checkout** is free to write there unless a maintainer opted in once with `git config atlas.agentWrites deny` (then the primary checkout is closed to agents, and `git worktree add` is the way out).
- **`git push` to `main`, `master` or `release/*`**, `--all`, `--mirror`, `--no-verify`, `-c core.hooksPath=`; a bare `git push` while on main.
- **Force-push** (`-f`, `--force`, `--force-with-lease`, `+refspec`).
- **Merging** (`gh pr merge`, the merge/auto-merge API, `bun run pr:merge` unless `--dry-run`).
- **`git stash`** (everything but `list` and `show`): every worktree of a clone shares one `refs/stash`.
- **Deleting** branches and tags (`git branch -d/-D`, `git tag -d/-f`, `git push --delete`, `git push origin :ref`, `git update-ref -d`), and changing `core.hooksPath` or `atlas.agentWrites`.

**Maintainer override.** A maintainer sets `ATLAS_MAINTAINER_OK` in the environment *before launching the session* (or around one command in their own terminal): a comma list of `write`, `push`, `force-push`, `merge`, `stash`, `delete`, `config`, or `all`. Example: `ATLAS_MAINTAINER_OK=push bun run release alpha` (the release pushes `main` and a tag). The guards read it only from their own process environment, and a command that mentions `ATLAS_MAINTAINER_OK` is refused outright, so an agent cannot give itself the override. The pre-push hook cannot tell an agent from a human: a command the agent runs in a shell the maintainer exported it into is the maintainer's.

Limits, so nobody over-trusts it: the guards read commands, they do not sandbox them. A script, `python -c`, `eval` code in an agent's REPL or `sed -i` can still write anywhere; variables (`> "$DIR/x"`) are not expanded. A bug in the guard itself lets the call through and says so on stderr (the pre-push hook fails closed). Debug a decision with `echo '{"tool_name":"Bash","cwd":"<dir>","tool_input":{"command":"git stash"}}' | bun scripts/agent-guard.ts hook --json`.

## CI and releases

CI (`.github/workflows/ci.yml`) runs on every pull request and on pushes to `main` and `release/**`: macOS, Linux and Windows run the Rust gate (one composite action, `.github/actions/rust-gate`, so they cannot drift), Linux and Windows run the repo scripts and their tests (`bun run release:test`, job "Repo scripts"), plus "Desktop opens (Linux)" (the app launched headless, `scripts/linux-launch-check.sh`), the desktop frontend checks, the site build, `cargo deny`, the duplication report and the workflow lint. Fork pull requests run it with a read-only token and no secrets. How it is wired:

- **One required check.** The last job, `CI`, passes only when every other job passed, so jobs can be renamed or split without touching the ruleset. `pr.yml` adds `Signed-off-by` (DCO) and `Conventional title` (the pull request title becomes the squash commit subject). These three are the required checks.
- **No second test run after a merge.** The branch must be up to date to merge, so the squash commit on `main` has exactly the tree its pull request tested. `scripts/ci-tested.sh` checks that (same tree as the pull request head, and a green CI run for it) and then the push skips the tests and only builds what keeps the cache warm. A different tree, a direct push or any API error runs everything. The i18n check is the exception: it runs on every push to `main`, because two pull requests that each pass alone can break the baseline together and only the merged tree shows it (about a second).
- **Caches come from `main`.** A pull request's cache is visible to that pull request only ([GitHub docs](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching#restrictions-for-accessing-a-cache)) and would evict the `main` cache that every pull request restores (10 GB per repository), so caches are saved only from `main` and `release/*`. Runs there are never cancelled; a new push to a pull request cancels its older run.
- **Pinned and audited.** Every action is pinned to a full commit SHA with its exact version in a comment (`# v7.0.1`), updated by Dependabot after a 7-day cooldown. `scripts/lint-workflows.sh` runs actionlint and zizmor (config: `.github/zizmor.yml`, every ignore with its reason) locally in `bun run check` and in CI.
- **Real language servers, outside the gate.** The four `#[ignore]`d tests of `crates/atlas-lsp/tests/real.rs` (rust-analyzer, the npm servers on a managed Node, gopls: they download their servers) run in `.github/workflows/lsp-real.yml`, one matrix job per test: weekly, by hand (`gh workflow run lsp-real.yml`) and on pull requests that touch `crates/atlas-lsp/**` or the workflow. It is not part of `CI`: a server release or a download outage must not block a merge, so read its last run when you change the LSP crate. Locally: `cargo test -p atlas-lsp --test real -- --ignored --nocapture`, with `HOME` and `TMPDIR` pointed at a scratch dir.
- **The Windows installer has its own job.** "Installer" runs `scripts/install-ps1.test.ts` on `windows-latest`, only when `site/public/install.ps1` or that test changes (area `installer` in `scripts/checks/map.ts`; any unknown path or workflow change runs it too). It runs `irm <url> | iex` the way a user does, in `powershell.exe` (Windows PowerShell 5.1, what a stock Windows has) and in `pwsh`, against a local server that plays GitHub (the script reads `ATLAS_GITHUB_ORIGIN`, a test hook that also allows plain http; never set it outside this test), with the real `curl.exe` and `tar.exe` and a stand-in `atlas.exe` compiled with the .NET Framework's `csc.exe`. It covers the newest Stable having no Windows build (the fall back to the alpha, #519), a checksum mismatch and the channel step. It cannot run on macOS or Linux (the script needs `curl.exe` and `tar.exe`); keep `install.ps1` 5.1-safe: no `2>$null` or `2>&1` on a native command outside `Invoke-Native`, ASCII only, no `??` or `$IsWindows`.

Releases: [Releasing](release.md).
