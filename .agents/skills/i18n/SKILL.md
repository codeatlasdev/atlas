---
name: i18n
description: Adding, changing or translating any user-visible string, language or docs page in the atlas repo (CLI, TUI, desktop, site, mobile, docs). Read before touching text a person reads.
---

# Translating Atlas: procedure

English is the source. pt-BR is a required language: a PR that adds or changes English text ships the pt-BR in the same PR, and `bun run i18n:check` (inside `bun run check`) fails otherwise. This file is THE procedure. Layout and why it is built this way: `i18n/README.md`. Style: `i18n/style/`. Terms: `i18n/glossary.yaml`.

## One source tree

Every message of every surface lives once, in `i18n/messages/<area>/`: `en.json`, `pt-BR.json`, `meta.json` (English-only descriptions and flags), `lock.json` (written by the tools). An area is a small folder by topic (`settings`, `nav`, `config`); **every key of an area starts with `<area>_`**, so the key tells you the folder and the folder tells you the keys. Shared words (Cancel, Delete, Trash, key names) are defined once in `common`, `common_os`, `common_keys` and every surface gets them. `i18n/surfaces.json` says which surface consumes which areas. Nothing generated is committed: Paraglide's input is built into the gitignored `.i18n/` of each app, Rust reads the same JSON at build time.

## The loop

1. **Find before you add.** `bun run i18n:find "Cancel"` (a text) or `bun run i18n:find settings_display_language` (a key: its file:line, values and usages). If the text exists, use its key. Shared words are `common_*`: `m.common_action_cancel()`, `t!("common_action_cancel")`.
2. **Add.** `bun run i18n:add settings_host_name "Terminal host" --desc "Name of the tile in Settings > General" [--pt-BR "Host de terminais"] [--surface desktop] [--new-area]`. It picks the area from the key (the longest area name it starts with), refuses a duplicate of a common text, writes en/meta/pt-BR sorted and stamps the lock. A new area needs `--new-area --surface <s>`. Without `--pt-BR`, run `bun run i18n:translate --locale pt-BR --area <area>` (your own `ANTHROPIC_API_KEY`, or `I18N_BASE_URL` for any OpenAI-compatible endpoint such as Ollama; it validates before it writes) or write it yourself; a native speaker still reads the PR (CODEOWNERS routes it).
3. **Use it.** Desktop and site: `m.settings_host_name()`. Rust: `t!("settings_host_name")`. Never a literal.
4. **Check.** `bun run i18n:check`. Every finding is `file:line rule key message` and names the command that fixes it; `--json` for tools, `--surface <name>` for one surface, `--status` for freshness per area. `bun run i18n:fix` sorts files and rewrites locks (**never hand-write a lock**); a typo-only English edit that must not invalidate the translation: `bun run i18n:fix --restamp <key>`.

An English edit that changes the meaning makes the pt-BR **outdated** (the lock holds the hash of the English the translation was made from); retranslate, then `i18n:fix`.

**Rename or move:** `bun run i18n:mv old_key new_key` moves values, meta and lock stamps across areas and rewrites `m.old(`, `m["old"]`, `t!("old"` and `{@old}`; what it could not prove is listed, not edited. **Delete a key:** remove it from `en.json`, `meta.json` and every locale file of its area, then `i18n:fix`. **Add a language:** `bun run i18n:add-locale <tag> --owner @user`; it creates `<area>/<tag>.json` in every area and edits `locales.json`, `SUPPORTED`, `Info.plist`, the project settings and CODEOWNERS, and `supported-sync` keeps those lists equal. All of these take `--json` for agents and exit 1 with the fix command in the error.

## Message syntax (one subset everywhere)

A value is a string, or one variants object. `{name}` is a variable. `{@common_os_trash}` references another message and is inlined: "Moved to {@common_os_trash}" becomes an `os` select by itself. Plurals and `os` use `declarations` / `selectors` / `match`:

```json
"files_count": [{ "declarations": ["input count", "local countPlural = count: plural"], "selectors": ["countPlural"],
  "match": { "countPlural=one": "{count} file", "countPlural=*": "{count} files" } }]
```

An exact zero is a second selector (`"local countExact = count"`, match `countExact=0, countPlural=*`). `os` is `"input os"`, selector `os`, match `os=mac`, `os=windows`, `os=*`. Markup is `{#link}..{/link}` (Paraglide only). The Rust evaluator and the Paraglide compiler read exactly this; `i18n:check` rejects anything else.

## Where the code calls it

| Surface | Call site | Notes |
|---|---|---|
| Desktop (`apps/desktop`) | `m.area_name()` (`import * as m from "$lib/paraglide/messages"`) | Call `m.*()` inside templates, `$derived` or functions, never at module top level or cached in a plain variable: the locale is Svelte `$state` and switches live. Dates, numbers, relative time: `$lib/i18n/fmt`, never a hand-made `ago()` or `n === 1 ? "" : "s"`. `bun run i18n:compile` (and vite) generate `.i18n/` first. |
| Site (`site`) | `m.*()` in `.astro`/`.svelte` | Chrome strings and docs banners. Pages that do not exist in pt-BR get no hreflang. |
| Rust: CLI, TUI, hostd, desktop shell | `atlas_i18n::t!("area_name", name = value)` | `os` is added by the i18n layer: call sites never pass it. |
| clap `--help` | the English doc comment stays the source | The English of `cli_*` areas is generated: add a command's path to `MIGRATED` in `crates/atlas-cli/src/cli_i18n.rs`, run `UPDATE_FIXTURES=1 cargo test -p atlas-cli cli_i18n`, then write the pt-BR lines. Subcommand names, flags and env vars are API and never translate. |
| Docs | `docs/pt-BR/<slug>.md` | Front matter `title`, `source`, `source_sha` (the git blob sha of the English file at translation time, `git hash-object docs/<slug>.md`, a quoted string). When the English changes the page is **outdated**: a warning, never a block. Code blocks stay byte-identical except comments. `CHANGELOG.md` is never translated. |
| Mobile (`apps/mobile`) | gen_l10n | A surface with `"arb": "<dir>"` in surfaces.json: `i18n:generate` writes the `.arb` files (ICU plural/select, descriptions) at build time. |

## What i18n:check enforces beyond the catalogs

`unused` (a key none of its consuming surfaces references; `common*`, generated and `cli_*` areas exempt; dynamic use: `// i18n-keys: prefix_*`), `undefined-key` / `unconsumed-key` (a `m.x(` or `t!("x")` whose key is not in the tree, or is in an area that surface does not consume), `dup-key` (one key in two areas), `dup-common` (an English text equal to a common one: use it, or say why in meta `distinct`), `dup-value` (warning), `common-sentence` (common areas hold terms, not sentences), `budget` (an area file over 150 keys or 24 KB: split it by key prefix), `dangling-ref`, `unconsumed-area`, `supported-sync`, plus the writing lint and the ratchets below.

## Never translate

`--json`, MCP tools and descriptions, `skills/`, logs, exit codes, the hostd protocol (stable `code` plus `args`; the English `message` is for logs and old clients), install scripts, `llms.txt`, `CHANGELOG.md`, push relay payloads (only event kind, pane key and `<agent> · <folder>` ever leave the machine). Human output (`println!` for a person, `error:`, `help:`, `▸` lines) is translated; **a program never parses it.**

**The machine channel.** A command that something else drives takes `--progress json` and prints NDJSON on stdout, one event per line: `{"v":1,"kind":"step"|"info"|"hint"|"error","code":"<stable dotted id>","args":{...},"message":"<English>"}` (contract and `Coded` error type: `crates/atlas-core/src/progress.rs`; `atlas crew install --progress json` is the first user). A consumer branches on `kind` and `code`, never on text; a new failure a client must tell apart gets a `code` where the error is created (`progress::Coded::new("sudo.password_rejected", args, english_message)`). Atlas Desktop must read only this channel; the old `step:` lines stay for old clients until the migration of `installProgress.ts`. `crates/atlas-cli/tests/machine_output.rs` runs the machine commands under `ATLAS_LANG=en` and `ATLAS_LANG=pt-BR` and compares the bytes, and `tests/crew_install.rs` does it for the install events: add a row for every new machine surface.

## Platform words (OS-aware text)

Atlas runs on macOS, Windows and Linux; no message may say "Mac" or spell a shortcut by hand.

- **Shortcuts** are a placeholder in the message (`{key}`), filled by the caller: desktop `kbd(stroke)` / `label()` / `modClick()` in `apps/desktop/src/lib/keys.ts` (follows OS and the user's remaps), Rust `atlas_i18n::shortcut("cmd-shift-p")` (`⌘⇧P` on macOS, `Ctrl+Shift+P` elsewhere; key names such as `Space` come from `common_keys_*`, pt-BR `Espaço`).
- **System nouns** come from the shared terms (`common_os_your_computer`, `common_os_file_manager`, `common_os_trash`, `common_os_system_settings`, `common_os_alt_key`, `common_os_enter_key`, `common_os_menu_bar`; the `os` entries in `i18n/glossary.yaml` list each platform's wording). Reference one as `{@common_os_trash}`: the message becomes an `os` select (`mac`|`windows`|`*`) by itself, and the i18n layer passes `os`, so call sites never do. pt-BR needs "no Finder" / "na bandeja": when a preposition precedes the noun, write the whole phrase with its own `os` select instead of composing.
- A screen that exists on one platform only (macOS capture permission) may name it; mark the key in `meta.json`: `{ "description": "...", "platform": "mac" }`. `i18n:check` rejects glyphs and `Cmd+`/`Ctrl+` chords in catalog values, platform nouns in a message without an `os` select, and an ad hoc `isMac` / `navigator.platform` outside `keys.ts` (`IS_MAC`).

## pt-BR rules

- Glossary terms stay English: Atlas, Crew, crewmate, hostd, worktree, branch, commit, merge, pull request, deploy, prompt, token, log, build. Check `i18n/glossary.yaml` before inventing a rendering.
- Register: você. Buttons and menu items in the infinitive ("Salvar", "Abrir projeto"). Sentence case. No em dashes. No gerundism ("vou estar enviando"). Errors say what happened, then what to do.
- Brazilian, never European: tela (not ecrã), usuário, arquivo, configurações, salvar, excluir, baixar. `i18n:check` has a deny-list.
- **Plurals.** pt-BR needs `one` and `other`; CLDR puts 0 in `one`, so a plain `one` variant prints "0 arquivo". Add an exact zero variant where that reads wrong ("Nenhum arquivo"): a `countExact=0` match (second selector) before `one`. Keep a default (`*[other]`) because engines disagree on large numbers.
- Numbers, dates, lists and relative time come from `Intl` (desktop `fmt`) or are written into the message; never concatenate translated fragments into a sentence.

## Where the language comes from

One implementation, `atlas_core::lang` (CLI, TUI, hostd, desktop shell), precedence `ATLAS_LANG`, `~/.atlas/language`, `LANGUAGE`/`LC_ALL`/`LC_MESSAGES`/`LANG` (C/POSIX = English), the OS, English; any `pt*` selects pt-BR. `atlas config lang` shows it and what decided it; `atlas config lang <tag>` and Desktop Settings write `~/.atlas/language`. The desktop receives the resolved tag from Rust (a Tauri command) and never reads `navigator.language`. Mobile follows the phone.

## The ratchets (text that must not spread)

`i18n:check` also flags hardcoded user-facing text (Svelte, Rust `println!`/`bail!`), spelled shortcuts, ad hoc platform checks and flagged phrases in existing docs. These are **ratchets**: they judge your change, not a snapshot.

- For each file you touch, the offenders at the merge-base are counted with the same detector; only the ones you **add** fail, with `file:line`. Old offenders in a file you edit are fine; moving them is fine; a file you do not touch is never looked at. So **when main adds screens there is nothing to re-snapshot**, and the baseline never needs a hand edit.
- Fix a new finding by using a message key (or `// i18n-ignore` / `<!-- i18n-ignore -->` when it is not for users). Never raise `i18n/hardcoded-baseline.json` to make it pass: a raised count fails (`baseline-grew`). `bun run i18n:fix` only lowers it.
- Run `git fetch origin main` before checking locally: the base is `I18N_BASE`, else `origin/main`, else `main`. With no usable base (shallow clone, no ref) the check compares with the committed baseline instead and says so.

## Traps that cost time

- A broken Paraglide plugin path compiles zero messages with only a warning: the desktop has a test that the compiled `m` exposes the catalog's keys.
- Paraglide `setLocale` with a custom Svelte `$state` strategy throws `state_unsafe_mutation` on the first render unless it writes only when the value changed. Keep the guard.
- The Rust evaluator does no number formatting: format numbers in Rust or avoid them in the message.
- A third-party name inside a rename sweep: never translate or rewrite external API strings (Telegram `/bot<token>/`, `is_bot`).
- Test assertions on English text run in a fixed locale; set it explicitly instead of relying on the machine's.
