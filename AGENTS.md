# Atlas — notes for coding agents

This file is a map, kept under 8 KB (`bun test scripts/agents-md.test.ts` fails past that). Details live in skills (`.agents/skills/`, also visible to Claude Code through the `.claude/skills` link), nested `AGENTS.md` files that load when you work in that directory, and `docs/`.

## What this is

- Rust workspace in `crates/*`: `atlas-cli` (the `atlas` binary), `atlas-tui` (runtime, TUI, MCP, runner), `atlas-host` (`atlas hostd`: PTYs, exec, fs), `atlas-core`, `atlas-skills`, `atlas-term` (GPU terminal), `atlas-desktop` (Tauri 2 shell).
- Desktop frontend `apps/desktop` (SvelteKit 3 + `@codeatlasdev/ui`); phone app `apps/mobile` (Flutter, [docs/mobile.md](docs/mobile.md)); film studio `studio/`; push relay `services/push-relay` (Go); website `site/`.
- User-facing agent skills are `skills/` (embedded into the binary). Skills for people and agents who work on this repo are `.agents/skills/`.
- Docs are plain markdown in `docs/`; [docs/contributing.md](docs/contributing.md) is the human reference for setup, the gate and every command.

## Hard rules (breaking these causes damage)

- **Merge, push, release.** Agents never merge a PR, never push to `main`, never use the ruleset bypass, unless a maintainer (`atlas-maintainers`) is the one running them and says so for that change. For an external contributor, or when unsure whom you work for: branch `type/short-desc`, open a PR, stop. Never force-push; never move or delete a tag.
- **Releases.** Alpha is the main channel and GitHub's "Latest" (maintainer decision, standing). Every release is `bun run release alpha` from `main`; do not cut beta/stable, set `make_latest=false` or flip the default channel unless the maintainer asks. Details: `.agents/skills/atlas-workflow/SKILL.md`.
- **Never show an Atlas window on the user's screen**, take his focus or send input to it. Start your own invisible instance with `bun run desktop:agent` (`.agents/skills/desktop-debug/SKILL.md`). Never run `desktop:build -- --debug` (its DMG step opens Finder).
- **Orphan kill** uses only the `.pids` file and exact PIDs: never `lsof` port-kill or negative PIDs (a container runtime's port forwarder owns those ports; a recycled group id can hit atlas itself).
- **Containers.** Never restart the container runtime (`orb restart docker`) and never run `docker compose down` from atlas.
- **Platform code.** Shared code goes through `atlas_core::sys` and `atlas_core::ipc`, never `libc`/`nix`. Every child of the GUI app or hostd needs `no_window()` (or `spawn_detached`), else a console window flashes.
- **Tests first.** For every bug: write a test that reproduces it, watch it fail for the reported reason, then fix. Real behavior or contract, never filler; cross-boundary contracts use the real producer's output. Full guide: `.agents/skills/real-tests/SKILL.md`.
- **Commits** need a DCO sign-off (`git commit -s`); the AI policy is in `CONTRIBUTING.md`. Stage explicit paths.
- Guards are enforced, not advisory: `.claude/settings.json`, `.codex/hooks.json`, `.omp/extensions/agent-guard.ts` and the `pre-push` hook (`scripts/setup.sh` installs it) refuse writes outside your worktree, pushes to `main`/`release/*`, force-push, merges (incl. `bun run pr:merge`), `git stash`
  and branch/tag deletes; a refusal names the fix, only a maintainer lifts it (`ATLAS_MAINTAINER_OK`, docs/contributing.md "Agent guards"). Use absolute paths under your worktree.
- Merging is `bun run pr:merge <pr>` only (maintainer-run; use `--dry-run --json` first); never type `gh pr merge`, `--admin` or a `Closes #N` yourself. See CONTRIBUTING.md#pull-request-flow-and-who-merges.

## Which command checks what

- `bun run check` is the only gate. Run it on its own and read the exit code; never chain `git push` or `release` after it with `;`.
- Edit loop: `bun run check:changed` runs the checks your changes affect, from the same map as the gate (`scripts/checks/map.ts`); `--dry-run --json` says what would run and why. `bun run check` runs every check CI can run on one machine. To add a check or map a path, edit that file only (docs/contributing.md#the-gate).
- `bun run desktop:check` frontend; `bun run mobile:check` Flutter; `bun run release:test` repo scripts (`bun test scripts`, includes the AGENTS.md and skills tests); `bun run lint:workflows` workflows; `bun run dup` duplication, keep it at or below 1.5%.
- Heavy commands (cargo build/test, the gate, benches) go through `atlas run --heavy -- <cmd>`: the machine-wide queue protects every worktree and agent.
- `CHANGELOG.md`: a line under `## [Unreleased]`, then `bun scripts/changelog-lint.ts` (merges can duplicate entries).
- Toolchain is pinned in `rust-toolchain.toml`; workspace lints apply to every crate: fix warnings, no global `allow`. `scripts/setup.sh` is the one idempotent preflight.

## Where things live

| Change | Where |
|---|---|
| a new `atlas` subcommand | `crates/atlas-cli/src/commands/` |
| config, validation, schema | `crates/atlas-tui/src/config/` |
| scheduling, health, restarts | `crates/atlas-tui/src/runtime/manager/` |
| MCP tools and resources | `crates/atlas-tui/src/mcp/` |
| terminals, exec, files | `crates/atlas-host/` |
| desktop backend / UI | `crates/atlas-desktop/src/` / `apps/desktop/src/` |
| pids, kills, sockets, paths | `crates/atlas-core/` (`sys`, `ipc`) |

The crates and how the processes talk to each other: [docs/architecture.md](docs/architecture.md).

## Before you touch X, read Y

| You are about to | Read first |
|---|---|
| commit, push, fix CI, branch, release | `.agents/skills/atlas-workflow/SKILL.md` |
| fix a bug or write or review a test | `.agents/skills/real-tests/SKILL.md` |
| run, inspect, screenshot or benchmark the desktop app; UI work in `apps/desktop` | `.agents/skills/desktop-debug/SKILL.md` |
| change the runtime or TUI | `crates/atlas-tui/AGENTS.md` |
| change the Tauri shell | `crates/atlas-desktop/AGENTS.md` |
| touch pids, kills, sockets, paths, Windows or Linux code | `crates/atlas-core/AGENTS.md` |
| change the phone app | `apps/mobile/AGENTS.md` |
| change the film studio | `studio/AGENTS.md` |
| add or change a user-facing skill | `skills/` and `scripts/check-skills.sh` |
| set up, run commands, build profiles, CI design | [docs/contributing.md](docs/contributing.md), [docs/build-perf.md](docs/build-perf.md) |
