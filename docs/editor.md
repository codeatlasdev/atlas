---
title: "Code editor"
description: "The editor in Atlas Desktop: language servers, editing shortcuts, find and replace, large files, external changes and the status bar."
---

# Code editor

Atlas Desktop opens text files in a CodeMirror 6 editor with built-in language servers. This page covers what it does, how to configure it and how its performance is kept predictable. The keys are listed in [Desktop settings](desktop-settings.md); shortcuts can be rebound in Settings → Keyboard shortcuts.

## The status bar

Under every text editor:

| Item | Click to |
|---|---|
| `Ln 12, Col 5 (3 selections) (24 selected)` | go to a line (⌃G) |
| `3 errors, 1 warning` | focus the editor at the first problem |
| `Spaces: 2` / `Tab Size: 4` | change the indentation, or convert the file's indentation |
| `UTF-8` | (read-only: Atlas edits UTF-8 text; other files open as binary) |
| `LF` / `CRLF` / `Mixed` | choose the line ending written on save |
| language name | (the syntax mode follows the file name) |
| language server chip | see below |

Above the editor, **breadcrumbs** show the path and the symbols around the caret (from the language server's document symbols, or from the syntax tree when there is none); click a symbol to jump to it.

## Language servers

Atlas runs language servers inside `atlas hostd` (one process per language and project, shared by every open file of it). Supported languages: TypeScript, JavaScript, Rust, Go, Python, PHP, HTML, CSS (SCSS, Less), JSON, YAML and Shell. A server on your `PATH` is used first; otherwise Atlas downloads a pinned, checksum-verified one into `~/.atlas/lsp` the first time you open a file of that language.

The status bar chip is always there for a text file and always says why, in one of these states. Click it for details and actions:

| State | What it means | Actions |
|---|---|---|
| **Ready** | the server's name and version, and whether it comes from `PATH`, Atlas or your settings | restart, open log |
| **Starting / Installing** | what is happening, with progress when the server reports it | cancel |
| **No language server** | there is none for this language; the popover lists the supported ones | docs, settings |
| **Language features need a newer terminal host** | the running `atlas hostd` predates language servers (see below) | restart terminal host, later |
| **Off in settings** | `enable_language_servers` is off, or the language is `disabled` | turn on, settings |
| **Needs Go** (gopls) / **needs curl** | the toolchain to install the server is missing | copy the install command, retry |
| **Offline / install failed** | the download failed; the message says what | retry, open log |
| **Crashed** | the server stopped; the last lines of its log are shown | restart, open log |
| **Custom command not found** | `lsp.<language>.command` is not on `PATH` | settings |

Pressing F12, F2, ⇧F12 or ⌘. in a file with no server explains why once, with the same actions, instead of doing nothing.

### An old terminal host

`atlas hostd` outlives app updates, so after one it can be older than the app. A host from before language servers cannot serve them. Atlas compares them on every connect:

- **A host that supports live upgrade** is replaced automatically. Your terminals keep running and the files reconnect by themselves.
- **An older host with terminals running** cannot hand them over. Atlas never ends a terminal without asking: the chip says _Language features need a newer terminal host_ and offers **Restart terminal host**, which lists the running terminals (`title — folder`) and says they will end, or **Restart later** (asked again on the next app start).
- **No terminal running**: the host is restarted silently.

The terminal host's own lifecycle is documented in [Desktop app](desktop.md#keeping-the-terminal-host-current).

### Configure a server

Settings → Editor → **Languages** lists every server with its state, version and whether it is on your `PATH` or managed by Atlas, and lets you install, update, uninstall, restart it and open its log. For each language you can set a custom `command`, `args`, `initialization_options` and `settings`, or turn it off.

The same settings can live in the project, in `.atlas/settings.json` at the worktree root:

```json
{
  "lsp": {
    "typescript": {
      "command": "/opt/tsserver/bin/typescript-language-server",
      "args": ["--stdio"]
    },
    "rust": { "settings": { "rust-analyzer": { "check": { "command": "check" } } } }
  }
}
```

The project file wins over `~/.atlas/desktop/settings.json` for the language it names; see [Desktop settings](desktop-settings.md#language-servers) for the exact rules. An invalid file is ignored and reported in the status chip.

Server logs are in `~/.atlas/lsp/logs/<server>.log`; the chip's **Open log** shows them.

## Editing

macOS shortcuts (all rebindable where the shell owns the key):

| | |
|---|---|
| Multiple cursors | ⌥-click, ⌘D next occurrence, ⌘⇧L all occurrences, ⌥⌘↑ / ⌥⌘↓ cursor above / below |
| Column selection | ⌥⇧-drag |
| Move / copy line | ⌥↑ ⌥↓ / ⌥⇧↑ ⌥⇧↓ |
| Delete line · select line | ⌘⇧K · ⌘L |
| Comment | ⌘/ line, ⌥⇧A block |
| Indent / outdent | Tab, ⇧Tab, ⌘] ⌘[ |
| Fold | click the gutter, ⌥⌘[ ⌥⌘], fold all and unfold all from the command palette |
| Go to line | ⌃G |
| Expand / shrink selection | ⌥⇧→ / ⌥⇧← |
| Word wrap | ⌥Z |
| Format document | ⇧⌥F |
| Save · save all · revert | ⌘S · ⌥⌘S · Revert file in the toolbar and the command palette |

When the cursor is inside an editor, ⌘/ ⌘[ ⌘] ⌘L and ⌘⇧\ edit the text. The shell commands they normally run (Help, Back, Forward, Logs, Split down) stay in the command palette and the menus.

Also built in: smart indentation, auto-closing brackets, quotes and tags, bracket pair colors, indent guides, rulers (`editor.rulers`), whitespace rendering (`editor.render_whitespace`), matching-bracket and selection highlighting, sticky scroll of the enclosing function or class, and an optional minimap (`editor.minimap`, off by default).

### Find and replace

⌘F opens find, ⌥⌘F or ⌘H adds replace. Toggles for case (Aa), whole word and regex, **in selection** (⌥⌘L), `n of m` counter (capped at 10 000), previous / next, select all matches (⌥Enter), replace and replace all, and a preview of what the current match becomes (regex groups `$1` resolved).

### Saving

Autosave runs 0.5 s after the last edit (⌘S saves at once). On save Atlas trims trailing whitespace and ensures a final newline (`editor.trim_trailing_whitespace_on_save`, `editor.insert_final_newline_on_save`), and writes the file with its own line ending. [`.editorconfig`](https://editorconfig.org) is honored (`indent_style`, `indent_size`, `tab_width`, `end_of_line`, `trim_trailing_whitespace`, `insert_final_newline`) and wins over the preferences. A file's indentation is detected from its content when there is no `.editorconfig`. The undo history of a file lasts as long as its tab is open.

### Changes made outside the editor

Atlas watches the open file. If it changes on disk and you have no unsaved edits it reloads (`editor.auto_reload`), keeping the caret. With unsaved edits it asks: **Load from disk** or **Keep my version** (which writes yours over it); autosave pauses meanwhile so it never overwrites the other change silently. A deleted file is reported and saving writes it again.

## Large files

Text files up to 32 MB open. From `editor.large_file_threshold_bytes` (2 MB by default), or when a line is longer than 10 000 characters (minified bundles), the editor switches to a lighter mode and says so in a notice and in the status bar: word wrap, bracket colors, indent guides, sticky scroll, minimap, selection matches, inlay hints and language features are off (the syntax tree is built on demand around what you see, as always). From five times the threshold, or a line over 200 000 characters, syntax highlighting and code folding are off too.

## Layout

The editor always fills its pane, whatever the file holds: the scroll area is the whole pane, horizontal scrolling appears only when a line is longer than the pane, the line numbers stay put while scrolling sideways, and scrolling never moves the window behind it.
