---
title: "GPU terminal"
description: "The native Metal terminal in Atlas Desktop: rendering, images, shell integration and performance."
---

# GPU terminal

Atlas Desktop draws local terminals natively instead of with xterm.js (default on; Settings → Appearance → Terminal → **Fast terminal (GPU)**, stored as `atlas.gpuTerminal`, `"0"` turns it off; applies to terminals opened afterwards). It covers terminals of local hostd worktrees (macOS); terminals of SSH-host worktrees keep xterm.js.

## Architecture

```
program ─pty─▶ atlas hostd ──pty_attach_raw (raw bytes)──▶ reader task ─▶ Vt (alacritty_terminal + atlas scanner)
                                                                                  │ snapshot (Frame)
                                                    native view over the pane ◀── render thread (wgpu / Metal)
```

- **`crates/atlas-term`**: no Tauri. `vt.rs` wraps `alacritty_terminal` (the parser and grid also used by Alacritty and Zed) behind one lock; `render.rs` draws a `Frame` snapshot with instanced pipelines (cell backgrounds, images below text, glyphs from a swash atlas, images above text, decorations, cursor, scrollbar); `graphics.rs` decodes inline images; `input.rs` encodes keys.
- **`crates/atlas-desktop/src/term.rs`**: one `NSView` per tab laid over the pane rect, a reader task per terminal and a render thread that sleeps until new bytes, a resize, the cursor blink or the bell flash.
- **hostd raw path**: one socket per terminal carrying the pty's bytes with no JSON or base64. It is bounded: when the terminal falls behind, the program blocks on its pty, as in a native terminal, and hostd memory stays flat. A reader that accepts nothing for 2 s is dropped so a hung client cannot freeze the program.
- **host version**: the raw stream needs the `pty_attach_raw` capability (`info.caps`). The app upgrades an older daemon live on connect (see [Desktop](desktop.md): terminals keep running); a protocol 1 daemon with running terminals cannot be replaced, so those terminals use xterm.js until it restarts, and Settings shows a notice.

## Supported

| Area | Details |
|---|---|
| Text | JetBrains Mono + Nerd Font symbols fallback, synthetic bold/italic, box drawing, min-contrast, theme ANSI palette |
| Decorations | underline, double, curly, dotted, dashed, colored (SGR 58), strikethrough |
| Interaction | selection (char, word, line with triple-click, block with ⌥-drag, ⌘A all), scrollback (10 000 lines) with scrollbar, copy on select (`terminal.copy_on_select`, on) and ⌘C, paste (bracketed), dropping a file pastes its quoted path, OSC 52 copy (program → clipboard: ask by default, `terminal.osc52` allow/ask/deny; reading the clipboard is denied), click on the prompt line moves the cursor (`terminal.click_to_move`), wheel on the alternate screen scrolls the program with arrow keys, ended terminals take no input |
| Search | ⌘F opens a find bar above the terminal: literal or regex (toggle), match case (toggle), Enter / ⇧Enter next / previous with wrap, Esc closes. Every match in the scrollback and screen is highlighted (amber) and the current one is strong (orange); the viewport scrolls to it and the bar shows `n/total`. Matches are recomputed as output arrives |
| Links | OSC 8 hyperlinks and plain `http(s)://` / `file://` URLs (also across soft-wrapped lines; trailing punctuation is trimmed). Hold ⌘ over a URL to underline it, ⌘-click opens it through the same path as the xterm tab (`openPreview`, else the system browser) |
| Live settings | Theme changes recolor open terminals (`term_recolor`: fg, bg, ANSI 16, min contrast). `terminal.font_size` (the renderer rebuilds its font and atlases, then the grid and pty are re-fitted), `terminal.max_scroll_history_lines`, cursor shape/blink, `terminal.osc52`, `terminal.ligatures` and `terminal.minimum_contrast` (WCAG ratio, 1 = off) apply to open terminals (`term_set_prefs`) |
| Grapheme clusters | A cell's zero-width characters (combining marks, VS16, ZWJ, skin-tone modifiers) travel with its base char in the snapshot (`GlyphCell::extra`), with `wide` for double-width cells, so the renderer draws the whole cluster. DECRQM `?2027` reports "always set" |
| Color emoji | RGBA atlas fed by Apple Color Emoji; wide cells fitted into 2 cells; ZWJ, flag, skin-tone, keycap and VS16 clusters shaped to one glyph (swash) |
| Ligatures and sprites | `calt`/`liga` ligatures for JetBrains Mono, on by default (`terminal.ligatures`; runs are same-style adjacent cells, broken by gaps, sprites, symbols and emoji; `Renderer::set_ligatures`). Procedural sprites so cells tile: box drawing U+2500–257F, block elements U+2580–259F, braille U+2800–28FF, sextants U+1FB00–1FB3B, powerline E0B0–E0B7. A per-row layout cache (content hash) makes a cached frame ~90 µs; a cold reshape of 144×44 is ~0.25 ms plain / ~0.4 ms code-heavy (release; `cargo test -p atlas-term --release -- --ignored --nocapture screen_bench`) |
| Shell integration | OSC 133 prompt marks (⌘↑ / ⌘↓ jump between prompts, exit code), OSC 7 cwd (Restart reopens there), OSC 0/2 title, bell (flash + Dock attention when unfocused) |
| Cursor | block, beam, underline (DECSCUSR), blinking, hollow when unfocused |
| Keyboard | xterm encoding; Kitty keyboard protocol flags 1 (disambiguate), 8 (all keys as escapes), 16 (associated text). Press events only. macOS line editing and Ghostty's terminal chords: see [Keyboard](#keyboard) |
| Modes | synchronized output (2026), bracketed paste, application cursor, alternate screen |
| Images | Kitty graphics: transmit/display/put/delete, RGB/RGBA/PNG, zlib, chunked (`m=1`), direct and file/temp-file media, `I=` numbers, `q=` quiet, `C=1`, `z` above/below text. Sixel with HLS/RGB palettes. `CSI 14 t` reports the text area in pixels. Images scroll with the text; alternate-screen images go away with it |

## Keyboard

Modelled on Ghostty's macOS defaults (`ghostty +list-keybinds --default`) and the line-editing keys of Terminal.app and iTerm2. These work with a GPU terminal focused; SSH terminals (xterm.js) keep the classic set.

**Line editing** (sent to the program):

| Keys | Legacy | Kitty protocol on | Does |
|---|---|---|---|
| ⌘⌫ | `^U` | `CSI 127;9u` | delete to line start |
| ⌘← / ⌘→ | `^A` / `^E` | `CSI 1;9D` / `CSI 1;9C` | line start / end |
| ⌥⌫ | `ESC DEL` | `CSI 127;3u` | delete word back |
| ⌥← / ⌥→ | `ESC b` / `ESC f` | `CSI 1;3D` / `CSI 1;3C` | word back / forward |
| ⌥Delete | `ESC d` | `CSI 3;3~` | delete word forward |

With the Kitty keyboard protocol on, the program receives the real key and modifiers (⌘ is super, `8`) and binds it itself. ⇧ or ⌃ with these keys keeps the plain xterm modifier parameters.

**Terminal actions** (handled by Atlas):

| Keys | Does | Ghostty |
|---|---|---|
| ⌘K | clear screen and scrollback, then `^L` so the shell redraws (not on the alternate screen) | `clear_screen` |
| ⌘A | select all (scrollback and screen) | `select_all` |
| ⌘C / ⌘V | copy selection / paste (bracketed) | `copy_to_clipboard` / `paste_from_clipboard` |
| ⌘F | find in scrollback | `start_search` |
| ⌘↑ / ⌘↓, ⌘⇧↑ / ⌘⇧↓ | previous / next prompt (OSC 133) | `jump_to_prompt` |
| ⌘Home / ⌘End | scroll to top / bottom | `scroll_to_top` / `scroll_to_bottom` |
| ⌘PgUp / ⌘PgDn, ⇧PgUp / ⇧PgDn | scroll a page (⇧: primary screen only; on the alternate screen the program gets `CSI 5;2~`) | `scroll_page_up` / `scroll_page_down` |
| ⌘+ / ⌘= / ⌘− / ⌘0 | font size up / down / reset, this terminal only, live | `increase_font_size` / `decrease_font_size` / `reset_font_size` |
| ⌘D / ⌘⇧D | split right / down | `new_split` |

⌘K and ⌘⇧D follow Ghostty only while a terminal has the keyboard focus: ⌘K is the Jump palette everywhere else (also reachable from ⌘⇧P and the Go menu) and ⌘⇧D is Changes. The menu owns those keys, so Atlas takes them off the menu items while a terminal is focused and gives them back on blur.

**Settings** (`settings.json`, Settings → Appearance → Terminal): `terminal.copy_on_select` (default true, Ghostty `copy-on-select`), `terminal.click_to_move` (true, `cursor-click-to-move`), `terminal.option_as_alt` (`both` | `left` | `right` | `none`, default `both`, `macos-option-as-alt`). With ⌥ as Alt, ⌥B sends `ESC b`; with `none` it types the macOS character (∫). ⌥ with arrows, ⌫ and Delete is always Alt: those keys have no macOS character.

**Click to move.** A plain click (no drag, no modifier) on the prompt being edited sends arrow keys that move the shell's cursor there, like Ghostty's `cursor-click-to-move`. It needs OSC 133 (the shell marks the prompt start `A` and the command start `C`/end `D`), so a running command, the alternate screen and a scrolled-up viewport never get arrows. Only the prompt's own lines count (from its first line to the cursor's line).

**Wheel on the alternate screen.** `less`, `vim` and `htop` have no scrollback, so the wheel becomes Up/Down arrows (xterm alternate scroll, DECSET 1007, on by default; honours application cursor mode, capped at 50 per event). A program that turned 1007 off, or asked for mouse reports, gets nothing (mouse reporting is not implemented, below).

**Edit menu.** ⌘C and ⌘V come from the native Edit menu: AppKit sends `copy:` / `paste:`, and the web view fires one `copy` / `paste` event on the terminal's input field, so each runs once. ⌘Z and ⇧⌘Z do nothing in a terminal and never reach the shell.

## Not yet

- Emoji font only on macOS (Apple Color Emoji, loaded lazily from `/System/Library/Fonts`): no bundled or other-OS fallback, so elsewhere emoji fall back to the monochrome Nerd font or nothing. No system fallback for CJK or other scripts. Only the first glyph of a cluster the emoji font cannot join is drawn.
- `alacritty_terminal` measures width per code point, so a ZWJ sequence that spans several wide cells in the grid is not one cell wide even though it is drawn as one glyph.
- Shade blocks U+2591–2593 are flat alpha (no dither).
- Search is limited to what alacritty's DFA engine supports: no look-around, back-references or Unicode `\b` (use `(?-u:\b)`); at most 20 000 matches are kept.
- Kitty graphics shared-memory medium (`t=s`), animation, Unicode placeholders, relative placements; key release events.
- Prompt marks and images are anchored to scrollback lines; once the 10 000-line history is full they drift with each dropped line.
- Mouse reporting (X10/SGR/UTF-8 click, drag and motion reports): clicks never reach the program, so `vim`/`tmux` mouse support, `mouse-shift-capture`, middle-click paste and right-click menus are missing.
- Ghostty bindings not carried over: tabs/windows/splits navigation beyond ⌘D (Atlas has its own pane keys), `write_screen_file`, `scroll_to_selection` (⌘J is the bottom dock), `adjust_selection` (⇧Home/End/arrows to extend a selection), `search_selection` (⌘E is Config), `navigate_search` (⌘G / ⇧⌘G), `toggle_fullscreen`, `inspector`, quick terminal, `mouse-hide-while-typing`, `scroll-to-bottom = keystroke`.

## Performance

Preliminary, single runs. M4 Pro, 144×44 grid, 120 Hz. Atlas: `release` profile built with `CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=true` (needed for the debug bridge; overflow checks on), so shipped builds should be at least this fast. Ghostty 1.x as the reference. The two terminals ran one after the other on the same files and window size.

| Workload | Atlas | Ghostty |
|---|---|---|
| `seq 1 10000000` | 8.20 s | 8.48 s |
| `cat` 1 GiB (text with SGR) | 8.02 s | 12.34 s |
| render work per frame, p50 / p95 / max | 0.22 / 0.29 / 1.6 ms | n/a |
| frame interval p50 / p95 | 8.33 / 8.45 ms | n/a |

vtebench, `--max-secs 3` (median ms per 1 MiB sample, 97–429 samples per cell; lower is better):

| Benchmark | Atlas | Ghostty |
|---|---|---|
| dense_cells | 9 | 8 |
| medium_cells | 8 | 11 |
| sync_medium_cells | 8 | 13 |
| unicode | 7 | 6 |
| scrolling | 18 | 16 |
| scrolling_fullscreen | 25 | 21 |
| scrolling_top_region | 30 | 19 |
| scrolling_top_small_region | 14 | 18 |
| scrolling_bottom_region | 14 | 18 |
| scrolling_bottom_small_region | 14 | 18 |

Parser throughput (`Vt::feed`, 64 KiB chunks) is about 170 MB/s optimized and 14 MB/s in a debug build, so measure with an optimized build.

## Debugging

With a debug build running (`atlas link --desktop`), the debug bridge drives it without input:

```bash
atlas desktop debug eval 'return await window.__TAURI_INTERNALS__.invoke("term_stats")'   # frames, timings, cell size
atlas desktop debug eval 'return await window.__TAURI_INTERNALS__.invoke("term_text",{tab:"<tab id>"})'
atlas desktop debug shot /tmp/term.png
```
