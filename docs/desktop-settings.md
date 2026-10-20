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
| `enable_language_servers` | `true` | language servers (completion, diagnostics, hover, go to definition) |
| `format_on_save` | `false` | format with the language server when saving |
| `inlay_hints.enabled` | `true` | inferred types and parameter names shown inline |

## Code editor

Settings → Appearance → Editor.

| Key | Default | Notes |
|---|---|---|
| `editor.tab_size` | `4` | 1–16 columns |
| `editor.insert_spaces` | `true` | Tab inserts spaces; `false` inserts tab characters |
| `editor.word_wrap` | `false` | wrap long lines at the edge of the editor |
| `editor.render_whitespace` | `"none"` | `none`, `selection` or `all` |
| `editor.line_numbers` | `true` | |
| `editor.minimap` | `false` | |
| `editor.sticky_scroll` | `true` | keeps the enclosing function or class pinned while you scroll its body |
| `editor.bracket_pair_colorization` | `true` | |
| `editor.indent_guides` | `true` | |
| `editor.rulers` | `[]` | columns for vertical lines, whole numbers 1–500, e.g. `[80, 100]`; anything else in the list is dropped |
| `editor.highlight_active_line` | `true` | |
| `editor.trim_trailing_whitespace_on_save` | `true` | |
| `editor.insert_final_newline_on_save` | `true` | |
| `editor.large_file_threshold_bytes` | `2000000` | 100000–100000000; bigger files open in a light mode without language features |
| `editor.scroll_past_end` | `true` | |
| `editor.smooth_scrolling` | `true` | |
| `editor.auto_reload` | `true` | reload a file changed on disk when it has no unsaved edits |

## Language servers

Settings → Appearance → Languages lists every language server Atlas knows, shows whether it is on your `PATH`, managed by Atlas or missing, and lets you install, update, uninstall, restart it and open its log. Atlas runs a server from your `PATH` when it finds one, and otherwise downloads one the first time you open a file of that language.

`lsp` replaces the server of a language or changes how it starts. Keys are LSP language ids (`typescript`, `rust`, `go`, `python`…); the same shape works in a project's `.atlas/settings.json`.

```json
{
  "lsp": {
    "typescript": {
      "command": "/opt/homebrew/bin/typescript-language-server",
      "args": ["--stdio"],
      "initialization_options": { "preferences": { "importModuleSpecifier": "relative" } },
      "settings": { "typescript": { "inlayHints": { "parameterNames": { "enabled": "all" } } } }
    },
    "python": { "disabled": true }
  }
}
```

| Key | Notes |
|---|---|
| `lsp.<language>.command` | run this program instead of the one Atlas finds or installs: a name on `PATH` or an absolute path. If it cannot be started the status bar says so |
| `lsp.<language>.args` | arguments for `command` |
| `lsp.<language>.initialization_options` | JSON object sent to the server once, when it starts |
| `lsp.<language>.settings` | JSON object sent as the server's workspace configuration |
| `lsp.<language>.disabled` | `true` turns language features off for that language |

Entries of the wrong type are dropped one by one (a `command` that is not a string, `args` that is not a list of strings, a non-object option), and an entry left empty is removed, so a bad edit never stops the others from working. Changes apply to servers started afterwards; **Restart** in the Languages tab applies them to a running one.

### Per-project overrides

A project can carry its own overrides in `.atlas/settings.json` at the root of the worktree, with the same `lsp` shape:

```json
{
  "lsp": {
    "go": { "command": "/usr/local/bin/gopls", "settings": { "gopls": { "staticcheck": true } } }
  }
}
```

The project file wins over `~/.atlas/desktop/settings.json` field by field for the language it names (`command`, `args` and `disabled` are replaced; `initialization_options` and `settings` are merged deeper, project keys winning); languages and fields it does not mention keep your own settings. Settings → Languages links to the file (`.atlas/settings.json` in the footer) and creates it when it does not exist. An unreadable file is ignored and reported in the editor's language server status.

## Terminal

| Key | Default | Notes |
|---|---|---|
| `terminal.font_family` | `SF Mono, Menlo, ui-monospace, monospace` | CSS font list for both terminals. The [GPU terminal](terminal.md) uses the first installed family (variable fonts work), else its bundled JetBrains Mono; applies live |
| `terminal.font_thicken` | `false` | draw text slightly heavier (Ghostty `font-thicken`; xterm.js uses weights 500/700); applies live |
| `terminal.font_size` | `12.5` | 6–48; applies live |
| `terminal.cursor_shape` | `"block"` | `block`, `bar`, `underline` or `hollow` |
| `terminal.blinking` | `false` | |
| `terminal.max_scroll_history_lines` | `10000` | 0–100000 |
| `terminal.osc52` | `"ask"` | what happens when a program writes your clipboard: `allow`, `ask` or `deny`. Reading the clipboard is always denied |
| `terminal.ligatures` | `true` | GPU terminal |
| `terminal.minimum_contrast` | `4.5` | 1–21; WCAG ratio a cell's text is lifted to at draw time (the theme's ANSI table is not changed), `1` = off. GPU terminal |
| `terminal.minimum_contrast_chosen` | `false` | Written `true` when you set the minimum contrast yourself. A stored `minimum_contrast` of `1` without it is the pre-4.5 default (the Terminal tab used to write it back) and is read as `4.5`; set `1` in the Terminal tab to keep it off |
| `terminal.copy_on_select` | `true` | GPU terminal |
| `terminal.option_as_alt` | `"both"` | `both`, `left`, `right` or `none`. GPU terminal |
| `terminal.click_to_move` | `true` | a click on the prompt moves the cursor (needs shell integration). GPU terminal |
| `terminal.padding_x` | `10` | 0–64 pt; inset left and right of the terminal. The margin uses the terminal theme's background |
| `terminal.padding_y` | `6` | 0–64 pt; inset above and below |

The GPU terminal itself is switched with **Settings → Appearance → Terminal → Fast terminal (GPU)**.

## Theme and shortcuts

| Key | Notes |
|---|---|
| `theme` | `{ "v": 2, "mode": "system", "light": "Atlas Light", "dark": "Atlas Dark", "terminal": "app" }`: `mode` is `system`, `light` or `dark`; `light` and `dark` name the theme used for each appearance; `terminal` is `app` (same as the app) or the name of any theme. Defaults are Atlas Dark and Atlas Light (a Tokyo Night based pair, see [Atlas Dark and Light](#atlas-dark-and-light)); Dracula and Alucard by Zeno Rocha and Luxonauta (MIT) stay bundled, and anyone who picked them keeps them. Your own themes live in `~/.atlas/themes/` as Zed theme files (v0.2.0 format) and are reloaded as you save; the older `~/.atlas/desktop/themes/` is still read. A theme name that no longer exists falls back silently to Atlas Dark (dark) or Atlas Light (light), and the terminal to the app theme. Settings → Appearance imports Ghostty configs, iTerm `.itermcolors`, VS Code theme `.json` and `.vsix`, Alacritty `.toml`/`.yml` and Base16 YAML, or a `.zip` / folder holding several themes (unpacked to a temporary folder on this Mac, nothing is uploaded; bring your own licensed copy, Atlas does not include paid themes), and exports a theme as JSON or a Ghostty config. Bundled: Atlas Dark/Light, Dracula, Alucard, One Dark/Light |
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
| _update channel_ | not a key: the channel is the one word in `~/.atlas/update-channel`, shared with the `atlas` CLI (`ATLAS_UPDATE_CHANNEL` overrides it per process). Choose it in Settings → General → Updates → _Update channel_. See [Desktop app › Updates](desktop.md#updates) |
| `onboarding` | progress of the first-run tour and the checklist. Managed by the app |

## Related files in `~/.atlas/desktop/`

| File | Content |
|---|---|
| `projects.json` | the projects you added |
| `workspace.json` | tabs, panes and docks per worktree |
| `themes/` | legacy location of your Zed theme files (still read; new ones go to `~/.atlas/themes/`) |
| `hook-endpoint.env` | loopback endpoint and token for agent hooks (new token per run) |

## Atlas Dark and Light

The default theme pair. Atlas Dark is built from the maintainer's Ghostty "Tokyo Night" custom palette (colors by [enkia](https://github.com/enkia/tokyo-night-vscode-theme), MIT): its 16 ANSI colors, foreground, cursor and syntax hues are used as they are, and the UI ground is rebuilt on one blue ramp. Everything lives in `apps/desktop/src/lib/themes/atlas.json`; the terminal, service logs, editor and chrome all read it, and the GPU terminal's fallback table (`atlas-term` `colors.rs`) and the `.dark` `--ansi-*` fallbacks in `app.css` are tests pinned to it.

Why it looks the way it does (what Dracula, Tokyo Night, Catppuccin and One share, adapted to real panels):

| Decision | Value | Reason |
|---|---|---|
| Surfaces on one hue | OKLCH hue ~262, chroma 0.029 to 0.036; `#0b1320` sunken, `#131c2b` editor and terminal, `#1d2637` raised, `#273143` overlay | A single blue hue reads as one material. Chroma stays low on large areas (calm, no saturated slabs) but above 0.025 so an IPS panel with backlight glow does not turn it grey. |
| Even steps | about 4% OKLCH lightness between levels (0.186, 0.226, 0.268, 0.312) | Visible on IPS and low-contrast panels, so panels, popovers and the sidebar separate without heavy borders. |
| No pure black | the darkest surface is L 0.186 | OLED black smears and crushes shadow detail while scrolling; near-black blue keeps pixels on and gradients smooth. |
| Soft foreground | text `#c0caf5` (L 0.85), never pure white | Bright text on a dark ground blooms (halation, PWM shimmer on OLED) and thins glyph edges; 12:1 on the editor is plenty. |
| Accents at equal lightness | red, orange, blue, magenta about L 0.75; yellow, green, cyan about L 0.80, chroma at most 0.15 | Dracula's rule: fixed roles, high-saturation accents on a desaturated ground. Equal perceived lightness keeps one accent from shouting over another, and every one is 5.5:1 or more on the overlay surface. |
| sRGB-safe chroma | no accent is pushed past what sRGB holds | A P3 display shows the same color; nothing clips on sRGB or shifts on wide gamut. |
| Readable at 1x | syntax colors fitted to at least 4.7:1 on every code surface (line, selection, active line); comments `#acbae2` italic | Anti-aliasing on a low-DPI panel thins light-on-dark text, so margin above AA 4.5 beats being on the line. |
| Syntax roles | keyword purple, function green, string amber, type and namespace cyan, constant orange, property blue | One hue per role everywhere (Dracula's spec), the Tokyo hues. |

Atlas Light is its Tokyo Night Day counterpart: pale blue-grey ground (`#e1e2e7`), navy text, the same role-to-hue map with deeper values so every color keeps AA on the pale ground. `bun run test` (`theme/atlasDefault.test.ts`) pins the palette, the OKLCH ladder rules above and the default selection; `bun scripts/contrast-audit.ts` checks the rendered screens for both.

