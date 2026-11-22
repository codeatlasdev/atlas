# The debug bridge, benches and recordings

Moved verbatim from the old root `AGENTS.md` ("Debugging Atlas Desktop"). Every command here talks to YOUR instance through `ATLAS_DEBUG_SOCK` (see [SKILL.md](SKILL.md)).

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
atlas desktop debug record /tmp/x.mov --seconds 5 | --until '[data-testid="welcome-up"]' [--fps 30]   # Linux: grim frames through ffmpeg (needs ffmpeg), H.264
atlas desktop debug wait '.ready' [--timeout ms] | wait 'window.__atlas.ws.loaded' --expr
atlas desktop debug click 'button.go' | type 'input' 'text' [--delay ms]
atlas desktop debug resize 1100 720                       # main window, logical px; the flicker sweep resizes mid-recording
atlas desktop debug a11y [selector]                       # roles, names, nesting; a control without a name is flagged
atlas desktop debug perf --seconds 3                      # rAF/s, p50/p95/max frame ms, long tasks
```

The gesture engine has its own bench: `bun scripts/gesture-bench.ts` replays the trace suite into your agent instance and exits 1 outside the fluidity budget (docs/desktop.md, "Gesture engine, bench and recordings"); `atlas desktop debug gesture-record|gesture-replay` record and replay raw trackpad streams. Flicker: `bun scripts/flicker-sweep.ts` films every surface at 60 fps in your agent instance (`--name sweep`) and exits 1 on any blip, blank frame or flashing (docs/desktop.md, "Flicker sweep"); run it alone before a release. Debug builds also expose the live stores as `window.__atlas` (`ws`, `store`, `agents`, `worktrees`, `api`, `host`); use them from `debug eval` instead of `import()`, which creates separate module instances. Scripted recordings: `atlas desktop record --script`, `bun studio run <recipe>` (they start their own agent instance). The user's own app, when open, is driven by `atlas wt|term|browser|file` over `~/.atlas/desktop.sock` in every build; a bare `atlas desktop debug …` without `ATLAS_DEBUG_SOCK` talks to a debug build of HIS app: do not do that.

Selection cost (a space switch must stay ≤ 8 ms main-thread stall p95, ≤ 8 ms flush p95, ≤ 120 nodes touched p95): `bun scripts/selection-bench.ts [--save docs/perf/x.json] [--no-check] [--keep]` seeds 13 fixture repos in an agent instance (`--name selbench`), runs A→B→A selections and exits 1 over budget (or when the page has no layers: a crashed app is not a pass). `--judge <report.json>` re-judges a saved report with no app. Baselines: `docs/perf/selection-before.json` / `selection-after.json` (the `before` was measured with a fixed 120 ms window per selection; the loop now waits until the page is quiet, so compare `nodes` only within one loop). WKWebView has no `requestIdleCallback`. Kit overlays need no deferral: since `@codeatlasdev/ui` 2.2 a closed one renders nothing (codeatlasdev/ui#27; `lib/components/kitOverlays.test.ts` runs the real kit and goes red if that regresses).
