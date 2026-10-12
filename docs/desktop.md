---
title: "Desktop app"
description: "Atlas Desktop: spaces, worktrees, agents, review, the embedded browser, SSH hosts and shortcuts."
---

# Desktop app

Atlas Desktop is a native macOS app (Tauri 2 + Svelte 5). It lists every atlas project in a sidebar and lets you start and stop each stack. It shows services live, streams and searches logs, runs actions, and edits `atlas.yaml` with validation.

It is a client of the same runners as the CLI: **Start** runs `atlas dev --detach`, and **Stop** is `atlas down`. The app, an attached TUI and an AI agent (through [MCP](mcp.md)) all see and control the same live state. Quitting or closing the app never stops a stack.

Contents: [Getting around](#getting-around) · [Install](#install) · [Updates](#updates) · [Architecture](#architecture) · [Projects and worktrees](#projects-and-worktrees) · [Tabs and panes](#tabs-and-panes) · [Agents](#agents) · [SSH hosts](#ssh-hosts) · [Keyboard](#keyboard) · [Automation](#automation) · [Developing the app](#developing-the-app)

## Getting around

**Spaces.** Every project is a space: it keeps its own worktrees, tabs, panes and docks, and has its own **icon and color**. The color is derived from the project unless you pick one, and tints the selection, the header and focus states, so you always know which project you are in. Pick a space in the sidebar, or move with ⌥⌘← / ⌥⌘→ and ⌃1…⌃9.

**Project icons.** Colors alone get confusing, so each project also shows an icon: your choice, else the repo's own logo, else a monogram (`my-app` → **MA**) on its color. Right-click a project (in the list or the bottom strip), or `⋯` → **Change icon and color…**: _Emoji_ (type or paste any emoji, or pick one), _Icons_ (search the ~1,900 Lucide icons), _Project logo_ (shown when Atlas found one), plus color swatches (the first one, dashed, is the automatic color). **Reset** goes back to automatic. Atlas looks for the logo in `package.json` `icon`, then `public/favicon.svg|png|ico`, `public/icon.*`, `public/logo.*`, `public/apple-touch-icon.png`, `static/favicon.*`, `app/icon.*` (and `src/app/`), `assets/logo.*`, `.github/logo.*`, and `logo.*` / `icon.*` / `favicon.ico` at the root; images over 256 KB are skipped. It is read through the host, so SSH projects get their logo too. The choice is saved per project in `~/.atlas/desktop/projects.json` (`identity`). The icon appears in the sidebar and its bottom strip, the header breadcrumb, the project panel, the jump palette, the Files header and the tray popover.

**Sidebars.** The left sidebar shows the **active project** (icon, name, live status dot that breathes while its stack runs, `up/total` services) and its worktrees; a project with a single checkout is just its own row (clicking it opens `main`, no redundant child). **Other projects** is collapsed by default (pinned first); each shows its icon, status and service count, with a chevron to expand worktrees only when it has more than one. Filtering opens the list and shows every match. At the bottom, a strip of project icons switches space (active one ringed, tooltip with the name and ⌃N). Hide the sidebar with ⌃⌘S. The right sidebar (⌘R) holds agent history and listening ports; hide it for a calmer view. Hidden sidebars can be peeked without losing your layout. A project's panel (stack health, services and their live CPU and memory) is the first thing a space shows.

**Peeking a hidden sidebar.** Move the pointer to the window edge (left for spaces, right for the context sidebar) and it slides in over the content: after ~60 ms, or at once if you flick into the edge. It stays while the pointer is on it (with a 24 px margin), slides out 250 ms after you leave, and ⎋ dismisses it. A menu or dialog opened from it keeps it open.

**Trackpad gestures.** Two-finger swipes work like Arc and never take over scrolling: a gesture only starts when nothing under the pointer (logs, editor, terminal, a horizontal list) can scroll further that way.

| Swipe…       | Over                               | Does                                                                                                                                                                                        |
| ------------ | ---------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| left / right | the sidebar                        | next / previous space. The content follows your fingers and the next space's accent glows at the edge; past ~30% of the width, or with a quick flick, it commits, otherwise it springs back |
| left / right | the tab strip                      | next / previous tab (lighter travel)                                                                                                                                                        |
| right / left | the content, where nothing scrolls | Back / Forward through visited worktrees and tabs (⌘[ / ⌘]), with an arrow chip that fills as you drag                                                                                      |

A swipe ends after ~120 ms without movement, and the momentum tail after it is ignored, so one swipe is one switch. With _Reduce motion_ on, nothing animates and the switch is instant. Pinch (ctrl + wheel) is never treated as a swipe.

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

- macOS 13+ on Apple silicon.
- The atlas CLI. The app looks for it at `$ATLAS_BIN`, then `~/.atlas/bin/atlas`, then `atlas` on `PATH`.
- `curl` (preinstalled on macOS). No GitHub login is needed: releases are public.
- Optional: `gh`, logged in (`gh auth login`), for the pull-request tab and checks.

### Shell environment

Opened from Finder or the Dock, macOS gives apps a bare environment (`PATH=/usr/bin:/bin:…`), so `bun`, `node` and anything from Homebrew, nvm, mise or asdf would be missing. At launch Atlas reads the environment of your shell in the background and starts every `atlas dev`, `git`, `gh` and update with it, exactly as your terminal would.

- Which shell: the one chosen in **Settings → General → Shell for your projects**, else your account's login shell (`chsh`), else `$SHELL`, else `/bin/zsh`. Any shell works (zsh, bash, fish, nushell, ksh, dash, tcsh…): Atlas only asks it to run `/usr/bin/env -0`, trying `-l -i -c`, then `-l -c`, then `-c`.
- Settings lists shells from `/etc/shells`, accepts any path, shows which flags worked, how long it took, the resulting `PATH`, and whether `bun`, `node`, `docker`, `gh`… are found. **Reload** re-reads it after you edit your rc files.
- A shell that fails or takes over 6 s is reported there; services then get the system `PATH` only.
- Projects already running keep the environment they started with: stop and start them after a change.
- The choice is saved in `~/.atlas/desktop/settings.json` (`{"shell": "/opt/homebrew/bin/fish"}`).

## Updates

The CLI and the app are released together with the same version, and they update together:

- **From the terminal:** `atlas update` updates the CLI, then brings an installed `Atlas.app` to the same version.
- **From the app:** the app checks on launch, when you return to it (at most every 6 hours) and every 6 hours while open; turn this off in Settings → General → Updates → _Check automatically_. When a newer release exists a calm **Update to X.Y.Z** pill with a soft glow appears in the sidebar footer, and you get one native notification per version (through the same switch as agent notifications, Settings → Agents). Clicking the pill, or ⌘K → _What's new_, opens the **What's new** sheet.
- **What's new sheet:** the release notes (the `CHANGELOG.md` section of each GitHub release, fetched from the public GitHub API) of every version between the one you run and the latest, rendered like a Markdown preview. **Update and relaunch** runs `atlas update` and shows _Download and verify → Install → Update the app_ with checkmarks (the raw output is under _Details_), then **Relaunch Atlas**. **Later** closes the sheet; **Skip this version** hides the pill and the notification until a newer version ships (stored under `updates` in `settings.json`, with `notified` and `lastSeen`).
- **After an update:** the first launch of the new version shows a one-time _Updated to X_ card with **See what's new**, opening the same sheet for that version.
- **Commands:** ⌘K → _Check for updates_ (checks now and says what it found, or opens the sheet) and _What's new_. Settings → General → Updates shows both versions, the channel, when it last checked, **Check now**, the auto-check switch and a link to the full changelog.
- **Repair:** **Reinstall app** in the sheet, or `atlas desktop --install --latest`.

## Architecture

Terminals live in **`atlas hostd`**, a per-user daemon (`~/.atlas/hostd.sock`, token `~/.atlas/hostd.token`) that the app starts on launch. It owns PTYs, one-shot `exec`, file access and scrollback, so terminals and agents keep running when the app quits, crashes or updates; reopening restores the tabs with their scrollback. The same daemon runs on SSH hosts.

**Updating the host without losing terminals.** The daemon outlives app updates, so after one it is older than the app. On connect the app compares the daemon's `info` (`version`, `proto`, `caps`) with its own version and acts:

- the daemon supports `upgrade` (protocol 2+): **live takeover**. The new `atlas hostd --takeover` receives every pty master fd (`SCM_RIGHTS` over a unix socket) plus the session state and the pid-file lock, binds the socket, confirms, and the old process exits. Programs keep running (they never notice); scrollback keeps appending to the same files; the app reconnects and reattaches, GPU terminals open their raw streams, and a _Terminal host updated_ toast appears. If the successor fails before confirming, the old daemon takes its ptys back and keeps serving. `atlas hostd --upgrade` does the same from a shell with the binary you run.
- an older (protocol 1) daemon cannot hand over. With no running terminal the app restarts it silently; with running terminals it keeps it (terminals use xterm.js) and Settings → Appearance → Terminal shows a notice with **Restart terminal host**. It updates by itself the next time the app starts with nothing running. Handing over from protocol 1 to 2 is the one update that needs this restart; later ones are live.
- pipe-mode sessions (`omp --mode rpc-ui`) have no pty master to pass and end with the old daemon.
- exit codes of programs adopted by a takeover come from kqueue (`NOTE_EXITSTATUS`) on macOS, registered at adoption; where that is unavailable the exit is still detected but its code is reported as unknown.

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

Settings has seven sections: **General** (updates, shell for your projects), **Appearance**, **Workspaces**, **Agents**, **Keyboard shortcuts**, **SSH hosts** and **Help & onboarding**. The search box at the top finds any setting by what it does ("font", "clipboard", "branch") and jumps to it. **Appearance** has four tabs: _Appearance_ (light/dark theme, Zed theme JSON, drop extra themes in `~/.atlas/desktop/themes/`, accent color), _Editor_ (code font, size, line spacing, with a live sample), _Interface_ (interface font and size, file panel side/icons/indent, active-pane border and dimming) and _Terminal_ (Fast terminal (GPU), font, size, ligatures, minimum contrast, cursor shape and blinking, scrollback, whether programs may copy to your clipboard: ask each time / always allow / never). Appearance changes apply immediately; terminal changes apply to terminals opened afterwards. The values are saved in `~/.atlas/desktop/settings.json` (`buffer_font_*`, `ui_font_*`, `terminal.*`, `project_panel.*`, `active_pane_modifiers`); **Advanced: edit settings.json** at the foot of Appearance opens it in an editor tab.

## Automation

With the app open, `atlas wt`, `atlas term`, `atlas browser` and `atlas file` drive it through `~/.atlas/desktop.sock` ([CLI](cli.md#atlas-wt)); the `atlas-desktop` skill teaches agents to use them.

## Tray icon

Clicking the menu-bar icon opens a small translucent popover under it (a borderless, always-on-top window with the native _Popover_ vibrancy and rounded corners), the way a system utility would. It closes on ⎋, when you click elsewhere, or when you click the icon again. Right-click the icon for a minimal menu: **Open Atlas** and **Quit Atlas**.

What it shows:

- **Header**: the Atlas mark and the overall state: _All stacks healthy_, _N services failed_, _N services starting_ or _Nothing running_ (failed beats starting beats healthy).
- **One card per running project**, failed first, then starting, then healthy (ties by name): the project's icon and accent color, a dot per service, `services up/total · CPU · memory`, a small CPU sparkline (the same samples the Panel charts use) and **Stop** / **Open**. Clicking the card opens that project's space. Stopped projects are listed below with a **Start** button.
- **Agents**: agents working and agents that need you; click to jump to the one waiting for you (or the Agents view).
- **Terminals**: live terminals in the terminal host and its version, marked when the host is outdated.
- **Recommended** (only when relevant): a service that failed in the last hour (_api failed 2 min ago_ → **View logs**, which opens Logs filtered to it), else _Atlas X is available_ → **Update**, else an outdated terminal host → **Open settings**.
- **Footer**: **Open Atlas** and the settings gear.

Anything that opens something brings the main window forward and runs there, through the same stores as a click in the app. When the window is not focused, a service failure, a runner alert or an agent that needs you still shows a native notification.

How it works: the popover is a second webview (`/tray`, label `tray`) created once at startup and only hidden afterwards. It has no stores of its own, so runners are subscribed once, by the main window. The main window builds a snapshot of projects, agents, terminals and updates (at most once a second, and the popover only receives it while visible), `tray.rs` caches it so the popover paints at once when shown, and clicks come back as `tray://action` to the main window. Start and Stop call the same `stack_start` / `stack_stop` commands as the sidebar. The popover follows the theme chosen in the app and sizes itself to its content (up to 560 px; beyond that it scrolls). It is positioned from the icon's rectangle and kept inside the screen it is on, including a second monitor.

## Developing the app

| Goal                                                                        | Command                                                           |
| --------------------------------------------------------------------------- | ----------------------------------------------------------------- |
| Hot reload (Vite + Tauri shell; Rust changes rebuild)                       | `atlas desktop --dev` (or `bun run desktop:dev` at the repo root) |
| Make the real `Atlas.app` (Dock, Spotlight, `atlas desktop`) run your build | `atlas link --desktop` (debug bundle; rerun to rebuild)           |
| Back to the release CLI and app                                             | `atlas unlink`                                                    |
| Release bundle + dmg locally                                                | `bun run desktop:build` → `target/release/bundle/`                |
| Typecheck the frontend                                                      | `bun run desktop:check` (also part of `bun run check`)            |

While linked (or running `cargo tauri dev`), the app shows a thin amber line across the top of the window and a small **Dev** chip next to the version in Settings and in the menu-bar popover. Hovering the top edge says _Dev build X — updates paused (atlas unlink to resume)_. Both the app and `atlas update` refuse to install over your build, and no update pill or notification appears. Settings → General has an **Unlink dev build** button and copyable rebuild commands.

### Debugging a running build

Debug builds (`desktop:dev` and `atlas link --desktop`) keep WebKit's context menu, so right-click → **Inspect Element** opens Web Inspector. They also run a bridge on `~/.atlas/desktop-debug.sock` that a terminal or an AI agent can use without touching the mouse or keyboard:

```bash
atlas desktop debug info                      # pid, URL, visible, native window id
atlas desktop debug logs [--level csp|error|warn] [--since N]   # console, uncaught errors, CSP violations
atlas desktop debug eval 'document.title'     # expression, or a body with `return`; awaits promises
atlas desktop debug eval 'return await window.__TAURI_INTERNALS__.invoke("projects_list")'
atlas desktop debug shot [/tmp/x.png]         # screencapture of the app window only
```

Logs are captured from before the page's own scripts run, so load-time errors and CSP violations show up. `desktop:dev` also prints them to the terminal as `[webview <level>] …`. Release builds do not contain the bridge or the menu.

The code lives in two places: the backend is in `crates/atlas-desktop`, which reuses `atlas-tui`'s `config`, `control`, `status` and `runner`, and the frontend is in `apps/desktop` (SvelteKit SPA, `@codeatlasdev/ui`). Installs and updates always go through the CLI (`atlas update`, `atlas desktop --install`), so the terminal and the app share one download-and-replace path.
