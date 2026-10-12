---
title: "Desktop settings"
description: "Every key of ~/.atlas/desktop/settings.json, its default and its range."
---

# Desktop settings

Atlas Desktop stores its preferences in `~/.atlas/desktop/settings.json`. Everything here can be changed in **Settings** (⌘,), which also has a search box that jumps to a setting by what it does. Edit the file by hand when you want to script or share a configuration; the app re-reads it and **drops values that are the wrong type or out of range**, using the default instead, so a bad edit never breaks the app.

Editor, interface and terminal preferences use [Zed's setting names](https://zed.dev/docs/configuring-zed).

## Editor and interface

| Key | Default | Notes |
|---|---|---|
| `buffer_font_family` | `ui-monospace, "SF Mono", SFMono-Regular, Menlo, monospace` | code editor font |
| `buffer_font_size` | `12.5` | 6–48 |
| `buffer_line_height` | `1.55` | 1–3 |
| `ui_font_family` | `-apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif` | interface font |
| `ui_font_size` | `13` | 9–24 |
| `project_panel.dock` | `"left"` | `left` or `right`: which side the file panel sits on |
| `project_panel.file_icons` | `true` | Material Icon Theme file icons |
| `project_panel.folder_icons` | `true` | |
| `project_panel.indent_size` | `12` | 4–40 px |
| `active_pane_modifiers.border_size` | `0` | 0–8 px border around the active pane |
| `active_pane_modifiers.inactive_opacity` | `1` | 0.2–1; lower dims inactive panes |

## Terminal

| Key | Default | Notes |
|---|---|---|
| `terminal.font_family` | `SF Mono, Menlo, ui-monospace, monospace` | xterm.js terminals only; the [GPU terminal](terminal.md) bundles JetBrains Mono |
| `terminal.font_size` | `12.5` | 6–48; applies live |
| `terminal.cursor_shape` | `"block"` | `block`, `bar`, `underline` or `hollow` |
| `terminal.blinking` | `false` | |
| `terminal.max_scroll_history_lines` | `10000` | 0–100000 |
| `terminal.osc52` | `"ask"` | what happens when a program writes your clipboard: `allow`, `ask` or `deny`. Reading the clipboard is always denied |
| `terminal.ligatures` | `true` | GPU terminal |
| `terminal.minimum_contrast` | `1` | 1–21; WCAG ratio text is lifted to, `1` = off. GPU terminal |
| `terminal.copy_on_select` | `true` | GPU terminal |
| `terminal.option_as_alt` | `"both"` | `both`, `left`, `right` or `none`. GPU terminal |
| `terminal.click_to_move` | `true` | a click on the prompt moves the cursor (needs shell integration). GPU terminal |

The GPU terminal itself is switched with **Settings → Appearance → Terminal → Fast terminal (GPU)**.

## Theme and shortcuts

| Key | Notes |
|---|---|
| `theme` | `{ "mode": "system", "light": "Atlas Light", "dark": "Atlas Dark" }`: `mode` is `system`, `light` or `dark`; `light` and `dark` name the theme used for each appearance. Zed theme files (v0.2.0 format) from `~/.atlas/desktop/themes/` are picked up automatically. Bundled: Atlas Dark/Light, One Dark/Light |
| `keymap` | rebind or unbind shortcuts, Zed syntax, `null` unbinds. See below |

```json
{
  "keymap": {
    "cmd-j": null,
    "cmd-shift-j": "workspace::ToggleBottomDock"
  }
}
```

Every shortcut is a named command (`workspace::ToggleLeftDock`, `pane::SplitRight`, `atlas::NewWorktree`…). ⌘⇧P lists every command with its current key, and Settings → Keyboard shortcuts rebinds them with a recorder. Modifiers are `ctrl`, `alt`, `shift` and `cmd`, joined with `-`: `cmd-shift-p`, `ctrl-tab`, `alt-cmd-left`. ⌘Z, ⇧⌘Z and ⌘A are fixed by the app menu and cannot be rebound. The defaults are listed in [Desktop app › Keyboard](desktop.md#keyboard).

## Workspaces and agents

| Key | Notes |
|---|---|
| `workspaceDir` | folder for new worktrees (default `~/.atlas/worktrees`) |
| `branchPrefix` | `"git-username"` (default: your `git config user.name`, slugified), `"none"` or `"custom"`; the prefix of branches Atlas creates |
| `branchPrefixCustom` | the prefix used when `branchPrefix` is `"custom"` |
| `nestWorkspaces` | `"false"` flattens worktree folders instead of `<repo>/<name>` |
| `defaultAgent` | agent started by default in a new worktree: `"omp"` (default), `"claude"` or `"codex"` |
| `agentPermissions` | `"manual"` (default, the agent asks before running commands or editing files) or `"yolo"` (never asks; use only in disposable worktrees) |
| `agentArgs` | extra command-line arguments per agent |
| `notifications` | `{ "enabled": true, "sound": true, "suppressWhenFocused": true }`: native notifications for agents that finished or need you |
| `quickCommands` | commands you can send to an agent with one click |
| `openServicesIn` | `system` opens service URLs in your browser instead of the Atlas browser |

## Shell, updates and onboarding

| Key | Notes |
|---|---|
| `shell` | the shell whose environment is used for your projects, e.g. `"/opt/homebrew/bin/fish"`. Unset = your account's login shell. See [Desktop app › Shell environment](desktop.md#shell-environment) |
| `updates` | auto-check switch, skipped version and what you have seen. Managed by the app |
| `onboarding` | progress of the first-run tour and the checklist. Managed by the app |

## Related files in `~/.atlas/desktop/`

| File | Content |
|---|---|
| `projects.json` | the projects you added |
| `workspace.json` | tabs, panes and docks per worktree |
| `themes/` | your Zed theme files |
| `hook-endpoint.env` | loopback endpoint and token for agent hooks (new token per run) |
