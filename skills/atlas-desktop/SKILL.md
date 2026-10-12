---
name: atlas-desktop
description: Use when Atlas Desktop is open and you need to create/list/remove worktrees, drive terminals or the embedded browser, or open files and diffs from a script or agent session.
metadata:
  version: "1.0.0"
---

# Atlas Desktop automation

Atlas Desktop exposes worktree management, terminals, browser and file navigation through the `atlas wt`, `atlas term`, `atlas browser` and `atlas file` CLI commands when the app is running (`atlas desktop`).

## Rules

- **Never kill a pty of another agent's session** — check `atlas term ls` first; tabs with an `agent` column belong to a running agent.
- `atlas wt create` is long (up to 15 min for setup scripts); always pass `--json` and poll, or use `atlas term wait <pty> --for idle` once the terminal starts.
- `browser snapshot` → `browser click @eN` → `browser snapshot` is the safe loop; refs expire after each snapshot.
- Verify changes by reading the file (`atlas file open`) or the terminal output, not by re-running the command.

## Worktrees

```bash
# List all worktrees
atlas wt ls --json

# Create a worktree with omp, start with a prompt, wait until idle
atlas wt create --repo /path/to/repo --name fix-login --agent omp --prompt "fix the login bug" --json
# → {"id":"…","path":"…","terminals":[{"pty":"<pty>",…}]}
atlas term wait <pty> --for idle

# Resolve the worktree from inside a checkout
atlas wt current --json     # or atlas wt current /path/to/file

# Remove (preserves unmerged branch)
atlas wt rm <id>
atlas wt rm <id> --force    # deletes branch too
```

## Terminals

```bash
# List terminals (pty, state, label, agent, worktree)
atlas term ls --json

# Send a prompt (agent) or raw text (plain terminal)
atlas term send <pty> "explain this code" --enter

# Read last 200 lines (ANSI stripped)
atlas term read <pty> --lines 50

# Wait until idle (agent done / no output for 2 s)
atlas term wait <pty> --for idle --timeout 300

# Wait until the process exits
atlas term wait <pty> --for exit --timeout 60

# Kill (also removes the scrollback)
atlas term kill <pty>
```

## Browser automation

The loop: snapshot → get @refs → click/fill → snapshot.

```bash
# Navigate to a URL (opens an embedded browser tab)
atlas browser goto http://localhost:3000 --json

# Take an accessibility snapshot (returns nodes with @eN refs)
atlas browser snapshot --json

# Click an element
atlas browser click @e3

# Fill a form field
atlas browser fill @e5 "user@example.com"

# Screenshot the current page (returns the PNG path)
atlas browser screenshot
```

## Files

```bash
# Open a file in the editor tab (by relative path)
atlas file open src/main.ts --worktree /path/to/checkout

# Open the Diff/Changes tab
atlas file diff --worktree /path/to/checkout
```

## Settings read by the CLI

- `agentPermissions`: `manual` (default) or `yolo` — whether agents auto-approve tool calls.
- `branchPrefix`: `git-username` (default), `custom`, `none`.
- `workspaceDir`: where new worktrees are created (default `~/.atlas/worktrees`).

## What the CLI cannot do

These require the app UI or another tool:

- Accept approval prompts that an agent in `manual` mode raised (⌘-click in the app).
- Operate the native Design Mode (use the app to capture UI elements).
- Control the right sidebar or Settings (use ⌘⌥B / ⌘,).
