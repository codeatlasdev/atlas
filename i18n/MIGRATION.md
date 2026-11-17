# Migration plan: moving every hardcoded string into the catalog

The foundation is on `main` (catalogs, `m.*()` / `t!`, the checks, the ratchets). About 2,800 user-visible strings are
still hardcoded: **2,213 in Svelte** (desktop and site islands), **578 `println!`/`bail!` sites in Rust**, 74 spelled
shortcuts, 4 platform checks, 282 writing-lint hits in existing prose, 30 untranslated docs pages and the phone app.
This file is the plan and the procedure. The procedure of one string is `.agents/skills/i18n/SKILL.md`; the tools are in
`scripts/i18n/`.

**Decisions** (maintainer):

- The migration starts **after the current merge queue drains**; alpha.27 waits for it. Until then nobody edits the
  files listed below for this purpose.
- The first pt-BR version ships **100% AI-translated, with no human review per batch**. Quality comes from a second
  independent AI pass and from blocking deterministic gates (below). Human review happens later, on the shipped app, as
  issues. `CODEOWNERS` on pt-BR files stays (`@mvilacad`, `@luancoleto`): they are asked, but a batch is merged by the
  maintainer's queue without waiting for them.
- Each batch is one PR. Agents never merge (`AGENTS.md`).

The numbers below come from `bun run i18n:extract -- --plan` and a `--dry-run` over the real tree (2026-11-17, `main`
at `8ba41f0b`). Re-run both when `main` moves; the plan is computed from the ratchet's own scan, so it never goes stale
by hand.

## What is automatic

`bun run i18n:extract <path-or-glob> [--area <area>] [--dry-run] [--json]` moves the text of Svelte components into the
catalog and calls `m.<key>()` (`import { m } from "#lib/i18n/m.ts"` on the desktop, the Paraglide import on the site).
It writes English and `meta.json` (a description derived from the component, the element and the variables, marked as
extracted so a reviewer refines it), reuses `common*` messages when the English is the same, and leaves pt-BR to the
next step. A second run changes nothing. Everything it will not touch is in the `--json` report with `file:line` and a
reason. Behavior and tests: `scripts/i18n/extract.test.ts`.

| Left by hand (reason) | What to do |
|---|---|
| `shadowed-m` | The file already uses an identifier named `m` (a `const m`, a `.map((m) => ...)` around text). Rename that local (`model`, `msg`), run extract again. About 130 strings in 9 files. |
| `plural` | One message with plural variants (skill: Message syntax), the count passed in. pt-BR needs an exact zero where "0 arquivo" reads wrong. |
| `concatenation` | Text joined to a call or a computation (`{ago(t)}`, `{n + 1} of {total}`). Compute the value, pass it as a variable. Dates, numbers and relative time go through `$lib/i18n/fmt`. |
| `inline-markup` | A sentence split by `<b>`, `<a>`, `<code>`. Write one message with `{#link}..{/link}` tags, never pieces. |
| `expression-literal` / `template-literal` | A string inside `{...}` (`{n ? "Has files" : "No files"}`). One message per branch, or a variable. |
| `shortcut` | Keys are never text: a `{key}` placeholder filled by `kbd()` / `label()` (`keys.ts`). Also clears the 74-site `shortcut` ratchet. |
| `near-common` | The English differs from a `common*` message only in case or a final `.`/`…`. Use the common key, or add by hand with meta `distinct`. |
| `catalog-rule` | The English breaks a catalog rule (writing lint, `macOS` without an `os` select, a platform noun). Fix the English, add it with `bun run i18n:add`. |
| `identifier-like`, `url`, `braces`, `ambiguous-variable` | Probably not prose, or not safe to name. Look; add by hand when a person reads it. |
| `ts-module`, `script-string` | `toast.error("...")`, `label: "..."` in `.ts` and `<script>`. Heuristic list: add a message and call `m.*()` where it runs (inside a function or `$derived`, never at module top level). |

**What stays untouched** (the codemod and the agents): code identifiers, CSS, test ids, URLs, `<code>`/`<pre>`/`<kbd>`,
machine output (`--json`, MCP, logs, `step:` lines), keyboard names, and anything under `<!-- i18n-ignore -->`.

### Dry run on the real tree

`bun run i18n:extract apps/desktop/src site/src --dry-run --json` (nothing written):

| | Strings |
|---|---:|
| Found by the ratchet (Svelte) | 2,213 |
| **Converted by the codemod** | **1,803 (81%)**, into 1,490 new keys (82 uses of a `common*` key, 267 reuses inside an area) |
| Left by hand | 410 (19%) |

By reason: shadowed `m` 131, concatenation 110, inline markup 83, plural 37, identifier-like 28, shortcut 9, catalog rule 9,
near-common 6, URL 2, braces 2, ambiguous variable 2, plus the strings the ratchet does not see: 342 literals inside `{...}`
and 216 `<script>` strings (heuristic) and 653 in `.ts` modules. The output of a full run on a scratch copy compiled
(`svelte/compiler`, 231 files) and `i18n:check` found only the missing pt-BR (1,490 `missing`, nothing else).

## The batches

Svelte batches are the areas of `scripts/i18n/extract/areas.ts` (topics of at most ~130 strings: the catalog fails at 150
keys or 24 KB per file) packed up to 30 files and 400 strings, **leaf components first** (fewest importers) and shared
wrappers last: the files many screens import change in the last batches, when their consumers already have their
messages, so an early batch never conflicts with a late one. An area never spans two batches except when it needs a
split.

| Batch | Areas | Files | Strings | By the codemod | By hand | New keys |
|---|---|---:|---:|---:|---:|---:|
| S01 | `islands` (site) | 6 | 55 | 50 | 5 | 39 |
| S02 | `capture`, `crew_automation`, `logs`, `viewers_devops` | 21 | 325 | 191 | 134 | 165 |
| S03 | `crew_settings`, `panel` | 27 | 169 | 141 | 28 | 113 |
| S04 | `crew_chat`, `crew_onboarding` | 20 | 124 | 108 | 16 | 94 |
| S05 | `onboarding`, `settings_theme` | 24 | 191 | 134 | 57 | 119 |
| S06 | `viewers_files`, `mobile`, `search` | 21 | 192 | 138 | 54 | 107 |
| S07 | `crew_workspace` | 25 | 127 | 118 | 9 | 92 |
| S08 | `workbench` | 25 | 118 | 102 | 16 | 80 |
| S09 | `git`, `settings_general`, `settings_terminal` | 27 | 317 | 289 | 28 | 238 |
| S10 | `shell`, `tray`, `settings_editor` | 29 | 247 | 232 | 15 | 194 |
| S11 | `settings_updates`, `sidebar`, `settings_agents`, `settings_capture` | 19 | 348 | 300 | 48 | 249 |
| | **Svelte total** | 244 | 2,213 | 1,803 | 410 | 1,490 |

`bun run i18n:extract -- --plan --json` lists the files of each batch. S02 carries the most by-hand work (the `m`
renames in the config viewers and the automation fields). `shell` (S10) holds the app layout and the
error panel: do it after S08, which owns the screens it frames.

Rust is **one module group per batch**, smallest first. The same areas are listed in `areas.ts` (`cmd_*` for the CLI,
`tui_runtime`, `hostd_daemon`, `native_shell`; none is a `cli_*` area, those are the generated clap help):

| Batch | Area | Files | Sites | text | vars | judgement | log |
|---|---|---:|---:|---:|---:|---:|---:|
| R01 | `cmd_skills` | 3 | 12 | 0 | 3 | 9 | 0 |
| R02 | `native_shell` (desktop shell) | 14 | 22 | 0 | 0 | 0 | 22 |
| R03 | `cmd_setup` | 8 | 28 | 11 | 5 | 12 | 0 |
| R04 | `cmd_hostd` | 3 | 31 | 12 | 18 | 1 | 0 |
| R05 | `cmd_deploy` | 7 | 33 | 3 | 26 | 4 | 0 |
| R06 | `cmd_misc` (config, dev, down, heavy, init, mcp...) | 10 | 33 | 3 | 27 | 3 | 0 |
| R07 | `cmd_install` (install, uninstall, version) | 3 | 38 | 8 | 10 | 20 | 0 |
| R08 | `cmd_link` (link, open, wt) | 3 | 40 | 7 | 12 | 21 | 0 |
| R09 | `tui_runtime` | 7 | 40 | 9 | 16 | 15 | 0 |
| R10 | `hostd_daemon` (1/2) | 18 | 42 | 6 | 2 | 0 | 34 |
| R11 | `cmd_desktop` | 7 | 48 | 7 | 30 | 11 | 0 |
| R12 | `hostd_daemon` (2/2) | 19 | 66 | 8 | 8 | 1 | 49 |
| R13 | `cmd_update` (self_update, update_*) | 4 | 68 | 12 | 22 | 34 | 0 |
| R14 | `cmd_crew` (crew/*, crew_spawn) | 10 | 77 | 7 | 39 | 31 | 0 |
| | **Rust total** | 116 | 578 | 93 | 218 | 162 | 105 |

`hostd_daemon` (83 of its 108 sites) and `native_shell` (22 of 22) are log lines: a
person reads them in a log, not in the app, and logs are never translated. Their batches are mostly
`// i18n-ignore: log` plus the few messages a user sees (hostd errors that reach a client use `Coded`).

### Not in these counts

- **Docs** (30 pages): a batch of its own after the code batches, in full, `docs/pt-BR/<slug>.md` with the front matter
  (`title`, `source`, `source_sha`). Code blocks stay byte-identical. The decision is to keep a "tradução automática"
  notice on AI-translated pages until someone reviews a page. **The notice does not exist yet**: `DocsNotice.astro` has
  only the untranslated and outdated states, and the front matter has no flag for it. That is a small PR before the docs
  batch (a front-matter field, a third banner state, two catalog messages); the docs batch depends on it.
- **Phone app** (`apps/mobile`): no ARB yet and no detector, so no count. A surface with `"arb": "<dir>"` in
  `surfaces.json` generates the files; the extraction there is by hand, after the desktop.
- **Astro pages and `.ts` modules**: the ratchet does not count them (`ts-module` / `script-string` lines above are the
  heuristic worklist). They are migrated with the batch that owns the screen.
- **Spelled shortcuts (74), platform checks (4), writing-lint hits (282)**: the ratchets already block new ones; they
  shrink as batches touch the files (`shortcut` is a by-hand reason above). Nothing to plan separately.

## The loop of one batch (agent)

Start from a fresh `origin/main` in your own worktree. Desktop batches need `bun install` in `apps/desktop`; Rust batches
go through `atlas run --heavy -- <cargo ...>`.

1. **Look first.** `bun run i18n:extract <the batch's files> --dry-run --json`: what converts, what is left, and the
   `shadowed-m` files (rename the local `m` first).
2. **Run it.** Drop `--dry-run`. Read `git diff`: a key name that reads badly is renamed with `bun run i18n:mv`, a
   description that says too little is improved in `meta.json` (they are marked "refine this description").
3. **Do the by-hand list** from the report, with the table above. Every new message follows the skill
   (`i18n:find` before `i18n:add`, `common*` for shared words, plurals as variants, shortcuts as `{key}`).
4. **Write the pt-BR.** Default: `bun run i18n:translate --locale pt-BR --area <area>` for each area of the batch when a
   key is configured (`ANTHROPIC_API_KEY`, or `I18N_BASE_URL` for any OpenAI-compatible endpoint). It is the default
   because every batch then gets the same prompt (glossary entries that occur, the per-platform nouns, `style/pt-BR.md`,
   each key's description, 20 neighbours of the area for voice) and the output is validated with the rules of
   `i18n:check` before a byte is written; a batch written by hand drifts in tone and misses placeholder and plural
   rules. Without a configured key, or when it rejects a key, the batch agent writes the pt-BR itself, following
   `i18n/style/pt-BR.md` and the glossary, and the same rules reject it if it is wrong. Then `bun run i18n:fix`.
5. **Deterministic gates (blocking).** `bun run i18n:check` (placeholders, plurals with the zero case, glossary terms,
   pt-PT words, writing lint, budgets, ratchets), `bun run desktop:check` (tests run in English), and for Rust the
   crate tests plus `crates/atlas-cli/tests/machine_output.rs`. Then `bun run check:changed`. These never get relaxed
   to make a batch pass; never raise `i18n/hardcoded-baseline.json` (`i18n:fix` only lowers it).
6. **Layout pass.** Portuguese is 20 to 30% longer than English: overflow, clipping and wrapping that breaks a row are
   **bugs of the batch**. Start your own invisible instance (`bun run desktop:agent`, `.agents/skills/desktop-debug/SKILL.md`;
   never a window on the user's screen), render every migrated screen in pt-BR (`ATLAS_LANG=pt-BR`) and in the
   pseudo-locale (`bun run i18n:pseudo`: padded 35%, wrapped in ⟦ ⟧, so a missing closing bracket is a truncation and
   plain text is something not extracted), and fix what overflows (CSS, not shorter text, unless the text is the problem).
   Screenshots of the worst screens go in the PR. Rust batches check the CLI output in a terminal 80 columns wide.
7. **Independent review pass.** A second agent, with a fresh context and none of the batch's reasoning, reads the pt-BR
   of the batch against `i18n/style/pt-BR.md` and the glossary: você, buttons and menu items in the infinitive,
   sentence case, no em dashes, no AI vocabulary, Brazilian and never European Portuguese, glossary terms kept in
   English, gender agreement with the term, length against the English for the layout, errors say what happened and
   what to do. It **fixes** what it finds (it edits the catalog, then `i18n:fix` and `i18n:check`), and lists every
   finding and every fix in the PR body. A finding it is unsure about is listed as such for the human review after the
   ship.
8. **Open a draft PR**: `feat(i18n): move <areas> to the catalog (batch S07)`. Body: the dry-run numbers and what was
   left and why, the layout screenshots, the review findings. No CHANGELOG line per batch; the batch that finishes the
   desktop (S11) adds the one entry for the whole language. `Closes` is not written by agents.

## Tests that assert English text

Tests run in the base language: the desktop test setup starts in `en`, so `expect(text).toBe("Save")` stays true after a
component moves to the catalog and no batch edits it for that reason. Rules:

- **Never assert pt-BR text** outside the catalog tests (`apps/desktop/src/lib/i18n/*.test.ts`, `test/locale.ts`'s
  `inLocale("pt-BR", ...)`, the Rust `cli_i18n` and `atlas-i18n` tests). A feature test that needs another language
  renders inside `inLocale` and asserts the catalog's own value (`m.key()`), never a typed Portuguese sentence.
- A test that breaks because text moved asserts the **key's value** (`m.settings_general_scan()`) rather than being
  changed to a looser match. If it asserted a literal that is now computed, fix the test to read it from the catalog.
- Rust tests that read English output set the language explicitly (`ATLAS_LANG=en`, or `atlas_i18n` with the tag),
  never the machine's.
- A new `t!`/`m.*()` key without its pt-BR fails `i18n:check`; the test for it is the gate.

## Rust: why a worklist and not a codemod

`i18n:extract` on a `.rs` path prints a per-file worklist (`--json`, never a write). Measured on the 578 sites the
ratchet counts: **93 have no placeholder and 218 have plain `{}` / `{name}`** (54%), **162 need a decision** (127 wrap
ANSI colors around the text, 28 use a width or `{:?}` spec, 27 are multi-line), **105 are daemon or shell log lines**
(`eprintln!` in `atlas-host` and `atlas-desktop`: not UI).

A rewrite of the mechanical half is not safe to automate, for reasons a compiler does not catch:

- `t!` values are `&str`, `String`, `&String` or numbers (`atlas_i18n::Value`): arguments like `path.display()` or an
  enum need `.to_string()`, and which ones is a type question.
- Error messages (`bail!`, `anyhow!`) may be matched by a caller or a test; only `grep` on the repo tells.
- Machine output must stay English and byte-identical (`machine_output.rs`: `atlas config schema`,
  `atlas heavy status --json`, `atlas mcp`); a string that looks like prose may be inside one of those surfaces.
- A wrong rewrite compiles and changes what a person reads; the only checks are a heavy build and a manual run.

So the worklist gives, per site, `file:line`, the macro, the format string, a **kind** (`text`, `vars`, `judgement`,
`machine`, `log`, `uncounted`), the reasons for `judgement`, notes, and for `text` and `vars` the suggested English
(placeholders named) and the exact replacement call (`println!("{}", t!("cmd_link_removed_n_links", n = n))`):
the agent applies it by hand. `machine` (JSON written as a format string) and `log` get
`// i18n-ignore: <reason>` instead of a message. `uncounted` lists text the ratchet does not count (`.context("...")`,
ratatui `Span::raw("...")`) in the files being migrated. Keep the glyph or color outside the message: styling stays in
code, the catalog holds words.

After a Rust batch: `cargo clippy` and the crate tests, then run the migrated commands once in `ATLAS_LANG=pt-BR`.

## Conflicts and order

- Two batches never edit the same component (an area belongs to one batch), so source conflicts do not happen.
- The shared files are `i18n/surfaces.json` (a new area is one entry in a sorted list: on a conflict keep both),
  `i18n/hardcoded-baseline.json` (counts per file: after a merge run `bun run i18n:fix`) and the lock files (also
  `i18n:fix`, never by hand). Run `git fetch origin main` before `i18n:check`: the ratchet compares with the merge base.
- Batches may run in parallel; the queue merges them in batch order. A batch that touches a file a merged batch also
  touched (a shared wrapper) rebases first and runs extract again; it is idempotent.
- The areas this plan creates are the `settings_*`, `crew_*`, `viewers_*`, `workbench`, `git`, `sidebar`, `panel`,
  `logs`, `mobile`, `onboarding`, `search`, `shell`, `tray`, `capture`, `islands` and the Rust `cmd_*`, `tui_runtime`,
  `hostd_daemon`, `native_shell` areas of `scripts/i18n/extract/areas.ts`. A new file of a screen goes to the area of its
  directory by the same table; if no rule fits, extract asks for `--area`.

## Owners

| Step | Owner |
|---|---|
| Run the batch: extract, by-hand list, pt-BR, gates, layout pass | one agent per batch |
| Independent review pass (fixes the pt-BR) | a second agent per batch, fresh context |
| Merge | the maintainers' queue (`bun run pr:merge`), no wait for human pt-BR review |
| Human pt-BR review | `@mvilacad`, `@luancoleto`, after the ship, on the running app; findings become issues, fixed in small PRs (`i18n/messages/<area>/pt-BR.json`, `i18n:fix`) |
| The "tradução automática" docs notice, the phone app surface | maintainers decide when |
