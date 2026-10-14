---
title: "Desktop app"
description: "Atlas Desktop: spaces, worktrees, agents, review, the embedded browser, SSH hosts and shortcuts."
---

# Desktop app

Atlas Desktop is a native app (Tauri 2 + Svelte 5), available for macOS today with Windows and Linux coming soon. It lists every atlas project in a sidebar and lets you start and stop each stack. It shows services live, streams and searches logs, runs actions, and edits `atlas.yaml` with validation.

It is a client of the same runners as the CLI: **Start** runs `atlas dev --detach`, and **Stop** is `atlas down`. The app, an attached TUI and an AI agent (through [MCP](mcp.md)) all see and control the same live state. Quitting or closing the app never stops a stack.

Contents: [Getting around](#getting-around) · [Install](#install) · [Updates](#updates) · [Architecture](#architecture) · [Projects and worktrees](#projects-and-worktrees) · [Tabs and panes](#tabs-and-panes) · [Agents](#agents) · [SSH hosts](#ssh-hosts) · [Keyboard](#keyboard) · [Automation](#automation) · [Developing the app](#developing-the-app)

## Getting around

**Spaces.** Every project is a space: it keeps its own worktrees, tabs, panes and docks, and has its own **icon and color**. The color is derived from the project unless you pick one, and tints the selection, the header and focus states, so you always know which project you are in. Pick a space in the sidebar, or move with ⌥⌘← / ⌥⌘→ and ⌃1…⌃9.

**Project icons.** Colors alone get confusing, so each project also shows an icon: your choice, else the repo's own logo, else a monogram (`my-app` → **MA**) on its color. Right-click a project (in the list or the bottom strip), or `⋯` → **Change icon and color…**: _Emoji_ (type or paste any emoji, or pick one), _Icons_ (search the ~1,900 Lucide icons), _Project logo_ (shown when Atlas found one), plus color swatches (the first one, dashed, is the automatic color). **Reset** goes back to automatic. Atlas looks for the logo in `package.json` `icon`, then `public/favicon.svg|png|ico`, `public/icon.*`, `public/logo.*`, `public/apple-touch-icon.png`, `static/favicon.*`, `app/icon.*` (and `src/app/`), `assets/logo.*`, `.github/logo.*`, and `logo.*` / `icon.*` / `favicon.ico` at the root; images over 256 KB are skipped. It is read through the host, so SSH projects get their logo too. The choice is saved per project in `~/.atlas/desktop/projects.json` (`identity`). The icon appears in the sidebar and its bottom strip, the header breadcrumb, the project panel, the jump palette, the Files header and the tray popover.

**Sidebars.** The left sidebar shows the **active project** (icon, name, live status dot that breathes while its stack runs, `up/total` services) and its worktrees; a project with a single checkout is just its own row (clicking it opens `main`, no redundant child). **Other projects** is collapsed by default (pinned first); each shows its icon, status and service count, with a chevron to expand worktrees only when it has more than one. Filtering opens the list and shows every match. At the bottom, a strip of project icons switches space (active one ringed, tooltip with the name and ⌃N). Hide the sidebar with ⌃⌘S. The right sidebar (⌘R) holds agent history and listening ports; hide it for a calmer view. Hidden sidebars can be peeked without losing your layout. A project's panel (stack health, services and their live CPU and memory) is the first thing a space shows.

**Peeking a hidden sidebar.** Move the pointer to the window edge (left for spaces, right for the context sidebar) and it slides in over the content: after ~60 ms, or at once if you flick into the edge. It stays while the pointer is on it (with a 24 px margin), slides out 250 ms after you leave, and ⎋ dismisses it. A menu or dialog opened from it keeps it open.

**Trackpad gestures.** Two-finger swipes work like Arc and never take over scrolling: a gesture only starts when it is clearly horizontal (15 pt of travel, at least twice the vertical) over a swipeable area, and a vertical scroll, a resting finger's jitter or a wheel mouse never starts one.

| Swipe…       | Over                               | Does                                                                                                                                                                                              |
| ------------ | ---------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| left / right | the sidebar                        | next / previous space. The content follows your fingers 1:1 and the next space slides in beside it; past half a page, or with a quick flick, it commits, otherwise it springs back                |
| left / right | the tab strip or the tab's content | next / previous tab, the same way                                                                                                                                                                 |
| right / left | the content, where nothing scrolls | Back / Forward through visited worktrees and tabs (⌘[ / ⌘]), with an arrow chip that fills as you drag                                                                                            |

The release is decided the instant your fingers lift (not after a quiet gap), the page settles in about 0.3 s with no overshoot, and the momentum tail after it is swallowed, so one swipe is one switch, never two. Swiping at the first or last space only stretches like a rubber band. A new swipe during a settle continues from where the page is. With _Reduce motion_ on, nothing animates and the switch is instant. Pinch (ctrl + wheel) is never treated as a swipe.

**Search and jump.**

| Want to…                              | Press     |
| ------------------------------------- | --------- |
| Go to a worktree, terminal or command | ⌘K        |
| Open a file in this worktree          | ⌘P        |
| Run any command (and see its key)     | ⌘⇧P       |
| Open a symbol in the current editor   | ⌘⇧O       |
| See every shortcut                    | ⌘/ or `?` |

**First run.** With no projects, Atlas shows a welcome in three short beats (projects become spaces, terminals and agents per worktree, one key to go anywhere) and one action: **Add your first project**. **Or find projects on this machine** scans the usual folders. **Skip for now** closes it; it does not come back once you add a project or skip.

**The tour.** After your first project, a guided tour spotlights the sidebar, spaces, header breadcrumb, the project panel, a service card, the right-sidebar toggle and the jump palette. →/↩ next, ← back, ⎋ skip. Steps whose part of the screen is not visible are skipped. Run it again from ⌘⇧P → _Take the tour_ or Settings → _Help & onboarding_.

**Getting started checklist.** A small card in the bottom-right corner tracks six first steps: add a project, start the stack, open a terminal (⌘T), run an agent (⌘⇧A), try the command palette (⌘⇧P), search a file (⌘P). Items tick themselves when you do the thing, whichever way you did it. Collapse it with its header, dismiss it with ✕ (it also leaves by itself when done). _Show getting started checklist_ in ⌘⇧P, or Settings → _Help & onboarding_, brings it back; _Start over_ resets the tour and the checklist.

**Help.** ⌘/ (or `?` outside a text field) opens a searchable cheat sheet generated from the live keymap, so it shows your own remaps. State is saved under `onboarding` in `~/.atlas/desktop/settings.json`.

## Install

```sh
atlas desktop
```

The first run downloads the `Atlas.app` that matches your CLI version from the public GitHub release (with `curl`, checksum verified), installs it into `/Applications`, and opens it. Later runs just open it. You can also download `Atlas-<version>-aarch64-apple-darwin.dmg` from the release by hand.

Requirements:

- macOS 13+ on Apple silicon, or Linux x86_64 (glibc 2.35+, WebKitGTK 4.1). On Linux terminals use xterm.js rather than the native GPU terminal, the window uses your desktop's own title bar, and capture/recording and agent test instances are macOS only.
- The atlas CLI. The app looks for it at `$ATLAS_BIN`, then `~/.atlas/bin/atlas`, then `atlas` on `PATH`.
- `curl` (preinstalled on macOS). No GitHub login is needed: releases are public.
- Optional: `gh`, logged in (`gh auth login`), for the pull-request tab and checks.

### Shell environment

Opened from Finder or the Dock on macOS, or from a menu entry on Linux, the app gets the session's bare environment (`PATH=/usr/bin:/bin:…`), so `bun` (`~/.bun/bin`), `node` and anything from Homebrew, nvm, mise or asdf would be missing (`exit 127: sh: 1: bun: not found`). At launch Atlas reads the environment of your shell in the background and starts every service, `pre_command`, action, terminal and agent, plus `git`, `gh` and updates, with it, exactly as your terminal would. Commands themselves still run through `sh -c`. The runner (`atlas dev`) and the terminal host (`atlas hostd`) do the same when started from a terminal or by the app, so all of them see one `PATH`.

- Which shell: the one chosen in **Settings → General → Shell for your projects**, else `$SHELL`, else your account's login shell from the user database (`chsh`), else `/bin/zsh` (`/bin/sh` where zsh is missing). Any shell works (zsh, bash, fish, nushell, ksh, dash, tcsh…): Atlas runs it as an interactive login shell and only asks it to run `/usr/bin/env -0`, trying `-l -i -c`, then `-l -c`, then `-c`; anything your rc files print around the result is ignored.
- Settings lists shells from `/etc/shells`, accepts any path, shows which flags worked, how long it took, the resulting `PATH`, and whether `bun`, `node`, `docker`, `gh`… are found. **Reload** re-reads it after you edit your rc files.
- A shell that fails or takes over 6 s is reported there, and Atlas falls back to the inherited `PATH` with the usual user bin directories that exist prepended (`~/.bun/bin`, `~/.cargo/bin`, `~/.local/bin`, mise and asdf shims, the newest nvm node).
- When a `pre_command` (the stack setup) exits 127, the setup panel and `.logs/setup.log` name the missing binary, the `PATH` Atlas used and what to do: put the directory on `PATH` in your shell's rc file (`~/.bashrc`, `~/.zshrc`, `~/.config/fish/config.fish`) or pick the right shell in Settings, then restart.
- Projects already running keep the environment they started with: stop and start them after a change.
- The choice is saved in `~/.atlas/desktop/settings.json` (`{"shell": "/opt/homebrew/bin/fish"}`).

## Updates

The CLI and the app are released together with the same version, and they update together:

- **From the terminal:** `atlas update` updates the CLI, then brings an installed `Atlas.app` to the same version.
- **From the app:** the app checks on launch, when you return to it (at most every 6 hours) and every 6 hours while open; turn this off in Settings → General → Updates → _Check automatically_. When a newer release exists a calm **Update to X.Y.Z** pill with a soft glow appears in the sidebar footer, and you get one native notification per version (through the same switch as agent notifications, Settings → Agents). Clicking the pill, or ⌘K → _What's new_, opens the **What's new** sheet.
- **What's new sheet:** the release notes (each version's `CHANGELOG.md` section, read from the public repository without the GitHub API) of every version between the one you run and the latest. Each version has a header (version, date, an _Alpha_/_Beta_/_RC_ badge for a pre-release, *Latest*/*Installed*/*New*) and the footer names the channel of the release shown and the one you follow. The sheet hides a pre-release section once its stable release is in range (the stable section is the complete summary), the changelog's one-line summary and its changes in cards by kind, each change a bold title plus its text (long ones fold behind *More*). With several versions a sticky index follows the scroll, versions fold, and `j`/`k` move between them. **Update to X and relaunch** (with the download size) runs `atlas update` and shows _Download → Verify → Install CLI → Update app → Terminal host → Relaunch_ with a progress bar for the download (the raw output is under _Details_), then **Relaunch Atlas**; a failure shows the reason inline with **Try again**. **Remind me later** only closes the sheet (the pill stays); **Skip this version** hides the pill and the notification until a newer version ships (stored under `updates` in `settings.json`, with `notified` and `lastSeen`), and Settings → General → Updates offers *Stop skipping*.
- **Update channel:** Settings → General → Updates → **Update channel** has three cards, _Stable_, _Beta_ and _Alpha_ (the default while Atlas is in alpha: a profile without `~/.atlas/update-channel` follows it) (see [Versioning and release channels](versioning.md#channels)). Each shows its promise, a risk badge, how often it ships, who it is for and what it offers **right now** ("Offers 1.4.0-beta.2", "Nothing newer than what you run", "Offers 1.3.0, older than yours"); _Following_ marks the one you use and _Installed_ the one the running build was cut for. While Atlas is in alpha a note above the cards says that every channel, Stable included, can still have rough edges. Picking a card opens an inline confirmation with what the switch means; only **Follow …** saves it. The choice is the one word in `~/.atlas/update-channel` that the CLI shares (`atlas update channel`), never a second copy in `settings.json`; if `ATLAS_UPDATE_CHANNEL` is set in the app's environment the cards are disabled and say so. Offline, the cards still render from their copy and **Retry** looks again. Switching never installs anything older: when the chosen channel only offers a build older than yours the panel adds a separate **Install X now (downgrade)** button (it runs `atlas update --downgrade --yes`, and warns that settings or data written by the newer build may not be readable), and automatic checks, the pill and the notification only ever react to a strictly newer release.
- **After an update:** the first launch of the new version shows a one-time _Updated to X_ card with **See what's new**, opening the same sheet for that version.
- **Commands:** ⌘K → _Check for updates_ (checks now and says what it found, or opens the sheet) and _What's new_. Settings → General → Updates shows both versions, when it last checked, **Check now**, the **Update channel** cards, the auto-check switch and a link to the full changelog.
- **Repair:** **Reinstall app** in the sheet, or `atlas desktop --install --latest`.
- **Terminal host:** `atlas update` ends with a _Terminal host_ step (`@@atlas phase terminal-host`): the running `atlas hostd` is compared with the new version and handed over live (no terminal is lost), restarted silently when no terminal runs, or, when terminals run and features such as language servers would stay off, you are asked first (`Restart now and end N terminals? [y/N]`). It never ends a terminal without your yes; if you decline or run without a TTY, it prints what is stale and the command to fix it. See [`atlas hostd`](#keeping-the-terminal-host-current).

### Keeping the terminal host current

The terminal host outlives the app and `atlas update`, so it can be older than the CLI. Three commands report and fix that (all take `--dir`, default `~/.atlas` or `$ATLAS_HOSTD_DIR`):

- `atlas hostd --check`: one human report (`Terminal host 0.6.2 (pid 812, up 40h, 2 terminals) is older than Atlas 1.2.1; language features … off until it restarts.`) with the running terminals (folder and command) and the exact fix commands. Exit `0` current, `3` stale, `1` not running. `--status` still prints the raw `info` JSON.
- `atlas hostd --reconcile [--choice ask|idle|later]`: acts on it. A host that can hand over (`upgrade` capability) is replaced live and verified (new version, no terminal lost); one that cannot is restarted when no terminal runs; otherwise it asks (`ask`), waits (`idle`) or leaves it (`later`). Exit `0` also when it is not running.
- `atlas hostd --restart`: explicit consent: stop it (ending its terminals), start a fresh one from this binary and verify it.

An idle host does not linger forever: with **no running terminal and no connected client for 10 minutes** (`ATLAS_HOSTD_IDLE_SECS` overrides, in seconds) it exits and removes its socket and pid file, and it also exits if its own directory disappears. A running terminal always keeps it alive. The app starts a host when it needs one, and remote hosts are started again by the app on every connect, so an exited one comes back current.

### How an update stays safe

`atlas update` (the app runs the same command) verifies the SHA-256 of every download (the CLI tarball and the app tarball) and refuses an asset without a checksum. It takes `~/.atlas/.update.lock` (`flock`, released by the kernel if the process dies), so a second update from another terminal or window stops with a clear message. The CLI is replaced in place after a backup of the previous one (`~/.atlas/bin/.atlas.prev`); if the app then cannot be brought to the same version the CLI is restored and the error says so, so the two never sit on different versions in silence. With `ATLAS_UPDATE_PROGRESS=1` the CLI prints `@@atlas phase|bytes|rollback|host …` lines next to its normal output; the app turns them into the step list and the progress bar, and an older CLI without them still advances the steps from its plain text.

## Architecture

Terminals live in **`atlas hostd`**, a per-user daemon (`~/.atlas/hostd.sock`, token `~/.atlas/hostd.token`) that the app starts on launch. It owns PTYs, one-shot `exec`, file access and scrollback, so terminals and agents keep running when the app quits, crashes or updates; reopening restores the tabs with their scrollback. The same daemon runs on SSH hosts.

### Reporting a problem

When the terminal host cannot be reached, one toast (with a count if it repeats) offers **Restart terminal host** and **Report**. Restarting ends the terminals it hosts (their tabs stay; use Restart on each) and works even when the host does not answer: the app ends the daemon named in `hostd.pid`, only if that process is an `atlas hostd`, and starts a clean one. **Report** opens the error screen with a diagnostics block built by the backend (`host_diagnostics`): versions of the app, the installed CLI and the running host, the OS, the last 40 lines of `hostd.log` and of `~/.atlas/desktop/app.log` (Rust panics, host incidents), and the newest crash report (`atlas_core::crash`, message and location only). **Copy details** and **Copy as Markdown** carry it; **Report issue** opens the bug form with its fields prefilled. Secrets (tokens, passwords, API keys, URL credentials) and home directories are redacted in the backend and again before anything is copied or sent. For the host's own state from a shell, `atlas hostd --check` is the doctor.

**Updating the host without losing terminals.** The daemon outlives app updates, so after one it is older than the app. On connect the app compares the daemon's `info` (`version`, `proto`, `caps`) with its own version and acts:

- the daemon supports `upgrade` (protocol 2+): **live takeover**. The new `atlas hostd --takeover` receives every pty master fd (`SCM_RIGHTS` over a unix socket) plus the session state and the pid-file lock, binds the socket, confirms, and the old process exits. Programs keep running (they never notice); scrollback keeps appending to the same files; the app reconnects and reattaches, GPU terminals open their raw streams, and a _Terminal host updated_ toast appears. If the successor fails before confirming, the old daemon takes its ptys back and keeps serving. `atlas hostd --upgrade` does the same from a shell with the binary you run.
- an older (protocol 1) daemon cannot hand over. With no running terminal the app restarts it silently; with running terminals and features missing the app asks once per session in a _Restart the terminal host?_ dialog listing them: **Restart now**, **Restart when idle** (also when you quit with nothing running) or **Later**. The _Terminal host needs a restart_ chip in the sidebar footer and the _Terminal host_ row in Settings → Updates stay until it is current (terminals use xterm.js meanwhile). It also updates by itself the next time the app starts with nothing running. Handing over from protocol 1 to 2 is the one update that needs this restart; later ones are live.
- pipe-mode sessions (`omp --mode rpc-ui`) have no pty master to pass and end with the old daemon.
- exit codes of programs adopted by a takeover come from kqueue (`NOTE_EXITSTATUS`) on macOS, registered at adoption; where that is unavailable the exit is still detected but its code is reported as unknown.

### Startup

The main window is created from `tauri.conf.json` (`create: false`) in `setup`, hidden, before anything else so WebKit's web process spawns while the rest of the shell initializes. `boot.rs` owns the rest:

- **Pre-paint theme.** The theme engine (`lib/stores/theme.svelte.ts`) stays the single source of truth; whenever it applies a theme it saves the resulting CSS variables, scheme and name through `theme_cache_write` to `~/.atlas/desktop/theme-cache.json`. At launch Rust reads that file before the webview exists, injects it as an initialization script (variables, `.dark`, `color-scheme`, the kit's `ca-scheme` in `localStorage`) and sets the native window appearance, so the first style computation already has your theme. Without a cache (first run, or the file was deleted) the previous defaults apply for that one launch.
- **Show gate.** `app_ready` (sent by `lib/shell/ready.svelte.ts` once the real theme, the project list and the workspace are in) shows the window; a 1 s deadline shows it anyway so a broken frontend never leaves it hidden. window-state restores size, position and maximized while it is hidden (it no longer shows the window itself).
- **Theme files** come from `themes_user_read` (plain disk reads of `~/.atlas/themes` and the legacy dir), not through hostd, so the theme never waits for the terminal host.
- **Bundle.** Heavy views (editor, diff, chat, browser tab, PR, panel, logs, config, terminal pane) load through `components/Lazy.svelte`; Settings, What's new and the new-worktree / PR dialogs through `LazyOpen.svelte` on first open. The shell reaches CodeMirror only through the `ops` slots in `lib/symbols.ts`, installed by `lib/outline.ts` when the first editor loads; do not import `@codemirror/*`, `yaml`, `marked`, `prettier` or `echarts` from startup modules (`routes/(app)/+layout.svelte`, `lib/shell/*`, `lib/stores/*`).
- **Measuring.** `ATLAS_BOOT_TRACE=/tmp/boot.txt <app binary>` appends `<ms since process start> <mark>` lines (Rust marks plus `js:` marks from `lib/boot.ts`); unset, every mark is a no-op.

## Projects and worktrees

Projects (main checkouts) are stored in `~/.atlas/desktop/projects.json`. Add them with **Add project…**, the search button (scans `~/Projetos`, `~/Projects`, `~/code`, `~/dev`), or the first-run scan.

Each project groups its **git worktrees** in the sidebar. **New worktree** (⌘N, or `+` on the project) asks for a name (blank = random), an agent and an optional first prompt; a GitHub PR or issue URL works as the name. Advanced: branch name and base ref. Atlas then:

1. creates `~/.atlas/worktrees/<repo>/<name>` on branch `<git user>/<name>` from `origin/<default>` (`git worktree add`), records the base (`branch.<b>.base`) and sets `push.autoSetupRemote`;
2. applies `worktree.shared` and runs `worktree.setup` from `atlas.yaml` in a "Setup" terminal ([configuration](configuration.md#worktree));
3. gives the worktree a **port offset** (100, 200, …) so its stack runs next to the main checkout's (`ATLAS_PORT_OFFSET`).

Deleting a worktree stops its stack, runs `worktree.archive`, removes the checkout and deletes the branch unless it is unmerged (then it is kept, and a toast says so). Settings → Workspaces sets the worktree folder and branch name prefix, and **Clean up** lists worktrees with size, last commit and git state. Worktrees created outside Atlas are imported.

Sidebar: glyph = agent state (spinner working, `?` needs you, ✓ done, • idle), **bold** = unread, chips for PR and port offset. ⌘-click selects several; ⌘⇧⌫ deletes the selection or the row under the pointer. **Agents** is a feed of agent events, **Dashboard** a board of agents by state.

## Tabs and panes

Each worktree has its own tab groups and splits (⌘\\ right, ⌘⇧\\ down; drag a tab to a pane edge to split). Tab kinds: terminal, agent, editor, changes (diff), browser, pull request, panel, logs, config. Layout is saved in `~/.atlas/desktop/workspace.json`.

| Tab                   | What it does                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| --------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Terminal              | xterm.js on a hostd PTY. ⌘F search; links open in the Atlas browser (⌘-click: system browser); dropping a file pastes its path. Local terminals are drawn natively on the GPU by default ([GPU terminal](terminal.md)); Settings → Appearance → Terminal → **Fast terminal (GPU)** switches back to xterm.js. SSH-host terminals always use xterm.js.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| Agent                 | an agent CLI in a terminal (omp, Claude Code, Codex). Its state comes from hooks (see below). **New omp chat** opens omp's native chat instead (`omp --mode rpc-ui`): streaming transcript, tool cards, `ask`/confirm cards, `/` commands, `@` files, image attachments, thinking level, Stop (Esc), Compact.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| Editor / viewers      | CodeMirror for text (autosave 500 ms after you stop typing; ⌥Z wrap). Other formats open in a viewer by extension: **images** (png, jpg, gif, webp, avif, bmp, ico, svg) on a checkerboard, fit to view, ⌘+ / ⌘− / ⌘0 or pinch / ⌘-wheel to zoom, drag to pan, double-click toggles 100%, pixel-crisp past 200%, info bar with size and format, copy image; **SVG** also has an _Edit source_ toggle; **PDF** rendered with pdf.js: continuous pages, pinch and ⌘+/⌘− zoom, fit width/page, page thumbnails and outline, ⌘F search; **video / audio** (mp4, mov, webm, mp3, wav, ogg, m4a) in native players; **Markdown** preview ⌘⇧M (syntax-colored code, tables, task lists, images relative to the file). Media loads in 8 MB chunks up to 200 MB; text opens up to 2 MB. Larger text and binary files show their size, **Open externally** / **Reveal in Finder**, and a hex dump of the first 4 KB. |
| Changes               | diff vs the branch base, unstaged or staged; stage/unstage per file or hunk; comment a line and **Send to agent**.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| Browser               | an embedded webview over the pane. **Design Mode** picks an element and sends its selector, HTML, computed styles and a screenshot to an agent (secrets in URLs and attributes are redacted).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| Pull request          | `gh pr view`: timeline, comments, checks (failed log, **Fix broken checks** sends it to the agent), merge, ready.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| Panel / Logs / Config | the stack views: the [project panel](#project-panel) (health, live charts, service cards, setup), [logs](#logs), the `atlas.yaml` editor with live validation.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |

### Project panel

Every worktree opens on its **Panel** (the old Services tab; the tab kind is still `services`, so saved layouts keep working). The window header carries the project's health pill (**All healthy**, **N starting**, **N failed**, **Stopped**) and, at the right, in this order: notifications bell, the dock toggles, **Activity**, **Actions ▾**, the `…` menu and **Start** / **Stop** (with the profile menu when the project has profiles). The Panel itself shows the project name in the space's accent color, uptime, mode and version, branch, the service ports and the root path.

- **Setup strip** (top of the Panel, only when the project has a dependency step: `pre_command` or an auto-detected installer; hidden otherwise): `Setup  bun install ⓘ · ran 1h ago · ok` with **Run again** and **View output**. While it runs the strip turns amber and tails the live output; if it fails it turns red, shows the reason and a **Retry** button with the last lines of output (**Full output** shows all 200). The ⓘ tooltip says when it runs: `always` = before every start, `lockfile` = when the lockfile changes (or `node_modules` is missing).
- **Actions** (header **⚡ Actions ▾**): every `actions:` entry of `atlas.yaml`, grouped like the TUI help (ungrouped first), with its bare key. Destructive ones (those with `confirm:`) are marked with a warning icon and ask first; disabled ones say why in their tooltip (stack stopped, already running, interactive session). While an action runs the button shows a spinner and its label, with progress in the operation strip; when it finishes a small chip (or a toast, for a project you are not looking at) reports the result. The same actions stay in the command palette (⌘⇧P) and on their bare keys.
- **Action output** (click the running indicator on the Actions button, **View output** at the bottom of the Actions menu or on the result chip, or ⌘⇧P → _Show action output_, unbound by default): a drawer on the right that keeps the output of the last 10 action runs of the project (up to 5000 lines each). The list shows newest first with a status dot (running / finished / failed), start time and duration; the selected run shows its output with ANSI colors, follows the end while it runs (scroll up to stop following), and offers **Copy** (plain text) and **Run again**. `path:line[:col]` references open in the editor and URLs in the preview browser (⇧ for the system browser). Opening the app mid-run shows what the runner kept: the last 500 lines of each recent run, with a note when earlier lines were dropped (the full output is always in `.logs/action-<slug>.log`); a runner older than this feature simply has no replay.
- **Activity** (header button with an unread count, or ⌘⇧P → _Show activity_): a drawer on the right with the stack's events grouped by time (_Just now_, _5 min ago_, _Today 19:00_…). Bursts collapse into one row (`server restarted 3×` instead of a stopped / starting / running row each), dots are colored by state, and **All / Errors / Restarts / Actions** filter the list. Click a row to open that service's logs at that moment.

- **KPI row**: services running of total, restarts in the last hour, error lines in the last 5 minutes, memory and CPU of the whole stack. Tiles turn amber or red when something needs a look.
- **Service cards**, failed and starting first. Each card has the state ring (a soft halo pulses while it runs), the port (click to open or copy the URL), CPU and memory sparklines of the last ~2 minutes, the last log line, a restart badge, `depends_on` chips and, on hover or focus, **Restart**, **Stop** / **Start**, **Logs** and **Terminal** (opened in the service's `dir`). A failed card shows its reason and **View logs**, which opens Logs filtered to that service.
- **Resources**: stacked-area charts for memory and CPU across the heaviest services, and an error-rate bar chart over the last 5 minutes. **Health over time** is a per-service timeline of running / starting / failed bands for the last 10 minutes; click a lane for its logs.
- **Keyboard**: arrows, Home and End move across cards, ↩ opens the service's logs, **R** restarts a running service after a confirmation. Cards scale from one column in narrow panes to as many as fit, and 20+ services scroll.
- **Empty states**: with the stack stopped, a card says what **Start** will launch; a project without services links to `atlas.yaml`.

History is sampled in the app (one sample per service every 3 s, 10 minutes kept; log throughput and errors come from the logs ring by reading only the lines appended since the last tick), so charts fill from the moment the app sees the stack and cost a handful of numbers per service. Charts follow the theme through the kit's chart tokens and refresh at most once a second.

### Logs

The Logs tab keeps the last 50k lines per project and stays at frame rate under bursts: the app side batches service output (one message per ~16 ms, at most 2000 lines, lines over 16 KiB are cut), the view is filtered incrementally (only new lines are tested; a filter change re-scans in idle slices, so typing never blocks) and only the visible rows are parsed and rendered. Service chips show an error count; `restarted` rules separate runs.

- **Find** (⌘F, or `/` with the log focused): literal or regex (`.*`), match case (`Aa`), `n/N` counter, ↩ / ⇧↩ (or ⌘G / ⌘⇧G) next / previous with wrap; every match is highlighted, the current one stronger. **Filter** toggles hiding lines that do not match. ⎋ clears.
- **Time range** (clock chip in the toolbar, next to Find): **Last 1 / 5 / 15 / 60 min**, **Since restart** (of the selected service; of the whole stack on All, using the `restarted` rules), **Since stack start** (the oldest line held) or a **custom** clock range of today: `14:05` (from) or `14:05–14:20` (`-`, `–`, `—` or `..` separate; both ends inclusive; an invalid time is explained under the field). An active range shows its label on the chip; ✕ clears it. It composes with the service chips, the level filter and Find/Filter, and the line counts, the error/warning totals and the **N new lines** pill all follow it. `Last N` windows slide: once a second the lines that aged out are dropped from the front of the view without rescanning the ring, so an idle window empties by itself. Changing the range re-filters once, in idle slices like any other filter change. Opening logs at a moment from the Activity drawer clears a range that would hide that moment (with a toast).
- **Levels**: `all levels / warn+ / error+`; buttons and `e` / `w` (⇧ or `E` / `W` for previous) jump to the next error / warning.
- **Links**: ⌘-click a `http(s)` URL or an OSC 8 hyperlink to open it in the embedded browser (⇧⌘-click: system browser); ⌘-click `path:line[:col]` to open the file at that line in an editor tab. Relative paths resolve against the service's `dir`, then the worktree; paths outside it are refused.
- **Colors** follow the active theme's terminal palette (16/256/truecolor, bold, dim, italic, underline styles and color, strike, inverse).
- Keys with the log focused: `j`/`k`/arrows, Space/PageUp/PageDown, `g`/`G` top/bottom, `f` follow, `t` timestamps. Wrap and timestamps have toolbar toggles; scrolling up pauses follow and shows a **N new lines ↓** pill.
- **Copy** copies the filtered view (selection copy gives plain text only); **Copy for AI** (✨) copies the last 300 filtered lines with a header describing the filter.

Docks: three toggle buttons in the worktree header (left, bottom, right) show which docks are open and their shortcut on hover. Drag a dock's edge to resize it (arrow keys nudge, double-click resets); sizes are remembered. Left dock: Explorer (⌘⇧E, git-colored, highlights the open file, live; arrows/←/→/Home/End/↩ navigate, F2 or _Rename_ and _New file_ edit in place, the locate button reveals the active file), Search (⌘⇧F: project-wide, below) or Source control (⌃⇧G: stage, commit, generate message with AI, push/pull, conflicts, create PR); it sits on the side chosen by `project_panel.dock`. Right dock (⌘R): agent session history with search and Resume, listening ports. Bottom dock (⌘J): a terminal tab group.

**Search in project (⌘⇧F).** Type to search every file in the worktree (debounced; a newer query cancels the older one). Toggles: match case, whole word, regular expression; _Files to include or skip_ takes globs (`src/**, *.ts`; a bare folder name matches everything under it). Results are grouped by file with the match highlighted; ↑ ↓ move, ← → fold a file, ↩ opens the file at the line and column. It runs `rg` through hostd (so it works on SSH hosts too, honoring `.gitignore`) and falls back to `git grep` when `rg` is not installed. At most 2000 matches in 200 files are shown; the status line says when results were cut.

A worktree with no open tabs shows a welcome state with **New terminal**, **Start an agent** and **Open a file**. Destructive questions and name prompts (delete, discard, force push, rename, new file, closing a busy terminal) use in-app dialogs, not browser ones.

## Agents

Agents run in worktree terminals with `ATLAS_HOOK_ENDPOINT` set. omp reports through the `atlas-status` extension that `atlas setup omp --apply` installs; Claude Code and Codex through `atlas hook` (`atlas setup claude-code|codex --apply`). Events go to a loopback server in the app (`~/.atlas/desktop/hook-endpoint.env`, new token per run). When a turn ends or the agent needs you while you look elsewhere, Atlas marks the worktree unread, adds it to the bell inbox, shows a notification and updates the Dock badge. After an app restart, omp's state is recovered from its terminal title. Settings → Agents: default agent, permissions (manual / auto-approve), extra args, notifications, quick commands, where links open.

## SSH hosts

Settings → SSH hosts adds a host from `~/.ssh/config`. Atlas uploads `atlas` if `~/.atlas/remote/atlas-<arch>-<os>` exists and the host lacks it, starts `atlas hostd` there and keeps an `ssh` tunnel (`-L` to its socket, `-R` for agent status). Add a remote project by path; its worktrees, terminals, editor, diff and agents then run on the host. Ports tab: **Forward** opens a remote port locally. Not yet for remote checkouts: the Services/Logs/Config views and Design Mode.

## Keyboard

Defaults follow macOS, Zed and browser conventions. Every shortcut is a named command: ⌘⇧P lists them with their keys (the last five you ran come first), and tooltips on buttons show the current key. Change them in **Settings → Keyboard shortcuts**: search a command, click its key, press the new one (⎋ cancels; ⌘, ⌃ or ⌥ is required). A key already used moves to the command you pick. ✕ unbinds, **Reset all** restores the defaults. The same thing by hand in `~/.atlas/desktop/settings.json`, Zed syntax, `null` unbinds:

```json
{ "keymap": { "cmd-j": null, "cmd-shift-j": "workspace::ToggleBottomDock" } }
```

| Keys                      | Action                                                                                                                       |
| ------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| ⌘⇧P                       | command palette                                                                                                              |
| ⌘K                        | jump palette: recent terminals and worktrees, commands, "Create worktree <query>"; Shift-Enter opens beside. In a focused terminal ⌘K clears the screen and scrollback instead (Ghostty's `clear_screen`); the palette is still on ⌘⇧P and the Go menu |
| ⌘P / ⌘⇧O                  | go to file / go to symbol in the open editor                                                                                 |
| ⌘⇧F                       | search in project                                                                                                            |
| ⌘B / ⌘J / ⌥⌘Y             | left dock / bottom dock (terminals) / all docks                                                                              |
| ⌘⇧E / ⌃⇧G / ⌃\` / ⌘?      | project panel / git panel / terminal panel (focus) / agents                                                                  |
| ⇧Esc                      | zoom the focused pane (palette: centered layout)                                                                             |
| ⌃⌘1…⌃⌘6                   | recent worktrees                                                                                                             |
| ⌘S / ⌥⌘S                  | save the focused editor (atlas.yaml: review the diff, then save) / save every open editor; files also autosave after 0.5 s    |
| ⌃⌘S                       | hide / pin the sidebar (hidden: move the pointer to the left edge to peek)                                                   |
| ⌘R                        | right sidebar: pinned beside the panes, or hidden and revealed from the right edge (also the button in the header)           |
| ⌘[ / ⌘]                   | back / forward through the worktrees and tabs you visited                                                                    |
| ⌥⌘← / ⌥⌘→ , ⌃1…⌃9         | previous / next space, space by position (a space is a repo; each has its own color)                                         |
| ⌘/                        | shortcuts cheat sheet                                                                                                        |
| ⌘N                        | new worktree                                                                                                                 |
| ⌘T / ⌘⇧A                  | new terminal / new agent session                                                                                             |
| ⌘W / ⌥⌘W / ⌘⇧W            | close tab (asks when a process is running) / close other tabs / close all tabs in the pane                                   |
| ⌘⇧T                       | reopen the last closed tab (editors, diffs, browser, logs, config and PRs return in place; terminals restart)                |
| ⌘1…⌘8 / ⌘9                | tab 1–8 by position in the focused pane / last tab                                                                           |
| ⌘⇧] / ⌘⇧[                 | next / previous tab                                                                                                          |
| ⌃Tab / ⌃⇧Tab              | recent tab / older tab: a switcher lists tabs by last use while ⌃ is held, releasing ⌃ opens the highlighted one (⎋ cancels) |
| ⌘\\ / ⌘⇧\\                | split right / down                                                                                                           |
| ⌘,                        | settings                                                                                                                     |
| ⌘⇧D                       | changes (in a focused terminal: split down, see [terminal keys](terminal.md#keyboard))                                       |
| ⌘L / ⌘E / ⌘⇧R             | logs / config / restart focused service                                                                                      |
| `open_key` / action `key` | same bare keys as the TUI                                                                                                    |

**The Panel tab is pinned.** It is always the first tab of the first pane, shown as a compact icon with no close button. ⌘W on it only says so and moves to the next tab; middle-click, close other tabs and close all tabs skip it; it cannot be dragged out, split or moved to another pane or the bottom dock; it is recreated when a worktree opens without one.

**App menu.** Atlas has a real macOS menu (Atlas, File, Edit, View, Go, Window, Help). Edit carries Undo/Redo/Cut/Copy/Paste/Select All. Cut, Copy and Paste are the standard macOS items. Undo (⌘Z), Redo (⇧⌘Z) and Select All (⌘A) are Atlas items that act on whatever has focus: the editor's own history and selection in an editor, `execCommand` in a text field, the terminal's selection (⌘A) in a GPU terminal (undo and redo do nothing there and never reach the shell). The standard AppKit items would send `undo:` to the web view, which knows nothing about CodeMirror's history. These three keys are fixed and cannot be remapped. The menu owns the keys of the commands it lists (File, Go, View and Settings), shows your current binding, follows remaps in Settings → Keyboard shortcuts, and works with focus in a terminal, an editor or the embedded browser. Keys that are not menu items (⌃Tab, ⇧Esc, ⌃1…⌃9, ⌘1…⌘9, ⌘[ ⌘]) are handled by the app window. There is no _Close Window_ item: ⌘W closes a tab, never the window.

The palettes highlight the characters your query matched and show their keys (↩ select, ⇧↩ open beside, ⎋ close).

Spaces and sidebars: every repo is a space with its own accent color (derived from its id), shown with its icon in a strip at the bottom of the sidebar and tinting the active space. Switching animates the content in the direction you moved; a two-finger horizontal swipe over the sidebar switches too. ⌃⌘S hides the sidebar so the content fills the window; moving the pointer to the left edge (about 8 px, after a short pause) slides it in as a floating panel, and it slides out when the pointer leaves (⎋ closes it). The right sidebar works the same from the right edge. The header breadcrumb `Space › worktree › tab` is clickable, and ⌘[ / ⌘] walk your visit history.

Settings has seven sections: **General** (updates, shell for your projects), **Appearance**, **Workspaces**, **Agents**, **Keyboard shortcuts**, **SSH hosts** and **Help & onboarding**. The search box at the top finds any setting by what it does ("font", "clipboard", "branch") and jumps to it. **Appearance** has four tabs: _Appearance_ (a picker of theme cards with Atlas Dark/Light (Tokyo Night based) as the default, next to Dracula/Alucard, One Dark/Light, a separate terminal theme, import from Ghostty, iTerm, VS Code, Alacritty and Base16, from a zip or folder of themes, or drop Zed theme JSON into `~/.atlas/themes/`, export, accent color), _Editor_ (code font, size, line spacing, with a live sample), _Interface_ (interface font and size, file panel side/icons/indent, active-pane border and dimming) and _Terminal_ (Fast terminal (GPU), font, size, ligatures, minimum contrast, cursor shape and blinking, scrollback, whether programs may copy to your clipboard: ask each time / always allow / never). Appearance changes apply immediately; terminal changes apply to terminals opened afterwards. The values are saved in `~/.atlas/desktop/settings.json` (`buffer_font_*`, `ui_font_*`, `terminal.*`, `project_panel.*`, `active_pane_modifiers`); **Advanced: edit settings.json** at the foot of Appearance opens it in an editor tab.

## Automation

With the app open, `atlas wt`, `atlas term`, `atlas browser` and `atlas file` drive it through `~/.atlas/desktop.sock` ([CLI](cli.md#atlas-wt)); the `atlas-desktop` skill teaches agents to use them.

## Tray icon

Clicking the menu-bar icon opens a small popover under it (a borderless, always-on-top window with rounded corners), the way a system utility would. It is painted on the active theme's ground and surface ladder (Atlas Dark's blue, Atlas Light's paper, any imported theme), never on the wallpaper's brightness. It closes on ⎋, when you click elsewhere, or when you click the icon again. Right-click the icon for a minimal menu: **Open Atlas** and **Quit Atlas**.

What it shows:

- **Header**: the Atlas mark (drawn from `assets/logo.svg`, like every other copy of the brand) and the overall state: _All stacks healthy_, _N services failed_, _N services starting_ or _Nothing running_ (failed beats starting beats healthy); the dot pulses slowly while something is unhealthy (not with reduced motion). The gear opens Settings.
- **One card per running project**, failed first, then starting, then healthy (ties by name): the project's icon, accent color and branch, a state chip that **names what is wrong** (_redis, api +2 failed_, _1 starting_, _All 10 services up_), one segment per service (red for a failed one), `services up/total · CPU · memory`, a CPU sparkline (the same samples the Panel charts use) and the actions. A failed stack offers **Restart failed** (restarts exactly the failed services, through the same `service_cmd` as the Services tab, not the whole stack) and **Logs** (opens Logs on the first failed one), next to **Open** and **Stop**. Stopped projects are listed below as a dense list; **Start** appears on hover or keyboard focus.
- **Agents** and **Terminals** sit side by side in one row: agents working and agents that need you (click to jump to the one waiting for you, or the Agents view), and live terminals with the terminal host's version, marked when the host is outdated.
- **Recommended** (only when it adds something the cards do not say): _Atlas X is available_ → **Update**, else a terminal host that needs a restart → **Review** (opens the restart dialog). A failed service is already named, with its actions, on its card.
- **Footer**: **Open Atlas**.

Anything that opens something brings the main window forward and runs there, through the same stores as a click in the app. When the window is not focused, a service failure, a runner alert or an agent that needs you still shows a native notification.

How it works: the popover is a second webview (`/tray`, label `tray`) created once at startup and only hidden afterwards. It has no stores of its own, so runners are subscribed once, by the main window. The main window builds a snapshot of projects, agents, terminals and updates (at most once a second, and the popover only receives it while visible), `tray.rs` caches it so the popover paints at once when shown, and clicks come back as `tray://action` to the main window. Start and Stop call the same `stack_start` / `stack_stop` commands as the sidebar. The popover is born in the app's cached theme (`boot::window_boot`, the same recipe as the main window, so it never starts on the kit's defaults) and follows the theme chosen in the app. It sizes itself to its content (up to 560 px and never taller than the room the screen leaves below the icon, `tray::room`); beyond that the list scrolls inside it while the header and footer stay put. Keyboard: ↑ ↓ Home End move through the buttons and rows, Enter activates, ⎋ closes. It is positioned from the icon's rectangle and kept inside the screen it is on, including a second monitor.

## Developing the app

| Goal                                                                        | Command                                                           |
| --------------------------------------------------------------------------- | ----------------------------------------------------------------- |
| Hot reload (Vite + Tauri shell; Rust changes rebuild)                       | `atlas desktop --dev` (or `bun run desktop:dev` at the repo root) |
| Make the real `Atlas.app` (Dock, Spotlight, `atlas desktop`) run your build | `atlas link --desktop` (debug bundle; rerun to rebuild)           |
| Back to the release CLI and app                                             | `atlas unlink`                                                    |
| Release bundle + dmg locally                                                | `bun run desktop:build` → `target/release/bundle/`                |
| Debug `.app` only (agents, Studio capture; never builds a DMG)             | `bun run desktop:build:app` → `target/debug/bundle/macos/`        |
| Typecheck the frontend                                                      | `bun run desktop:check` (also part of `bun run check`)            |

While linked (or running `cargo tauri dev`), the app shows a thin amber line across the top of the window and a small **Dev** chip next to the version in Settings and in the menu-bar popover. Hovering the top edge says _Dev build X — updates paused (atlas unlink to resume)_. Both the app and `atlas update` refuse to install over your build, and no update pill or notification appears. Settings → General has an **Unlink dev build** button and copyable rebuild commands.

### Debugging a running build

Debug builds (`desktop:dev` and `atlas link --desktop`) keep WebKit's context menu, so right-click → **Inspect Element** opens Web Inspector. They also run a bridge (`~/.atlas/desktop-debug.sock`, or `$ATLAS_DEBUG_SOCK`) that a terminal or an AI agent can use without touching the mouse or keyboard. Release builds do not contain the bridge or the menu.

#### Agent instances: `bun run desktop:agent`

An agent that wants to look at the app must never open a window on your desktop. `atlas desktop --agent` (or `bun run desktop:agent`, which also builds the debug bundle the first time) starts an isolated instance and prints the socket to use:

```bash
bun run desktop:agent -- --name a1 [--scratch]
# atlas-agent ready name=a1 pid=123 ATLAS_DEBUG_SOCK=/private/tmp/atlas-agent-a1/debug.sock
export ATLAS_DEBUG_SOCK=/private/tmp/atlas-agent-a1/debug.sock
atlas desktop debug state
bun run desktop:agent -- --stop --name a1      # or --stop for all of them
```

Each instance has its own `HOME`, `ATLAS_HOSTD_DIR` and bridge socket under `/tmp/atlas-agent-<name>/`, so agents run side by side and never touch your app, hostd or `~/.atlas`. `--scratch` copies only the files of `~/.atlas/desktop` (settings, theme, layout) into its `HOME`, never your project registry, tokens or sockets. The instance has no Dock icon or tray, never takes focus, and its window is placed on a private 2x `CGVirtualDisplay` that is not one of your monitors; the page there runs at the full 60 rAF/s because the display is never occluded. The display is retina (DPR 2); `ATLAS_AGENT_SCALE=1 bun run desktop:agent` gives a standard 1x display, to check what a non-retina user sees. Before showing the window the app reads its real frame back from AppKit and refuses unless it lies entirely on that display.

```bash
atlas desktop debug info                          # pid, URL, visible, window id, agent, socket
atlas desktop debug state                         # projects + service states, worktrees, selection
atlas desktop debug logs [--level csp|error|warn] [--since N]   # console, uncaught errors, CSP violations
atlas desktop debug eval 'document.title'         # expression, or a body with `return`; awaits promises
atlas desktop debug shot x.png [--selector css | --rect x,y,w,h | --full | --window]
atlas desktop debug record x.mov --seconds 5 | --until '[data-intro="done"]' [--fps 30]
atlas desktop debug wait '.ready' [--timeout ms] | wait 'window.__atlas.ws.loaded' --expr
atlas desktop debug click 'button.go' | type 'input' 'text' [--delay ms]
atlas desktop debug a11y [selector]               # roles, names, nesting; unnamed controls are flagged
atlas desktop debug perf --seconds 3              # rAF/s, p50/p95/max frame ms, long tasks
```

`shot` is a `WKWebView` snapshot: no permission, retina, works with the window unfocused. `--selector` picks the first match that is visible and on screen. `click` and `type` are DOM events inside the webview, never OS input. Logs are captured from before the page's own scripts run, so load-time errors and CSP violations show up. `desktop:dev` also prints them to the terminal as `[webview <level>] …`.

#### Gesture engine, bench and recordings

The gesture system is one engine with one source of truth, in Rust, fed by the raw `NSEvent` scroll stream:

- `crates/atlas-desktop/src/swipe_engine.rs`: pure physics and decisions (claim gate, 1:1 tracking, rubber band, release decision from the lift velocity, critically damped spring, programmatic `go`). Every decision carries a generation (`seq`) so a consumer applies each commit exactly once.
- `swipe_mac.rs`: the `NSEvent` monitor (the only place an event becomes a `Sample`), zone hit-testing, the display-rate clock (`swipe_clock.rs`, `NSView.displayLink`), the recorder and the replay entry.
- `page_stage.rs` + `page_layers.rs`: the native stage. It draws the space slide from cached `WKWebView.takeSnapshot` images on `CALayer`s moved at the display's refresh, so the slide never waits on JS or the 60 Hz webview. The live DOM is swapped under the overlay, which comes down only after JS acks the new page painted (watchdog 400 ms). Where no snapshot exists yet the frontend pager draws the same engine frames.
- Frontend: `shell/pager.ts` (space layers), `shell/tabPager.ts` (tab panes), `shell/stageSync.ts` (keeps the snapshots warm behind a cover), `shell/nativeSwipe.ts` (events and zones).

Snapshot cost (`WKWebView.takeSnapshot` runs on the main thread), measured on a 2x display with a 1264 x 774 pt pager: 36 ms at full resolution, 22 ms at 0.75, 11 ms at 0.5, 8 ms at 0.35. Warm-up snapshots therefore render at 0.5 (`SNAP_SCALE` in `SpacePager.svelte`), one page per idle task, and only after 300 ms without a selection, gesture or content change in a page (`stageSync.ts`). A selection re-snapshots nothing: only a change inside a page marks it stale (`pageDirty.ts`), and role flips on the layer roots (class, inert, data-role) do not.

AppKit's `trackSwipeEvent` is deliberately not used: its `gestureAmount` is a fixed `scrollingDeltaX / 1000` whatever the view size (a half-page drag is a 500 pt reach) and its settle is opaque.

**Tests.** `cargo test -p atlas-desktop swipe` runs the engine against the trace suite (`apps/desktop/src/lib/shell/__fixtures__/traces/*.json`, synthesized from the trackpad physics by `bun scripts/gesture-fixtures.ts`, plus any `recorded-*.json`). `bun test src/lib/shell src/lib/debug` covers the pagers, the stage scheduler and the bench's assessment rules.

**Bench.** Against a real agent instance (never your window, never OS input):

```bash
bun run desktop:agent -- --name gesture --scratch       # prints ATLAS_DEBUG_SOCK=…
ATLAS_DEBUG_SOCK=/private/tmp/atlas-agent-gesture/debug.sock bun scripts/gesture-bench.ts [--only name,name] [--repeat N] [--hz 60] [--json out.json]
```

It replays every trace into the engine through the same entry point real events use and holds the app to the budget: exactly the expected page (never two steps, never a bounce to the wrong one), tracking monotonic toward the target, settle 250-350 ms with no overshoot, no frame gap over 1.2 frames + 3 ms, no main-thread stall over 20 ms, zero forced layouts, natives parked and restored together, a consistent end state. It seeds its own spaces (five tiny repos registered through `projects_add`), aims every trace at the zone it is meant for by reading the zone table the engine actually hit-tests (`pager_state`), prints a table and exits 1 on any violation.

Timing on a shared machine: the bench first records 3 s of the page doing nothing (the **idle floor**: frame-gap p95 and the worst stall) and judges the timing rules against the larger of the budget and 1.5x that floor; a trace that fails only on timing is retried (`--retries`, default 4) and reports its best attempt. Deterministic rules (wrong page, double commit, forced layouts, settle time, overshoot, end state) are never relaxed or retried. The agent's virtual display is 60 Hz: the engine ticks at the display's refresh, so frame-time claims for 120 Hz need a bench run on a ProMotion panel (`--hz 120`).

`bun scripts/gesture-baseline.ts` measures the one thing every build shares, the programmatic space switch (what a click on the space strip or ⌃⇥ does), by injecting the same probe through `debug eval`, so it also runs against a bundle built from an old commit (`ATLAS_BIN=<that tree>/target/debug/atlas`).

What replay cannot cover: the `NSEvent` → `Sample` conversion (`sample_of` in `swipe_mac.rs`) and the OS delivering real events to the monitor. Measured with a standalone probe: a scroll `NSEvent` built from a `CGEvent` (`scrollWheelEventScrollPhase`, `IsContinuous`, point deltas) carries the right phases and deltas, but `-[NSWindow sendEvent:]` and `-[NSApplication postEvent:atStart:]` delivered none of a synthetic stream to a view in a background window, and calling `scrollWheel:` on the view directly started `trackSwipeEvent` with only its first callback (it reads the rest from the real event queue). So replay feeds the engine at `on_sample`, one step after the conversion, and the conversion stays verified by recordings.

**Recording your own swipes.** In a debug build of your app (`bun run desktop:dev`, or `atlas link --desktop`), run the command and swipe on the trackpad while it records:

```bash
atlas desktop debug gesture-record /tmp/my-swipe.json --seconds 8
```

The file is the raw event stream (timestamps, phases, deltas, pointer) with what the engine did with each event. Set `expect` in it (`zone`, `steps`: `1` next, `-1` previous, `0` stay, `untouched`) and keep it as `apps/desktop/src/lib/shell/__fixtures__/traces/recorded-<name>.json`: the Rust suite and the bench pick it up. `atlas desktop debug gesture-replay <trace.json>` feeds one into the running app in-process.

#### Capture and record (agents)

The app records and screenshots its own windows without focusing or driving the desktop (ScreenCaptureKit inside the app; every build, local only):

```bash
atlas desktop capture shot /tmp/x.png [--mode window|snapshot] [--size WxH]
atlas desktop capture start /tmp/x.mov [--fps 24|30|60] [--size WxH] [--cursor]   # then: atlas desktop capture stop
atlas desktop record --script tour.json --out /tmp/rec     # video.mov + timeline.json + shots/
bun studio run studio/recipes/tour.ts                      # recipe -> cached clip + timeline + shots
```

`snapshot` renders the page only (no permission, works occluded); `window` includes the native chrome and needs Screen Recording. Measured on macOS 26 with the user's windows covering the dev window 100 %: WebKit marks the page hidden (0 rAF/s) because AppKit clears the window's occlusion bit; none of the WebKit preferences or `_setWindowOcclusionDetectionEnabled:` changes that, only `-[NSWindow _setOcclusionStateIsVisible:]` does (30 rAF/s). `capture start` applies it for the recording and restores the real bit on stop. A window that is not key renders at 30 Hz, so the video container runs at the requested fps while page content is 30 fps; `start` reports this as `page_fps_cap`. macOS draws a "being shared" pill into the title bar of a captured window; no `SCStreamConfiguration` setting removes it (`sharingType = none` makes the frames black), so the standard window buttons are hidden for the recording.

Agent instances (`ATLAS_AGENT=1`, debug builds only) have no Dock icon or tray, never take focus, and park their window on a private `CGVirtualDisplay` (2x, placed right of the user's displays, destroyed with the process). Verified: creating it leaves the user's display bounds, main display, menu bar and Spaces unchanged. `ATLAS_DEBUG_SOCK=<absolute path>` selects another instance's bridge socket.

The code lives in two places: the backend is in `crates/atlas-desktop`, which reuses `atlas-tui`'s `config`, `control`, `status` and `runner`, and the frontend is in `apps/desktop` (SvelteKit SPA, `@codeatlasdev/ui`). Installs and updates always go through the CLI (`atlas update`, `atlas desktop --install`), so the terminal and the app share one download-and-replace path.
