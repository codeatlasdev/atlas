---
title: "TUI"
description: "The atlas dev dashboard: layout, key bindings, log viewer, palette and sessions."
---

# The TUI (`atlas dev`)

`atlas dev` opens a full-screen dashboard and starts your services. It needs an interactive terminal (minimum 60×12; below that it shows "Terminal too small" with the current size). Quit with `q`: all services are stopped.

## Layout

```text
┌ tabs ───────────────────────────────────────────────────────────┐
│ All  api  web  worker                                           │
├──────────────────────────────────────────┬──────────────────────┤
│                                          │ ● All systems operational
│  log viewer                              │ SERVICES             │
│  12:01:03 [api] listening on :3001       │  ● api   :3001   ⋯   │
│  12:01:04 [web] ready in 312ms           │  ✗ web   :3002   ⋯   │
│                                          │ OPERATIONS / MOBILE  │
│                                          │ EVENTS               │
├──────────────────────────────────────────┴──────────────────────┤
│ ? Help  q Quit  r Restart all  / Search  : Commands  f Following │
└─────────────────────────────────────────────────────────────────┘
```

- **Tabs**: `All` (every service merged) plus one per service and one per open [session](#sessions-and-attach). Each label starts with its jump digit (`0`–`9`) and a service tab shows a state dot. Services with `hide_logs: true` get no tab and are left out of `All`. Tabs are clickable. When they do not all fit, the bar shows whole labels only and `‹ +N` / `› +N` at the edges (N = hidden tabs; clickable).
- **Log viewer**: lines are `HH:MM:SS [service] text`, colored per service, with ANSI colors preserved and long lines hard-wrapped. Each service keeps up to 50,000 lines in memory (capped at 8 MiB per service, whichever comes first); the full history is on disk in `.logs/`.
- **Sidebar** (shown when the terminal is at least 90 columns plus the sidebar width wide; the sidebar is 34–44 columns, growing with the longest service name, so roughly 124–134 columns. Narrower terminals, and `z` (zoom) at any width, give logs the full width and add a one-line **status strip** above the footer: the worst issue first (`✗ api gave up after 3 restarts · 5/7 running`), clickable to jump to that tab, then the health counts and any running operation):
  - health summary and gauge: `5/5 running · 1 done · 1 stopped`. Completed one-shots and services you stopped, disabled or that an action suspended are left out of the denominator; the dot pulses only while something is failed or starting;
  - **SERVICES**: state icon, name, port, and a `⋯` hint (open the context menu with `m`). A second line `↳ …` shows scheduler notes: `waiting for migrate (completed_successfully)`, `restarting in 4s (2/5)`, `waiting for :3000 (12s)`, `gave up after 5 restarts`, `done`, `restarted ×2`. Failed rows also show `exit <code>`, and services that restarted show `↻<n>`;
  - **MOBILE**: live build phase of active mobile sessions;
  - **OPERATIONS**: the running non-session action with elapsed time and last output line;
  - **⚠ ISSUES**: failed `critical` services with the last log line and `critical_help`;
  - **TUNNEL** (only when a `tunnel` is configured): state icon, `ssh :<local_port> → <ssh_host>` and a second line `→ <remote_host>:<remote_port>` (`off (opens on demand)` until started), and the **Kill tunnel on exit** checkbox. Its ssh output has its own `tunnel` log tab;
  - **EVENTS**: the last 50 notable events, newest first, one icon/color per kind: `✗` crash (with exit code), `✓` done, `●` started, `○` stopped, `⏸ web paused by Reset DB`, `⧗` waiting, `↻` restart, `⚠` critical alert. The time column shows only when the minute changes.

  The sidebar gives SERVICES / OPERATIONS / ISSUES their full height first; EVENTS gets what is left, so `critical_help` is never squeezed out.
- **Footer**: hints generated from the same keymap table as `?` and filtered by context: only keys that do something here (a stopped service offers `r Start`, not `s Stop`), with `/ Search` and `m Menu`. `f` reads `Following` (green) or `Paused` (amber). While typing a search the hints become `Enter keep · Esc clear · Ctrl+R regex · Tab find/filter`. When an operation is running, hints are dropped from the right so nothing overlaps; only drawn hints are clickable. An `ATTACHED` bar replaces the hints while attached to a session.
- The **brand row** and the splash show the project `name`, so two instances are easy to tell apart.
- **Toasts** slide in from the right edge and disappear after 3 seconds ("Restarting api", "Logs copied (3min)"). In-progress toasts show a spinner; facts (`Tunnel will close on exit`) show a static `ℹ`.
- **Update modal**: when the background check finds a newer release, a modal asks `Atlas v<new> available — update when you quit?` (**Enter** = yes, **Esc** = not now). It appears once per version per session, after the splash, and waits for any other open modal to close. Accepting puts `⬆ on exit` in the brand row; once the TUI has exited, services are stopped and the terminal is restored, Atlas runs the update and prints the result. Attached to a runner, the runner keeps the old version until `atlas down && atlas dev --detach`. Declining leaves the plain `⬆` mark and the banner printed after you quit. See [CLI › Update prompt](cli.md#update-prompt-atlas-dev).

Service state icons: `●` running, `◎`/spinner starting, `✗` failed, `○` stopped, `⊘` disabled (stopped by you; a service left out of the run by `atlas dev SERVICE…` / `--profile` also shows a dim `off` after its name), `⏸` suspended (paused by an action).

On startup a splash screen lists each service live (left-aligned in a centered block: `✓` running, `✗` failed, spinner starting, `○` stopped) until all app services have been queued for start (or 1.5 s have passed, whichever comes first).

## Key bindings

Dashboard keys (the **All** tab and service tabs). `?` shows the same list in-app (generated from the keymap table in `tui/keymap.rs`, two columns at ≥100 columns, scrollable with `j`/`k`/`PgUp`/`PgDn` when it does not fit), plus your `open_key`s and a **PROJECT ACTIONS** section. Actions with a `group:` are listed under a header per group, after the ungrouped ones; groups appear in order of first use.

### General

| Key | Action |
|---|---|
| `?` | Help (close with `Esc` or `?`) |
| `q` | Quit dialog (see [below](#quitting)); attached to a runner: Detach / Stop everything |
| `D` | Only [attached](#attached-mode): detach, services keep running. In a TUI that owns its services it shows the hint `run atlas dev --detach to keep services running in the background` |
| `Ctrl+C` | Quit immediately, no dialog, from any layer (also from the palette and search bar; it is never typed as text) |
| `:` | [Command palette](#command-palette) |
| `Ctrl+Z` | Suspend Atlas to the shell (`fg` resumes; terminal modes are restored) |
| `Ctrl+U` / `Ctrl+W` | In the palette: clear the query / delete the last word. Other Ctrl/Alt chords are ignored in text inputs |
| `z` | Zoom: hide the sidebar at any width (toggle; the footer shows `Zoom on`). Remembered per project |
| `r` | On **All**: asks `Restart N services? Completed oneshots will not re-run.`, then restarts every service that is not a finished one-shot. On a service tab: restarts that service (starts it when stopped) |
| `t` | Toggle "kill tunnel on exit"; only bound when a `tunnel:` is configured |

### Tabs

| Key / gesture | Action |
|---|---|
| `Tab` / `Shift+Tab` | Next / previous tab |
| `0`–`9` | Jump to tab by index (`0` = All) if it exists |
| **mouse click — tab chip** | Switch to that tab |
| **mouse click — `‹` / `›`** | Scroll the tab bar left / right when there are too many to fit |
| **mouse click — sidebar service row** | Switch to that service's log tab |
| **mouse click — sidebar `⋯`** | Open the service context menu |
| **mouse click — footer key hint** | Same as pressing that key (only where the key is active) |
| **mouse click — search bar** | Start a log search (same as `/`) |
| **mouse click — command palette item** | Select and run (same as Enter) |
| **mouse click — device picker item** | Select and confirm (same as Enter) |
| **mouse click — quit modal buttons** | Cancel or Quit (attached: Detach or Stop everything) |
| **mouse click — confirm modal buttons** | Cancel (Esc) or Accept (Enter) |
| **mouse click — service menu item** | Run that action |
| **mouse click — help modal** | Close (same as `?` / Esc) |
| **mouse click — toast** | Dismiss the toast |
| **mouse click — modal backdrop** | Close the modal (same as Esc) |
| **mouse wheel — logs** | Scroll 3 rows |
| **mouse wheel — command palette** | Move selection up / down |
| **mouse wheel — device picker** | Move selection up / down |
| **log scrollbar — click** | Jump to that position in the log |
| **log scrollbar — drag** | Continuous scroll |
| `M` | Toggle mouse capture. Off lets your terminal select text natively; with it on, hold `⌥` (Option) while dragging. |

### Scrolling

| Key | Action |
|---|---|
| `j` / `k`, `↓` / `↑` | Scroll one row |
| `PgUp` / `PgDn` | Scroll a page |
| `g` / `Home` | Oldest line |
| `G` / `End` | Newest line, and turn follow back on |
| `f` | Follow on / off |

### Services

| Key | Action |
|---|---|
| `r` | On a service tab: restart it (starts it when stopped). On **All**: restart everything, asking first |
| `s` | Stop this service (it becomes `disabled`: no health checks, never auto-restarted) |
| `S` | Restart this service (service tabs only; on **All** it does nothing, use `r`) |
| `m` | [Service menu](#service-menu) |
| `t` | Keep the SSH tunnel open on exit (only with a `tunnel:` block) |
| `i` | Attach to a [session](#sessions-and-attach) |

### Copying and clearing

The copy keys act on the current tab (everything on **All**) and use the system clipboard; a toast confirms.

| Key | Action |
|---|---|
| `L` | Copy the logs |
| `P` | Copy the logs as a prompt for an AI agent |
| `E` | Copy only the errors, grouped by service |
| `Ctrl+L` | Clear the in-memory log buffer of **every** service, in this TUI only (the files in `.logs/` are untouched) |

## Log viewer

- **Follow**: new lines auto-scroll while follow is on. Scrolling up turns it off and the view stays put as new lines arrive; reaching the bottom (`G`, or scrolling down to it) turns it on again. While paused, the last log row shows a clickable chip `↓ N new · G` (N counts lines that arrived in the current view since the pause; click or `G` jumps to the bottom). Switching tabs, searching, and changing the level filter also jump to the bottom and follow.
- **Wrapping and scrolling** work on wrapped screen rows, so what you scroll is what you see. Continuation rows hang under the message column (aligned after the time / `[service]` prefix) and are dimmed.
- **Single-service tabs** drop the `[service]` prefix (it is on every line otherwise); **All** keeps it. Prefix colors avoid amber and red so they are never mistaken for warn / error lines.
- **Copy** keys use the current tab (everything on **All**) and the system clipboard; a toast confirms the result. `E` (errors) groups lines under one `── service ──` header per service.
- **Buffers**: each service keeps its own ring (last 50,000 lines **and** at most 8 MiB), so a chatty service cannot push out another service's history. **All** merges the rings in arrival order. Memory stays bounded however much a service prints.
- **Ingestion** assembles real lines from the raw pipe: a partial line (a prompt, `Password: `) is shown after 150 ms of silence, `\r\n` is one line break, and a lone `\r` rewinds the line so a progress bar (`10%\r20%\r…`) shows only its last state. Invalid UTF-8 is replaced with `�`. A single line longer than 16 KiB is cut and ends with ` … [truncated]`. Level, timestamp and a monotonic sequence number are computed once per line when it arrives, never per frame.
- **Restart separators**: when a service starts again after having printed output, a dim `──── HH:MM:SS restarted ────` line separates the runs (it is never searched, counted as output or counted as an error).
- **Performance**: the view is an incremental index over the buffer. A new line is classified, filtered and searched once; a frame or a keypress costs O(visible rows), not O(buffer). Wrapped row heights are cached per width/layout. Release benchmark (1,000,000 lines through 8 services): ingest 4 M lines/s, a regex find over all ~437k held lines 45 ms, appending 10k lines to an active find 0.6 ms (`cargo test -p atlas-tui --release -- --ignored --nocapture bench_one_million`).
- **Colors**: SGR bold / dim / italic / underline (plain, double, `4:3` styles and the `58` underline color) / strikethrough / reverse, 16 / 256 / truecolor foreground and background (both `;` and `:` forms) follow the theme's palette. OSC 8 hyperlinks are honored.

### View toggles

| Key | Action |
|---|---|
| `W` | Wrap on/off. Off truncates each line with `→`; pan with `←`/`→` (the top-right chip shows `← +N`). |
| `T` | Timestamps on/off. |
| `J` | JSON compaction on/off. A line that is exactly one JSON object renders as `LEVEL message key=value …`, colored by level (pino / bunyan numeric levels `30`/`40`/`50` and string levels `info`, `warn`, `error`, `fatal`, …). Off shows the raw line. Find and filter search the rendered text. |
| `-` | Insert a `──── HH:MM:SS mark ────` line into the current tab's stream (and into that service's `.logs/<name>.log`). On **All** the mark goes to the in-memory `atlas` stream only. |
| `z` | Zoom: hide the sidebar and give the logs the full width. |

Toggles that differ from the default are listed on the search/status line (`wrap off · no ts · json`). `W`, `T` and `J` persist across runs in `.logs/.atlas-ui.json`.

### Searching and filtering

- `/` opens the search bar at the bottom and starts **find** mode: every line stays visible, all matches in view are highlighted and the current one is stronger. The bar shows `3/41` (current / total matches in the current tab; the total covers the whole buffer, not just the screen). `n` / `N` jump to the next / previous match (wrapping, scrolling the match into view and pausing follow); they are only active while a find query exists, so they never shadow `open_key`/action keys otherwise.
- `Tab` inside the bar switches **find ↔ filter**. Filter shows only matching lines (the pre-existing behavior); the bar shows the number of lines kept.
- `Ctrl+R` toggles **regex** (Rust `regex` syntax; an invalid pattern shows `invalid regex` and matches nothing). Matching is **smartcase**: an all-lowercase query is case-insensitive, any uppercase letter makes it case-sensitive.
- `Enter` keeps the query and returns to normal keys (`n`/`N` keep working); `Esc` clears it. With no matches the pane shows a centered `0 matches in <tab>`.
- `v` sets the minimum level shown. Levels are inferred from the text: lines containing `error`, `err `, `fatal`, `panic` or red ANSI are *error*; `warn` or yellow are *warn*; `debug`, `trace` or gray are *debug*; the rest are *info*. `warn+` shows warn and error; `error` shows only errors. An active level shows as a badge next to the search bar.
- `V` sets the **time range** of the view, composed with the tab, level, find and filter. It cycles `last 1m → 5m → 15m → 1h → since restart → since start`, then opens a small prompt for a **custom** range (clock times of today: `14:05` for "from", `14:05–14:20` for "between", both ends inclusive; `-`, `–`, `—` or `..` separate; an invalid time is explained in the prompt and nothing changes), and then returns to all time. `Enter` applies, `Esc` returns to all time. The active range is a `time: …` badge next to the level badge, and the footer's `V Time` is clickable. `last N` windows are relative and slide: they are re-evaluated at most once a second and lines that age out are dropped from the front of the view without rescanning the buffer, so an idle window empties by itself. `since restart` starts at the newest `restarted` separator of the service on a service tab (of any service on All; with none yet it is the whole buffer); `since start` starts at the oldest line held. A range change re-filters the view once and, like the level filter, jumps to the newest line. The range is not remembered between runs (a relative window would be stale on the next start).

### Links, error jumps and open

- **Links** in a line are underlined: URLs (`http(s)://`, `file://`, OSC 8 hyperlinks) and `path:line[:col]` references from compilers, stack traces and test output (`src/main.rs:12:5`, `/app/a.ts:30:9`, `util.py:10`). Numbers such as `12:30:45` or `localhost:3000` are not links.
- `}` / `{` move the link cursor to the next / previous link (starting from the newest visible line; the focused link is highlighted and scrolled into view). `o` opens it: a URL in the system browser; a path in `$VISUAL` / `$EDITOR` when that is a GUI editor (`code`, `cursor`, `zed`, `subl`, JetBrains launchers get their `file:line` flags), else `code` / `zed` / `cursor` from `PATH`, resolved against the project root. A terminal editor (`vim`) is never launched inside the TUI; when no editor applies, or the file does not exist, the `path:line` is copied instead. `y` copies the focused link. Nothing blocks the event loop: launchers run detached and are reaped in the background.
- `]e` / `[e` jump to the next / previous **error** line, `]w` / `[w` to the next / previous **warning or error**, wrapping around the current view (tab + level filter). The first jump starts from the top of the viewport; later jumps continue from the last target. The jump pauses follow so the line stays on screen; `No error lines` is toasted when the view has none.

### Tab badges

A tab that is not focused shows `◆` when it has output you have not seen and a red `✗N` with the number of error lines since you last viewed it. Opening the tab clears both. Clicking the `✗N` badge opens the tab with the level filter set to `error`.

## Command palette

`:` opens a fuzzy command list. The query is a case-insensitive **subsequence** match (`rsa` finds `Restart api`); matched characters are highlighted, name matches rank above description-only matches. `↑`/`↓`/`PgUp`/`PgDn`/`Ctrl+N`/`Ctrl+P` select, `Enter` runs, `Esc` closes, `Ctrl+U`/`Ctrl+W` edit the query. The list scrolls to keep the selection visible; the separator shows `↑ n/total ↓` and each row shows its key on the right.

Order with an empty query: **recent commands first** (kept in memory and in `.logs/.atlas-ui.json`), then project [actions](configuration.md#actions) (ungrouped first, then by group with a dim `Group ›` prefix; searching `android` finds every action in group `Android`) and `Open <service>` entries, then built-ins (Restart All, Toggle Zoom, Help, Scroll to Bottom/Top, Copy Logs / for AI / Errors, Quit), then generated per-service entries. Per-service entries follow the service menu rules: `Restart`/`Stop` while running or starting, `Restart` for failed, `Start` for stopped and suspended (services you stopped yourself get none; services left `off` by `atlas dev SERVICE…` / `--profile` do get `Start`). Pasting works in the palette.

Modals (palette, help, confirm, menus) dim the dashboard behind them instead of hiding it.

## Service menu

`m` on a service tab opens a small menu that only offers what makes sense for the service's state:

| State | Entries |
|---|---|
| running, starting | `r` Restart, `s` Stop |
| failed | `r` Restart |
| stopped, disabled, suspended | `t` Start |

`1`, `2`, `3` are aliases for restart, stop, start (when that entry is offered). `Esc` cancels.

## Confirm modals

An action with `confirm:` opens a modal titled with the action label and the confirm text. `Enter` or `y` runs it; `Esc` or `n` cancels. This applies whether you press the action's key or pick it in the palette. When the action has `pause_services`, the modal adds a `Pauses: web, api` line.

`r` on the **All** tab uses the same modal (title "Restart all").

## Remembered UI state

`.logs/.atlas-ui.json` keeps, per project: the active tab (by **name**, so it survives service reordering), zoom, the wrap / timestamps / JSON toggles, the level filter and the recent palette commands. It is restored on start. The time range (`V`) is deliberately not saved. The file is best-effort: if it is missing or invalid Atlas starts with defaults, and a failed write is ignored.

## Quitting

Attached to a runner: see [Attached mode](#attached-mode); quitting is a detach unless you pick **Stop everything**.

`q` opens "Quit? All services will be stopped." with **Cancel** (default) and **Quit**. `y` quits, `n`/`Esc` cancels, `Tab`/`←`/`→` switch the selection, `Enter` confirms it. `Ctrl+C`, SIGTERM and SIGHUP (closing the terminal window) quit directly. In every case Atlas stops all services and open sessions before exiting. See [Runtime › Stopping](runtime.md#stopping).

## Attached mode

`atlas dev --detach` starts a **background runner** (see [Runtime › Runner and attach](runtime.md#runner-and-attach)); running plain `atlas dev` while it is up attaches to it instead of starting anything. The dashboard is the same one (same widgets, same keys, same log viewer): the sidebar brand row shows `⛓ attached`, the footer and `?` list `D Detach`. State, logs and events are streamed from the runner; on attach you get its recent log tail (up to 1000 lines per service) and the last events, then the live feed.

| Key | In attached mode |
|---|---|
| `r` / `s` / `S` / `m`, palette, `open_key`, project actions | same as always; the request is sent to the runner (an action with `confirm:` still asks first, in your terminal) |
| `D` | **Detach**: the TUI exits, the runner and every service keep going. Prints `detached — services keep running · atlas dev to reattach · atlas down to stop` |
| `q` | Dialog **Detach** (default) / **Stop everything**: `d` detaches, `s` stops the runner (like `atlas down`), `Tab`/`←`/`→` switch, `Enter` confirms, `Esc`/`n` cancels |
| `Ctrl+C`, SIGTERM, SIGHUP | Detach. Closing the terminal never stops the runner's services |
| `t` | Explains that the runner owns the tunnel (it is closed when the runner stops) |
| `i`, session actions, mobile device picker | **Owner-only**: a toast says so. Interactive sessions need the terminal that spawned them; use `atlas down` and a plain `atlas dev` for those |

Several clients can be attached at once (`atlas dev --status` counts them) and see each other's restarts. A client that disconnects or crashes never affects the runner. If the runner dies or is stopped from elsewhere (`atlas down`), the client exits with `atlas runner stopped`. If `atlas.yaml` changed since the runner started and the service list differs, attaching refuses with a message to run `atlas down` first.

A TUI that **owns** its services (`atlas dev` with no runner) cannot be attached to: its quit dialog says `keep them running: atlas dev --detach`. Handing a live TUI's services over to a runner would mean re-parenting running processes, so the supported way to keep services alive is to start the runner first.

## Sessions and attach

An action with `session: true` opens its own tab streaming stdout/stderr live. Pressing the action's key again while it is running kills it.

On a session tab the dashboard is read-only except for navigation, scrolling, `f`, `v`, `V`, `M`, `L`, `P`, `?`, `:` and:

| Key | Action |
|---|---|
| `i` | **Attach**: every key is forwarded to the session's stdin (pasted text too) |
| `Ctrl+C` | Kill the session |

While attached, the footer shows an `ATTACHED` bar. Keys are sent raw (`Enter` → newline, `Backspace`, `Esc`, `Tab`, arrows as escape sequences); characters listed in the action's `input_map` are replaced by the mapped string. Two keys are reserved:

| Key | Action |
|---|---|
| `Ctrl+B` | Detach, back to the dashboard (session keeps running) |
| `Ctrl+C` | Kill the session and detach |

When the process exits the tab closes and any `pause_services` are restarted.

## Mobile sessions

An action with `platform: ios|android` first opens a **device picker** (unless `device_id` pins one): `↑`/`↓` or `j`/`k` move, `Enter` selects, `Esc`/`q` cancels. While devices are being detected it shows a spinner; with no device, `Enter` spawns without one. iOS lists booted simulators first, then available ones, plus devices from `xcrun devicectl`; Android uses `adb devices -l`.

Once running, Atlas parses the build output into phases (detecting → rustc → gradle/xcode → install → launch → running, or failed) shown in the **MOBILE** sidebar section and in the footer of the session tab.

The session gets `TAURI_DEV_HOST` and `ATLAS_DEV_HOST` set from the action's [`host_mode`](configuration.md#host_mode): loopback for simulators/emulators, the machine's LAN IPv4 for physical devices (`auto`) or always (`lan`). Values you set yourself are kept.
