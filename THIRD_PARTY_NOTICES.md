# Third-party notices

Atlas is licensed under MIT or Apache-2.0, at your option (see
[LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE)). It bundles or builds on the
components below. Each keeps its own license; none is modified by Atlas unless
stated.

## Assets shipped inside the binary or app bundle

| Component | Used for | License | Where |
|---|---|---|---|
| [JetBrains Mono](https://github.com/JetBrains/JetBrainsMono) (Regular, Bold, Italic, Bold Italic) | Default font of the GPU terminal | SIL OFL 1.1 | `crates/atlas-term/fonts/JetBrainsMono-*.ttf`, text in `JetBrainsMono-OFL.txt` |
| [Nerd Fonts Symbols Only](https://github.com/ryanoasis/nerd-fonts) (`SymbolsNerdFontMono-Regular.ttf`) | Icon glyph fallback in the GPU terminal | MIT (the font collection bundles glyph sets under MIT, Apache-2.0 and OFL; see the Nerd Fonts repository for the per-set breakdown) | `crates/atlas-term/fonts/`, text in `NerdFontSymbols-LICENSE` |
| [Material Icon Theme](https://github.com/material-extensions/vscode-material-icon-theme) (`material-icon-theme` npm) | File-type icons in the desktop explorer and tabs (SVGs are bundled at build time by `apps/desktop/scripts/gen-icons.ts`) | MIT | `apps/desktop` build |
| [pdf.js](https://github.com/mozilla/pdf.js) (`pdfjs-dist` npm) | PDF viewer; CMaps, standard fonts and ICC profiles are served same-origin from the bundle (`apps/desktop/scripts/pdfjs-assets.ts`) | Apache-2.0 | `apps/desktop` build |
| [Atom One](https://github.com/atom/atom/tree/master/packages/one-dark-ui) color palette | The "One Dark" / "One Light" desktop themes reuse its colors | MIT | `apps/desktop/src/lib/themes/one.json` |

The OFL requires that the font software is not sold on its own and that its
license text accompanies redistributions; both hold (the text ships in the repo
and the fonts are only embedded in the terminal renderer).

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
