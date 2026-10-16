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
| Text | `terminal.font_family` (installed system fonts, first installed family of the list wins, variable fonts included; none installed = bundled JetBrains Mono) + Nerd Font symbols fallback, `terminal.font_thicken`, synthetic bold/italic, box drawing, min-contrast, theme ANSI palette |
| Decorations | underline, double, curly, dotted, dashed, colored (SGR 58), strikethrough |
| Interaction | selection (char, word, line with triple-click, block with ⌥-drag, ⌘A all), scrollback (10 000 lines) with scrollbar, copy on select (`terminal.copy_on_select`, on) and ⌘C, paste (bracketed), dropping a file pastes its quoted path, OSC 52 copy (program → clipboard: ask by default, `terminal.osc52` allow/ask/deny; reading the clipboard is denied), click on the prompt line moves the cursor (`terminal.click_to_move`), wheel on the alternate screen scrolls the program with arrow keys, ended terminals take no input |
| Search | ⌘F opens a find bar above the terminal: literal or regex (toggle), match case (toggle), Enter / ⇧Enter next / previous with wrap, Esc closes. Every match in the scrollback and screen is highlighted (amber) and the current one is strong (orange); the viewport scrolls to it and the bar shows `n/total`. Matches are recomputed as output arrives |
| Links | OSC 8 hyperlinks and plain `http(s)://` / `file://` URLs (also across soft-wrapped lines; trailing punctuation is trimmed). Hold ⌘ over a URL to underline it, ⌘-click opens it through the same path as the xterm tab (`openPreview`, else the system browser). ⌘-click on `path:line:col` opens the file in the editor |
| Live settings | Theme changes recolor open terminals (`term_recolor`: fg, bg, cursor, cursor text, selection bg/fg, ANSI 16, min contrast). `terminal.font_size`, `terminal.font_family` and `terminal.font_thicken` (the renderer rebuilds its font and atlases, then the grid and pty are re-fitted), `terminal.max_scroll_history_lines`, cursor shape/blink, `terminal.osc52`, `terminal.ligatures` and `terminal.minimum_contrast` (WCAG ratio, 1 = off) apply to open terminals (`term_set_prefs`). `terminal.padding_x` / `terminal.padding_y` move the view's rect |
| Grapheme clusters | A cell's zero-width characters (combining marks, VS16, ZWJ, skin-tone modifiers) travel with its base char in the snapshot (`GlyphCell::extra`), with `wide` for double-width cells, so the renderer draws the whole cluster. DECRQM `?2027` reports "always set" |
| Color emoji | RGBA atlas fed by Apple Color Emoji; wide cells fitted into 2 cells; ZWJ, flag, skin-tone, keycap and VS16 clusters shaped to one glyph (swash) |
| Ligatures and sprites | `calt`/`liga` ligatures for JetBrains Mono, on by default (`terminal.ligatures`; runs are same-style adjacent cells, broken by gaps, sprites, symbols and emoji; `Renderer::set_ligatures`). Procedural sprites so cells tile: box drawing U+2500–257F, block elements U+2580–259F, braille U+2800–28FF, sextants U+1FB00–1FB3B, powerline E0B0–E0B7. A per-row layout cache (content hash) makes a cached frame ~90 µs; a cold reshape of 144×44 is ~0.25 ms plain / ~0.4 ms code-heavy (release; `cargo test -p atlas-term --release -- --ignored --nocapture screen_bench`) |
| Shell integration | OSC 133 prompt marks (⌘↑ / ⌘↓ jump between prompts, exit code, command output bounds), OSC 7 cwd (Restart reopens there), OSC 0/2 title, OSC 9 / 777 notifications, bell (flash + Dock attention when unfocused) |
| Cursor | block, beam, underline (DECSCUSR), blinking, hollow when unfocused |
| Colors | Foreground, background, cursor, cursor text, selection and the ANSI 16 come from the terminal theme (the app theme, or the one picked under Settings → Appearance → **Terminal theme**: `terminal.cursor`, `terminal.cursor_text`, `terminal.selection.*`, `terminal.ansi.*` of the theme file). A theme that leaves a color out gets one derived from the palette |
| Font and padding | `terminal.font_family` is a CSS list (`"Fira Code", Menlo, monospace`): quotes and generic keywords (`monospace`, `ui-monospace`, `system-ui`, `sans-serif`, `serif`) are dropped and the first installed family is used. When the first one is missing, Atlas shows a toast once (`Font "Fira Code" is not installed for the fast terminal; using JetBrains Mono.`, event `term://font`). `terminal.padding_x` / `terminal.padding_y` (pt, 0–64, default 10 and 6) inset the terminal inside its pane; the margin is painted with the terminal background (Ghostty `window-padding-color = extend`) |
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
| ⌘G / ⌘⇧G | next / previous match of the open find bar (opens it when closed) | `navigate_search` |
| ⌘⇧Y | select (and copy) the last command's output, again = the one before | none (Atlas) |

⌘K and ⌘⇧D follow Ghostty only while a terminal has the keyboard focus: ⌘K is the Jump palette everywhere else (also reachable from ⌘⇧P and the Go menu) and ⌘⇧D is Changes. Both the native menu and the window's key handler (`terminalKeeps`) hand the key to the terminal while one is focused; before 1.5.0 the key handler still sent ⌘K to the Jump palette.

**Settings** (`settings.json`, Settings → Appearance → Terminal): `terminal.copy_on_select` (default true, Ghostty `copy-on-select`), `terminal.click_to_move` (true, `cursor-click-to-move`), `terminal.option_as_alt` (`both` | `left` | `right` | `none`, default `both`, `macos-option-as-alt`). With ⌥ as Alt, ⌥B sends `ESC b`; with `none` it types the macOS character (∫). ⌥ with arrows, ⌫ and Delete is always Alt: those keys have no macOS character.

**Click to move.** A plain click (no drag, no modifier) on the prompt being edited sends arrow keys that move the shell's cursor there, like Ghostty's `cursor-click-to-move`. It needs OSC 133 (the shell marks the prompt start `A` and the command start `C`/end `D`), so a running command, the alternate screen and a scrolled-up viewport never get arrows. Only the prompt's own lines count (from its first line to the cursor's line).

**Wheel on the alternate screen.** `less`, `vim` and `htop` have no scrollback, so the wheel becomes Up/Down arrows (xterm alternate scroll, DECSET 1007, on by default; honours application cursor mode, capped at 50 per event). A program that turned 1007 off gets nothing. A program that asked for mouse reports gets them instead (next paragraph).

**Mouse reporting.** A program that turns on DECSET 1000 / 1002 / 1003 (vim, tmux, htop, agent TUIs) owns the mouse: press, release, drag (1002/1003) and wheel notches reach it as X10, UTF-8 (1005) or SGR (1006) reports, with ⇧ / ⌥ / ⌃ in the button bits. ⇧ (Ghostty `mouse-shift-capture = false`) and ⌘ keep the click for Atlas, so ⇧-drag still selects and ⌘-click still opens links. Reports past what the legacy encoding can address (column or row above 222) are dropped rather than corrupted. `crates/atlas-term/src/mouse.rs` (bytes), `crates/atlas-desktop/src/term/mouse.rs` (commands), `terminal/pointer.ts` + `mouseRoute.ts` (routing).

**Notifications.** OSC 9 (`ESC ] 9 ; text`) and OSC 777 (`ESC ] 777 ; notify ; title ; body`) raise a desktop notification, the way Ghostty does; use them from a script or agent hook (`printf '\e]9;done\a'`) to be told when it finishes. The notice lands in the notifications inbox and the worktree's unread dot like an agent finishing, and shows a system banner when Atlas is in the background or another tab is showing (Settings → Notifications: enabled, sound, skip when focused). ConEmu progress (`OSC 9;4;…`) is ignored; control characters are stripped and the body is capped at 120 characters.

**⌘-click on files.** ⌘-click on `path`, `path:line` or `path:line:col` (also `path(line,col)`) in the output opens the file in the editor at that position when it is inside the worktree. A relative path resolves against the terminal's current directory (OSC 7), `..` may not leave the worktree, and anything else (URLs go to the browser, `~/…`, other directories, `12:34:56`) does nothing. The path is not underlined on ⌘-hover yet, only URLs are.

**Select command output.** ⇧⌘Y selects the output of the last finished command (between OSC 133 `C` and `D`, never the prompt or command line) and copies it; pressing it again walks back to the command before. A command that printed nothing is skipped, and so is one still running. Not on the alternate screen.

**Edit menu.** ⌘C and ⌘V come from the native Edit menu: AppKit sends `copy:` / `paste:`, and the web view fires one `copy` / `paste` event on the terminal's input field, so each runs once. ⌘Z and ⇧⌘Z do nothing in a terminal and never reach the shell.

### Paste

The web view's `paste` event only exposes text, so ⌘V is a trigger and the host reads the macOS pasteboard itself (`term_paste_clipboard`, decided by `atlas_term::input::paste_clipboard`). What it holds decides the bytes, with the precedence Ghostty uses (Finder also puts a file's name and a generated icon bitmap next to its URL, so files win over image and text):

| Pasteboard | Sent |
| --- | --- |
| Files (Finder ⌘C) | their paths, shell-quoted (`shlex.quote`), space separated |
| Text | the text (bracketed when the program asked; a pasted `ESC[201~` is dropped) |
| Image, no text (screenshot tools, "Copy Image") | bracketed paste on: an empty bracketed paste (`ESC[200~ ESC[201~`, what iTerm2 and Terminal.app send; omp reads the image itself on it). Off: Ctrl+V (`0x16`, or `CSI 118;5u` under the Kitty keyboard protocol), the key omp, Claude Code and Codex bind to "paste image" |
| Empty | nothing |

A terminal cannot carry a bitmap, so for an image the program reads the clipboard itself; Atlas only tells it to. Dropping a file on the terminal still pastes its quoted path. Terminals that use xterm.js (SSH hosts, or when Fast terminal is off) paste text only.

**Clipboard keys in xterm.js terminals (Linux, SSH hosts).** Ctrl+Shift+C copies the selection and Ctrl+Shift+V pastes it (bracketed); Ctrl+C copies only while text is selected and otherwise stays SIGINT. With `terminal.copy_on_select` (on) selecting also copies. The PRIMARY selection (middle-click paste) is not filled. macOS keeps ⌘C/⌘V through the Edit menu.

## Compared with Ghostty

Audit against Ghostty's documented features (ghostty.org/docs/config/reference and keybind actions). **present** = behaves as Ghostty, **partial** = some of it, **missing** = not there. Evidence is the code that does it; the proof of each is a test in the same crate.

| Ghostty feature | Status | Evidence / what is missing |
|---|---|---|
| Ligatures (`font-feature`) | present | `font/layout.rs`, `terminal.ligatures`; `calt`/`liga` only, no free-form `font-feature` list |
| `font-thicken` | present | `terminal.font_thicken`, `render/mod.rs` |
| Variable fonts (`font-variation`) | partial | `font/system.rs` drives `wght` / `ital` / `slnt` from bold / italic; no per-axis setting |
| `font-family`, fallback | partial | `font/system.rs`; Nerd symbols and emoji fall back, no system fallback for CJK or other scripts |
| Cursor styles, blink, `cursor-style-blink` | present | `vt/mod.rs::set_options`, DECSCUSR, hollow when unfocused |
| `macos-option-as-alt` | present | `termKeys.ts::optionActsAsAlt`, `terminal.option_as_alt` |
| macOS keys ⌘K, ⌘←/→, ⌥←/→, ⌘⌫, ⌥⌫ | present | `input.rs` (`macos_line_editing_map_*` tests); ⌘K needed the window key handler fixed (`terminalKeeps`) |
| ⌘A / ⌘C / ⌘V, font size, ⌘Home/End, ⌘PgUp/PgDn | present | `termKeys.ts`, `term/keys.rs` |
| `copy-on-select` | present | `terminal.copy_on_select`, `term/select.rs` |
| Scrollback search ⌘F, `navigate_search` ⌘G / ⇧⌘G | present | `search.rs`, `TermSearchBar.svelte`; ⌘G is new |
| `start_search` selection, `search_selection` | missing | ⌘E is Config in Atlas |
| Shell integration: OSC 133 prompt marks, `jump_to_prompt` | present | `vt/shell.rs` |
| Select command output | present (Atlas) | `vt/shell.rs::select_last_output`, ⇧⌘Y; Ghostty has no default key |
| OSC 7 cwd for Restart | present | `term://cwd`, `GpuTerminal.svelte` |
| OSC 7 cwd for new tabs and splits | partial | `workspace.svelte.ts::newTerminal` always starts in the worktree root; a split moves an existing tab, so neither inherits the focused terminal's directory |
| OSC 8 hyperlinks, plain URLs, ⌘-click | present | `vt/find.rs::url_at`, `pointer.ts` |
| ⌘-click on file paths (`path:line:col`) | present (Atlas) | `search.rs::token_at`, `termPath.ts`; no ⌘-hover underline |
| OSC 52 write | present | `terminal.osc52` allow / ask / deny; reading is denied |
| Kitty keyboard protocol | partial | `input.rs`: flags 1, 8, 16, press events only; no release events (flag 2), no alternate keys (4) |
| Kitty graphics, Sixel | partial | `graphics/`; no shared memory, animation, Unicode placeholders |
| Quick terminal | missing | needs a global shortcut and a dropdown window (`quick-terminal-*`) |
| Splits, resize | partial | Atlas panes (`pane::SplitRight/Down`, ⌘D from the terminal); not Ghostty's per-split navigation and equalize actions |
| Mouse reporting (1000/1002/1003, 1005/1006), `mouse-shift-capture` | partial | `mouse.rs` (new); bare motion without a button, focus events and pixel mode are missing |
| Middle-click paste, right-click menu | missing | `pointer.ts` handles the left button only for selection |
| Bell | present | `VtEvent::Bell`: flash plus Dock attention |
| Desktop notifications OSC 9 / OSC 777 | present | `vt/shell.rs::notify`, `termNotice.ts`, `agents.svelte.ts` (new); no `desktop-notifications` / `bell-features` settings, Atlas's notification settings apply |
| Title (OSC 0/2) | present | `pty_title` from hostd, `Terminal` tab label |
| Unicode and emoji width, grapheme clusters (mode 2027) | partial | `vt/frame.rs`; width is per code point in `alacritty_terminal`, a ZWJ sequence spans several cells |
| Performance on large output | present | see [Performance](#performance): `cat` 1 GiB 8.0 s vs 12.3 s |
| `minimum-contrast` | present | `terminal.minimum_contrast`, `colors.rs` |
| `window-padding-x/y`, `window-padding-color = extend` | present | `terminal.padding_x/y`; no `window-padding-balance` |
| `scroll-to-bottom = keystroke`, `mouse-hide-while-typing` | missing | |

## Not yet

- Emoji font only on macOS (Apple Color Emoji, loaded lazily from `/System/Library/Fonts`): no bundled or other-OS fallback, so elsewhere emoji fall back to the monochrome Nerd font or nothing. No system fallback for CJK or other scripts. Only the first glyph of a cluster the emoji font cannot join is drawn.
- `alacritty_terminal` measures width per code point, so a ZWJ sequence that spans several wide cells in the grid is not one cell wide even though it is drawn as one glyph.
- Shade blocks U+2591–2593 are flat alpha (no dither).
- Search is limited to what alacritty's DFA engine supports: no look-around, back-references or Unicode `\b` (use `(?-u:\b)`); at most 20 000 matches are kept.
- Kitty graphics shared-memory medium (`t=s`), animation, Unicode placeholders, relative placements; key release events.
- Prompt marks and images are anchored to scrollback lines; once the 10 000-line history is full they drift with each dropped line.
- Mouse reporting gaps: motion is reported only while a button is held, so a DECSET 1003 program (hover effects) gets drags but not bare motion; no focus events (DECSET 1004), no pixel coordinates (1016); middle-click paste is not implemented.
- Ghostty bindings not carried over: tabs/windows/splits navigation beyond ⌘D (Atlas has its own pane keys), `write_screen_file`, `scroll_to_selection` (⌘J is the bottom dock), `adjust_selection` (⇧Home/End/arrows to extend a selection), `search_selection` (⌘E is Config), `toggle_fullscreen`, `inspector`, quick terminal (needs a global shortcut and an accessory window), `mouse-hide-while-typing`, `scroll-to-bottom = keystroke`.

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
