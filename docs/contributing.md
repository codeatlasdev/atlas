---
title: "Contributing"
description: "Set up a development environment, run the checks and send a pull request."
---

# Contributing

Thank you for helping with Atlas. This page is the long version of [CONTRIBUTING.md](https://github.com/codeatlasdev/atlas/blob/main/CONTRIBUTING.md). By taking part you agree to the [Code of Conduct](https://github.com/codeatlasdev/atlas/blob/main/CODE_OF_CONDUCT.md). Atlas is released under the [MIT](https://github.com/codeatlasdev/atlas/blob/main/LICENSE-MIT) or [Apache-2.0](https://github.com/codeatlasdev/atlas/blob/main/LICENSE-APACHE) license, at your option, and your contributions are too.

## Ways to help

- **Report a bug** or ask for a feature in the [issue tracker](https://github.com/codeatlasdev/atlas/issues). Include `atlas version`, your OS, the steps and the relevant lines of `.logs/<service>.log` (redact secrets).
- **Improve the docs.** Every page on the website has an *Edit on GitHub* link; the source is `docs/*.md`.
- **Send a fix or a feature.** For anything bigger than a small fix, open an issue first so we can agree on the shape before you spend time on it.
- **Security issues** do not go in public issues. See [SECURITY.md](https://github.com/codeatlasdev/atlas/blob/main/SECURITY.md).

> **Sign your commits.** Atlas uses the [DCO](https://developercertificate.org/): commit with `git commit -s`. AI-assisted changes follow the policy in [CONTRIBUTING.md](https://github.com/codeatlasdev/atlas/blob/main/CONTRIBUTING.md#ai-assisted-contributions).

## Set up

You need `rustup`, [`bun`](https://bun.sh) and, for the desktop app, the Xcode command line tools. `gh` is optional.

```bash
git clone https://github.com/codeatlasdev/atlas.git
cd atlas
scripts/setup.sh        # idempotent: checks tools, installs frontend deps, cargo-tauri and cargo-deny
```

The Rust toolchain is pinned in `rust-toolchain.toml`; `rustup` installs it on the first `cargo` command. Workspace lints apply to every crate: fix warnings, do not add global `allow`s.

## The gate

```bash
bun run check
```

`bun run check` is the only gate and the one CI runs. It chains, in order:

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets --locked -- -D warnings`
3. `cargo test --workspace --locked`
4. `cargo deny check` (licenses, advisories, duplicate and banned crates)
5. `scripts/check-skills.sh` (validates `skills/*/SKILL.md` against the Agent Skills spec)
6. `bun run desktop:check` (svelte-check, prettier and `bun test` for the frontend)

Run it on its own and read the exit code before you push. A pull request should pass it.

Also useful:

| Command | Use |
|---|---|
| `bun run dup` | jscpd duplication report; keep it at or below 1.5% |
| `cargo test -p atlas-cli --test mcp` | the MCP integration suite (real binary, live headless run; needs `python3`) |
| `cargo test -p atlas-tui --release -- --ignored --nocapture bench_one_million` | the log pipeline benchmark |
| `cargo test -p atlas-term --release -- --ignored --nocapture screen_bench` | the terminal renderer benchmark |

## Build and run your changes

```bash
cargo build -p atlas-cli                   # builds the `atlas` binary (atlas-tui alone is only a library)
codesign -f -s - target/debug/atlas        # macOS: ad-hoc sign or Gatekeeper kills it
atlas link                                 # symlink ~/.atlas/bin/atlas to the debug build
atlas unlink                               # restore the release binary
```

### Develop the desktop app

```bash
bun run desktop:dev        # hot reload: Vite + the Tauri shell; Rust changes rebuild
atlas link --desktop       # make /Applications/Atlas.app your debug bundle
```

While linked, the app shows a **Dev build** badge and refuses to update over your build.

Debug builds run a bridge on `~/.atlas/desktop-debug.sock`, so you (or an AI agent) can inspect the app without the mouse:

```bash
atlas desktop debug info
atlas desktop debug logs --level csp        # console, errors, CSP violations
atlas desktop debug eval 'return await window.__TAURI_INTERNALS__.invoke("projects_list")'
atlas desktop debug shot /tmp/x.png
```

Debug builds also expose the live stores as `window.__atlas` (`ws`, `store`, `agents`, `worktrees`, `api`, `host`); use them from `debug eval` instead of `import()`, which creates separate module instances. Check CSP and IPC on the **bundled** build (`tauri://localhost`), not only on Vite. After a hot reload, reload the webview before trusting socket behavior.

## Code layout

See [Architecture](architecture.md) for the crates and how the processes talk to each other. Where things go:

| Change | Where |
|---|---|
| a new `atlas` subcommand | `crates/atlas-cli/src/commands/` |
| config fields, validation, the JSON Schema | `crates/atlas-tui/src/config/` |
| scheduling, health, restarts, actions | `crates/atlas-tui/src/runtime/manager/` |
| TUI widgets and keys | `crates/atlas-tui/src/tui/` (the key table is `keymap.rs`) |
| MCP tools, resources, prompts | `crates/atlas-tui/src/mcp/` |
| terminals, exec, files | `crates/atlas-host/` |
| terminal rendering | `crates/atlas-term/` |
| desktop backend | `crates/atlas-desktop/src/` |
| desktop UI | `apps/desktop/src/` |
| agent skills | `skills/<name>/SKILL.md` (embedded at build time) |

## Conventions

- **Tests.** Prefer behavior tests: boundaries, failures and state transitions over getters. Manager tests live in `runtime/manager/tests.rs` and are wired by `#[cfg(test)] mod tests;` in `mod.rs`; when you split modules, check the test count does not drop. Integration tests drive the real binary.
- **Docs move with code.** A change that alters a flag, a key, a config field or a behavior updates the page that documents it, in the same pull request. The TUI help and footer are generated from one keymap table; the desktop shortcuts are `DEFAULT_KEYMAP` in `apps/desktop/src/lib/keys.ts`.
- **Changelog.** User-visible changes get a line under `## [Unreleased]` in `CHANGELOG.md`. Release notes are built from it.
- **Formatting.** `.editorconfig` applies: tabs in TypeScript, Svelte and shell; 4 spaces in Rust; 2 spaces in YAML, Markdown and JSON.
- **Commits.** Short imperative subject, conventional prefixes (`feat:`, `fix:`, `docs:`, `chore:`) as in the history. Explain *why* in the body when it is not obvious.
- **Dependencies.** Prefer the standard library and what is already in the tree. A new crate or package needs a reason in the pull request; `cargo deny check` must pass.

### Runtime rules that came from real incidents

Keep these when you touch the runtime ([Architecture](architecture.md#safety-rules-the-code-relies-on)):

- Orphan cleanup uses only the `.pids` file and exact PIDs; never `lsof` port-kill or negative PIDs.
- Never run `docker compose down` or restart the docker daemon from Atlas.
- Children: `setsid`, `stdin(null)`, `sh -c` for commands.
- TUI: event-stream input, dirty-flag rendering, non-blocking startup.
- Desktop: child webviews make the main window multi-webview; use `get_window("main")` for window operations and `get_webview("main")` for the app's own webview. Never put a secret in content loaded into the embedded browser.

## Making videos

Every published video (the film on the website, its vertical cut, release videos) is built from code in [`studio/`](https://github.com/codeatlasdev/atlas/tree/main/studio). A film is typed data in `studio/films/<id>/film.ts`; [HyperFrames](https://github.com/heygen-com/hyperframes) (Apache-2.0) renders it and `ffmpeg` mixes the audio, so a rebuild is deterministic and works on a laptop or in CI. The engine choice, with the licensing reasoning, is in [`studio/DECISION.md`](https://github.com/codeatlasdev/atlas/blob/main/studio/DECISION.md); agent-oriented notes are in [`studio/AGENTS.md`](https://github.com/codeatlasdev/atlas/blob/main/studio/AGENTS.md).

You need `bun`, `ffmpeg`/`ffprobe` and, for the TUI take, [`vhs`](https://github.com/charmbracelet/vhs). Recordings, takes, voice and music are **not in git**: they live in `studio/.cache/` (see its manifest with `bun studio assets`) and the published files are fetched with `scripts/fetch-media.sh`.

```bash
bun install                                    # once, at the repo root (workspace)
bun studio check                               # typecheck, brand tokens, film data, 1 s-per-scene smoke render, determinism
bun studio preview atlas-1-0                   # live preview; rebuilds when you save film.ts or a scene
bun studio render atlas-1-0 [--aspect 9:16]    # full render -> studio/out/atlas-1-0/ (mp4, webm, vtt, poster, scrub sprite, report)
bun studio publish atlas-1-0                   # copy into site/public/media with content hashes + film-chapters.json
bun studio release unreleased --as 1.1 --draft # "What's new" video from the CHANGELOG.md section (16:9 + 9:16)
```

Capture and narration:

- `bun studio capture app <scene>` records the **real** Atlas Desktop debug build in a separate instance (own `HOME` and `ATLAS_HOSTD_DIR`, scratch projects, window capture by id; it never sends input to your desktop). Build the debug bundle first (`atlas link --desktop` or `cargo tauri build --debug`). `bun studio capture tui <take>` runs `vhs` on `demo/demo.tape` with its output redirected into the cache.
- `bun studio voice <film>` synthesizes the narration with ElevenLabs and caches each line by a hash of its text, voice and model. Secrets come only from the environment (`ELEVENLABS_API_KEY`, `MAGNIFIC_API_KEY`), never from the repo.
- The final mix is measured: the render fails when the integrated loudness is not within the film's tolerance of -14 LUFS.

Narration is a synthetic voice and the music bed is generated; keep the disclosure and licence notes in `site/public/media/README.md` accurate when you change either. A change to `studio/` is checked with `bun studio check`, not `bun run check`.

## Pull requests

1. Fork, branch from `main`.
2. Make the change with its tests and doc updates.
3. Run `bun run check`.
4. Open a pull request using the template; link the issue it closes.

A maintainer will review it. Small, focused pull requests get merged faster than large ones.

## CI and releases

CI (`.github/workflows/ci.yml`) runs on every pull request and on pushes to `main`: macOS (the full workspace gate) and Ubuntu (the workspace without the macOS-only crates), plus the desktop frontend checks, the site build and `cargo deny`. Fork pull requests run it with a read-only token and no secrets. Releases are built and published from a maintainer's machine through [`act`](https://github.com/nektos/act) (`bun run release`); see [Releasing](release.md). `bun run ci:act` runs the macOS job of the CI workflow locally (`-j rust-macos`).
