---
title: "Native GPU terminal on Linux: design"
description: "Options, spike measurements, recommendation and phased plan for a native GPU terminal in Atlas Desktop on Linux."
---

# Native GPU terminal on Linux: design

Tracking: [#249](https://github.com/codeatlasdev/atlas/issues/249), Linux phase [#247](https://github.com/codeatlasdev/atlas/issues/247), hidden-terminal GPU release [#254](https://github.com/codeatlasdev/atlas/issues/254), bench setup [#239](https://github.com/codeatlasdev/atlas/issues/239), engine [#252](https://github.com/codeatlasdev/atlas/issues/252) / [#253](https://github.com/codeatlasdev/atlas/issues/253) / [#255](https://github.com/codeatlasdev/atlas/issues/255). Roadmap [#289](https://github.com/codeatlasdev/atlas/issues/289). Background: [terminal.md](terminal.md) (the macOS terminal and the Windows/Linux roadmap) and [perf/linux-webkit-study.md](perf/linux-webkit-study.md) (where a Linux terminal spends CPU today).

This is research, not an implementation. Everything marked **measured** was measured on one machine in one session (2026-11-23); everything marked **[HYPOTHESIS]** was not. The maintainer decides from section 7.

## 1. Answer in ten lines

1. **Wayland needs no GTK4 migration.** A `wl_subsurface` of the GDK Wayland window, placed *below* a transparent WebKitGTK page, carries an `atlas-term` wgpu/Vulkan surface (RADV) today, on the shipped tao 0.37 + wry 0.57 + GTK 3 stack. HTML menus and tooltips draw over the terminal because the page is above it. **Measured, works** (section 4.1).
2. **The X11 child window (#247's plan) has the airspace problem for real**: a native child window covers every HTML overlay, and the page cannot be made to draw over it (section 4.2). It also needs `GDK_BACKEND=x11` for the whole app on a Wayland session.
3. **GTK4/webkitgtk-6.0 is not needed for Wayland and is not ready**: wry's GTK4 port is still an open issue (#1474) with unmerged branches, Tauri's PR is open, and GTK4 drops libappindicator (Atlas's tray). `webkitgtk-6.0` 2.54.1 exists in Arch (section 4.3).
4. **A web-process terminal (WebGPU/WASM) is not viable as the fast path**: `navigator.gpu` is `undefined` in WebKitGTK 2.54.1; WebGL2 works; the VT core compiles to WASM at ~0.8x native speed. It keeps the whole pty → hostd → app → IPC → page pipeline (section 4.4).
5. **The terminal is not slow because of parsing alone.** `Vt::feed` is 88-168 MB/s on this machine (target in docs/terminal.md: 500). xterm.js with large writes is in the same range (75 MB/s ASCII). What costs 170 % CPU today is the transport and the page's share of it (section 3).
6. **The grid write is the bottleneck of `Vt::feed`, not the VT parser or atlas's scanners**: vte alone parses 3.3 GB/s on ASCII, alacritty's `Term` 95 MB/s, `Vt::feed` 88. A 250-sample stack profile puts ~35 % of ASCII/`seq` time in dropping `Option<Arc<CellExtra>>` while overwriting cells (section 5.2).
7. **libghostty-vt is 1.2-8.8x faster on 6 of 8 corpora and 0.58x on unique-style truecolor** (section 5.3). Its Rust crate needs Zig **0.15.2**, not 0.17 as #255 says (0.17.1 fails the build).
8. **Linux pty reads are 4 KiB** whatever buffer you pass, and with output processing on a `seq` flood they were ~160 B (907,000 reads for 143 MB): per-read cost (three scans and two locks per `Vt::feed`) matters more than the 64 KiB benchmark suggests (section 5.4).
9. **Key to presented frame: p50 0.8 ms, p95 2.2 ms** on the subsurface path (79 keys; ends at the wgpu present call, not at photons). Idle: 0.2 % of a core with a one-second clock running.
10. **Recommendation**: stay on GTK3; Wayland via wl_subsurface + wgpu Vulkan with a transparent page and the terminal below; keep xterm.js for X11 sessions, NVIDIA proprietary and any failure; fix the engine (grid write, #252/#253) in parallel because it pays on macOS too (section 6).

## 2. Method and limits

Machine: Ryzen 7 7735HS (16 threads), 5.5 GB RAM shared with other agents (load1 1.0-2.9 during the quiet runs, 5-6 during the first ones; every table says which), Radeon 680M, Mesa 26.2.5 (RADV `REMBRANDT`, radeonsi), WebKitGTK 2.54.1, GTK 3.24.52, sway 1.12, Linux 7.2.15. Rust 1.99.0, release profile with `debug = 1`. Private invisible compositor: `WLR_BACKENDS=headless WLR_RENDERER=gles2 WLR_RENDER_DRM_DEVICE=/dev/dri/renderD128` (the real GPU), private D-Bus with `--config-file` and no servicedirs, `WAYLAND_DISPLAY`/`DISPLAY` unset for the daemon, input only through `wtype`/`grim` on the private seat. `hyprctl clients` listed 1 window before and after.

What was **not** done, so nothing below claims it: the user's real Hyprland (subsurface stacking and `wp_fractional_scale` were verified on sway only), NVIDIA, Intel, X11 sessions other than Xwayland, IME (the preedit path is only sketched in the spike page), accessibility, several terminals or splits in one window, a window drag-resize film (a resize was exercised and the slot followed, not filmed at 60 fps), rendering inside the web process (the WASM spike measured the parser only), and release builds of Atlas Desktop itself (the xterm.js/IPC rows quote the committed `docs/perf/terminal-binary-channel.md`).

Spike code lives in `~/.cache/GpuTermResearch` and is not committed: `vtbench` (parser benches over `atlas_term::vt::Vt`), `ghbench` (libghostty-vt), `subspike` (GTK3 + WebKitGTK + wl_subsurface + wgpu; variants `x11` and `tw` = tao + wry), `vtwasm`, an xterm.js test page, and a corpus generator that reuses `scripts/term-agent-gen.ts`'s `frameBytes` so the agent patterns are the bench's own. `atlas-term` was copied and patched in the scratch tree only: `Renderer::new_raw(RawDisplayHandle, RawWindowHandle, …)` instead of the `NSView` constructor, wgpu `vulkan`+`gles` features, and the alpha mode picked from the surface capabilities.

Corpora (120x40, in `~/.cache/GpuTermResearch/corpus`): `seq` (`1..N` lines, 64 MiB), `ascii` (96-column prose lines, 64 MiB), `sgr` (8 colour runs per line, 32 MiB), `repaint256` (a full-screen repaint, 256-colour fg+bg on every cell with CUP per row inside 2026, the zed#61819 shape, 16 MiB), `truecolor` (the same with a random 24-bit foreground per cell: many unique styles, the "known slow path"), and the three agent patterns `ink-2026`, `ratatui-diff`, `omp-boxes` from `term-agent-gen.ts` (16 MiB each).

## 3. Where the time goes today (xterm.js on Linux)

From `docs/perf/terminal-binary-channel.md` (release, `yes … | head -c 20M` flood loop, 30 s window, GPU display, after #734): **170 %** of a core in total (app 58.7, web 85.1, hostd 26.4), web process 307 MB PSS; llvmpipe 197 %. The web process alone is about one core: it parses with xterm.js and draws with its WebGL addon.

In-page xterm.js 6.0.0 + `@xterm/addon-webgl` 0.19.0 in a plain GTK3 WebKitGTK window (no Tauri, no IPC; the corpus is written straight into `term.write`), 100x30, same machine, load 0.6-1.0. "acked" = 64 KiB chunks, each awaiting `write`'s callback (the pattern `ptyStream.ts` uses); "bulk" = 1 MiB chunks, a callback every fourth:

| corpus | acked: MB/s | acked: web-process CPU per MB | bulk: MB/s | bulk: web CPU per MB |
|---|---|---|---|---|
| ascii (64 MiB) | 8.1 | 48 ms | 75 | 17 ms |
| seq (64 MiB) | 7.2 | 98 ms | 17 | 66 ms |
| sgr (32 MiB) | 8.0 | 64 ms | n/m | n/m |
| truecolor (16 MiB) | 8.3 | 39 ms | 57 | 8 ms |
| ink-2026 (16 MiB) | 8.3 | 42 ms | n/m | n/m |

(n/m = not measured.) The "acked" rate sits near 8 MB/s in every row regardless of content, so it is set by the per-chunk `await` of `write`'s callback (one round trip per 64 KiB), not by the parser [HYPOTHESIS: not profiled]; and shows why the page's chunking and ack policy (`ptyStream.ts`, 64 KiB or 500 ms batches) decides throughput. The "bulk" rows are the parser+renderer ceiling: ASCII 75 MB/s is **not slower than native `Vt::feed` (88)**. `seq` is the outlier (17 MB/s): one-line-per-row output scrolls the grid on every line, which xterm.js's WebGL path pays in the render step. With the WebGL addon removed (DOM renderer) the same ASCII run was 7.8 MB/s acked and 5.3 s of web CPU against 3.2 s with WebGL.

Reading: a native terminal removes the hostd → app → channel → page hop and the web process's second copy of the work, but it does **not** win by parsing ASCII faster than xterm.js. It wins where xterm.js is structurally weak: scroll-heavy output, memory, many terminals, and the page's main thread. Do not promise 500 MB/s from the transport change; that is an engine question (section 5).

## 4. Presentation options

### 4.1 Wayland: wl_subsurface of the GDK window (spike 1, **measured, recommended**)

How: `gdk_wayland_window_get_wl_surface` (the toplevel's `wl_surface`) and `gdk_wayland_display_get_wl_display`, both from `libgdk-3`, wrapped with `wayland-backend`'s `from_foreign_display` and `wayland-client`; bind `wl_compositor` + `wl_subcompositor` + `wp_viewporter` + `wp_fractional_scale_manager_v1`; `create_surface`, `get_subsurface(child, parent)`, `set_desync()`, `place_below(parent)`, an empty input region (so pointer and keys stay with the webview, which owns focus, selection and IME); `wgpu::Instance::create_surface_unsafe` with `RawWindowHandle::Wayland` (child surface) and `RawDisplayHandle::Wayland` (GDK's display). The page reports its slot's `getBoundingClientRect()` (a `ResizeObserver` on a transparent `<div>`) and Rust calls `wl_subsurface.set_position` plus `wp_viewport.set_destination` with it; the swapchain is the slot's size in physical pixels.

What the spike proved (sway gles2 on renderD128, GTK3 window, transparent WebKitGTK page, a `sh` printing `date` every second through `Vt`):

* **It renders**: a 1020x764 RADV Vulkan surface (`PreMultiplied`/`Opaque`, `Fifo`) under the page; renderer creation 18-77 ms with a warm instance (a cold start measured 65 ms).
* **Overlays**: with the terminal *below* the page, an HTML `div` (z-index 10, an opaque menu) opened over the terminal **is drawn over it** (screenshot compared against the *above* order, where the terminal covered the menu). Placing the subsurface *above* the page reproduces the airspace problem exactly; so *below + a transparent page region* is the order that keeps menus, tooltips, the command palette and dialogs working with no hiding logic.
* **The shipped stack works**, not just a hand-made GTK window: the same code on `tao::WindowBuilder` + `wry::WebViewBuilder::with_transparent(true).build_gtk(window.default_vbox())` (the `tw` variant) produced the identical result, using `tao::platform::unix::WindowExtUnix::gtk_window()` to reach the `GdkWindow`.
* **Transparent page**: `window.set_visual(rgba_visual)`, `set_app_paintable(true)`, `webview.set_background_color(0,0,0,0)` (wry does all three for `transparent: true`). The page's slot is `transparent`; everything around it is opaque. The study measured a transparent window as **no CPU cost on sway** (`linux-webkit-study.md`, Knobs); the Linux `transparent_on` rule in `main.rs` says "inferred, not measured" and would need to turn on for the main window.
* **Scale 2 and 1.5**: with `wp_fractional_scale_v1` bound on the **toplevel's** surface (`preferred_scale` = 180 for 1.5, i.e. 1.5) the swapchain is 1.5x the logical slot and the glyphs are sharp (crop compared against the webview's integer-2 buffer). Bound on the **child** surface the event never arrives (the surface has no buffer and no output yet): measured 1,000 ms of waiting and a fallback to GTK's integer `scale_factor()` (2) in both runs, which draws at the wrong size at 1.5. So the scale source is the toplevel.
* **Resize**: the slot message follows the layout (`ResizeObserver`), the grid is re-fitted from the slot's pixel size, and sway's `resize set` to 800x500 and back to 1100x780 produced correct frames. Not filmed.
* **Memory**: the spike process (GTK + WebKitGTK UI process + wgpu device + terminal) 76 MB PSS idle with no terminal, 75-106 MB with one terminal and a flood; the WebKit web process 64-102 MB; GPU memory could not be attributed (system-wide `mem_info_*` moved by ±100 MB with other agents running, so no GPU-memory number is claimed).

Costs and open problems (all **[HYPOTHESIS]** unless stated):

* **Clipping and scrolling.** A subsurface does not obey CSS `overflow`, `border-radius`, `clip-path` or a scrolling ancestor; a terminal inside a scrolling panel, behind a sticky header, or under a rounded modal must be hidden or resized by Rust. The page must report visibility (an `IntersectionObserver` plus an "an overlay covers the slot" signal) the way macOS's `Placement` does.
* **Frame sync with the page.** `set_desync()` lets the terminal commit on its own clock; during a resize the page and the swapchain can disagree for a frame (visible as a gap or an overdraw band). Fix candidates: `set_sync()` with commits on the main thread during layout changes, or draw the slot's background in the page.
* **Compositors.** Verified on wlroots (sway). Hyprland is not wlroots-based and nothing here ran on it; GNOME/Mutter and KDE/KWin need a run each. Hyprland-specific: none known.
* **XWayland sessions and X11** do not have this path at all (section 4.2).
* **The `wl_display` is shared with GDK**: wgpu's Wayland surface code must use GDK's connection (it did, via the raw handle); a second `wl_event_queue` for our objects is required, as `wayland-client`'s `registry_queue_init` provides.
* **Input and IME**: with an empty input region the webview keeps pointer, keyboard and IME. The spike's page used a hidden focused `<textarea>` (the xterm.js model), so IME composition, dead keys and the compositor's key repeat are the page's. The key path measured below went through a GTK key-press handler instead. Both reach the pty; the choice is the same one `termKeys.ts` makes on macOS.

Latency (79 `wtype` keys at 120 ms spacing to a `cat` pty; stamp at GTK `key-press-event`, stop when the next `Renderer::render` returns `Ok(true)` with that key's echo already in the grid; this ends at the wgpu present call, **not at photons**, so add up to one refresh): **p50 0.8 ms, p95 2.2 ms, max 4.7 ms**. The xterm.js equivalent (keydown → `term.write` callback → next `requestAnimationFrame`, 79 keys, local echo with no pty) was **p50 10 ms, p95 17 ms, max 17 ms**; it ends on a frame boundary, so the two numbers are not the same quantity; they bracket "native presents within the same millisecond; the page presents on the next frame". A real key-to-photon figure needs a compositor frame timestamp (sway has none headless) and is a phase 0 item.

Render cost (`Renderer::render` wall time including `get_current_texture` wait, flood of 3 passes): p50 0.48-0.82 ms on ASCII/`seq`/`sgr`, 3.4-3.6 ms p50 on the repaint corpora (every cell changes, 33 frames per 25 MB). Frames per second stayed at the 16 ms floor (`FRAME_INTERVAL`).

### 4.2 X11 / XWayland child window (#247's plan; spike 2, **measured, airspace confirmed**)

How: `GDK_BACKEND=x11` on the private sway's XWayland (`DISPLAY=:2`), a `GtkOverlay` with the webview as its child and a `GtkDrawingArea` overlay; its `GdkWindow` made native (`has_native() == true`, XID `0x400011`), `gdk_x11_window_get_xid`, and `RawWindowHandle::Xlib` + `RawDisplayHandle::Xlib` to wgpu.

Results: wgpu Vulkan on an Xlib surface under XWayland **renders** (the capabilities offered only `PreMultiplied` and `Inherit` alpha modes, not `Opaque`: `Renderer::new` must pick from `caps.alpha_modes`, or the configure fails with a validation error as it did at first). The HTML menu opened at the same slot as in 4.1 **was not visible**: the native X window is composited over the whole GTK toplevel's drawing, so any menu, tooltip, dialog, command palette, toast or dropdown that overlaps a terminal is hidden by it. There is no stacking order in X11 that puts page pixels over a child window of the same toplevel; the options are the macOS approach (hide or shrink the terminal while an overlay is open, with a flash), or WebKit rendering on a separate surface (not available).

Other costs: forcing `GDK_BACKEND=x11` makes the *entire* Atlas window an XWayland client on a Wayland session (blurry at fractional scale, no Wayland input method, no native gestures); on a real X11 session none of that applies. The scale-2 run waited 1,005 ms for a fractional-scale event that cannot exist on X11.

Verdict: X11 is the only place the child-window design applies, it has the airspace problem unsolved, and X11 sessions are a shrinking share. Keep xterm.js there until a user base proves otherwise; do not build the XCB/Xlib path first.

### 4.3 GTK4 / webkitgtk-6.0 (research, nothing measured)

* **Status (read 2026-11-23).** [wry#1474](https://github.com/tauri-apps/wry/issues/1474) is open since January 2025; the maintainers wrote that "no real work has been done on the working group side". Community branches exist (`Ranrar/wry` `gtk4-webkit6`, `gtk4` 0.11 + `webkit6` 0.6, tested on Wayland/Ubuntu 26.04 and an X11 VM; `johncarmack1984`'s tao port [tao#1258](https://github.com/tauri-apps/tao/pull/1258), draft; a muda GTK4 branch; [tauri#14684](https://github.com/tauri-apps/tauri/pull/14684), open). A Tauri app ran on that stack in a contributor's test. The wry port lists breaking changes: an opaque `WebViewHandle`, `with_related_view`, `HardwareAccelerationPolicy` moved, and `RawWindowHandle::Xlib` child embedding is **broken by design** on GTK4 (use `build_gtk(container)`). A maintainer asked for a GTK3+GTK4 feature flag in July 2026 and was overruled; there is no release plan.
* **What it would cost Atlas.** Replace `gtk` 0.18, `webkit2gtk` 2.0, tao, wry, muda, tray-icon and every `with_webview` caller (`linux_gfx.rs`, `webkit` crash handling, title row, tray). **GTK4 drops libappindicator**, so the tray moves to ksni (StatusNotifierItem over D-Bus); Atlas's tray (`tray.rs`) and the `libayatana-appindicator` hard dependency of the Arch package (#419) change. Packaging changes in `tauri.linux.conf.json`, the CI apt line, AppImage bundling and the distro matrix (`verify-linux`). `webkitgtk-6.0` 2.54.1 is in Arch's repo (38 MB download); Ubuntu 24.04 and Debian stable ship older WebKitGTK 6 packages [INFERENCE: not checked per distro].
* **What it would buy the terminal.** Ghostty's current model (issue #14052, merged for Ghostty 1.4, Aug-Sep 2026): the renderer draws off the UI thread and exports each frame as a DMA-BUF; a custom `GtkWidget` turns it into a `GdkDmabufTexture` in `snapshot()`. Not `GtkGLArea` (main-thread GL context; Ghostty dropped it). Ghostty's issue thread documents the costs: NVIDIA drivers lack the usual EGL export path (a CPU-copy fallback was needed), lag from missing GPU sync between the render thread and GTK's context, triple-buffering extra draws.
* **wgpu and DMA-BUF.** `wgpu-hal` 30.0.1 (the version `atlas-term` pins) has `vulkan::Device::texture_from_dmabuf_fd` (**import** only), gated by `Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF`, which RADV on this machine **does expose** (measured with `enumerate_adapters`: `true` on the Vulkan adapter, `false` on the GL one). There is **no export**; #247's "needs raw Vulkan (`VK_EXT_image_drm_format_modifier` + `VK_KHR_external_memory_fd`)" is confirmed for the export side. A wgpu-based DMA-BUF producer therefore needs `ash` calls through `Device::as_hal` to allocate an exportable image, `vkGetMemoryFdKHR`, plus the DRM modifier, and a sync object for GTK. That is a bigger spike than 4.1 and was **not done**.
* **The decisive point**: nothing in 4.1 needs GTK4. GTK4 also exposes the toplevel's `wl_surface` (`gdk_wayland_surface_get_wl_surface`), so a `wl_subsurface` below the page should work there too **[HYPOTHESIS: GTK4's renderer may not leave a transparent hole for a subsurface placed below; not tested]**. GTK4 would give a better integration (a texture that GTK composites and clips, `GdkDmabufTexture`, `GtkGraphicsOffload` only as an optimization with no clipping or overlays), but it is a year-scale dependency on other people's work.

### 4.4 Rendering in the web process (WebGL2 / WebGPU / WASM) (spike 4, **measured: parser only**)

* **WebGPU is not available**: `navigator.gpu` is `undefined` in this WebKitGTK 2.54.1 window (the 2.54 highlights list no WebGPU line; Skia compositor and damage tracking are the graphics news).
* **WebGL2 is available** (`getParameter(UNMASKED_RENDERER_WEBGL)` reports the engine's own string, `Apple GPU`, not the real device). xterm's addon already uses it.
* **WASM with SIMD works** (`WebAssembly.validate` on a SIMD module: true). `SharedArrayBuffer` is `undefined` (no cross-origin isolation); a worker would need COOP/COEP headers on the app's own scheme **[HYPOTHESIS: Tauri can set them; not checked]**.
* **Parser in WASM**: `alacritty_terminal` 0.26 compiled to `wasm32-unknown-unknown` (a vendored copy without its tty/event-loop modules, a no-clock `Timeout`; 156 KB) and fed 64 KiB chunks in the page, best of 3, MB/s: ascii 80, seq 92, sgr 94, truecolor 162, ink-2026 154; with `+simd128` 82, 75, 91, 168, 171. The same `Term` natively (section 5.1): 95, 118, 114, 197, 188. So **WASM is ~0.8x native, and SIMD does nothing for a parser that is bound by grid writes**.
* **Not measured**: a WebGL2 terminal renderer of our own, and the cost of moving bytes from hostd into the page (the existing channel).

Verdict: option (d) keeps the IPC hop and the page's main thread, depends on WebKit's compositor for pixels, and cannot use the GPU outside the web process. Its one merit is that overlays, clipping and scrolling are exactly the page's. It is the right *fallback shape* if the engine moves to Rust (the same `Vt` and glyph code could drive an `OffscreenCanvas`), and the wrong primary path.

### 4.5 Options table

| | A. wl_subsurface + wgpu (4.1) | B. X11 child window (4.2) | C. GTK4 + DMA-BUF widget (4.3) | D. web process WASM/WebGL2 (4.4) | E. xterm.js today |
|---|---|---|---|---|---|
| Works on | Wayland sessions, GTK3 (shipped stack) | X11 or XWayland | Wayland and X11, after the wry port | everywhere WebKit runs | everywhere |
| Measured here | renders, overlays, scale 1/1.5/2, keys, flood, idle | renders; overlay hidden | wgpu-hal DMA-BUF import available; nothing else | parser only (0.8x native) | in-page 75 MB/s ASCII; 170 % flood in app |
| HTML overlays | work (terminal below, page transparent) | **cover-up, unsolved** | work (GTK composites) | work | work |
| Clipping/scroll in the page | Rust must clip/hide | Rust must clip/hide | GTK clips | CSS | CSS |
| Key → present | 0.8 / 2.2 ms (p50/p95, ends at present) | not measured | not measured | page frame (~10 / 17 ms to rAF) | ~10 / 17 ms to rAF |
| New dependencies | wayland-client, wayland-protocols, wayland-backend; wgpu `vulkan` (+`gles`) | an Xlib/XCB handle (wgpu has it); `GDK_BACKEND=x11` | gtk4, webkit6, tao/wry/muda/tray-icon ports, ksni; plus raw Vulkan export | none | none |
| Blocked by | nothing | the airspace problem | wry#1474, tao#1258, tauri#14684 | no GPU path outside WebGL2 | nothing |
| Risk | compositor differences, clipping, NVIDIA/WebKit DMA-BUF (#247's risk note) | blurry XWayland, hide-while-overlay UX | the largest | gain is small | the status quo |

## 5. The VT core (spike 3)

Everything here is single-threaded `feed` of a corpus that is already in memory, 120x40, best of 5 / median, MB/s, release profile, load 1.5-1.7 (`vt-final-64k.txt`, `gh-final-64k.txt`; the earlier loaded runs agreed within ±10 %).

### 5.1 Layers, same corpora

| corpus | `vte` parser, no-op performer | `alacritty_terminal::Term` (parser + grid) | `atlas_term::Vt::feed` | `libghostty-vt` 0.2.2 (`vt_write`, ReleaseFast) |
|---|---|---|---|---|
| seq | 3,426 | 118 | 106 | 167 |
| ascii | 3,297 | 95 | **88** | 771 |
| sgr | 485 | 114 | 94 | 144 |
| repaint256 | 337 | 203 | 150 | 178 |
| truecolor | 319 | 197 | 143 | **83** |
| ink-2026 | 950 | 188 | 168 | 577 |
| ratatui-diff | 902 | 222 | 174 | 765 |
| omp-boxes | 807 | 227 | 169 | 220 |

* **`Vt::feed` vs `Term`**: atlas's scanners (`ClearScan`, `SeqScan` for OSC 133/7/9/777, APC, sixel; `PixelScan` for DECSET 1016) and its locks cost 7-30 %: 10 % on `seq`, 7 % ASCII, 18 % sgr, 26 % repaint256, 27 % truecolor, 11 % ink, 22 % ratatui, 26 % omp. **This is the ceiling of #252** (single pass): up to ~25 % on repaint-shaped traffic, about 7 % on text, consistent with the 5-10 % measured on Windows. 4,095-byte chunks (Linux pty reads) changed `Vt::feed` by under 5 % (seq 104, ascii 88, sgr 93, truecolor 140, ink 158).
* **Snapshot** (`Vt::snapshot` of a full 120x40 truecolor screen into a reused `Frame`): 86-137 µs depending on load (88.7 µs in the quiet run); libghostty-vt's render-state walk of the same screen (`graphemes_len` + fg + bg per cell, no `Frame` building): 61 µs. Not a bottleneck at 60 Hz (0.5 % of a frame).
* The earlier finding of 70-105 MB/s on a Windows laptop is the same range as these (88-174) on this CPU. docs/terminal.md's "about 170 MB/s optimized" for the M4 Pro is the same order. **The 500 MB/s target is out of reach with `alacritty_terminal`'s grid** on any of the machines measured: the cap is the grid write, not the parser.

### 5.2 Where `Vt::feed` spends its time (stack sampling)

`perf` is not installed (`kernel.perf_event_paranoid=2`, no `perf` binary). `gdb` is: a poor-man's sampler stopped `vtbench` (`Vt::feed` only, 40 passes) every ~40 ms and recorded the top frame, 250 samples per corpus (inlined frames attribute to their enclosing function, so read it as a ranking, not exact percentages):

| top frame | seq | ascii |
|---|---|---|
| `drop_glue<Option<Arc<CellExtra>>>` | **84** (34 %) | **91** (36 %) |
| `Row::index_mut` | 16 | 27 |
| `Term::input` / `write_at_cursor` | 29 | 34 |
| `InternalBitFlags::intersects` | 15 | 13 |
| `Storage::compute_index` | 8 | 10 |
| `Row::reset` / `Cell::reset` | 8 | 11 |
| `scan.rs` / `pixel.rs` `position::<u8>` (atlas scanners) | 14 (6 %) | 16 (6 %) |
| `vte` `ground_dispatch` / `print` | 13 | 15 |

The truecolor run produced too few samples to rank (the run is short); its top frames were `vte::Parser::advance` and `advance_csi_param`: parsing SGR with five parameters per cell, as expected.

**Cause (the ranking is measured; the mechanism is read from the type, not proven by a counter)**: writing a character into a cell in alacritty's grid drops the cell's old `extra: Option<Arc<CellExtra>>` (zero-width characters, hyperlink, underline colour) even when it is `None`, through a generic drop glue called for every cell written or reset, and `Row::reset`/scroll reset every cell of every scrolled row. libghostty-vt's cell layout (styles in per-page tables, cells without owned pointers) avoids it [INFERENCE from its documentation and speed, its source was not read]. The scanners are 6 % on text, which agrees with the 7 % (ASCII) gap between `Term` and `Vt::feed` in 5.1.

What that means for the plan: a faster `Vt::feed` on this core is a grid-representation change (alacritty's `Cell` is 24 bytes with the `Arc`), i.e. an upstream patch or a fork of `alacritty_terminal`, or replacing the core (5.3). #252's single pass is worth ≤ 7 % on text and ≤ 27 % on repaints.

### 5.3 libghostty-vt (#255) on Linux

* **Build.** `libghostty-vt` 0.2.2 / `libghostty-vt-sys` 0.2.2 build with `zig build` through `build.rs` (it fetches Ghostty at pinned commit `a887df42…` and requires **Zig 0.15.2**: Zig **0.17.1 fails** with "Your Zig version v0.17.1 does not meet the required build version of v0.15.2"; #255's "Zig 0.17" is wrong for this crate version). **A pitfall that cost 10 minutes**: the crate picks the Zig optimize mode from cargo's `DEBUG`; a profile with `debug = 1` (like `atlas-term`'s bench profile and the release profile used by the desktop's `CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS` builds) silently builds libghostty in Zig `Debug` mode (ASCII `seq` did not finish a 64 MiB pass in minutes). Set `LIBGHOSTTY_VT_SYS_OPTIMIZE=ReleaseFast`. The numbers above are ReleaseFast. Build time here: 55-95 s with 4 jobs (`zig build` of the static library + bindings).
* **Speed.** vs `Vt::feed`: ascii **8.8x**, ratatui-diff 4.4x, ink-2026 3.4x, seq 1.6x, sgr 1.5x, omp-boxes 1.3x, repaint256 1.2x, **truecolor 0.58x**. That last row is the "many unique styles" case Ghostty's maintainer calls a known slow path, and it is the shape of full-screen agent redraws with per-cell colour. The agent patterns (ink, ratatui, omp) are 1.3-4.4x faster, because they repaint mostly-identical text. Ghostty's own 725 MB/s synthetic ASCII claim is within 6 % of the 771 measured.
* **API fit (read, not built into Atlas).** `Terminal`, `RenderState`, `RowIterator`, `CellIterator` are `!Send + !Sync` ("drive all operations from a single thread"); `begin_update`/`end` is the two-phase lock-friendly snapshot. Atlas's `frame.rs`, `pick.rs`, `find.rs`, selection, `images.rs` and `search.rs` are written against alacritty's `Term`/`Grid`; hyperlinks, OSC 133 marks, sixel and the kitty graphics protocol (the crate's `kitty-graphics` feature) would have to be re-plumbed. Gaps to check, none verified: OSC 7/9/777 delivery, DECSET 1016, the 1 s synchronized-update hold (`vt/sync.rs`; Ghostty's own is 1 s), scrollback byte budget, clear-with-history semantics.
* **Cost of adopting.** Zig 0.15.2 in CI on three OSes (Windows and macOS not tried), a pinned git fetch in `build.rs` (network at build time: Atlas builds in sandboxes and for Arch packaging; the `pkg-config` and `link-dynamic` features exist for a prebuilt library), and a young crate (0.2.x, "pre-1.0").
* **Verdict**: a **defer, with a decision to take**: it is the only route to ≥ 3x on agent-TUI traffic without writing our own grid, but it loses on the unique-style repaint and is a cross-platform rewrite of ~3,000 lines of `atlas-term`. Run the full #255 spike (parse + snapshot through an adapter that fills `Frame`) before deciding, and do the cheaper grid-write fix first (below).

### 5.4 Pty read sizes and #253

A `cat` of the corpus into a pty master on this kernel (output processing off, `os.read(fd, 64 KiB)`; `ptysz.py` in the scratch tree): **ASCII 67 MB in 0.27 s = 249 MB/s, 12,209 reads, mean 5,497 B, 86 % of reads exactly 4,095 B**; truecolor the same (mean 5,453, 87 % 4,095); a 1 MiB buffer changed nothing (mean 4,467). The Linux pty hands out one 4 KiB tty buffer at a time; the 64 KiB read in `lifecycle.rs::spawn_reader` is not a 64 KiB read.

With the spike's real configuration (output processing on, `cat` writing to the pty slave, reader thread → `Vt::feed` → render thread, `PASSES=2-3`): ASCII 187 MB in ~2.75 s = **68 MB/s**, 28,968 reads (6.4 KB each), reader thread 2.59 s CPU (13.9 ms/MB); `seq` 143 MB at **34 MB/s**, **907,476 reads (157 B each)**, 22.2 ms/MB; `sgr` 52 MB at 69 MB/s. The render thread used 0.14 s for 181 frames. **So the `seq` case is limited by per-read overhead, not by parsing** (cat's writes through the tty line discipline arrive in tiny pieces; Atlas's reader takes the first available chunk and takes three scan passes and two mutexes per call). Measured here as a fact; the cause of the tiny chunks (line discipline vs scheduling) is **[HYPOTHESIS]**.

Consequences for #253: (a) on Linux the producer side is 250 MB/s at 4 KiB reads and the consumer is 90-170 MB/s, so the consumer is the bottleneck and a parse thread overlaps only the read syscall (2.4 ms/MB of 11 ms/MB, ≤ 20 %); (b) **coalescing small reads before `feed`** (one lock and one scan pass per ~64 KiB or per 1 ms) helps tiny-chunk floods more than the thread does; (c) the first short read must still go straight through so typing latency does not change.

## 6. Recommendation and phased plan (acceptance numbers are proposals)

**Architecture**: keep GTK3/webkit2gtk-4.1 and tao/wry as shipped. Linux Wayland sessions get a native terminal through a **wl_subsurface placed below a transparent page region**, rendering with `atlas-term`'s wgpu renderer on **Vulkan** (RADV/ANV; the GL backend only as a probe fallback, behind the existing watchdog). X11 sessions, XWayland (`GDK_BACKEND=x11`), NVIDIA proprietary and any adapter failure stay on xterm.js; the frontend gate `gpuTerminalSupported()` becomes a Rust-decided capability (session type, backend, adapter) rather than `platform === "macos"`. The engine work (grid write, coalescing) is shared with macOS and Windows and proceeds independently.

One PR per phase, each with acceptance numbers measured with the phase 0 bench setup on this class of machine (Ryzen/Radeon iGPU, Wayland, load < 2):

| Phase | PR | What | Acceptance (numbers to beat/hold) |
|---|---|---|---|
| 0 | `test(bench): terminal corpora and parser bench on Linux` (#239 Linux row) | commit the corpus generator (reusing `term-agent-gen.ts`), a `#[ignore]` parser bench in `atlas-term` (`seq`, `ascii`, `sgr`, `repaint256`, `truecolor`, agent patterns; best/median), a pty read-size probe, `docs/perf/terminal-linux-baseline.json` with machine/driver/build, and a key-to-present stamp so phase 3 has a baseline | table in section 5.1 reproduced within ±10 %; xterm.js in-page row of section 3 reproduced; `hyprctl`/compositor frame-timestamp method for key-to-photon decided and written down |
| 1 | `perf(term): grid write without per-cell drop` (engine, #252 folded in) | remove the `Option<Arc<CellExtra>>` drop from the hot path (a thin patch/fork of alacritty's `Cell`/`Row`, or a `ManuallyDrop` fast path when `extra.is_none()`), one scan pass over the bytes, `feed` takes whole reads | `Vt::feed` ≥ 150 MB/s ASCII and ≥ 140 MB/s `seq` (from 88 / 106), no regression on truecolor (143) or sgr (94), all `agent_frames` and `vt/tests.rs` tests unchanged; macOS `cat` 1 GiB not slower |
| 2 | `perf(term): coalesce reads, parse off the reader thread` (#253) | gather small reads (4 KiB on Linux) into one `feed` per ~64 KiB or 1 ms, then a parse thread behind a bounded ring; the first short read goes straight through | `seq` flood in the phase-0 spike config ≥ 100 MB/s (from 34) and ASCII ≥ 120 MB/s (from 68); key echo p95 unchanged (≤ 1 frame + 2 ms) |
| 3 | `feat(desktop): Linux wl_subsurface terminal behind a capability` (#247, Wayland) | `term/linux.rs`: subsurface below the page, wgpu Vulkan surface, slot sync, park/hide, fractional scale from the toplevel surface, `Renderer::new_raw` taking raw handles, alpha mode from caps, font dirs + fontconfig, `term_stub.rs` split so Linux has real commands, main window transparent on Wayland | scale 1, 1.5, 2 sharp and the right size; a menu, tooltip and the palette drawn over a running terminal (screenshot test on the private sway); resize 60 fps with no gap or overdraw frame (film); key → present p95 ≤ 4 ms in-process; 4 agent TUIs at 30 redraws/s < 5 % total CPU (the #239 target is 3 %); idle terminal ≤ 0.5 % of a core; `terminal-output` flood app + web + hostd < 100 % (today 170) |
| 4 | `perf(term): release GPU of hidden terminals` (#254) | policy in an ungated module (macOS plan from `batch3-plan.md`), then drop the Vulkan surface, buffers and atlases of parked terminals after T and rebuild on show | 20 terminals, 1 visible: GPU/RSS before and after measured with `Renderer::gpu_bytes()` + PSS; rebuild-on-show < 16 ms with no visible flash (flicker sweep, Linux variant) |
| 5 | `feat(term): IME, clipboard and a11y on Linux` | text-input through the page's focused element or `zwp_text_input_v3`; primary selection; AT-SPI | `fcitx5`/`ibus` compose, dead keys, copy/paste round trip, screen reader announces output: each as a test with the real producer |
| 6 | `spike(term): libghostty-vt behind vt/` (#255) | the full spike with a `Frame` adapter, Zig 0.15.2 in CI, `LIBGHOSTTY_VT_SYS_OPTIMIZE=ReleaseFast`, the OSC/DECSET/sixel gaps listed | decision write-up with ≥ 2x on agent patterns and no worse than 0.8x on truecolor, or drop |

Phases 1, 2, 4 and 6 are independent of 3 and also pay on macOS; phase 3 depends on 0 only. Default-on needs phases 3 and 5 and one release of opt-in (`Settings → Terminal → Fast terminal`, already `atlas.gpuTerminal`).

## 7. Decisions only the maintainer can make

1. **GTK3 subsurface or GTK4 migration** for Wayland. Recommendation: GTK3 now (it is measured working and shippable in weeks, and the GTK4 port is unmerged upstream); revisit when wry#1474 lands. The subsurface code is mostly reusable on GTK4 **[HYPOTHESIS]**.
2. **Overlay strategy**: *terminal below a transparent slot* (recommended; overlays just work; costs a transparent main window on Linux, which the study measured as free on sway only) versus *terminal above and hide/shrink while an overlay is open* (the X11 and macOS-child-view approach; flashes, needs an overlay registry in the page).
3. **wgpu backend**: Vulkan only (recommended: RADV/ANV work; smallest dependency surface) or Vulkan with a GL fallback (adds the GL/EGL path measured available here, a bigger test matrix) or neither on NVIDIA proprietary (xterm.js). Also whether Linux ships `vulkan` in the same feature list as Metal (`atlas-term` builds `metal` only today; tests that skip on Linux would start running).
4. **X11 and XWayland**: stay on xterm.js (recommended) or build the hide-while-overlay child-window path later.
5. **Engine strategy**: patch/fork `alacritty_terminal`'s grid (phase 1) versus adopt libghostty-vt (phase 6) versus both in order (recommended). Adopting needs a Zig toolchain and a network fetch at build time in every build (or a vendored/pre-built library), on three OSes.
6. **Which numbers become CI gates**: the Linux idle gate already runs on llvmpipe (`linux-idle-gate.sh`); a GPU terminal cannot be gated on a runner without a GPU. Recommendation: parser throughput gated in CI (CPU only, relative to a recorded baseline), GPU and latency numbers local with machine and driver recorded in `docs/perf/`.
7. **Target figures**: the roadmap's "parse ≥ 500 MB/s" is not reachable with the current core (≤ 170 MB/s here; libghostty-vt 770 on ASCII, 83 on unique-style repaints). Confirm or replace it with per-corpus floors (phase 0 proposes them).
8. **Dependencies**: `wayland-client`, `wayland-protocols`, `wayland-backend` (client_system) for phase 3, each reviewed under `docs/supply-chain.md`.

## 8. Raw spike output

Parser benches (`vtbench`, 64 KiB chunks, best/median of 5, load 1.6): `seq 106/102, ascii 88/88, sgr 94/93, repaint256 150/148, truecolor 143/142, ink-2026 168/153, ratatui-diff 174/172, omp-boxes 169/166`; `Term` alone `118/109, 95/94, 114/112, 203/202, 197/196, 188/186, 222/221, 227/213`; `vte` no-op `3426, 3297, 485, 337, 319, 950, 902, 807`; `libghostty-vt` `167/164, 771/756, 144/143, 178/177, 83/82, 577/572, 765/752, 220/219`; snapshot 85.6 µs; ghostty render-state walk 61.2 µs.

Subsurface spike floods (`FLOOD=…`, 2-3 passes of the corpus, a 64 KiB read buffer): ascii 187 MB, 68 MB/s, self CPU 2.82 s (15.1 ms/MB), 28,968 reads, 181 frames, render p50/p95/max 716/2,879/5,761 µs; truecolor 25 MB in 0.25 s, 100 MB/s, 12.1 ms/MB, 33 frames, render p50 3.6 ms; ink-2026 25 MB, 104 MB/s, 11.4 ms/MB, 32 frames; seq 143 MB, 34 MB/s, 22.2 ms/MB, 907,476 reads; sgr 52 MB, 69 MB/s, 13.9 ms/MB. Idle after the flood (the page and a 1 s `date` loop running): 0.20 % of a core over 10 s, in every run.

xterm.js 6.0.0 + addon-webgl 0.19.0 in-page (section 3): acked: ascii 8.14 MB/s (web CPU 3.22 s of 67 MB), sgr 7.96 (2.16 s of 33.5 MB), truecolor 8.33 (0.65 s of 16.8 MB), ink-2026 8.33 (0.71 s of 16.8 MB), seq 7.15 (6.56 s of 67 MB); bulk 1 MiB: ascii 75.0, seq 17.1, truecolor 56.8. Web PSS 158-364 MB (seq the largest). Key echo p50/p95/max 10/17/17 ms (n = 79).

WASM `alacritty_terminal` (best of 3, MB/s): nosimd seq 92, ascii 80, sgr 94, truecolor 162, ink-2026 154; simd seq 75, ascii 82, sgr 91, truecolor 168, ink-2026 171.

Subsurface key latency: n = 79, p50 0.8, p95 2.2, max 4.7 ms. Fractional scale: `preferred_scale` 1,500 milli on the toplevel surface at output scale 1.5 (swapchain 1170x846 for a 780x564 logical slot); the same request on the child surface: no event in 1,005 ms, fallback 2.
