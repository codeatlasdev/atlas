# Atlas — notes for coding agents

Rust workspace (`crates/*`: `atlas-cli` = the `atlas` binary, `atlas-tui` = runtime/TUI/MCP/runner, `atlas-host` = `atlas hostd` (PTYs/exec/fs daemon behind the app), `atlas-core`, `atlas-skills`, `atlas-term` = the GPU terminal, `atlas-desktop` = Tauri 2 shell) plus the desktop frontend in `apps/desktop` (SvelteKit 3 + `@codeatlasdev/ui`). Docs are plain markdown in `docs/`; the user-facing agent skills in `skills/` are embedded into the binary at build time.

## Gate and workflow

- `scripts/setup.sh` — the one preflight (rustup, bun, gh, frontend deps, `cargo-tauri`, `cargo-deny`; on Linux it also checks for WebKitGTK and names the packages). Idempotent.
- Linux: the whole workspace builds (Atlas Desktop on WebKitGTK; the native GPU terminal, page stage, capture and agent instances are macOS only, stubbed in `term_stub.rs` / `page_layers_stub.rs`, and terminals use xterm.js). Packaging: `tauri.linux.conf.json` (AppImage, .deb, .rpm, each with the CLI) and `packaging/arch/` (pacman package from the .deb).
- `bun run check` — the only gate: fmt, `clippy --workspace -D warnings`, workspace tests, `cargo deny check`, `scripts/check-skills.sh`, `desktop:check` (svelte-check + prettier + bun test). Run it on its own and read the exit code; never chain `git push`/`release` after it with `;`.
- CI (`.github/workflows/ci.yml`) runs on pull requests and pushes to `main` and `release/**`. A GitHub ruleset ("main and release branches") requires a pull request, 2 human approvals, the six CI jobs plus `Signed-off-by`, and an up-to-date branch on those branches; `v*` tags are immutable. Only maintainers (team `atlas-maintainers`: mvilacad, luancoleto) bypass it, which is how they merge without approvals and how `bun run release` pushes. The 2-approval rule applies to external contributors (write access alone, e.g. Richards0nd, does not make a maintainer). Releases are maintainer-run: `release.yml` is `workflow_dispatch` only and `bun run release <alpha|beta|rc|stable|patch>` builds and publishes it through `act` (see `docs/release.md`). `bun run ci:act` runs the macOS CI job locally through `act`.
- **Before any commit, push, CI fix or release, read `.agents/skills/atlas-workflow/SKILL.md`** (branches, gate, staging, how to read CI, which release channel). The one PR flow (branch `type/short-desc` → draft PR to `main` → CI green → 2 human approvals for external contributors, none for maintainers → a maintainer merges, squash) is in `CONTRIBUTING.md#pull-request-flow-and-who-merges`.
- **Agents never merge a PR, never push to `main`, and never use the ruleset bypass, unless a maintainer (`atlas-maintainers`) is the one running them and says so for that change** (a merge, a release or an emergency hotfix). Working for an external contributor, or when unsure whom you work for, you open a `type/short-desc` branch and a PR, and stop there: you are not one of the two approvals, and an external contributor's PR is merged by a maintainer.
- **Maintainer decision (2026-10-14), standing until he says otherwise: Alpha is the main channel and GitHub's "Latest".** Every release is `bun run release alpha` from `main`; each alpha is published as a normal release and becomes "Latest" (`crates/atlas-core/data/channels.json` `default: alpha` drives it). Do not cut beta/stable, set `make_latest=false`, mark alphas as pre-releases, or flip the default back to Stable unless the maintainer explicitly asks. 1.4.0 binaries follow "Latest" and so move to Alpha on purpose; newer clients that chose Stable still get the real Stable (newest release without a prerelease label).
- Commits need a DCO sign-off (`git commit -s`); see `CONTRIBUTING.md`. Agent-assisted changes follow the AI policy there.
- `bun run dup` — jscpd; keep duplication ≤ 1.5%.
- Toolchain pinned in `rust-toolchain.toml`. Workspace lints apply to every crate; fix warnings, no global `allow`.

## Deploying a local build

- CLI: `cargo build -p atlas-cli && codesign -f -s - target/debug/atlas && cp target/debug/atlas ~/.atlas/bin/atlas`, or `atlas link` (symlink). Verify with `strings ~/.atlas/bin/atlas | grep <version>`.
- Desktop (macOS only): `atlas link --desktop` (debug bundle at `/Applications/Atlas.app`), `bun run desktop:dev` (hot reload), `bun run desktop:build:app` (debug `.app` only; never use `desktop:build -- --debug`, its DMG step mounts an image and opens Finder), `atlas unlink` to go back.

## Debugging Atlas Desktop (agents: `desktop:agent`)

**An agent never shows an Atlas window on the user's desktop, takes his focus, or sends input to it.** Not for a screenshot, not for a recording, not "just for a second". Start your own instance:

```bash
bun run desktop:agent                    # builds the debug app (`bun run desktop:build:app`: `.app` only, never a DMG, which would open Finder on the user's screen) + CLI the first time; same as `atlas desktop --agent`
# atlas-agent ready name=a1 pid=123 ATLAS_DEBUG_SOCK=/private/tmp/atlas-agent-a1/debug.sock
export ATLAS_DEBUG_SOCK=/private/tmp/atlas-agent-a1/debug.sock     # every `atlas desktop debug …` below talks to THIS instance
bun run desktop:agent -- --name pdf --scratch   # own name; --scratch seeds HOME with his desktop settings (never projects/tokens)
bun run desktop:agent -- --stop --name a1       # stop one; --stop alone stops every agent instance
```

Each instance has its own `HOME`, `ATLAS_HOSTD_DIR` and bridge socket under `/tmp/atlas-agent-<name>/`, so any number of agents run side by side and none touches the user's app, hostd or `~/.atlas`. It runs with `ATLAS_AGENT=1` (debug builds only; a release app ignores it): no Dock icon or tray (`Prohibited` activation policy), never frontmost, and its window sits on a private 2x virtual display (`CGVirtualDisplay`, placed right of his monitors, destroyed with the process; `agent_mode.rs`, `display_mac.rs`, `agent_display.rs`). That display is never occluded, so the page runs at the full 60 rAF/s and ScreenCaptureKit can record it. It is retina (DPR 2); `ATLAS_AGENT_SCALE=1` gives a 1x display. The window is shown only through `agent_display::order_front`, which needs the `ShowGranted` proof of `agent_mode::show_decision`: it fails CLOSED unless the frame re-read from AppKit after placement (and `constrainFrameRect:toScreen:` of it) is wholly inside the display this process created and still owns (vendor `0x4147`, the mode asked for, no overlap with any other display), inset by `SAFE_MARGIN` (the window server outsets windows by a point and AppKit moves them out of the menu-bar band). Any doubt hides the window and exits the instance with code 3 and the reason; a 250 ms watchdog (`guard_user_displays`) is the second line. Starts are serialized: `atlas desktop --agent` (and so `desktop:agent`) takes an exclusive `flock` on `<tmp>/atlas-agent.lock` until the window is shown; a second start waits `ATLAS_AGENT_START_WAIT` seconds (default 120) and then fails naming the lock. **Never start two instances at once, and never start one to test display code before `ATLAS_AGENT_DRYRUN=1 atlas desktop --agent` (create the display, place, decide, log, never order in, exit 0) has logged a sane decision.**

The bridge (`crates/atlas-desktop/src/debug.rs`, `debug_req.rs` validates every request; page side `apps/desktop/src/lib/debug/agent.ts` = `window.__atlasDebug`). Everything works while the window is unfocused or hidden-behind, and `click`/`type` are DOM events inside the webview, never OS input:

```bash
atlas desktop debug info                                  # pid, url, visible, window id, agent: true, socket
atlas desktop debug state                                 # projects + service states, worktrees, selection
atlas desktop debug eval 'return await window.__TAURI_INTERNALS__.invoke("projects_list")'
atlas desktop debug logs [--level csp|error] [--since N]  # console, uncaught errors, CSP violations
atlas desktop debug shot /tmp/x.png                       # page snapshot (WKWebView.takeSnapshot, retina, no permission)
atlas desktop debug shot /tmp/x.png --selector 'aside.sidebar'   # one element (first VISIBLE match); also --rect x,y,w,h (CSS px)
atlas desktop debug shot /tmp/x.png --full                # the whole scrollable page/element (view grows for the shot)
atlas desktop debug shot /tmp/x.png --window              # native window as the compositor shows it (Screen Recording permission)
atlas desktop debug record /tmp/x.mov --seconds 5 | --until '[data-intro="done"]' [--fps 30]
atlas desktop debug wait '.ready' [--timeout ms] | wait 'window.__atlas.ws.loaded' --expr
atlas desktop debug click 'button.go' | type 'input' 'text' [--delay ms]
atlas desktop debug a11y [selector]                       # roles, names, nesting; a control without a name is flagged
atlas desktop debug perf --seconds 3                      # rAF/s, p50/p95/max frame ms, long tasks
```

The gesture engine has its own bench: `bun scripts/gesture-bench.ts` replays the trace suite into your agent instance and exits 1 outside the fluidity budget (docs/desktop.md, "Gesture engine, bench and recordings"); `atlas desktop debug gesture-record|gesture-replay` record and replay raw trackpad streams. Debug builds also expose the live stores as `window.__atlas` (`ws`, `store`, `agents`, `worktrees`, `api`, `host`); use them from `debug eval` instead of `import()`, which creates separate module instances. Scripted recordings: `atlas desktop record --script`, `bun studio run <recipe>` (they start their own agent instance). The user's own app, when open, is driven by `atlas wt|term|browser|file` over `~/.atlas/desktop.sock` in every build; a bare `atlas desktop debug …` without `ATLAS_DEBUG_SOCK` talks to a debug build of HIS app: do not do that.

Selection cost (a space switch must stay ≤ 8 ms main-thread stall p95, ≤ 8 ms flush p95, ≤ 120 nodes touched p95): `bun scripts/selection-bench.ts [--save docs/perf/x.json] [--no-check] [--keep]` seeds 13 fixture repos in an agent instance (`--name selbench`), runs A→B→A selections and exits 1 over budget (or when the page has no layers: a crashed app is not a pass). `--judge <report.json>` re-judges a saved report with no app. Baselines: `docs/perf/selection-before.json` / `selection-after.json` (the `before` was measured with a fixed 120 ms window per selection; the loop now waits until the page is quiet, so compare `nodes` only within one loop). WKWebView has no `requestIdleCallback`: deferred UI work goes through `lib/idleQueue.ts` (one item per task, after a quiet window). Kit overlays in non-current layers are wrapped in `lib/components/Overlay.svelte` (placeholder of the trigger's size, built when idle or when the layer becomes current) because the kit mounts a Portal even while closed (codeatlasdev/ui#27).

Prove you did not touch his screen when it matters: list the windows of your pid with `CGWindowListCopyWindowInfo` and check none is on-screen inside a non-virtual display. Check CSP and IPC on the **bundled** build (`tauri://localhost`), not only on Vite.

## Runtime rules that bit before

- Orphan kill uses only the `.pids` file and exact PIDs; never `lsof` port-kill or negative PIDs (port forwarders of container runtimes such as OrbStack own those ports, and a recycled process-group id can hit atlas itself).
- Never restart the container runtime (`orb restart docker`) or run `docker compose down` from atlas.
- Children: `setsid`, `stdin(null)`, `sh -c` for commands.
- TUI: EventStream input, dirty-flag rendering, non-blocking startup.
- `pre_command` output goes to `.logs/setup.log` / `.logs/setup.json`; `pre_command_when: always` is the default with an explicit command.

## Desktop rules that bit before

- Child webviews (`unstable` feature) make the main window multi-webview: use `get_window("main")` for window ops and `get_webview("main")` for the app's own webview, never `get_webview_window`.
- Never put a secret in content loaded into the embedded browser; Design Mode results are read back through `eval_with_callback`.
- After HMR, a store listener registered in `onMount` can survive with stale module instances; reload the webview before trusting socket/ctl behavior.
- Codex runs hooks in its own daemon without the terminal's env: `atlas hook` falls back to the default endpoint and the app matches by agent + cwd.

## Tests

**Tests first, always.** For every bug found or reported, before touching the fix:

1. Write a regression test that reproduces the bug and **watch it fail** for the reported reason.
2. Find the root cause and list edge cases of the same class of mistake (same contract, same pattern elsewhere); add tests for them. They often find a second bug: the `languageLabels` crash in 1.3.0 also uncovered labels deduplicated out of step with `languages`.
3. Fix, then show every new test passing. Name the test after the behavior and say in a comment which incident it guards.

What a test must be (full guide: `.agents/skills/real-tests/SKILL.md`):

- **Real behavior or contract**, not wiring: boundaries, failures, transitions, invariants, precedence. It must go red if the code is plausibly wrong; break the code on purpose to check.
- **Cross-boundary contracts use the real producer output.** Rust → frontend payloads are tested by serializing the real Rust value and checking the keys/shape the TS types read. Never hand-write the other side's JSON in a mock: a camelCase mock hid a snake_case field that crashed every real build.
- No filler: no "does not throw", no tautologies, no tests of mocks echoing mocks, no snapshots of everything, no tests written for coverage.

Manager tests live in `runtime/manager/tests.rs` and are wired by `#[cfg(test)] mod tests;` in `mod.rs` — when splitting modules, check the test count does not drop.
