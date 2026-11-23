# The stack health panel draws only while someone can see it (#289)

Round 2 (`linux-webkit-round2.md`) found that a running stack costs 2.5-3 % of a core on an otherwise still app, and that the JSC sampler put 159 of 177 samples under Svelte `flush` and zrender `update`: the panel redraws five canvases (the resources chart and a CPU and a memory sparkline for each of two services) for every metrics sample, every 3 s. Maintainer decision (2026-11-23): the charts redraw only while the panel is visible and the window focused, and keep the 3 s cadence while it is.

## Change

* `Panel` gets `visible` (its tab is the one on screen, in the live space). A tab behind another stays mounted, so before this its charts kept drawing.
* `stores/windowFocus.svelte.ts`: reactive window focus (an agent instance counts as focused, like the intro globe, so recordings and benches still see motion).
* `panel/liveRev.svelte.ts`: the panel provides a revision that follows `metrics.rev` only while `visible && windowFocus.focused`. `Panel`'s KPIs and timeline, `Charts` and every `CardSpark` derive from it instead of `metrics.rev`, so while hidden they do not recompute at all and take the newest sample, once, when the panel is shown again. The tray still reads `metrics.rev` directly.
* `CardSpark` read `service.name` inside its derivations, and the runner re-sends every row (a new `service` object) between samples, so each re-sent row painted both sparklines again. The name is its own derived value now. A history that did not change (an idle service: 0 % CPU, steady memory, once the 40-point window is full, 2 minutes after Start) hands the chart the array it already drew.

## Tests (`panel/liveCharts.test.ts`: the real `Panel`, `ServiceGrid`, `ServiceCard`, `CardSpark`, `Charts` and metrics store; the kit's canvas charts are a probe that records every draw)

Red before the change, for the stated reason: with the panel behind another tab, three samples drew 3 line charts and 12 sparklines (the visible-cadence test passed, so the probe does see every draw). Green after: nothing while hidden or blurred, one catch-up draw with the newest sample when shown or focused again, and the 3 s cadence (1 line chart + 4 sparklines per sample) while visible. A first draft that gated only the revision still drew the sparklines for every re-sent row (their derivation read `service.name`): the red run that found it. Putting that read back makes "rows re-sent between two samples redraw no sparkline" red; making `read` always return the new array makes the steady-sparkline test red.

## Bench (`scripts/linux-idle-bench.ts`, debug build, GPU display, `stack-running` and two new states)

New scenarios (not gated): `stack-running-behind-tab` (a terminal tab in front of the Panel) and `stack-running-unfocused` (a second window takes the keyboard). An agent instance counts as focused, so for the unfocused rows the flag was removed in the page first (`delete window.__ATLAS_AGENT__`, the `-noagent` rows). 60 s windows, % of one core, whole instance; `before` is `main` (ccfcd42b) built the same way, `after` is the visibility gate only, `after2` adds the `CardSpark` changes. Raw rows (with the 1-minute load before and during every window) are in `stack-panel-hidden-bench.jsonl`.

| State | before: total (web) | after / after2: total (web) | loads (before → during) |
|---|---|---|---|
| panel hidden behind a terminal tab (settle 20 s) | 4.1 (2.3), 4.3 (2.5) | after 1.9 (0.3), 2.5 (0.5, load 2.43 during: marked); after2 2.0 (0.5) | 0.5-1.2 |
| window unfocused, panel visible (settle 20 s) | 3.8 (2.0) | after2 2.1 (0.5) | 0.3-0.5 → 0.2-1.3 |
| panel visible and focused, first minute (settle 20 s) | 4.1 (2.4), 4.0 (2.4) | after 4.8 (2.6, load 2.64: marked), 3.7 (2.0); after2 4.0 (2.1); one after2 row at load 12 before is discarded | 0.3-1.5 |
| panel visible and focused, settled (settle 130 s) | 3.4 (1.7), 3.5 (1.7) | after2 2.9 (1.2), 3.2 (1.3, load 3.06 during: marked) | 0.3-0.8 |

What is left of a hidden or unfocused stack is the stack itself: hostd 0.5, the `atlas` runner 0.7, the two fixture `python3` servers 0.2, the app 0.2-0.3 (1.6-1.7 % in every row), plus the web process at 0.3-0.5. The visible, focused state is unchanged in kind: it still draws every 3 s, by decision, and costs 1.2 % of web once settled (1.7 before). **The "≤ 1 % steady" target is met by the web process in the hidden states (0.3-0.5 %) and not by the total, whose floor on this fixture is the stack's own 1.7 %, nor by the visible state.**

JSC sampler, visible and focused, after2, 30 s (`stack-panel-hidden-profile.txt`): 135 of 150 samples are still under `flush`, most of them in the line chart's `update`. In the first two minutes of a stack the sparklines are painted every sample (the 40-point window is still filling with zeros, so its numbers move); counted with a `clearRect` hook over 30 s at 40-70 s after Start: 10 repaints each for the five canvases, one per 3 s sample.

## Not done

* The visible state's remaining 1.2 % is the resources line chart (100 points, up to 6 series, an axis whose labels move every sample) redrawn at the 3 s cadence the maintainer asked to keep.
* The unfocused path is verified by the unit test and by the bench with the agent flag removed; a real (non-agent) window was not driven, since an agent never takes the user's focus.
* Debug build only. [INFERENCE] the web process runs the same bundled frontend in a release build, so its numbers should carry over; the app process is lighter there.
