---
name: i18n
description: Adding, changing or translating any user-visible string, language or docs page in the atlas repo (CLI, TUI, desktop, site, mobile, docs). Read before touching text a person reads.
---

# Translating Atlas: procedure

English is the source. pt-BR is a required language: a PR that adds or changes English text ships the pt-BR in the same PR, and `bun run i18n:check` (inside `bun run check`) fails otherwise. Layout, formats and the lock design: `i18n/README.md`. Style: `i18n/style/`. Terms: `i18n/glossary.yaml`.

## The loop (any surface)

1. Write the English and its **description** (what UI element it is, where, what a word means: "Run" = execute, not jog). A description that repeats the text is rejected.
2. Write the pt-BR next to it. Without a native speaker or a translator at hand: `bun run i18n:translate` drafts only the missing or changed keys (your own `ANTHROPIC_API_KEY`, or `I18N_BASE_URL` for any OpenAI-compatible endpoint such as Ollama). It validates before it writes; a native speaker still reads the PR (CODEOWNERS routes it).
3. `bun run i18n:check`. Every finding is `file:line rule key message`; `--json` for tools, `--surface <name>` for one surface, `--status` for freshness per area.
4. `bun run i18n:fix` sorts the catalogs and rewrites `i18n/lock/**` and the hardcoded baseline. **Never hand-write a lock file.** A typo-only English edit that must not invalidate the translation: `bun run i18n:fix --restamp <key>`.

An English edit that changes the meaning makes the pt-BR **outdated** (the lock holds the hash of the English the translation was made from); translate again, then `i18n:fix`.

## Per surface

| Surface | Call site | Catalog | Notes |
|---|---|---|---|
| Desktop (`apps/desktop`) | `m.area_name()` (`import { m } from "$lib/paraglide/messages.js"`) | `apps/desktop/messages/{en,pt-BR}.json`, descriptions in `descriptions.json` | Keys flat, `area_snake_name`, area = text before the first `_`. Call `m.*()` inside templates, `$derived` or functions, never at module top level or cached in a plain variable: the locale is Svelte `$state` and switches live. Dates, numbers, relative time: `$lib/i18n/fmt`, never a hand-made `ago()` or `n === 1 ? "" : "s"`. |
| Site (`site`) | `m.*()` in `.astro`/`.svelte` | `site/messages/` | Only chrome strings so far. Pages that do not exist in pt-BR get no hreflang. |
| Rust: CLI, TUI, hostd, desktop shell | `atlas_i18n::t!("area-thing", name = value)` | `crates/atlas-i18n/locales/<lang>/<area>.ftl` | Ids kebab-case, description = the `#` comment above the message. Small files per area; one test, `cargo test -p atlas-i18n`, names what is missing or unused. |
| clap `--help` | the English doc comment stays the source | `locales/<lang>/cli.ftl` (English file generated) | A command is migrated by adding its path to `MIGRATED` in `crates/atlas-cli/src/cli_i18n.rs`, then `UPDATE_FIXTURES=1 cargo test -p atlas-cli cli_i18n`, then the pt-BR lines. Subcommand names, flags and env vars are API and never translate. |
| Docs | `docs/pt-BR/<slug>.md` | front matter `title`, `source`, `source_sha` | `source_sha` is the git blob sha of the English file at translation time (`git hash-object docs/<slug>.md`), a quoted string. When the English changes the page is **outdated**: a warning, never a block. Code blocks stay byte-identical. `CHANGELOG.md` is never translated. |
| Mobile (`apps/mobile`) | gen_l10n | ARB | Lands in its own PR with a checker file `scripts/i18n/checks/arb.ts`. |

## Never translate

`--json`, MCP tools and descriptions, `skills/`, logs, exit codes, the hostd protocol (stable `code` plus `args`; the English `message` is for logs and old clients), install scripts, `llms.txt`, `CHANGELOG.md`, push relay payloads (only event kind, pane key and `<agent> · <folder>` ever leave the machine). Human output (`println!` for a person, `error:`, `help:`, `▸` lines) is translated; **a program never parses it.**

**The machine channel.** A command that something else drives takes `--progress json` and prints NDJSON on stdout, one event per line: `{"v":1,"kind":"step"|"info"|"hint"|"error","code":"<stable dotted id>","args":{...},"message":"<English>"}` (contract and `Coded` error type: `crates/atlas-core/src/progress.rs`; `atlas crew install --progress json` is the first user). A consumer branches on `kind` and `code`, never on text; a new failure a client must tell apart gets a `code` where the error is created (`progress::Coded::new("sudo.password_rejected", args, english_message)`). Atlas Desktop must read only this channel; the old `step:` lines stay for old clients until the migration of `installProgress.ts`. `crates/atlas-cli/tests/machine_output.rs` runs the machine commands under `ATLAS_LANG=en` and `ATLAS_LANG=pt-BR` and compares the bytes, and `tests/crew_install.rs` does it for the install events: add a row for every new machine surface.

## Platform words (OS-aware text)

Atlas runs on macOS, Windows and Linux; no message may say "Mac" or spell a shortcut by hand.

- **Shortcuts** are a placeholder in the message (`{key}` in Paraglide, `{ $key }` in Fluent), filled by the caller: desktop `kbd(stroke)` / `label()` / `modClick()` in `apps/desktop/src/lib/keys.ts` (follows OS and the user's remaps), Rust `atlas_i18n::shortcut("cmd-shift-p")` (`⌘⇧P` on macOS, `Ctrl+Shift+P` elsewhere; key names such as `Space` come from the catalog, pt-BR `Espaço`).
- **System nouns** come from the shared terms (`term-your-computer`, `term-file-manager`, `term-trash`, `term-system-settings`, `term-alt-key`, `term-enter-key`, `term-menu-bar`; `os` entries in `i18n/glossary.yaml`). Every message may select on `$os` (`mac`|`windows`|`linux`); the i18n layer adds it to every call, call sites never pass it. A Fluent message that references a term (`{ term-trash }`) keeps `$os`. pt-BR needs "no Finder" / "na bandeja": when a preposition precedes the noun, write the whole phrase with its own `$os` select instead of composing.
- A screen that exists on one platform only (macOS capture permission) may name it; mark the key `platform: mac` (Fluent: `# platform: mac`; Paraglide: `i18n/platform-keys.json`). `i18n:check` rejects glyphs and `Cmd+`/`Ctrl+` chords in catalog values, platform nouns in a message without an `os` select, and an ad hoc `isMac` / `navigator.platform` outside `keys.ts` (`IS_MAC`).

## pt-BR rules

- Glossary terms stay English: Atlas, Crew, crewmate, hostd, worktree, branch, commit, merge, pull request, deploy, prompt, token, log, build. Check `i18n/glossary.yaml` before inventing a rendering.
- Register: você. Buttons and menu items in the infinitive ("Salvar", "Abrir projeto"). Sentence case. No em dashes. No gerundism ("vou estar enviando"). Errors say what happened, then what to do.
- Brazilian, never European: tela (not ecrã), usuário, arquivo, configurações, salvar, excluir, baixar. `i18n:check` has a deny-list.
- **Plurals.** pt-BR needs `one` and `other`; CLDR puts 0 in `one`, so a plain `one` variant prints "0 arquivo". Add an exact zero variant where that reads wrong ("Nenhum arquivo"): `[0]` before `[one]` in Fluent, a `count=0` match in Paraglide. Keep a default (`*[other]`) because engines disagree on large numbers.
- Numbers, dates, lists and relative time come from `Intl` (desktop `fmt`) or are written into the message; never concatenate translated fragments into a sentence.

## Where the language comes from

One implementation, `atlas_core::lang` (CLI, TUI, hostd, desktop shell), precedence `ATLAS_LANG`, `~/.atlas/language`, `LANGUAGE`/`LC_ALL`/`LC_MESSAGES`/`LANG` (C/POSIX = English), the OS, English; any `pt*` selects pt-BR. `atlas config lang` shows it and what decided it; `atlas config lang <tag>` and Desktop Settings write `~/.atlas/language`. The desktop receives the resolved tag from Rust (a Tauri command) and never reads `navigator.language`. Mobile follows the phone.

## Traps that cost time

- A broken Paraglide plugin path compiles zero messages with only a warning: the desktop has a test that the compiled `m` exposes the catalog's keys.
- Paraglide `setLocale` with a custom Svelte `$state` strategy throws `state_unsafe_mutation` on the first render unless it writes only when the value changed. Keep the guard.
- Fluent's `NUMBER` ignores the locale: format numbers in Rust or avoid them in the message.
- A third-party name inside a rename sweep: never translate or rewrite external API strings (Telegram `/bot<token>/`, `is_bot`).
- Test assertions on English text run in a fixed locale; set it explicitly instead of relying on the machine's.
