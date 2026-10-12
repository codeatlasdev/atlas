---
title: "Troubleshooting"
description: "Symptoms, causes and fixes for the CLI, the TUI, the desktop app and agent setup."
---

# Troubleshooting

Start with `atlas dev --status` (what is running and in what state) and `atlas version`. Logs for every service are in `<project>/.logs/`.

## Install and update

| Symptom | Cause and fix |
|---|---|
| `atlas: command not found` after installing | Open a new shell: `atlas install` edits your shell's startup file. Check `~/.atlas/bin` is on `PATH`. |
| macOS kills the binary (`zsh: killed`) | The binary is not signed. Release builds are ad-hoc signed; for a binary you built, run `codesign -f -s - target/release/atlas`. A browser download may also need `xattr -d com.apple.quarantine ./atlas`. |
| `atlas update` says to run `brew upgrade` | The binary lives under a Homebrew prefix; update it with Homebrew. |
| `atlas update` refuses to run | You are using a linked dev build (`atlas link`). Run `atlas unlink` first. |
| The update prompt is in the way | `ATLAS_NO_UPDATE_PROMPT=1`, or answer `n` (silences that version for 24 hours). |

## `atlas dev` and the stack

| Symptom | Cause and fix |
|---|---|
| `atlas dev needs an interactive terminal` (exit 2) | stdin or stdout is not a TTY. From scripts and agents use `atlas dev --status`, `--ci`, `--headless` or `--detach`. |
| `atlas dev is already running (pid N)` | One instance per project directory. Attach with `atlas dev`, check with `atlas dev --status`, stop with `atlas down`. |
| `atlas.yaml: services.web.helth: unknown field` | A typo. Unknown keys are errors at every depth; the message suggests the right name. Run `atlas config check`. |
| A service stays `starting` forever | It is waiting on its `port`, `health` URL or `ready_log`. The sidebar note says what (`waiting for :3001 (12s)`). After `start_timeout` (default 60 s) a warning is logged; the service keeps trying. Fix the probe or the port. |
| A service is `failed` with `dependency X failed` | A dependency failed, exited non-zero or gave up restarting. Fix `X` first; its dependents start by themselves once their conditions hold, or restart them. |
| `gave up after N restarts` | The restart budget (`max_restarts`, default 5) is spent. Read the service log, fix the cause, restart it from the menu (`m`) or with `atlas_restart_service`. |
| A service is green but not answering | A service without `port`, `health` or `ready_log` counts as running after 2 s alive. Add a probe. |
| `EADDRINUSE` | Something else holds the port, often an orphan of an earlier run. Atlas cleans only what it started itself (from `.logs/.pids`) and never kills by port. Find the process with `lsof -iTCP:<port> -sTCP:LISTEN` and stop it yourself. |
| `docker compose` failed and a `docker` alert shows | The daemon was not reachable within 15 s. Start Docker or OrbStack; app services still start. |
| Quitting leaves containers running | By design: Atlas never runs `docker compose down`. Stop them with `docker compose down`. |
| `pre_command failed` toast | The dependency install failed; services start anyway. Read `.logs/setup.log`. |
| Stale `.logs/atlas.sock` after a crash | Harmless; the next run replaces it. |
| Colors look wrong or text cannot be selected | `M` releases the mouse so your terminal can select; hold ⌥ while dragging otherwise. |

## MCP and agents

| Symptom | Cause and fix |
|---|---|
| The atlas tools are missing in the agent | `atlas` is not on the agent's `PATH` (GUI apps often lack your shell `PATH`). Re-run `atlas setup <agent> --apply` from a shell where `which atlas` works so the absolute path is written. In omp: `/mcp test atlas`. |
| `atlas dev is not running; start it in a terminal` | Control tools need a runner: `atlas dev`, `atlas dev --detach` or `--headless`. `--ci` does not serve control. |
| `atlas_status` says `running: false` but the TUI is open | The agent is looking at a different project directory. Compare `--dir` or `cwd`. |
| `atlas_wait_ready` is cut off after 30 s | The client timeout is shorter than `timeout_s`. Keep `timeout_s` ≤ 25 or raise the client timeout. |
| The agent still runs `atlas dev` | The omp hook loads at session start: restart omp and check `omp plugin list`. `/reload-plugins` refreshes skills and MCP only. |
| Codex ignores the hooks | Trust them once with `/hooks` in Codex. |
| `CONFLICT … an atlas entry not created by atlas` | You have your own `atlas` server entry. Rename it, or re-run with `--force`. |

The full table is in [Agent setup](agents.md#troubleshooting) and [MCP](mcp.md#troubleshooting).

## Atlas Desktop

| Symptom | Cause and fix |
|---|---|
| `atlas desktop` cannot download the app | Check your connection and that the release for your CLI version exists. `atlas desktop --status` shows what is installed; `atlas desktop --install --latest` reinstalls. |
| `bun` or `node` not found inside the app | An app opened from Finder gets a bare environment. Atlas reads your login shell's environment at launch; Settings → General → *Shell for your projects* shows what it found and has **Reload**. Restart a running stack after changing it. |
| Terminals use the classic renderer with GPU on, and Settings says the terminal host is from an older Atlas | The daemon predates the app and could not be replaced live. Click **Restart terminal host** in Settings → Appearance → Terminal when no terminal is busy. |
| A terminal is blank right after opening | Turn off *Fast terminal (GPU)* in Settings → Appearance → Terminal to compare with the xterm.js renderer, and file an issue with `atlas desktop debug logs` output. |
| A worktree was not deleted with its branch | The branch is unmerged, so Atlas keeps it (a toast says so). Delete it with `git branch -D` when you are sure. |
| `atlas wt` or `atlas term` says the app is not running | They use `~/.atlas/desktop.sock`; open Atlas Desktop first. `atlas term` falls back to the local hostd when the app is closed. |
| The app and CLI versions differ | `atlas update` brings both to the latest; **Update CLI & app** in the app does the same. Running stacks keep their version until `atlas down && atlas dev --detach`. |

### Debugging a debug build

```bash
atlas desktop debug info
atlas desktop debug logs --level csp   # CSP violations
atlas desktop debug shot /tmp/x.png
```

## Reporting a bug

If the CLI panics it prints a path such as `crash-<timestamp>.toml` under `~/Library/Application Support/atlas/crashes/` (`$XDG_DATA_HOME/atlas/crashes` on Linux). Crash reports stay on your machine and are never uploaded; review one for private data before attaching it.

Open an issue at [github.com/codeatlasdev/atlas/issues](https://github.com/codeatlasdev/atlas/issues) with `atlas version`, your OS, what you ran and the relevant lines of `.logs/<service>.log` (redact secrets). Security problems go through [SECURITY.md](https://github.com/codeatlasdev/atlas/blob/main/SECURITY.md), not public issues.
