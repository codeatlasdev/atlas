# Changelog

All notable changes to Atlas (CLI, TUI, MCP server and Atlas Desktop). Versions follow [SemVer](https://semver.org); the CLI and the desktop app always ship together with the same version.

> **Atlas is in alpha.** Expect bugs and breaking changes between releases. Please [report what breaks](https://github.com/codeatlasdev/atlas/issues/new). Releases come in [Stable, Beta and Alpha channels](docs/versioning.md); every one of them is alpha-quality until Atlas meets the general availability criteria there.

## [Unreleased]

### Added

- **Zoom the whole layout with ⌘+ / ⌘− / ⌘0** (Ctrl+ / Ctrl− / Ctrl0 on Linux). Steps from 50% to 300% like a browser tab, remembered across restarts, also in the View menu and Settings → Appearance → Zoom. Native views follow: the GPU terminal, embedded browser pages, the space-swipe stage and its gesture zones are placed at the zoomed layout, and clicks in the terminal land on the right cell. A focused GPU terminal keeps ⌘+ ⌘− ⌘0 for its own font size, as before. On Linux and Windows Ctrl− now zooms out (it used to be "go back" in the editor; that command stays in the palette and can be rebound).
- **Side-by-side (split) diff in the Changes tab.** A Unified / Split selector in the diff toolbar shows the old version on the left and the new one on the right, deletions paired with additions row by row (uneven runs leave an empty cell), the changed characters of a modified line marked, and both halves scrolling together. Staging a hunk and commenting a line for the agent work in both layouts, and the choice is remembered in `settings.json` (`diff.view`).

- **Whole-app flicker sweep (`bun scripts/flicker-sweep.ts`).** Films every surface of a real agent instance at 60 fps (boot, sidebar, space switch, Panel with a failed service, action toast, Logs, Changes, Settings, dialogs, menus, palette, error toast, terminal, window resize, browser, tabs) and judges the pixels: single-frame blips, blank frames and BT.1702 flashing. Exits 1 on any, with timestamps and crops in `report.md`; a surface it could not drive, or that left a dialog open, fails too. `atlas desktop debug resize W H` resizes the window mid-recording. See `docs/desktop.md`, "Flicker sweep".

### Fixed

- **Source control lists every untracked file, and paths read correctly.** A new folder was one row (`.logs/`) instead of its files, so they could not be reviewed, staged or discarded one by one; `git status` now runs with `--untracked-files=all` (ignored folders such as `node_modules/` stay one dimmed entry via `--ignored=matching`). Paths clipped at the start with `direction: rtl` also had their edge punctuation reordered (`.logs/` showed as `/logs.`); the text is now an LTR isolate inside the clipping box in Source control, the review file list, the command palette, Find projects and the LSP popover. [#29](https://github.com/codeatlasdev/atlas/issues/29)

- **Desktop: terminals and space swipes flickered.** The GPU terminal's native view is now held off screen until its first frame is drawn (it used to be placed over the pane while still empty, showing the webview through it), and its slot is painted in the terminal colour from the first paint. Slot geometry from the resize/mutation observers is coalesced into one `requestAnimationFrame` per frame instead of one IPC and one forced layout per observer. A dialog now parks a native terminal or browser only when it overlaps it. The xterm.js terminal loads WebGL before `open`, fits right after it, stays hidden until its first render (and during a replay), falls back to the DOM renderer when the WebGL context is lost, and releases its WebGL context while its space is parked. The pill, toasts, checklist, first-run and PDF toolbar no longer use `backdrop-filter` over content the pager moves.

## [1.5.0-alpha.10] — 2026-10-14

Atlas is in **alpha**: expect bugs and breaking changes. Please [report what breaks](https://github.com/codeatlasdev/atlas/issues/new).

> `v1.5.0-alpha.9` was tagged but never published: two timing-sensitive tests failed under the local release runner. This release is the same code plus those tests made deterministic.

### Changed

- **Code of Conduct updated to the Contributor Covenant 3.0** (was 2.1), with the reporting channel (a private report on the repository) and the moderators (the maintainers) filled in. Linked from the README badge, the issue chooser and the pull request checklist.
- **Faster, leaner Rust builds** for contributors: clean `cargo build --workspace` 82 → 49 s and `target/` 3.5 → 2.3 G (dev profiles without dependency debug info, `ring` instead of the C-built `aws-lc-rs`). Tests run through cargo-nextest (installed by `scripts/setup.sh`), intermediates live in `target/build` and `bun run sweep` frees them. Details: `docs/build-perf.md`.
- **CI reworked for contributors.** One required check, `CI`, aggregates every job, and pull requests also need a Conventional Commits title (`Conventional title`; the title becomes the squash commit). After a merge, `main` no longer re-runs the tests its pull request already passed on the same tree; caches are saved only from `main`, so pull requests always restore a warm cache instead of filling the 10 GB quota. The macOS and Linux Rust jobs share one composite action. Workflows are linted by actionlint and zizmor in `bun run check` and CI, every action is pinned to an exact version, and Dependabot waits 7 days before proposing an update. Details: `docs/contributing.md`, "CI and releases".

### Fixed

- **Linux: the tray's Quit did nothing, so Atlas could not be exited.** The menu used the predefined Quit item, which the GTK backend only draws as a disabled entry, and closing the window always hid it. Quit is now a normal item that exits (an explicit exit is never held back), and the tray menu has **Show / Hide Atlas** and **Quit Atlas**, since Linux delivers no tray click events. The tray icon is white (the black macOS template was invisible on dark panels). On a desktop with no tray host (GNOME without the AppIndicator extension) closing the window now quits instead of hiding an app nobody can reach.
- **Linux: docker-backed infra services (minio, postgres, redis…) all showed `failed` / "Failed to start" with an empty View logs, and nothing said why.** Three causes, one rule: a failed start now always leaves its reason on the card, in `.logs/<service>.log` and in View logs. (1) A command-less infra service has no process to print anything, so a failed `docker compose up` was only a toast; the reason (now named: *permission denied on the docker socket → `sudo usermod -aG docker $USER`, log out and in*, *daemon not running*, *docker not found* with the PATH searched, *compose plugin missing*) is written to every infra service, as is a port that never opened (`nothing is listening on :5432 after 30s…`). (2) `docker` was looked up on the app's own `PATH` instead of the login shell's, so a menu-launched app did not find it; the shell's `DOCKER_HOST` was also overridden by `/var/run/docker.sock`, which broke rootless docker (`$XDG_RUNTIME_DIR/docker.sock` is now found). (3) A service whose process could not even be spawned (missing `dir:`, empty `command:`) turned Failed silently; it now says why. macOS (OrbStack) is unchanged.
- **Terminal host stuck on "another atlas hostd is running" forever, terminals unable to start.** After a live upgrade, the pty and lock file descriptors handed to the new terminal host were inheritable (macOS has no close-on-exec flag for received descriptors), so every program it started (language servers, the next host) kept them: descriptors piled up across upgrades until "Too many open files", and once the host died its children still held the singleton lock, so no host could start again and Restart went through the same dead end. Received descriptors are now close-on-exec, and a lock held with no live terminal host behind it is recovered on start (a live host is never bypassed).
- **Error reports carry the cause, not a wall of one repeated line.** The log tails in a report (Report button, diagnostics) strip terminal colour codes and collapse repeated lines into one `(repeated N×)` line, so the lines before a burst of the same error (here the "Too many open files" that started it) reach the issue.
- **A no-probe service could flash Running during a crash loop** when a health watcher of the previous spawn fired right after a restart. Running is now accepted only from a live child, and for services without a port or health check only after the minimum uptime of the current spawn.

## [1.5.0-alpha.8] — 2026-10-14

Atlas is in **alpha**: expect bugs and breaking changes. Please [report what breaks](https://github.com/codeatlasdev/atlas/issues/new).

### Changed

- **The menu-bar popover, redesigned.** It sits on the active theme's ground and surface ladder (Atlas Dark's blue, Atlas Light's paper) instead of pure black, with AA contrast on every text and state color in both. Each running project is one card with a state rail and a chip that names what is wrong (_redis, api +2 failed_), one bar segment per service, a clean CPU sparkline, and **Restart failed** / **Logs** / **Open** / **Stop**. Agents and Terminals are one balanced row, stopped projects a dense list that reveals **Start** on hover or focus, and the popover is keyboard-navigable (↑ ↓ Home End, Enter, ⎋).

### Fixed

- **The popover header showed the old purple tile.** The mark is now drawn from `assets/logo.svg` through the generated `mark.ts` (which also carries the logo's gradient), so `bun run icons --check` covers it.
- **The popover was pure black, not the theme.** The kit declares its whole token set on any element with the class `dark`, and the popover put that class on its root, replacing the applied theme. The popover window is also now created with the cached theme and native appearance, like the main window.
- **The sparkline under a project card was a broken, clipped line.** It was an ECharts canvas handed `var()`/`oklch()` colors it cannot parse, in a 22 px box; it is now an inline SVG that stretches to the card.
- **The project list was cut off at the bottom with no scroll.** The list scrolls inside the popover with the header and footer pinned, and the window is never taller than the room the screen leaves below the menu bar.
- **The Agents and Terminals cards left an empty cell** next to a lone project card.
- **"Find projects" offered a repo's own demo stack as a project.** `atlas/demo` (named `orbit-shop`) was found and added next to your real projects. A `demo/`, `examples/`, `fixtures/`, `testdata/` or `samples/` folder inside a git checkout belongs to that checkout and is no longer offered, by the folder walk or by Spotlight. To drop one already added, use **Remove from Atlas** in its sidebar menu (`⋯`).

## [1.5.0-alpha.7] — 2026-10-14

> `v1.5.0-alpha.6` was tagged but never published: its Linux build failed under the local release runner. This release carries everything from it plus the fixes below.

Atlas is in **alpha**: expect bugs and breaking changes. Please [report what breaks](https://github.com/codeatlasdev/atlas/issues/new).

### Fixed

- **Linux: the stack setup failed with `exit 127: sh: 1: bun: not found` when Atlas Desktop was started from the menu.** A menu launch has no shell rc, so `~/.bun/bin` (and nvm, mise, cargo…) was not on `PATH`. The user's login-shell environment (`$SHELL`, else the passwd entry, run as an interactive login shell with a timeout; rc output around it is ignored) is now one shared probe in `atlas-core` used by every spawn on Linux and macOS: services, `pre_command`, actions, mobile sessions, docker, `atlas dev` itself, and the terminal host's ptys and `exec`. If the shell cannot be probed, the usual user bin directories are prepended and the Settings panel says so. An exit 127 in the setup names the missing binary, the `PATH` Atlas used and how to fix it. See [Shell environment](docs/desktop.md#shell-environment).
- **The Linux release builds under `act`.** The Linux job failed on `sudo: command not found` (the `act` container runs as root); the system-library step now uses `sudo` only when it exists, in `release.yml` and `ci.yml`. The AppImage bundle needs `APPIMAGE_EXTRACT_AND_RUN` (no FUSE in a container), and the Arch job needs `pacman --disable-sandbox` (its seccomp sandbox fails in an emulated amd64 container). `scripts/act-release.sh` gained `-j <job>` to run one job (the Linux build, say) without the macOS ones, and `.actrc` maps `ubuntu-22.04`.

### Changed

- The workflows' GitHub Actions moved to their Node 24 releases (`checkout` v6, `download-artifact` v8, `upload-artifact` v7, `action-gh-release` v3.0.3), still pinned by SHA. Under `act` the artifact upload stays on `upload-artifact` v5, the newest its artifact server accepts.

## [1.5.0-alpha.6] — 2026-10-14

Atlas is in **alpha**: expect bugs and breaking changes. Please [report what breaks](https://github.com/codeatlasdev/atlas/issues/new).

### Added

- **Atlas on Linux.** Releases now ship Linux x86_64 builds of the CLI and Atlas Desktop: `install.sh` and `atlas update` work on Linux, `atlas desktop` installs the AppImage to `~/.local/bin/atlas-desktop` with a menu entry, and every release has an Arch package (`sudo pacman -U codeatlas-atlas-bin-*.pkg.tar.zst`, plus its `PKGBUILD`), a `.deb`, an `.rpm` and an AppImage, each with both `atlas` and `atlas-desktop`. On Linux terminals use xterm.js (the GPU terminal is macOS only), and an `atlas` installed by a package is updated by the package manager, not `atlas update`.
- **Report a terminal-host problem in one click.** **Report** opens the error screen with versions of the app, the CLI and the terminal host, the OS, the last lines of `hostd.log` and of the app log (`~/.atlas/desktop/app.log`, which also records Rust panics and host incidents), the newest crash report's message and location, and the recent host incidents. **Copy details** and **Copy as Markdown** carry the same, and **Report issue** opens the GitHub bug form prefilled. Tokens, passwords, API keys, URL credentials and home directories are removed first. See [Reporting a problem](docs/desktop.md#reporting-a-problem).

### Changed

- **Atlas Dark and Atlas Light are the default theme again, rebuilt as a blue pair on the Tokyo Night palette.** Atlas Dark keeps the Tokyo Night ANSI colors, foreground and syntax hues and sits on one blue ramp (surfaces about 4% lighter per level, no pure black for OLED, a soft foreground, accents at equal lightness inside sRGB); Atlas Light is its Tokyo Night Day counterpart. The terminal, service logs, editor and chrome all read the same file. Dracula, Alucard and One stay bundled, and anyone who picked a theme (Dracula included) keeps it. See [Atlas Dark and Light](docs/desktop-settings.md#atlas-dark-and-light).

### Fixed

- Settings → Shell no longer offers `rbash`, `tmux` or `screen` from `/etc/shells` (they cannot report an environment), and the shell fallback is `/bin/sh` where zsh is not installed.
- **After an update, "Terminal host not reachable" toasts (three identical ones) and "Terminal is gone" on every tab.** Three separate causes. (1) An app older than the CLI that `atlas update` had just installed handed the terminal host over again after every handoff, in a loop (11 takeovers within 6 seconds in a real `hostd.log`), because the loop guard remembered the old daemon's pid and each handoff makes a new one; it is now tried once per session. (2) While the host was briefly unreachable, the terminal list came back empty instead of failing, so every tab was declared gone and **Restart** called the same missing host; the list now errors, the tabs keep their state and **Restart** says the host is down instead of failing silently. (3) A host that is alive but refuses the app (its token file replaced, or wedged) held the singleton lock forever, so every start died with "another atlas hostd is running"; the app now ends exactly that daemon (the pid in `hostd.pid`, only when that process is an `atlas hostd`) and starts a clean one, and says so in the report.
- **One toast per outage, with something to do.** The same host failure from several tabs is one toast with a count (`×3`), a plain sentence instead of a log path, a **Restart terminal host** button (works when the host answers and when it does not) and a **Report** button.
- **⌘V of an image did nothing in the fast terminal.** A screenshot (CleanShot, ⇧⌘⌃4, "Copy Image") pasted into omp, Claude Code or Codex running in Atlas was dropped, because the web view's paste event carries no image. The terminal now reads the macOS pasteboard itself, like Ghostty and iTerm2: text is pasted (bracketed), files copied in Finder paste as shell-quoted paths, and an image-only clipboard is handed to the program the way it expects (an empty bracketed paste, or Ctrl+V when bracketed paste is off), so the agent reads the clipboard and attaches the image. See [Paste](docs/terminal.md#paste).


## [1.5.0-alpha.5] — 2026-10-14

Atlas is in **alpha**: expect bugs and breaking changes. Please [report what breaks](https://github.com/codeatlasdev/atlas/issues/new).

### Changed

- **Settings, reorganized.** A larger window and clearer sections: General (updates, update channel, links and notifications), Appearance, **Editor** (editor preferences and Languages, which used to sit under Appearance), **Terminal**, Workspaces, Agents, Keyboard shortcuts, SSH hosts and Help. Every row has the same layout, the update channel is a compact list with details on demand, and search takes you to each setting's new place.
- **Gitignored files are dimmed** in the explorer (and stay out of Source Control), so `node_modules`, `target` or `.env` no longer look like your code. Readable in every bundled theme.

### Fixed

- The **+** for a new tab disappeared when a group had more tabs than fit, for example with a terminal or browser tab open. The tabs now scroll on their own and **+** always stays in view.

## [1.5.0-alpha.4] — 2026-10-13

Atlas is in **alpha**: expect bugs and breaking changes. Please [report what breaks](https://github.com/codeatlasdev/atlas/issues/new).

### Changed

- **GitHub's "Latest" release is now the newest alpha, while Alpha is the default channel.** The releases page showed `v1.4.0` as "Latest" although every install that never chose a channel follows the newest alpha, which confused people. A build of the default channel is now published as a normal release marked "Latest"; betas and release candidates stay pre-releases. At GA, when the default goes back to Stable, only a Stable is "Latest" again with no other edit. The release flags come from the `default` recorded in `crates/atlas-core/data/channels.json`.
- **Stable never follows that "Latest" badge.** `atlas update`, Atlas Desktop, the website (`/release.json?channel=stable`, `/download`) and both installers now read Stable as the newest release without a prerelease label from the release feed, so choosing Stable still means a real Stable (`v1.4.0` today), not the newest alpha. Installs on `v1.4.0` that never chose a channel keep resolving through `releases/latest`, so they are offered the newest alpha, which is the new default.

## [1.5.0-alpha.3] — 2026-10-13

Atlas is in **alpha**: expect bugs and breaking changes. Please [report what breaks](https://github.com/codeatlasdev/atlas/issues/new).

### Changed

- **Alpha is now the default update channel.** Until a Stable release meets the [general availability criteria](docs/versioning.md#general-availability), an install or update that never chose a channel (no `~/.atlas/update-channel`, no `ATLAS_UPDATE_CHANNEL`) follows Alpha, the newest build from `main`: `atlas update`, Atlas Desktop, `install.sh`, `bootstrap.sh` and the website's Download button all offer it. The default is built into each version, so Atlas 1.4.0 keeps following Stable: to move it to Alpha, run `atlas update channel alpha` (or pick Alpha in Settings → General → Updates) once; versions from this one on follow Alpha by default. Stable and Beta work as before: choose one with `atlas update channel stable`, in Settings → General → Updates, or `ATLAS_CHANNEL=stable` for the installer, and it is kept. When a real Stable ships, the default goes back to Stable with a one-line change.

## [1.5.0-alpha.2] — 2026-10-13

> `v1.5.0-alpha.1` was tagged but never published: a flaky file-watcher test failed its release build. This is the same release with that test fixed.

Atlas is in **alpha**: expect bugs and breaking changes. Please [report what breaks](https://github.com/codeatlasdev/atlas/issues/new).

### Changed

- **Swiping between spaces follows your fingers.** A new gesture engine in Rust drives the trackpad swipe between spaces: it lands on the right space, settles in about 300 ms, no longer goes the wrong way after two quick swipes, and bounces at the first and last space instead of wrapping around. Back/Forward swipes use the same engine.
- **Switching spaces is lighter.** A switch no longer rebuilds the neighbouring space, and menus, tooltips and dialogs of spaces you are not looking at are built only when needed (about 6× fewer page elements touched per switch).
- **For contributors and agents:** `main` and `release/*` now require CI to pass, release tags can no longer be moved or deleted, and `.agents/skills/atlas-workflow` explains branches, the gate and release channels. `atlas desktop debug gesture-record` / `gesture-replay` record and replay real trackpad swipes.

### Fixed

- An agent test instance could briefly show its window on your screen when several started at once; agent windows now appear only when fully inside their own virtual display, and instances start one at a time.

## [1.4.0] — 2026-10-13

> Soak override: promoted after 0h on beta/rc, below the 48h minimum (forced with --force-soak).

Atlas is in **alpha**: expect bugs and breaking changes. Please [report what breaks](https://github.com/codeatlasdev/atlas/issues/new).

### Added

- **Go to definition that works.** ⌘-click (with an underline on ⌘-hover) and F12 jump to the exact place, also into `node_modules` `.d.ts` files and Rust's standard library and crates (opened read-only). ⇧F12 lists references, ⌥F12 and ⌘F12 go to type definition and implementation, and ⌃- / ⌃⇧- go back and forward through your jumps.
- **Language servers start on their own.** Open a file and its server installs and starts with progress in the status bar, no setup: TypeScript uses your project's own `typescript`, dependency files share the project's server instead of starting another, and an old terminal host is explained with a button to restart it instead of a silent "no language server".
- **A welcome worth opening.** The first screen assembles the Atlas globe from tiles of light, locks it with its own sound, and lights a tile for every project *Find my projects* discovers; starting your first stack completes the globe. It plays once and Esc or a click skips it.
- **Sounds, sparingly.** Short sounds for meaningful moments only: your stack is up, a service failed, an agent needs you or finished, an update is ready, discovery finished. They respect macOS's *Play user interface sound effects*, stay quiet in the background, and can be tuned or turned off in Settings → General → Sounds.
- **A richer error screen**: a likely cause, *Copy as Markdown*, system details, the last things the app did, and prompts you can paste to an AI agent to fix, report or explain the error.
- **Release channels: Stable, Beta and Alpha.** Stable stays the default, so nothing changes for you unless you switch. Pick a channel in Settings → General → Updates → *Update channel*, or with `atlas update channel beta` (`ATLAS_UPDATE_CHANNEL` overrides it for scripts, `ATLAS_CHANNEL` does for the installer). Atlas never downgrades by itself; when your build is newer than what a channel offers it says so and `atlas update --downgrade` installs the older one on request. See [Versioning and release channels](docs/versioning.md).
- **Prereleases use `-alpha.N`, `-beta.N` and `-rc.N` versions** and are published as GitHub *pre-releases*, so the Stable channel and the "Latest release" badge never point at them. Each release now also lists a `SHA256SUMS` file and a CycloneDX SBOM, and its `manifest.json` records the channel, the commit and, for a Stable release, the beta it was promoted from.

### Changed

- **`bun run release` takes a channel** (`alpha`, `beta`, `rc`, `stable`, `patch`) instead of `minor`/`major`. A Stable release is a soaked beta plus version metadata: the tool refuses it when anything else changed, or when the beta is younger than 48 hours (`--force-soak` overrides and says so in the release notes). `patch` is now a hotfix cut from `release/X.Y`. See [Releasing](docs/release.md).
- **Readable in every theme.** Every text, icon, field border and focus ring now meets WCAG AA (4.5:1 for text, 3:1 for controls) on the surface it actually sits on, in all six bundled themes, light and dark: sidebar, panel, logs, editor (syntax colours included), settings, menus, tooltips, dialogs, toasts and What's new. Light themes were the worst off: the sidebar sat over the translucent macOS material and secondary text used the "disabled" grey. Theme colours that already read are kept; the rest are lifted in lightness only, keeping their hue.
- **The terminal's minimum contrast defaults to 4.5**, so every ANSI colour reads on every theme's background. If you set it yourself, your value is kept.
- **The welcome screen paints its copy with your dark theme**, since its stage is always a night sky, so it stays readable when the app is in a light theme.
- **For contributors and agents:** `bun run desktop:agent` runs an isolated Atlas on a virtual display that never appears on your screens, and `atlas desktop debug` can take screenshots, record, click, type, wait and audit contrast without focusing anything. Debug builds no longer build a DMG, so Finder stops popping up. See [AGENTS.md](AGENTS.md).

### Fixed

- Settings → Appearance → Languages crashed with `undefined is not an object (evaluating 'languageLabels[i]')`, and language labels could be paired with the wrong language.
- The welcome animation was drawn small in the top-left corner on Retina screens.
- The site could serve an old cached copy of the film after it was re-cut; media URLs now carry the version of the published file.

## [1.4.0-beta.1] — 2026-10-13

Atlas is in **alpha**: expect bugs and breaking changes. Please [report what breaks](https://github.com/codeatlasdev/atlas/issues/new).

### Added

- **Go to definition that works.** ⌘-click (with an underline on ⌘-hover) and F12 jump to the exact place, also into `node_modules` `.d.ts` files and Rust's standard library and crates (opened read-only). ⇧F12 lists references, ⌥F12 and ⌘F12 go to type definition and implementation, and ⌃- / ⌃⇧- go back and forward through your jumps.
- **Language servers start on their own.** Open a file and its server installs and starts with progress in the status bar, no setup: TypeScript uses your project's own `typescript`, dependency files share the project's server instead of starting another, and an old terminal host is explained with a button to restart it instead of a silent "no language server".
- **A welcome worth opening.** The first screen assembles the Atlas globe from tiles of light, locks it with its own sound, and lights a tile for every project *Find my projects* discovers; starting your first stack completes the globe. It plays once and Esc or a click skips it.
- **Sounds, sparingly.** Short sounds for meaningful moments only: your stack is up, a service failed, an agent needs you or finished, an update is ready, discovery finished. They respect macOS's *Play user interface sound effects*, stay quiet in the background, and can be tuned or turned off in Settings → General → Sounds.
- **A richer error screen**: a likely cause, *Copy as Markdown*, system details, the last things the app did, and prompts you can paste to an AI agent to fix, report or explain the error.
- **Release channels: Stable, Beta and Alpha.** Stable stays the default, so nothing changes for you unless you switch. Pick a channel in Settings → General → Updates → *Update channel*, or with `atlas update channel beta` (`ATLAS_UPDATE_CHANNEL` overrides it for scripts, `ATLAS_CHANNEL` does for the installer). Atlas never downgrades by itself; when your build is newer than what a channel offers it says so and `atlas update --downgrade` installs the older one on request. See [Versioning and release channels](docs/versioning.md).
- **Prereleases use `-alpha.N`, `-beta.N` and `-rc.N` versions** and are published as GitHub *pre-releases*, so the Stable channel and the "Latest release" badge never point at them. Each release now also lists a `SHA256SUMS` file and a CycloneDX SBOM, and its `manifest.json` records the channel, the commit and, for a Stable release, the beta it was promoted from.

### Changed

- **`bun run release` takes a channel** (`alpha`, `beta`, `rc`, `stable`, `patch`) instead of `minor`/`major`. A Stable release is a soaked beta plus version metadata: the tool refuses it when anything else changed, or when the beta is younger than 48 hours (`--force-soak` overrides and says so in the release notes). `patch` is now a hotfix cut from `release/X.Y`. See [Releasing](docs/release.md).
- **Readable in every theme.** Every text, icon, field border and focus ring now meets WCAG AA (4.5:1 for text, 3:1 for controls) on the surface it actually sits on, in all six bundled themes, light and dark: sidebar, panel, logs, editor (syntax colours included), settings, menus, tooltips, dialogs, toasts and What's new. Light themes were the worst off: the sidebar sat over the translucent macOS material and secondary text used the "disabled" grey. Theme colours that already read are kept; the rest are lifted in lightness only, keeping their hue.
- **The terminal's minimum contrast defaults to 4.5**, so every ANSI colour reads on every theme's background. If you set it yourself, your value is kept.
- **The welcome screen paints its copy with your dark theme**, since its stage is always a night sky, so it stays readable when the app is in a light theme.
- **For contributors and agents:** `bun run desktop:agent` runs an isolated Atlas on a virtual display that never appears on your screens, and `atlas desktop debug` can take screenshots, record, click, type, wait and audit contrast without focusing anything. Debug builds no longer build a DMG, so Finder stops popping up. See [AGENTS.md](AGENTS.md).

### Fixed

- Settings → Appearance → Languages crashed with `undefined is not an object (evaluating 'languageLabels[i]')`, and language labels could be paired with the wrong language.
- The welcome animation was drawn small in the top-left corner on Retina screens.
- The site could serve an old cached copy of the film after it was re-cut; media URLs now carry the version of the published file.

## [1.3.0] — 2026-10-12

Atlas is in **alpha**: expect bugs and breaking changes. Please [report what breaks](https://github.com/codeatlasdev/atlas/issues/new).

### Added

- **A redesigned *What's new* sheet.** Each version gets a header with its version, date and a *Latest*, *Installed* or *New* badge, the changelog's one-line summary, and its changes in cards by kind (Added, Changed, Fixed, Security, Removed), each change as a bold title with its text under it (long ones fold behind *More*; code and links are styled). When several versions are between yours and the latest, a sticky version index follows your scroll, each version folds, and `j`/`k` or the arrows move between them. Loading, offline and empty states have a Retry, and the footer has *Full changelog* and *View on GitHub*. Light and dark themes, reduced motion respected.
- **Updating shows what it is doing.** The button says what you get (*Update to 1.2.2 and relaunch · 15 MB*) and, while it runs, the footer shows Download, Verify, Install CLI, Update app, Terminal host and Relaunch with a real progress bar and byte count for the download.
- **An update can no longer leave the CLI and the app on different versions.** If the app cannot be updated after the CLI was, the previous CLI is put back and Atlas tells you so; if even that fails it says exactly what to run. Only one update runs at a time (`~/.atlas/.update.lock`), across the app, the terminal and other windows.
- **Skipping a version can be undone.** Settings → General → Updates says when you skipped the latest version and offers *Stop skipping*. *Remind me later* only closes the sheet; the pill stays.
- **The "Updated to X" card shows what changed**, in one line from the changelog.

- **`atlas update` never leaves the terminal host stale in silence.** After the app updates, a new _Terminal host_ step compares the running `atlas hostd` with the new version: it hands it over live without ending terminals, restarts it silently when none is running, and otherwise asks before ending anything (`Restart now and end N terminals? [y/N]`). Declined or non-interactive runs print what stays old, what that turns off (for example language features) and the command to fix it.
- **`atlas hostd --check`, `--reconcile` and `--restart`.** `--check` reports the host's version, uptime and running terminals against the CLI and exits `0` current, `3` stale or `1` not running; `--reconcile [--choice ask|idle|later]` brings it up to date without losing terminals when it can; `--restart` replaces it on request and verifies the new one.

- **The app tells you when the terminal host is stale.** On every connect Atlas Desktop compares the running host with itself: it hands it over live (a _Terminal host updated_ toast, no terminal lost) or restarts an empty one silently. When terminals run and features such as language servers would stay off, a _Restart the terminal host?_ dialog lists them (`title — ~/folder`) with **Restart now**, **Restart when idle** (also applied when you quit with nothing running) and **Later**; it asks once per host per session and never ends a terminal without your choice. A _Terminal host needs a restart_ chip in the sidebar footer, a _Terminal host_ row in Settings → Updates and a line in the menu-bar popover stay visible until it is current; a host that only differs in version shows a calm _Terminal host 1.1.0_ chip.

- **The editor says why a language server is not working, and how to fix it.** A status bar chip under every text file shows its state (ready with name and version, starting, installing with progress, off in settings, no server for this language with the supported ones listed, needs Go or curl with the command to install it, offline, crashed with the last lines of the server's log, custom command not found) with the actions that fix it. F12, F2, ⇧F12 and ⌘. in a file without a server now explain once instead of showing "No language server for this file". With a terminal host too old for language servers the chip reads _Language features need a newer terminal host_ and offers **Restart terminal host** (listing the terminals it would end) or **Restart later**.
- **Settings → Appearance → Languages** lists every language server (on your `PATH`, managed by Atlas or missing, with version and path) and installs, updates, uninstalls, restarts it and opens its log. Per language: a custom server `command` and `args`, `initialization_options`, `settings`, or off, in `settings.json` under `lsp` or per project in `.atlas/settings.json`.
- **A VS Code-grade editor core.** Status bar (position, selections, indentation, line ending, encoding, language, language server, problems; click to change), breadcrumbs of the symbols around the caret, multiple cursors (⌥-click, ⌘D, ⌘⇧L, ⌥⌘↑/↓), column selection, move and copy line, delete line, line and block comments, indent and outdent, folding, go to line (⌃G), expand and shrink selection, sticky scroll, bracket pair colors, indent guides, rulers, whitespace rendering, an optional minimap, and a find and replace panel with regex, whole word, case, in selection, match counter and replacement preview. New `editor.*` preferences for all of it.
- **`.editorconfig` and file-shape awareness.** `indent_style`, `indent_size`, `tab_width`, `end_of_line`, `trim_trailing_whitespace` and `insert_final_newline` are honored; without one, the file's own indentation and line endings are detected and kept (CRLF files stay CRLF). Saving trims trailing whitespace and ends the file with a newline unless told otherwise.
- **Changes made outside the editor are followed.** A file edited on disk reloads (keeping the caret) when you have no unsaved edits; with unsaved edits Atlas asks to load from disk or keep your version, and autosave never overwrites the other change silently. Deleted files are reported. **Revert file** in the toolbar and the command palette.
- **Large files open.** Text files up to 32 MB open instead of being refused; past `editor.large_file_threshold_bytes` (2 MB), or with a very long line (minified bundles), the editor turns off the heavy extras and says so.

### Changed

- **The editor always fills its pane.** The scroll area was as wide as the longest line, so short files left dead space and long ones scrolled the whole editor; it is now exactly the pane, scrolls horizontally only when a line needs it, keeps the line numbers in place and no longer moves the window behind it.
- **⌘/ ⌘[ ⌘] ⌘L ⌘⇧\ edit the text while the cursor is in an editor** (comment, outdent, indent, select line, jump to bracket); the shell commands they normally run (Help, Back, Forward, Logs, Split down) stay in the command palette and the menus.
- **One `.editorconfig` implementation** for the editor and Format, with the full rules (`root = true`, section precedence, braces, ranges).
- **An idle terminal host exits on its own.** With no running terminal and no connected app or CLI for 10 minutes (`ATLAS_HOSTD_IDLE_SECS` to change), `atlas hostd` stops and removes its socket and pid file, so an old one no longer survives updates for days; it also stops if its folder is deleted. A host with a running terminal never exits.

### Fixed

- **The release date in *What's new* was one day early.** A calendar date such as `2026-10-12` was read as UTC midnight and shown in local time, so it showed the 11th west of Greenwich. Dates are now shown as the calendar day they are.

## [1.2.1] — 2026-10-12

Atlas is in **alpha**: expect bugs and breaking changes. Please [report what breaks](https://github.com/codeatlasdev/atlas/issues/new).

### Added

- **Language servers, built in.** Completion (with snippets and auto-imports), hover docs, signature help, inline errors, go to definition (F12 / ⌘-click), find references, rename (F2), format (⇧⌥F), quick fixes (⌘.), inlay hints and document symbols for Rust, Go, TypeScript/JavaScript, PHP, Python, HTML, CSS, JSON, YAML and Bash. The servers run inside `atlas hostd` (new `atlas-lsp` crate, also on remote projects): one process per language and project, shared by every open file, stopped after 5 minutes unused, restarted if it crashes. On first use Atlas downloads the server into `~/.atlas/lsp` and verifies its checksum; a copy on your PATH (e.g. `rust-analyzer` from rustup) is used first.
- **HTML live preview.** The globe button or ⌘⇧V opens the file in the built-in browser next to the editor, served from a local live server; saving reloads the page and CSS changes apply without a reload.
- **A view for every dev file.** Syntax colors for Dockerfile, Makefile, `.env`, `go.mod`, ignore files, Procfile and dozens more, detected by file name. The eye button (⌘⇧M) shows a dedicated view for `.env` (values hidden, compared with `.env.example`), `package.json` (scripts you can run), Dockerfile (stages and warnings), Compose files, Makefile/Justfile (targets you can run), `atlas.yaml` (services with inline validation), Cargo.toml, pyproject.toml, go.mod, composer.json, GitHub Actions workflows, tsconfig, CSV (sortable table), JSON/YAML/TOML (collapsible tree) and notebooks. Format works without a language server (JSON built in, web formats via Prettier loaded on demand).
- **Missing projects.** Projects whose folder is gone show as *Not found* with Locate…, Check again and Remove; Refresh and Remove all in the sidebar; Relink when a moved copy is found.
- **Find projects** scans your code folders plus Spotlight, streams results with branch, remote and service count, adds several at once, and can list folders that could use Atlas with an Init button.
- **Dracula is the default theme**, with its official light variant **Alucard** for light mode. Colors, the ANSI table, the UI palette and the syntax scope mapping follow the [Dracula specification](https://draculatheme.com/spec) exactly; one theme now styles the whole app: editor, terminal, diffs, logs, Markdown, language-server popups, git colors and scrollbars. You keep your own choice: only people still on the old default move over.
- **Use your Ghostty theme**: Atlas reads `~/.config/ghostty/config` (including `theme =`, `config-file =` and the Ghostty app's bundled themes) and imports its colors, font and cursor settings into a theme of your own, stored only on your Mac. Dracula is by [Zeno Rocha](https://github.com/zenorocha) and [Luxonauta](https://github.com/luxonauta) and the Dracula team ([draculatheme.com](https://draculatheme.com), MIT); if you love it, get [Dracula Pro](https://draculatheme.com/pro).
- **A theme system you can fully customize.** Any partial theme is completed automatically (surfaces, borders, accents and syntax derived from a few colors, with readable contrast). Import Ghostty configs, iTerm2 `.itermcolors`, VS Code themes and Base16 schemes, export as JSON or a Ghostty config, and drop files into `~/.atlas/themes/` (reloaded live). The terminal can use its own theme, font family (any installed font, including variable fonts such as Fira Code), thicken, cursor colors, selection colors and padding.
- **Theme picker** in Settings → Appearance with a preview of every theme (click to apply), and ⌘K commands: `Theme: <name>`, `Theme: Next/Previous`, `Terminal theme: …`.
- **Import theme packages.** Settings → Appearance → Import theme accepts a zip, `.vsix` or folder, finds every theme inside (VS Code, iTerm2, Ghostty, Alacritty, Base16, Atlas/Zed), merges the variants that exist in several formats (VS Code syntax plus the terminal palette) and lets you pick which to import. Archives are unpacked to a temporary folder on your Mac and nothing is uploaded.
- **Dracula credit.** The theme picker credits Dracula by Zeno Rocha and Luxonauta and links to Dracula Pro.

### Changed

- Diff hunks and the Config diff preview are highlighted with the editor's own colors, so they follow the theme.
- The `theme` setting is now `{ v: 2, mode, light, dark, terminal }`; older values are read as before.
- YAML parsing moved from the deprecated `serde_yaml` to `serde_norway`.

### Fixed

- Dragging the window by its title areas failed with `plugin:window|start_dragging not allowed by ACL`; dragging and double-click to zoom work again.
- **No more theme flash or empty window at launch.** The window now opens hidden and appears when the first real frame (your theme, the sidebar, the restored tab) is ready, instead of an empty translucent window, then a default-theme skeleton, then your theme. Atlas remembers the applied theme in `~/.atlas/desktop/theme-cache.json` and applies it before the page paints; your own themes are read straight from disk instead of waiting for the terminal host. The "Enable omp integration" banner no longer pushes the workbench down when it shows up.
- **Faster startup.** The first screen no longer loads the editor, language-server, diff, chat, charts, Settings and dialog code (initial JavaScript −42 % raw, −47 % gzip); each loads the first time you use it. Projects and the workspace load in parallel, and views waiting for the terminal host continue the moment it connects instead of on the next 100 ms tick.
- Opening a project could crash with `state_unsafe_mutation` (also fixed in 1.1.2).

## [1.1.2] — 2026-10-12

Atlas is in **alpha**: expect bugs and breaking changes. Please [report what breaks](https://github.com/codeatlasdev/atlas/issues/new).

### Fixed

- Atlas Desktop crashed on launch with `state_unsafe_mutation` ("Where: route /") when a project had no saved layout yet, e.g. on a fresh install. The breadcrumbs and the file explorer created the missing layout while rendering; they now only read it.

## [1.1.1] — 2026-10-12

### Changed

- **Native trackpad swipes.** Two-finger swipes over the sidebar (spaces) and over a tab group (tabs) are tracked by macOS itself, the same way Safari swipes between pages: the view follows your fingers, rubber-bands at the ends, gives a light haptic tick at the commit point, and one swipe moves exactly one space or tab. Momentum never chains into a second switch. Tab swipes now also work on tab strips that scroll. They follow System Settings → Trackpad → *Swipe between pages*; when that is off, the previous recognizer is used.
- **Instant space switching.** The spaces next to the current one stay ready in the background, so a switch shows them instead of building them, and the slide springs from where your fingers let go. Clicks and shortcuts use the same animation.
- **Error screens.** A crash in one tab or panel now shows an error card in that spot only (Retry, Close tab, Copy details, Report issue, and the stack under Details) while the rest of the app keeps working. A crash of the whole window shows a proper error page with Reload app instead of a bare "500". Errors outside rendering show a toast with Details, and repeats are grouped.

### Fixed

- Opening an SVG file crashed the app with a "500" page (an update loop in the SVG preview). Spaces with an SVG tab open crashed when you switched to them.
- Update checks and *What's new* failed with `403` once GitHub's unauthenticated API limit (60 requests per hour per network) was used up. Atlas now reads the latest version from the release page and the notes from `CHANGELOG.md`, with no API calls and no limit.
- `atlas … | head` crashed with "failed printing to stdout: Broken pipe"; short commands now end quietly.
- The installer works when the GitHub API is rate-limited.

## [1.1.0] — 2026-10-12

Atlas 1.1 is the first open-source release: the code is public under MIT OR Apache-2.0, with a public repository, CI on pull requests and a docs site at https://atlas.codeatlas.com.br/docs. Atlas Desktop gets a new shell (a space per project, sidebars that peek in from the window edge, trackpad swipes), a project panel with live charts, viewers for images, PDF, media and Markdown, a Ghostty-style terminal keyboard, and updates you can read before you install them. `atlas.yaml`, the CLI, the TUI and the MCP tools stay compatible with 1.0.

### Highlights

- **Open source.** MIT OR Apache-2.0, a public repository, DCO sign-off, CI on every pull request, governance, a privacy statement and a docs site.
- **A new desktop shell.** Every project is a space with its own icon and color. Sidebars hide and peek in from the window edge, two-finger swipes switch spaces and tabs, and the header shows a breadcrumb with back and forward history.
- **A project panel that shows the stack live.** KPIs, memory, CPU and error-rate charts, a health timeline, service cards with sparklines, an Actions menu, an Action output drawer, an Activity drawer and a Setup strip.
- **Look at more things without leaving the app.** Images, PDF, video and audio, Markdown and a hex fallback open in viewers; project-wide search runs on ⌘⇧F.
- **Terminal keys that match Ghostty.** ⌘⌫, ⌥⌫, ⌘←/→, ⌘K clear, font zoom, click to move the cursor, copy on select and Option as Alt. The terminal host can now be upgraded without closing your terminals.
- **Logs by time.** A time range in the TUI (`V`) and the desktop, burst-safe streaming with gap markers, and one level-detection rule shared by both.
- **Updates, shown properly.** An Update pill, a What's new sheet with the release notes of every version you skipped, and an unobtrusive amber line on dev builds.
- **Native macOS behavior.** Real menus, ⌘S to save, ⌘⇧T to reopen a closed tab, ⌃Tab by recent use, a menu-bar popover and a new app icon.

### Workspace and navigation

- **Spaces.** Every project is a space with its own worktrees, tabs, panes and docks, and a color that tints the selection, the header and focus states. Switch with ⌥⌘← / ⌥⌘→ or ⌃1…⌃9.
- **Project icons.** Each project gets an icon: your emoji or Lucide icon, the repo's own logo or favicon (auto-detected), or a monogram, plus an optional color. Right-click a project → **Change icon and color…**. Shown in the sidebar, the header breadcrumb, the panel, the palettes, the Files header and the tray.
- **Sidebar.** Shows the active project and its worktrees (a single checkout is just its own row); other projects are collapsed under **Other projects**; the bottom strip is now project icons, not dots.
- **Magic sidebars.** ⌃⌘S hides the left sidebar and ⌘R the right one. Move the pointer to the window edge and the sidebar slides in over the content (after ~60 ms, or at once on a flick), stays while the pointer is on it, slides out 250 ms after you leave, and ⎋ closes it. A menu or dialog opened from it keeps it open. The peeked panel is opaque.
- **Trackpad gestures.** Two-finger swipes over the sidebar switch space, over the tab strip switch tab, and over content where nothing can scroll go back or forward (⌘[ / ⌘]). A swipe only starts when nothing under the pointer can scroll that way, one swipe is one switch, and with _Reduce motion_ on the switch is instant. Pinch is never a swipe.
- **Breadcrumb and history.** The header shows `Space › worktree › tab`, each part clickable. ⌘[ / ⌘] walk the worktrees and tabs you visited, skipping ones that no longer exist.
- **Compact header.** One row with the health pill, the notifications bell, dock toggles, **Activity**, **Actions ▾**, the `…` menu and **Start / Stop**.
- **Pinned Panel tab.** The Panel is always the first tab of the first pane, a compact icon without a close button. ⌘W on it only says so; it cannot be dragged, split or moved; it is recreated if a worktree opens without one.
- **Docks.** Left, bottom and right docks have header toggles that show their state and shortcut. Drag an edge to resize (arrow keys nudge, double-click resets); sizes are remembered.

### Project panel

- **KPI row:** services running of total, restarts in the last hour, error lines in the last 5 minutes, and memory and CPU of the whole stack. Tiles turn amber or red when something needs a look.
- **Service cards,** failed and starting first: state ring, port (click to open or copy the URL), CPU and memory sparklines of the last ~2 minutes, last log line, restart badge, `depends_on` chips, and on hover **Restart**, **Stop / Start**, **Logs** and **Terminal** (opened in the service's `dir`). A failed card shows its reason and **View logs**. Arrows, Home, End and ↩ navigate; **R** restarts a running service after a confirmation.
- **Live charts:** stacked-area memory and CPU for the heaviest services, an error-rate bar chart over the last 5 minutes, and a **Health over time** timeline of running, starting and failed bands for the last 10 minutes; click a lane to open its logs. Samples are taken in the app every 3 s and kept for 10 minutes.
- **Actions menu** (header **⚡ Actions ▾**): every `actions:` entry of `atlas.yaml`, grouped like the TUI help, with its bare key. Destructive ones (`confirm:`) are marked and ask first; disabled ones say why. A running action shows a spinner and its label, and finishes with a result chip (or a toast for a project you are not looking at).
- **Action output drawer.** Keeps the output of the last 10 action runs per project (up to 5000 lines each), newest first with status, start time and duration. ANSI colors, follows the end while it runs, **Copy** and **Run again**; `path:line[:col]` opens in the editor, URLs in the preview browser. Opening the app mid-run replays the last 500 lines the runner kept per recent run. Open it from the Actions button, the result chip, or ⌘⇧P → _Show action output_.
- **Activity drawer.** The stack's events grouped by time. Bursts collapse into one row (`server restarted 3×`) and identical consecutive alerts fold (`boom ×4`); filter by **All / Errors / Restarts / Actions**; click a row to open that service's logs at that moment (a time range that would hide it is cleared, with a toast). The header button shows an unread count.
- **Setup strip.** Shown when the project has a dependency step (`pre_command` or an auto-detected installer): last run, result, **Run again** and **View output**. While it runs it tails live output; on failure it turns red with the reason, **Retry** and the last lines.
- **Empty states:** with the stack stopped, a card says what **Start** will launch; a project without services links to `atlas.yaml`.

### Files and search

- **Viewers by extension.** Images (png, jpg, gif, webp, avif, bmp, ico, svg) on a checkerboard: fit to view, ⌘+ / ⌘− / ⌘0, pinch or ⌘-wheel zoom, drag to pan, double-click toggles 100%, pixel-crisp past 200%, an info bar and copy image; SVG has an _Edit source_ toggle. **PDF** renders with pdf.js (page thumbnails, zoom, find). **Video and audio** (mp4, mov, webm, mp3, wav, ogg, m4a) use native players. **Markdown** previews on ⌘⇧M (syntax-colored code, tables, task lists, images relative to the file). Anything else shows its size, **Open externally**, **Reveal in Finder** and a hex dump of the first 4 KB. Media loads in 8 MB chunks up to 200 MB; text opens up to 2 MB.
- **Search in project (⌘⇧F).** Searches every file in the worktree as you type (a newer query cancels the older one), with match case, whole word and regex toggles and include/exclude globs. Results are grouped by file; ↑ ↓ move, ← → fold, ↩ opens the file at the line and column. Runs `rg` through the terminal host (so it also works on SSH hosts, honoring `.gitignore`) and falls back to `git grep`. Shows at most 2000 matches in 200 files and says when results were cut.
- **Explorer keyboard.** Arrows, ←/→, Home, End and ↩ navigate; F2 or _Rename_ and _New file_ edit in place; the locate button reveals the active file. The Explorer is git-colored and live.

### GPU terminal

- **Ghostty-style keys** (with a GPU terminal focused): ⌘⌫ deletes to line start, ⌥⌫ deletes a word back, ⌘←/⌘→ go to line start/end, ⌥←/⌥→ move by word, ⌥Delete deletes a word forward. With the Kitty keyboard protocol on, programs get the real key and modifiers instead.
- **Terminal actions:** ⌘K clears the screen and scrollback; ⌘A selects all; ⌘↑/⌘↓ jump between prompts; ⌘Home/⌘End and ⌘PgUp/⌘PgDn scroll; ⌘D / ⌘⇧D split right / down. See [docs/terminal.md](docs/terminal.md#keyboard).
- **Font zoom:** ⌘+ / ⌘= / ⌘− / ⌘0 change the font size of that terminal only, live (6–48 pt).
- **Click to move:** a plain click on the prompt being edited moves the shell cursor there (needs OSC 133; `terminal.click_to_move`, on by default). **Copy on select** (`terminal.copy_on_select`, on). **Option as Alt** (`terminal.option_as_alt`: `both`, `left`, `right`, `none`; default `both`).
- **Alternate-screen wheel:** `less`, `vim` and `htop` get Up/Down arrows from the wheel (xterm alternate scroll, DECSET 1007).
- **Search and links:** ⌘F regex search with a match counter and highlights; hold ⌘ over a URL or OSC 8 link to underline it, ⌘-click opens it (Atlas browser, else system browser).
- **Live preferences:** theme, font size, cursor, scrollback, ligatures, minimum contrast and the OSC 52 policy apply to terminals that are already open.
- **Live terminal-host upgrade.** After an update, the new `atlas hostd --takeover` receives every pty master (`SCM_RIGHTS`), the session state and the pid-file lock, and the old process exits. Programs keep running, scrollback keeps appending to the same files, the app reattaches and shows a _Terminal host updated_ toast. If the successor fails before confirming, the old daemon takes its ptys back. `atlas hostd --upgrade` does the same from a shell. A protocol 1 host (the one in 1.0) cannot hand over: it restarts silently when no terminal runs, otherwise terminals keep using xterm.js and Settings offers **Restart terminal host**; later upgrades are live. Pipe-mode sessions (`omp --mode rpc-ui`) end with the old daemon.

### Logs

- **Time ranges.** The TUI's `V` cycles `last 1m → 5m → 15m → 1h → since restart → since start`, then a custom prompt (`14:05` or `14:05–14:20`, today, both ends inclusive; `-`, `–`, `—` or `..` separate), then all time. The desktop has a clock chip in the Logs toolbar with the same presets and custom range. The range composes with the service, level, find and filter, and counts, error/warning totals and the "N new lines" pill follow it. `last N` windows slide: lines that age out are dropped without rescanning. The TUI shows a `time: …` badge; it is not saved between sessions.
- **Burst-safe streaming.** A client that falls behind skips log lines and shows an "N lines skipped" marker instead of being disconnected; the full output stays in `.logs/`. The TUI keeps its view in sync with the buffer by trimming exactly the rows that were evicted, so a chatty service no longer leaves stale rows on the All tab.
- **Consistent levels.** The TUI and the desktop use the same level rule, checked against one shared fixture (`testdata/log-levels.json`): `0 errors found`, `GET /api/error 500` and `TypeError: x` stay info, `ERROR:`, `panic:` and red ANSI are errors.
- **"Since restart" survives eviction:** the newest restart marker per service is remembered even after its line leaves the ring.

### Keyboard and menus

- **Native macOS menus:** Atlas, File, Edit, View, Go, Window and Help. The menu owns the keys of the commands it lists, shows your current binding and follows remaps.
- **Undo, redo and select all** (⌘Z, ⇧⌘Z, ⌘A) act on whatever has focus: the editor's own history, a text field, or the selection in a GPU terminal (where undo and redo do nothing and never reach the shell). These three are fixed.
- **⌘S saves** the focused editor (`atlas.yaml` shows the diff first); ⌥⌘S saves every open editor. Files still autosave 500 ms after you stop typing.
- **⌘W** closes a tab (asks when a process runs); ⌥⌘W closes other tabs; ⌘⇧W closes all in the pane. **⌘⇧T** reopens the last closed tab: editors, diffs, browser, logs, config and PRs return in place; terminals restart.
- **⌘1…⌘8** go to tab 1–8 and ⌘9 to the last tab; ⌘⇧] / ⌘⇧[ go to the next and previous tab.
- **⌃Tab / ⌃⇧Tab** switch by recent use: a switcher lists tabs while ⌃ is held, releasing ⌃ opens the highlighted one, ⎋ cancels.
- **Rebindable keymap and Settings → Keyboard shortcuts:** search a command, click its key, press the new one (a key already in use moves to the command you pick); ✕ unbinds, **Reset all** restores defaults. The same via `keymap` in `~/.atlas/desktop/settings.json`.
- **⌘/ or `?`** opens a cheat sheet generated from the live keymap, so it shows your own remaps.

### Settings

- **Reorganized** into General, Appearance, Workspaces, Agents, Keyboard shortcuts, SSH hosts and Help & onboarding. Appearance has Appearance, Editor, Interface and Terminal tabs.
- **Plain-language copy.** Settings say what they do ("Programs can copy to your clipboard", "Draws local terminals on the graphics chip…") instead of internal names.
- **Search.** The box at the top finds a setting by what it does ("font", "clipboard", "branch") and jumps to it.
- **General** holds Updates and the shell used for your projects.

### Updates

- **Update pill and notification.** When a newer release exists, a soft-glow **Update to X.Y.Z** pill appears in the sidebar footer, and you get one native notification per version (same switch as agent notifications).
- **What's new sheet.** Shows the release notes of every version between yours and the latest (the changelog section of each GitHub release), then **Update and relaunch** with live steps (_Download and verify → Install → Update the app_, raw output under _Details_), **Later**, or **Skip this version** (remembered until a newer one ships). The first launch after an update shows a one-time _Updated to X_ card with **See what's new**.
- **Settings → General → Updates:** current versions, channel, last check, **Check now**, an automatic-check switch (on launch, on returning to the app at most every 6 hours, and every 6 hours) and a link to the full changelog. Command palette: _Check for updates_ and _What's new_.
- **Dev line.** Dev builds (`atlas link`, `cargo tauri dev`) show a 2 px amber line across the top of the window, a **Dev** chip next to the version in Settings and in the menu-bar popover, and a tooltip: _Dev build X — updates paused (atlas unlink to resume)_. The big Dev build card in the sidebar is gone.

### Menu-bar popover

- Clicking the menu-bar icon opens a small translucent popover. It shows the overall state (_All stacks healthy_, _N services failed_, _N services starting_, _Nothing running_), one card per running project (icon, a dot per service, `up/total · CPU · memory`, a CPU sparkline, **Stop** and **Open**), stopped projects with **Start**, agents working or waiting for you, live terminals and the terminal-host version, and a **Recommended** line (a recent failure with **View logs**, an available update, or an outdated terminal host).
- It closes on ⎋, on a click elsewhere, or on a second click on the icon. Right-click the icon for **Open Atlas** and **Quit Atlas**. Everything it opens runs in the main window.

### Onboarding

- **First run.** With no projects, a three-beat welcome and **Add your first project**; **Or find projects on this machine** scans the usual folders; **Skip for now** closes it for good.
- **Tour.** After your first project, a guided tour spotlights the sidebar, spaces, breadcrumb, the project panel, a service card, the right-sidebar toggle and the jump palette (→/↩ next, ← back, ⎋ skip). Run it again from ⌘⇧P → _Take the tour_.
- **Getting started checklist.** A small card tracks six first steps (add a project, start the stack, open a terminal, run an agent, try the command palette, search a file) and ticks them as you do them, however you did them.
- **Cheat sheet** on ⌘/ (see above). **Settings → Help & onboarding** brings the tour and checklist back, or starts over.

### App icon

- A new macOS-style app icon and a template menu-bar icon, both redrawn in the cobalt identity.

### Open source

- **License:** MIT OR Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`), with `NOTICE` and `THIRD_PARTY_NOTICES.md`.
- **Contributing:** contributions are accepted under the Developer Certificate of Origin (`Signed-off-by`, no CLA); a DCO check runs on pull requests. `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, issue and pull request templates, and `AGENTS.md` for contributors and coding agents.
- **CI:** a public pull request workflow (macOS full workspace and Linux without the macOS-only crates) with a read-only token and no secrets, a CODEOWNERS file and weekly Dependabot for Cargo, npm and GitHub Actions. Rust is pinned to 1.99.
- **Governance and policy:** `GOVERNANCE.md`, `SECURITY.md`, `SUPPORT.md`, `ROADMAP.md` and `PRIVACY.md`. Privacy: no telemetry, analytics, crash reporting or accounts; the only network requests Atlas makes itself are update checks to the public GitHub API and release downloads, listed in full.
- **Docs and website:** docs at https://atlas.codeatlas.com.br/docs, rendered from `docs/` and this changelog, with a mobile layout for the website.
- **Studio:** `studio/` builds the site film and "What's new" release videos from typed data, using HyperFrames, with ElevenLabs narration and measured loudness (`bun studio`).
- **Cleanup:** internal infrastructure code (`atlas-infra`) was removed from the public tree, and heavy media is no longer in git (`scripts/fetch-media.sh`).

### Changed

- **Shortcuts moved** so common keys behave like other macOS apps. Any of them can be rebound.

  | Action | Now | Was |
  |---|---|---|
  | Save the focused editor | ⌘S | not bound |
  | Hide or pin the sidebar | ⌃⌘S | not bound |
  | New agent session | ⌘⇧A | ⌘⇧T |
  | Reopen the last closed tab | ⌘⇧T | not bound |
  | Tab 1–8 / last tab | ⌘1…⌘8 / ⌘9 | ⌘1…⌘6 opened recent worktrees |
  | Recent worktrees 1–6 | ⌃⌘1…⌃⌘6 | ⌘1…⌘6 |
  | Restart the focused service | ⌘⇧R | ⌥⌘R |
  | Close other tabs | ⌥⌘W (every kind of tab) | ⌥⌘W (editors only) |
  | Switch space by position | ⌃1…⌃9 | not bound |

- **⌘K clears the terminal** when a GPU terminal has the keyboard focus (Ghostty's `clear_screen`); everywhere else it is still the jump palette, also on ⌘⇧P and the Go menu. ⌘⇧D splits down in a focused terminal and opens Changes elsewhere.
- **OSC 52 defaults to `ask`:** a program that writes to your clipboard asks first (`terminal.osc52`: `ask`, `allow` or `deny`); reading the clipboard is always denied.
- **Update checks use the public GitHub API** (`api.github.com`, over `curl`) instead of requiring the `gh` CLI, so they work without a GitHub login. A token from `GITHUB_TOKEN`, `GH_TOKEN` or `gh auth token` is used when present, to avoid rate limits, and is sent to GitHub only.
- The Panel tab replaces the old Services tab in the UI; saved layouts keep working.
- The sidebar no longer shows the big Dev build card (see Updates).

### Fixed

- **Browser tab:** the native browser view no longer outlives its tab or sits at a stale position. Views are parked while a dialog or the palette is open, orphans are closed after a reload or HMR, open/resize/park calls run in order with the rect measured when they run, and a view cannot be un-parked after the tab is disposed (⌘W, ⌘⇧T).
- **Stale All-tab rows in the TUI:** when a chatty service evicted lines from its ring, other services' older rows could stay in the All view as dead rows; the view now drops exactly the evicted rows (and rebuilds if it fell too far behind).
- **Update checks without `gh`:** the update check, `atlas update` and the update prompt before `atlas dev` no longer fail when the GitHub CLI is missing or not logged in; the error messages now point to your network and to the releases page instead of `gh`.
- **Settings:** alerts and groups in the Settings page were squeezed vertically; panes now scroll with children at their content height.

## [1.0.1] — 2026-10-11

### Changed

- Dependencies updated to their latest versions: Rust crates (`clap` 4.6, `tempfile` 3.27, `flate2` 1.1, `insta` 1.49, `pollster` 1.0, `toml_edit`, `serde_json`, `syn`, `cc`) and the desktop frontend packages. `@codeatlasdev/ui` is already at its latest release (0.7.1). A few transitive crates (the `toml` 0.8 family, `psm`, `generic-array`, `brotli-decompressor`) stay where their parent crates pin them.
- TypeScript stays on the latest 6.x: TypeScript 7 (the native Go compiler) no longer ships the JavaScript API that SvelteKit's `sync` uses.

## [1.0.0] — 2026-10-11

Atlas 1.0 turns Atlas Desktop into a full development environment: a native GPU terminal, a Zed-style workspace around it, and a log viewer built for real traffic, in both the desktop app and the TUI. `atlas.yaml`, the CLI, the TUI and the MCP tools stay compatible with 0.6.

### Highlights

- **A GPU terminal built into Atlas Desktop.** Terminals are drawn natively with Metal (wgpu) instead of a web view: 120 Hz, under 1 ms of render work per frame, images, color emoji, ligatures and shell integration. On by default for local terminals.
- **A Zed-style workspace.** Left, right and bottom docks, pane zoom, a command palette with every action, go to file and go to symbol, rebindable shortcuts, Zed themes, file icons, and a settings page for fonts, terminal and panels.
- **Logs built for real traffic, in the TUI and the desktop.** Search like a terminal (match counter, next/previous, highlights), jump to the next error or warning, click a URL or `path:line` to open it, full color fidelity, and no freezes under bursts of hundreds of thousands of lines.

### GPU terminal

- **Rendering:** Metal through wgpu. Each terminal is a native view over its pane, with its own render thread that sleeps until there is output. Frames are vsync-paced at 120 Hz on ProMotion displays; render work is ~0.2–0.3 ms p95 for a 144×44 grid in an optimized build.
- **Text:** bundled JetBrains Mono with a Nerd Font symbols fallback, bold/italic, programming ligatures (`->`, `!=`, `=>`, `===`; toggle in settings), color emoji including ZWJ sequences, flags, skin tones and VS16, a minimum-contrast option (WCAG ratio), and pixel-exact box drawing, block elements, braille, sextants and powerline glyphs that tile without gaps.
- **Underline styles:** single, double, curly, dotted and dashed, in color (SGR 58), plus strikethrough.
- **Images:** the Kitty graphics protocol (PNG, RGB/RGBA, zlib, chunked transfers, files, placement ids, z-order above or below text, quiet mode) and sixel. Images scroll with the text and leave with the alternate screen.
- **Shell integration:** OSC 133 prompt marks (⌘↑ / ⌘↓ jump between prompts, exit codes), OSC 7 working directory (Restart reopens there), titles, bell (flash, Dock attention when unfocused).
- **Interaction:** native selection (character, word, line and ⌥ block), 10,000 lines of scrollback (configurable) with a scrollbar, ⌘F regex search with match counter and highlights, clickable URLs and OSC 8 links (⌘-click), copy and bracketed paste, dropping a file pastes its path.
- **Keyboard and modes:** the Kitty keyboard protocol, synchronized output (mode 2026), grapheme clusters (mode 2027), cursor shapes and blinking (hollow when unfocused).
- **Clipboard safety:** a program writing the clipboard with OSC 52 now asks first (`terminal.osc52`: ask, allow or deny). Reading the clipboard is always denied.
- **Live settings:** theme, font size, cursor, scrollback, ligatures and contrast apply to open terminals.
- **Backpressure:** the terminal host now slows a program down when the screen cannot keep up, like a native terminal, instead of buffering its output in memory. A client that stops reading for 2 s is dropped so it cannot freeze the program.
- Terminals of SSH-host worktrees keep using the classic (xterm.js) renderer, as does a terminal host started by an older Atlas until it restarts (Settings → Editor & terminal → Restart terminal host).

Benchmarks against Ghostty on the same machine are in [docs/terminal.md](docs/terminal.md#performance).

### Workspace

- **Docks:** ⌘B left (project or git panel), ⌘R right, ⌘J bottom (its own terminal tabs), ⌥⌘Y all docks, ⇧Esc zooms the focused pane, plus a centered layout. Docks are resizable by dragging and remember their size; header buttons show their state.
- **Palettes:** ⌘⇧P command palette (every command with its current shortcut, recently used first, matched characters highlighted), ⌘K jump to worktrees and terminals, ⌘P go to file with file icons, ⌘⇧O go to symbol in the open editor.
- **Shortcuts:** every shortcut is a named command with Zed's names and defaults. Rebind or unbind them in Settings → Keyboard shortcuts, or under `keymap` in `~/.atlas/desktop/settings.json`.
- **Themes:** Zed theme files (v0.2.0 format) color the whole app, the code editor and the terminals. Bundled: Atlas Dark/Light and One Dark/Light; add your own in `~/.atlas/desktop/themes/`. Separate light and dark choices follow the system appearance.
- **File icons:** the Material Icon Theme in the explorer, tabs and go to file.
- **Settings → Editor & terminal:** editor and interface fonts, terminal font size, cursor, scrollback, OSC 52 policy, ligatures and contrast, explorer side, icons and indentation, active-pane border and inactive-pane dimming (Zed setting names).
- **Polish:** welcome screens for a first run and for an empty worktree, loading skeletons, in-app confirmation dialogs instead of browser prompts, tooltips that show the current shortcut, tidier sidebar, explorer, agents and dashboard views.

### Logs

- **Robust ingestion (runner):** real lines are assembled from the raw pipe. Partial lines (prompts) show after 150 ms of silence, `\r\n` is one line break, a lone `\r` keeps only a progress bar's last state, invalid UTF-8 is replaced, and lines over 16 KiB are cut with `… [truncated]`. Level, timestamp and sequence number are computed once per line.
- **Restart separators:** a dim `──── HH:MM:SS restarted ────` rule separates a service's runs.
- **Bursts never cut the desktop off:** a client that falls behind skips log lines and gets a "N lines skipped" marker (the full output stays in `.logs/`), instead of being disconnected. Output reaches the app in ~16 ms batches.
- **TUI:** `/` find with `n/N` counter and highlights across the whole buffer, `]e`/`[e` and `]w`/`[w` jump between errors and warnings, `}`/`{` move between URLs and `path:line[:col]` links and `o` opens them (browser or your GUI editor at the line; copied otherwise), `y` copies a link. Rendering is O(visible rows) with a cached wrap index.
- **Desktop:** ⌘F find with regex/case toggles, `n/N` counter, Enter/⇧Enter and highlights; filter mode hides non-matching lines; `e`/`w` jump to the next error/warning; ⌘-click opens URLs in the Atlas browser and `path:line` in an editor tab at that line; error counts on service chips; timestamps and wrap toggles; a "N new lines" pill while scrolled up; Copy for AI of the filtered view. The view keeps 50k lines, filters incrementally (a filter change re-scans in idle slices) and parses ANSI only for visible rows. Colors follow the theme's terminal palette, including 256 colors, truecolor, underline styles and OSC 8 links.
- **MCP:** `atlas_logs_search` uses the same matcher as the TUI and stays case-insensitive.

### Changed

- **Shortcuts follow Zed:** the jump palette is ⌘K only (⌘J opens the bottom dock), restart the focused service is ⌥⌘R (⌘R toggles the right dock), the right sidebar is ⌘R (was ⌘⌥B), the git panel is ⌃⇧G. Any of them can be rebound.
- The GPU terminal is on by default; turn it off in Settings → Editor & terminal.
- Release notes now come from this changelog.

### Fixed

- The desktop Logs tab could stay empty when the service filter from another project did not exist in the current one.

- First-run project discovery could throw when the registry had already loaded.
- The sidebar showed ⌘J for the jump palette.
