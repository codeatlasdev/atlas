# Writing English for Atlas

English is the source language. Read [`README.md`](README.md) first. Every string you write here gets translated, so write it so a translator (often an agent) cannot misread it.

## Mechanics

- **Sentence case** for everything, including buttons and dialog titles: `Install crew server`, not `Install Crew Server`. Atlas already writes this way, and title case in a source string tends to leak into translations as odd capitals (the Microsoft pt-BR guide documents this problem).
- **Contractions are fine** in UI and docs (`can't`, `isn't`, `you're`). In CLI errors keep the fixed phrase `couldn't` or `can't`.
- **Numerals for counts**: `3 worktrees`, not `three worktrees`. A space between number and unit: `10 MB`, `30 s`, `2 min` ([Vercel](https://vercel.com/design/guidelines)).
- **Dates and times**: relative in the UI (`5 min ago`, `yesterday`) through the shared formatter, never hand-built. Absolute timestamps in logs and CLI output use ISO 8601 (`2026-11-16 14:05`).
- **Plurals go through the message format**, never `n > 1 ? "s" : ""`. Write the zero case as words: `No worktrees`, not `0 worktrees`.
- **No "we".** Name the actor: `Atlas`, `hostd`, `your agent`. Apple's guidance: "we" in an error leaves it unclear who "we" is ([Apple HIG](https://developer.apple.com/design/human-interface-guidelines/writing)).
- **No `please`** in instructions and no `sorry` in errors, unless data was lost ([Google](https://developers.google.com/style/tone), [Atlassian](https://atlassian.design/content/writing-guidelines/writing-error-messages)).
- **Concatenate nothing.** One full sentence per message with named placeholders: `Removed {name} from {server}`. Fragments glued in code cannot be translated (word order differs in Portuguese).
- **Placeholders say what they hold**: `{branch}`, `{count}`, `{server}`. Never `{0}`, `{s}`.

## By surface

### Buttons and menu items

Verb first, then the object: `Create worktree`, `Install crew server`, `Copy logs for AI`. Standard one-word actions stay one word: `Done`, `Cancel`, `Close`, `Retry` ([Polaris](https://web.archive.org/web/20241202110733/https://polaris.shopify.com/content/actionable-language), [Atlassian](https://atlassian.design/content/writing-guidelines/writing-error-messages)).

| Don't | Do | Why |
|---|---|---|
| `OK` | `Delete worktree` | The label says what happens |
| `Continue` | `Save API key` | Specific beats generic ([Vercel](https://vercel.com/design/guidelines)) |
| `Yes` / `No` | `Delete 3 worktrees` / `Keep` | Each answer names its result ([NN/g](https://www.nngroup.com/articles/confirmation-dialog/)) |
| `Click here` | `Read the crew docs` | Link text must make sense alone |
| `Let's go!` | `Add project` | Apple: "Send" works better than "Let's do it!" |

### Errors

Three parts, in this order: what happened, why (if Atlas knows), what to do. Show it next to the thing that failed and keep the user's input.

| Don't | Do |
|---|---|
| `An error occurred.` | `Couldn't save atlas.yaml: the file is read-only. Make it writable and save again.` |
| `Invalid version` | `` 1.5 is not a version. Use X.Y.Z, for example 1.5.0. `` |
| `sudo password rejected`, an em dash, `check your password and try again` (today's text in `crew/privileged.rs`) | `Wrong sudo password. Try again.` |
| `Oops! Something went wrong 😕` | `Couldn't start api: port 3000 is in use by another process (pid 4182).` |
| `Failed.` | `Push failed: the remote has commits you don't have. Pull, then push again.` |

Atlas already does this well in places: `No projects yet. Add any project folder: Atlas detects how to run it.` and `Not the server name. Nothing was deleted.` Follow those.

### CLI output

The CLI follows Unix and compiler conventions ([clig.dev](https://clig.dev/), [rustc](https://rustc-dev-guide.rust-lang.org/diagnostics.html)):

```text
error: cannot reach crew server box: ssh timed out after 10 s
help: check that box is up, then run `atlas crew status box`
```

- `error:`, `warning:`, `help:` prefixes, lowercase start, no final period for a single clause. The fix lives in the `help:` line, not in the error line.
- Results to stdout, progress and errors to stderr. The most important line comes last, where the eye lands.
- Progress lines say what is happening now, in the present continuous with a real object: `▸ uploading atlas 1.5.0 to box`.
- Success is one line with the result, not a celebration: `✓ Browsers ready on box: crewmates can use the capture tool.`
- An unexpected failure prints where the full log is and how to report it, not a stack trace.

### Toasts

Past tense, short, no final period when it is a fragment: `Worktree created`, `Copied 120 lines`. At most one action (`Undo`, `Open`). A toast that needs a decision is a dialog, not a toast.

| Don't | Do |
|---|---|
| `Success! Your worktree has been created successfully.` | `Worktree created` |
| `atlas.yaml was saved` | `atlas.yaml saved` |

### Empty states

What is empty, why, and the one action that fills it ([NN/g](https://www.nngroup.com/articles/empty-state-interface-design/), [Apple HIG](https://developer.apple.com/design/human-interface-guidelines/writing)).

| Don't | Do |
|---|---|
| `Nothing to see here!` | `No tasks yet. Send one below.` |
| `It looks like you don't have any automations. Automations are a great way to...` | `No automations yet. Add one to start tasks on a schedule, a webhook or a GitHub event.` [Add automation] |
| `No results` | `No integrations match "lin". Clear the search to see all 42.` |

### Confirmations

Only for actions that destroy work or cannot be undone; offer undo instead where possible. Name the object, the count and the loss. The confirm button repeats the verb. For the most dangerous actions (uninstall a crew server), ask the person to type the name ([NN/g](https://www.nngroup.com/articles/confirmation-dialog/)).

```text
Title:   Uninstall Atlas Crew from box?
Body:    This stops 4 crewmates and deletes their users, memory and task history on box.
         Type box to confirm.
Buttons: [Uninstall from box]  [Cancel]
```

Don't: `Are you sure?` with `OK` / `Cancel`.

### Notifications

The title says who and where, the body says what changed and what it needs. On the push relay, only the event kind and `<agent> · <folder>` leave the machine (privacy invariant, `crates/atlas-host/src/agents/push.rs`).

| Don't | Do |
|---|---|
| `🎉 Great news! Your agent finished its task!` | `claude · atlas` / `Done` |
| `Attention required` | `codex · web` / `Waiting for your approval` |

### Onboarding

Each step does one real thing and the copy says what that thing is. No welcome speech, no feature tour before the first useful action.

| Don't | Do |
|---|---|
| `Welcome to the future of AI-powered development!` | `Add a project folder to start.` |
| `Atlas Crew lets you effortlessly unlock 24/7 agents` | `Run agents on your own Linux server, without your laptop. You need SSH access and sudo on it.` |
| `Let's get you set up! 🚀` | `Step 1 of 3: Connect to your server` |

### Docs

- Task-first headings in the imperative: `Install a crew server`, not `Crew server installation`.
- Start with the outcome and the requirements, then the steps. One idea per paragraph.
- Every command in a fenced block, copyable as is, with the expected output when it helps.
- No `simply`, `just`, `easy`, `quickly` in procedures ([Google](https://developers.google.com/style/tone)). If a step were easy, the reader would not be in the docs.
- Explain a term the first time it appears, link the glossary after that.

### Commit messages and changelog

Written by agents most of the time, read by people and by the release notes. Same rules: specific, no hype, no dash. `fix(crew): keep the SSH key when install retries` beats `fix: improve crew robustness`.

## Platform words

Atlas runs on macOS, Windows and Linux. No string assumes one of them.

- **Native menus (menu bar, tray, context menus) take the casing of their own system.** AppKit, Windows and GTK add their own items next to ours (macOS puts "Start Dictation…" and "Emoji & Symbols" in Edit), so an item in another casing looks broken. macOS: title case, as Apple's own menus (`Select All`, `Hide Others`). Windows: sentence case, as the [Microsoft Writing Style Guide](https://learn.microsoft.com/en-us/style-guide/capitalization) and the [Windows menu guidance](https://learn.microsoft.com/en-us/windows/win32/uxguide/cmd-menus) require for menu labels (`Select all`). Linux: header capitalization, as the [GNOME HIG](https://developer.gnome.org/hig/guidelines/writing-style.html) asks for menu items (`Select All`). Title case keeps articles, conjunctions and short prepositions lowercase except as first or last word, capitalizes the particle of a phrasal verb (`Zoom In`) and each part of a hyphenated compound edge (`All-in-One Capture`). The message is an `os` select; buttons, dialogs and the rest of the app stay sentence case.

- **A shortcut never sits inside message text.** The message takes a placeholder (`{key}`) and the code passes the platform's own spelling (`⇧⌘P` on macOS, `Ctrl+Shift+P` elsewhere; desktop: `kbd()` / `label()` / `modClick()` in `apps/desktop/src/lib/keys.ts`, Rust: `atlas_i18n::shortcut`). Remapped keys then show correctly too.
- **System nouns come from the shared terms** (`os` entries in `i18n/glossary.yaml`, messages `common_os_*`), selected on an `os` variable (`mac`, `windows`, `linux`) that the i18n layer adds to every message. Call sites never pass it. Write `your Mac` / `this PC` / `this computer`, `Finder` / `File Explorer` / `the file manager`, `Trash` / `Recycle Bin`, `System Settings` / `Settings`, `Option` / `Alt`, `Return` / `Enter`, `the menu bar` / `the system tray` through them, never by hand.
- **A screen that exists on one platform only** (the macOS screen-capture permission) may name that platform; mark its key `platform: mac`.
- **Don't branch on the platform in code to pick words.** Use `os` in the message; one message with variants translates as one unit.
