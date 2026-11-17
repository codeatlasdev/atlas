---
name: desktop-debug
description: Inspecting, screenshotting, driving or benchmarking the Atlas desktop app from an agent (UI work in apps/desktop, atlas desktop debug, agent instances, Atlas Lab, mirror, gesture, flicker and selection benches). Read before starting any Atlas window; an agent never shows one on the user's screen.
---

# Debugging Atlas Desktop (agents: `desktop:agent`)

Pick the lightest level that covers the change:

| Level | Run | Covers |
|---|---|---|
| 0 | `bun run test`, `cargo test` | logic, stores, protocol, Rust↔TS contracts |
| 1 | `bun run desktop:lab` (macOS) | any Svelte screen, HMR, against an invisible instance: [lab-mirror.md](lab-mirror.md) |
| 2 | `bun run desktop:agent` | native surfaces, GPU terminal, gestures, flicker, CSP/IPC of the bundle |

**An agent never shows an Atlas window on the user's desktop, takes his focus, or sends input to it.** The one exception is the user asking for it: then start the `desktop` service of this repo's atlas.yaml (`atlas_start_service desktop`, the real Atlas Dev window) and stop it (`atlas_stop_service desktop`) when he says so. Not for a screenshot, not for a recording, not "just for a second". Start your own instance:

```bash
bun run desktop:agent                    # builds the debug app (macOS: `bun run desktop:build:app`, `.app` only, never a DMG, which would open Finder on the user's screen; Linux: `bun run desktop:build:debug`, the debug binary with the bundled frontend) + CLI the first time; same as `atlas desktop --agent`
# atlas-agent ready name=a1 pid=123 ATLAS_DEBUG_SOCK=/private/tmp/atlas-agent-a1/debug.sock
export ATLAS_DEBUG_SOCK=/private/tmp/atlas-agent-a1/debug.sock     # every `atlas desktop debug …` below talks to THIS instance
bun run desktop:agent -- --name pdf --scratch   # own name; --scratch seeds HOME with his desktop settings (never projects/tokens)
bun run desktop:agent -- --stop --name a1       # stop one; --stop alone stops every agent instance
atlas desktop --agent --gc                      # delete WebKit stores left by instances that no longer exist
```

Then drive it with `atlas desktop debug …` (every command needs `ATLAS_DEBUG_SOCK` set to YOUR socket; a bare `atlas desktop debug` talks to a debug build of the user's own app: do not do that):

```bash
atlas desktop debug info                                  # pid, url, visible, socket
atlas desktop debug state                                 # projects, service states, worktrees, selection
atlas desktop debug logs [--level csp|error]              # console, uncaught errors, CSP violations
atlas desktop debug shot /tmp/x.png [--selector '…']      # page snapshot, no permission needed
atlas desktop debug eval 'return await window.__TAURI_INTERNALS__.invoke("projects_list")'
```

Everything works while the window is unfocused or hidden-behind; `click`/`type` are DOM events inside the webview, never OS input.

## Read next, by need

| Need | File |
|---|---|
| every `debug` subcommand, `record`, `wait`, `a11y`, `perf`; gesture, flicker and selection benches; `window.__atlas` | [commands.md](commands.md) |
| how an instance is isolated, the virtual display, the fail-closed `ShowGranted` proof, start serialization, `ATLAS_AGENT_DRYRUN`, Linux (headless sway), proving you left his screen alone | [instances.md](instances.md) |
| let the human watch (`desktop:mirror`), Atlas Lab, dev icon, migration e2e | [lab-mirror.md](lab-mirror.md) |

Never start two instances at once, and never start one to test display code before `ATLAS_AGENT_DRYRUN=1 atlas desktop --agent` has logged a sane decision ([instances.md](instances.md)). Check CSP and IPC on the **bundled** build (`tauri://localhost`), not only on Vite.
