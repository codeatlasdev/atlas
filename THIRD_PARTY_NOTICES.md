# Third-party notices

Atlas is licensed under MIT or Apache-2.0, at your option (see
[LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE)). It bundles or builds on the
components below. Each keeps its own license; none is modified by Atlas unless
stated.

## Assets shipped inside the binary or app bundle

| Component | Used for | License | Where |
|---|---|---|---|
| [JetBrains Mono](https://github.com/JetBrains/JetBrainsMono) (Regular, Bold, Italic, Bold Italic) | Default font of the GPU terminal; fallback text font of `Atlas-Setup.exe` when Segoe UI is missing | SIL OFL 1.1 | `crates/atlas-term/fonts/JetBrainsMono-*.ttf`, text in `JetBrainsMono-OFL.txt` |
| [Nerd Fonts Symbols Only](https://github.com/ryanoasis/nerd-fonts) (`SymbolsNerdFontMono-Regular.ttf`) | Icon glyph fallback in the GPU terminal | MIT (the font collection bundles glyph sets under MIT, Apache-2.0 and OFL; see the Nerd Fonts repository for the per-set breakdown) | `crates/atlas-term/fonts/`, text in `NerdFontSymbols-LICENSE` |
| [Material Icon Theme](https://github.com/material-extensions/vscode-material-icon-theme) (`material-icon-theme` npm) | File-type icons in the desktop explorer and tabs (SVGs are bundled at build time by `apps/desktop/scripts/gen-icons.ts`) | MIT | `apps/desktop` build |
| [pdf.js](https://github.com/mozilla/pdf.js) (`pdfjs-dist` npm) | PDF viewer; CMaps, standard fonts and ICC profiles are served same-origin from the bundle (`apps/desktop/scripts/pdfjs-assets.ts`) | Apache-2.0 | `apps/desktop` build |
| [Atom One](https://github.com/atom/atom/tree/master/packages/one-dark-ui) color palette | The "One Dark" / "One Light" desktop themes reuse its colors | MIT | `apps/desktop/src/lib/themes/one.json` |
| [Dracula Theme](https://draculatheme.com) by [Zeno Rocha](https://github.com/zenorocha) and [Luxonauta](https://github.com/luxonauta) and the Dracula team: the [color specification](https://draculatheme.com/spec) (Dracula Classic, Alucard Classic) | The "Dracula" and "Alucard" desktop themes (default) | MIT, © 2023 Dracula Theme ([license](https://github.com/dracula/dracula-theme/blob/main/LICENSE)) | `apps/desktop/src/lib/themes/dracula.json` |

The OFL requires that the font software is not sold on its own and that its
license text accompanies redistributions; both hold (the text ships in the repo
and the fonts are only embedded in the terminal renderer).

**Deviation from the Dracula/Alucard specs (legibility).** The theme files keep the spec hexes and the terminal ANSI table is never changed. The *editor's syntax roles* and a few UI tokens are what `complete()` applies: a spec colour that reads 4.5:1 on every surface code is read on (editor, active line, selection focused/unfocused/on the active line) is kept byte for byte; one that does not is moved in lightness only (hue and chroma kept) until it does, with a 0.15 margin. Measured before the rule (ratio on the editor background, worst surface 1.9-3.9): Dracula comment `#6272a4` 3.03 (1.94 on the selection) lifted to `#aec1f7`; keyword/tag/operator `#ff79c6` 5.97 (3.84 on the active line) to `#fcaad5`; number/boolean/constant `#bd93f9` 5.90 (3.79) to `#d1b7fb`; string.regex `#ff5555` 4.53 (2.91) to `#ffada6`. Alucard comment `#6c664b` 5.56 (3.75 on the selection) to `#5f593f`; string `#846e15` 4.80 (3.23) to `#69570f`; number/constant `#644ac9` to `#5c40bf`; type `#036a96` to `#045f86`; function `#14710a` to `#14670c`; parameter `#a34d14` to `#90430f`; regex `#cb3a2a` to `#ae180a`. Search-match and selection-match paints are capped in strength (Dracula's 70% orange search match left the theme's own text 2.7:1) and `state.error` (Dracula) and `state.success`/`state.info` (Alucard) are lifted to 3:1 against the hover/selected fills. Rule and measurements: `apps/desktop/src/lib/theme/derive.ts` (`legible`), pinned by `theme.test.ts` ("applied syntax: spec colour when it reads, same hue lifted when it does not").

**Dracula Pro is not included.** [Dracula Pro](https://draculatheme.com/pro) is a separate, paid product by Zeno Rocha and Luxonauta under its own license (per-buyer, no redistribution). Atlas ships none of its files, names or colors; if you bought it, you can import your own licensed copy into your own `~/.atlas/themes` from Settings → Appearance (Import theme → archive or folder). Support the author: https://draculatheme.com/pro

## Rust dependencies

Every crate in `Cargo.lock` is checked by `cargo deny check licenses`
(allow-list in `deny.toml`): MIT, Apache-2.0 (with or without the LLVM
exception), BSD-2/3-Clause, ISC, Zlib, Unicode-3.0, MPL-2.0, CC0-1.0 and
CDLA-Permissive-2.0. Notable ones:

| Crate | License |
|---|---|
| [alacritty_terminal](https://github.com/alacritty/alacritty) (VT parser/grid in `atlas-term`) | Apache-2.0 |
| [tauri](https://tauri.app) and plugins, [wry](https://github.com/tauri-apps/wry), [tao](https://github.com/tauri-apps/tao) | MIT OR Apache-2.0 |
| [wgpu](https://wgpu.rs), [swash](https://github.com/dfrg/swash) | MIT OR Apache-2.0 |
| [ratatui](https://ratatui.rs), [crossterm](https://github.com/crossterm-rs/crossterm) | MIT |
| [tokio](https://tokio.rs), [serde](https://serde.rs), [reqwest](https://github.com/seanmonstar/reqwest), [clap](https://github.com/clap-rs/clap) | MIT OR Apache-2.0 |
| [portable-pty](https://github.com/wez/wezterm/tree/main/pty) | MIT |
| cssparser, selectors, dtoa-short, option-ext (transitive, via Tauri) | MPL-2.0 (file-level copyleft; used unmodified) |

Regenerate the exact list with `cargo deny list` (or `cargo metadata`).

## Frontend dependencies (`apps/desktop`)

Direct dependencies and their licenses (from each package's `package.json`):

| Package | License |
|---|---|
| [@codeatlasdev/ui](https://www.npmjs.com/package/@codeatlasdev/ui) | MIT |
| CodeMirror 6 (`codemirror`, `@codemirror/*`, `@lezer/highlight`, `codemirror-json-schema`) | MIT |
| [@xterm/xterm](https://github.com/xtermjs/xterm.js) and addons | MIT |
| [shiki](https://shiki.style), [marked](https://github.com/markedjs/marked) | MIT |
| [echarts](https://echarts.apache.org) | Apache-2.0 |
| [@lucide/svelte](https://lucide.dev) | ISC |
| [pdfjs-dist](https://github.com/mozilla/pdf.js) | Apache-2.0 |
| [@tauri-apps/api](https://tauri.app) and plugins | MIT OR Apache-2.0 |
| Svelte, SvelteKit, Vite (build-time) | MIT |

Transitive npm packages are covered by their own licenses; list them with
`cd apps/desktop && bunx license-checker --summary` (or `bun pm ls --all`).
