# Watching an instance: mirror and Lab

Moved verbatim from the old root `AGENTS.md` ("Debugging Atlas Desktop"). The human reference is `docs/contributing.md` ("Atlas Lab", "Linux and Windows gaps").

**Let the human watch:** `bun run desktop:mirror --name <instance>` prints a tokened URL with a live view of your instance (native layers included, ~30 fps) where the user can also click and type. Give him that URL instead of screenshots when he wants to follow along.

**Prefer Atlas Lab for frontend work:** `bun run desktop:lab` serves the Svelte app with Vite/HMR on `http://localhost:1421` against an invisible agent instance (every `invoke()`/event goes through its bridge), so most UI work needs no app build and the user can open the same URL to watch or click. Levels and limits: docs/contributing.md "Atlas Lab". Native surfaces and the bundled CSP/IPC check still need the instance itself. Agent instances, Lab and the mirror run on macOS and Linux; per-capability table: docs/contributing.md "Linux and Windows gaps". The app-id migration has a real-WebKit check: `scripts/migration-e2e.sh` (throwaway bundle ids, never the user's store). Dev builds carry the orange Atlas Dev icon and id `br.com.codeatlas.atlas.dev` (`tauri.dev.conf.json`; regenerate with `bun run icons:dev`).
