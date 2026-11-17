# Desktop shell rules (atlas-desktop)

Nested notes, loaded when you work under `crates/atlas-desktop/`. Moved verbatim from the old root `AGENTS.md`. To run or inspect the app from an agent, read `.agents/skills/desktop-debug/SKILL.md` first.

- Child webviews (`unstable` feature) make the main window multi-webview: use `get_window("main")` for window ops and `get_webview("main")` for the app's own webview, never `get_webview_window`.
- Never put a secret in content loaded into the embedded browser; Design Mode results are read back through `eval_with_callback`.
- After HMR, a store listener registered in `onMount` can survive with stale module instances; reload the webview before trusting socket/ctl behavior.
- Codex runs hooks in its own daemon without the terminal's env: `atlas hook` falls back to the default endpoint and the app matches by agent + cwd.

Local desktop build (macOS only):

- Desktop (macOS only): `atlas link --desktop` (debug bundle at `/Applications/Atlas.app`), `bun run desktop:dev` (hot reload), `bun run desktop:build:app` (debug `.app` only; never use `desktop:build -- --debug`, its DMG step mounts an image and opens Finder), `atlas unlink` to go back.
