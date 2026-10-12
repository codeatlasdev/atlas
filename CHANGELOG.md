# Changelog

All notable changes to Atlas (CLI, TUI, MCP server and Atlas Desktop). Versions follow [SemVer](https://semver.org); the CLI and the desktop app always ship together with the same version.

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
