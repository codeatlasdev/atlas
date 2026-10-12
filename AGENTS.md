# Atlas — notes for coding agents

Rust workspace (`crates/*`: `atlas-cli` = the `atlas` binary, `atlas-tui` = runtime/TUI/MCP/runner, `atlas-host` = `atlas hostd` (PTYs/exec/fs daemon behind the app), `atlas-core`, `atlas-skills`, `atlas-term` = the GPU terminal, `atlas-desktop` = Tauri 2 shell) plus the desktop frontend in `apps/desktop` (SvelteKit 3 + `@codeatlasdev/ui`). Docs are plain markdown in `docs/`; the user-facing agent skills in `skills/` are embedded into the binary at build time.

## Gate and workflow

- `scripts/setup.sh` — the one preflight (rustup, bun, gh, frontend deps, `cargo-tauri`, `cargo-deny`). Idempotent.
- `bun run check` — the only gate: fmt, `clippy --workspace -D warnings`, workspace tests, `cargo deny check`, `scripts/check-skills.sh`, `desktop:check` (svelte-check + prettier + bun test). Run it on its own and read the exit code; never chain `git push`/`release` after it with `;`.
- CI (`.github/workflows/ci.yml`) runs on pull requests and pushes to `main`. Releases are maintainer-run: `release.yml` is `workflow_dispatch` only and `bun run release <patch|minor|…>` builds and publishes it through `act` (see `docs/release.md`). `bun run ci:act` runs the macOS CI job locally through `act`.
- Commits need a DCO sign-off (`git commit -s`); see `CONTRIBUTING.md`. Agent-assisted changes follow the AI policy there.
- `bun run dup` — jscpd; keep duplication ≤ 1.5%.
- Toolchain pinned in `rust-toolchain.toml`. Workspace lints apply to every crate; fix warnings, no global `allow`.

## Deploying a local build

- CLI: `cargo build -p atlas-cli && codesign -f -s - target/debug/atlas && cp target/debug/atlas ~/.atlas/bin/atlas`, or `atlas link` (symlink). Verify with `strings ~/.atlas/bin/atlas | grep <version>`.
- Desktop (macOS only): `atlas link --desktop` (debug bundle at `/Applications/Atlas.app`), `bun run desktop:dev` (hot reload), `atlas unlink` to go back.

## Debugging Atlas Desktop (no mouse/keyboard)

Debug builds run a bridge on `~/.atlas/desktop-debug.sock` (`crates/atlas-desktop/src/debug.rs`, never in release builds):

```bash
atlas desktop debug info                         # pid, URL, visible, window id
atlas desktop debug logs [--level csp|error]     # console, uncaught errors, CSP violations
atlas desktop debug eval 'return await window.__TAURI_INTERNALS__.invoke("projects_list")'
atlas desktop debug shot /tmp/x.png              # window capture by the CLI
```

Debug builds also expose the live stores as `window.__atlas` (`ws`, `store`, `agents`, `worktrees`, `api`, `host`); use them from `debug eval` instead of `import()`, which creates separate module instances. With the app open, `atlas wt|term|browser|file` drive it over `~/.atlas/desktop.sock` in every build.

Check CSP and IPC on the **bundled** build (`tauri://localhost`), not only on Vite. Never send synthetic input to the user's desktop.

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

Prefer behavior tests (boundaries, failures, transitions). Manager tests live in `runtime/manager/tests.rs` and are wired by `#[cfg(test)] mod tests;` in `mod.rs` — when splitting modules, check the test count does not drop.
