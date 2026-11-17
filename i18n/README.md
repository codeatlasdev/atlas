# i18n

Atlas is written in English and translated into pt-BR (required) and any language a native speaker owns later. This
directory holds what every surface shares; the tools are in `scripts/i18n/`. **The procedure for changing text is
`.agents/skills/i18n/SKILL.md`**; this file says what the layout is and why.

## Layout

```
i18n/
  messages/<area>/        one folder per area, snake_case, small
    en.json               { "<area>_<name>": "English" | [variants] }
    pt-BR.json            the same keys, translated (one file per locale)
    meta.json             { "<key>": "description" | { "description", "platform", "distinct", "maxLength", "sentence" } }
    lock.json             { "pt-BR": { "<key>": "<12 hex of sha256 of the English value>" } }   (written by the tools only)
  surfaces.json           which surface consumes which areas, the size budget, the generated areas
  locales.json            the languages and their tier (source, required, community), with owners
  glossary.yaml           product terms: never translated, fixed renderings, gender, advice, per-platform nouns
  style/                  the writing guide and flagged-terms.yaml (the writing lint)
  hardcoded-baseline.json today's offenders per file for each ratchet; it only shrinks
```

**Every key of an area starts with `<area>_`** (`settings_display_language` is in `messages/settings/`), and the longest
area name a key starts with owns it (`common_os_trash` is in `common_os`, not `common`). The key tells you the file, the
file tells you the keys, and a key means the same thing on every surface: Paraglide's `m.settings_display_language()`
and Rust's `t!("settings_display_language")` read the same entry. **A key lives in one area** (`dup-key`).

### Shared words are defined once

Cancel, Delete, Trash, the names of keys on a keyboard are not repeated per screen. Three areas, consumed by every surface
without listing: `common` (`common_action_cancel`, `common_state_loading`), `common_os` (the nouns that change with the
operating system: `common_os_trash`, `common_os_file_manager`), `common_keys` (Esc, Enter, PgUp). They hold terms, not
sentences (`common-sentence`: more than four words, or a sentence end, needs meta `sentence: "<reason>"`). A new English
text equal to a common one fails `dup-common` ("use `common_action_cancel`") unless its meta says `distinct: "<reason>"`.

A message references another with `{@common_os_trash}`; the reference is **inlined**, with selectors composed: "Moved to
{@common_os_trash}" is, without anyone writing it, an `os` select (Trash on macOS and Linux, Recycle Bin on Windows). A
message that references a plural one gets both selectors. References may only reach a common area or an area the same
surfaces consume (`dangling-ref`).

### surfaces.json

```json
{ "budget": { "keys": 150, "bytes": 24576 }, "generatedAreas": { "cli_config": "UPDATE_FIXTURES=1 cargo test -p atlas-cli cli_i18n" },
  "surfaces": { "desktop": { "areas": ["os", "settings"], "paraglide": "apps/desktop/.i18n" },
                "rust":    { "areas": ["cli_chrome", "cli_config", "config"], "rust": "crates" } } }
```

A surface consumes the three common areas plus the areas it lists, explicitly (no prefix convention); an area no surface
lists is an error (`unconsumed-area`). A key's usage is searched only in the sources of the surfaces that consume its
area (`unused`: a key none of them references; `undefined-key`: a `m.x(` or `t!("x")` that names no key). The budget is
per area file: over 150 keys or 24 KB fails `budget` and the message names the sub-areas to split into, by key prefix.
`generatedAreas` marks the areas whose English is written by a test (the clap help): checks point at that command rather
than at hand editing.

### Why meta.json, and why nothing generated is committed

- **meta.json, not descriptions inside en.json.** `en.json` and `pt-BR.json` stay the same plain shape: diffable side by
  side, translatable by a person or a model, hashable for the lock. A description (and flags such as `platform`) belongs
  to the English source only, and one folder holds everything of an area.
- **Paraglide's input is generated and gitignored.** Paraglide wants one file per locale for the whole app; the source is
  one folder per area. `bun run i18n:generate` (called by the apps before they compile) writes `apps/desktop/.i18n/` and
  `site/.i18n/`, sorted and deterministic, with references inlined. Committing it would duplicate the truth and invite
  merge conflicts.
- **Rust reads the same JSON.** `crates/atlas-i18n/build.rs` embeds `i18n/messages/<area>/<locale>.json` for `common*` and
  the areas of the `rust` surface and a small evaluator runs the same subset (variables, `{@refs}`, plural, exact, `os`).
  There is no second format and nothing to regenerate.
- **The ARB generator is ready for mobile.** A surface with `"arb": "<dir>"` writes ICU plural/select `.arb` files with
  `@key.description`; no surface declares it yet.

## Message values

A value is a string (`Hello {name}`) or one variants object with `declarations`, `selectors` and `match`; markup tags
(`{#link}..{/link}`) exist for Paraglide only. Plural categories, an exact zero (a second selector on the same input) and
`os` (`mac`|`windows`|`*`) are the only selectors. `{@key}` is a reference. Keys of `match` list one literal (or `*`) per
selector; an earlier selector decides ties, a literal beats `*`. Anything else is a `catalog` error.

## Commands

```bash
bun run i18n:check [-- --json --surface x --status --info]   the gate: offline, seconds; in bun run check and check:changed
bun run i18n:find <text|key> [--json]          does it exist? where, in which languages, who uses it
bun run i18n:add <key> "English" --desc "..." [--pt-BR "..."] [--surface s] [--new-area] [--distinct "why"]
bun run i18n:mv <old> <new> [--new-area --surface s]   rename or move, with locks, references and call sites
bun run i18n:translate [--locale pt-BR] [--area x] [--surface y] [--dry-run]
bun run i18n:fix [--restamp <key>] [--baseline]           format files, rewrite locks, shrink the ratchet baseline
bun run i18n:add-locale <tag> [--owner @user]           a new language everywhere it is listed
bun run i18n:generate [--surface x]                     Paraglide/ARB inputs (the apps call it themselves)
bun run i18n:pseudo [-- --print <area>]                 the en-XA pseudo-locale, gitignored
```

Each finding is one line, `file:line rule key message`, and any `error` makes the exit code 1; every message names the
command that fixes it. `warn` is for the reviewer and `info` is hidden unless asked for. `find`, `add` and `mv` take
`--json` and exit 1 with the fix in the error.

## What `i18n:check` enforces

| Rule | Meaning |
|---|---|
| `missing` | A key of the English the locale lacks, or an empty translation. Error in `required` locales, warning in `community`. |
| `outdated` / `unstamped` | The lock hash differs from the current English hash / the translation has no lock entry (`i18n:fix` stamps it). |
| `orphan` | A key in a translation that is not in the English of its area. Always an error. |
| `key`, `key-area`, `dup-key` | Keys are snake_case; start with their area's name (longest prefix owns); are defined once. |
| `placeholder`, `verbatim`, `plural` | Variables and markup must match; code spans and URLs identical; selectors and plural arms must match, and pt-BR needs `one` and `other` (add an exact zero where "0 arquivo" reads wrong). Compared on the resolved message, so a translation may write its own `os` select where the English references a term. |
| `description`, `length` | Every English key has a description in meta.json that is not just the text; meta `maxLength` caps every text. |
| `dnt`, `glossary`, `pt-pt` | Do-not-translate terms kept verbatim (error), glossary renderings (warning), pt-PT words in pt-BR (error). Waive one hit in the key's description: `i18n-allow dnt:log`. |
| `shortcut`, `platform-noun` | A glyph or `Cmd+` chord in a value (use `{key}`); `Mac`, `Finder`, `Windows`, `Explorer` in a message with no `os` select, unless meta `platform` says it is about one platform. |
| `dup-common`, `dup-value`, `common-sentence` | See above. |
| `budget`, `unconsumed-area`, `dangling-ref`, `generate` | Area size; an area nobody consumes; a reference that does not resolve, loops or leaves the surfaces; a message a generator cannot express. |
| `unused`, `undefined-key`, `unconsumed-key` | A key nobody consuming it uses (common*, generated and `cli_*` areas exempt); a reference in a source to a key that is not there, or is in an area the surface does not consume. |
| `supported-sync` | `i18n/locales.json` equals `SUPPORTED` in `crates/atlas-core/src/lang.rs`, `CFBundleLocalizations` of `crates/atlas-desktop/Info.plist` and the `locales` of each Paraglide project. |
| `format`, `lock`, `config`, `catalog` | Files sorted, one key per line, LF; locks canonical; unknown locale files and a malformed surfaces.json; unreadable JSON or a malformed variants object. |
| `hardcoded`, `shortcut`, `platform-check`, `writing-*` | The ratchets below. |
| `docs-frontmatter`, `docs-outdated`, `docs-structure`, `docs-missing` | Translated docs, below. |

### Translated docs

`docs/<locale>/<page>.md` carries front matter with `title`, `source` (the English file in `docs/`) and `source_sha`,
the **git blob sha** of that file when it was translated (`git hash-object docs/<source>`, quoted: an all-digit sha
unquoted is a YAML number). A different sha today is a warning (a typo fix must not block a PR); invalid front matter or
a source that is gone is an error; a page without a translation is `info`; a changed heading count or code block (comments
aside) is a warning. pt-PT words in prose warn.

### Ratchets: a change may not add offenders

A ratchet is a scanner for something that exists today and must not spread (hardcoded text, shortcuts spelled by hand,
platform sniffing, flagged phrases in existing prose). It judges the **change**, not a snapshot:

- **With a diff base** (the normal case: a git checkout; the resolver is `scripts/lib/base.ts`, shared with `check:changed` and CI), for every file the
  change touches (committed, staged, unstaged, untracked; renames followed) the offenders of the file at the
  merge-base are counted with the same detector, and only the ones the change **adds** are errors, reported with
  `file:line`. Moving, reordering or re-wrapping old code adds nothing; one more copy of a string that was already there
  does. A new file starts at zero. Files the change does not touch are not looked at at all, so when main adds screens
  there is nothing to re-snapshot, and an old file with a hundred offenders never fails someone else's PR.
- **`hardcoded-baseline.json`** is then the shrinking floor: today's count per file, kept by `bun run i18n:fix`, which
  only lowers it (`--baseline` starts a section that does not exist yet). A change that **raises** a count fails with
  `baseline-grew` ("fix the offenders, do not snapshot them"). A baseline that is lower or higher than reality for a
  file does not fail in diff mode.
- **Without a diff base** (not a git checkout, no `origin/main` or `main`, or CI with no base) the committed baseline is
  the judge, as it was before: a file may not exceed its count, and a count above reality is an error that `i18n:fix` clears.

`bun run i18n:check` says which mode it used (stderr; `ratchets` in `--json`). Where the base comes from, best first:
an explicit `ATLAS_BASE=<ref or sha>` (a bad one is an error); in CI a pull request uses the first parent of the merge
ref GitHub checked out (the exact main tip it was built on, so main code merged after the branch started is not the PR's),
a push uses the commit it replaced, anything else has no base; locally the merge-base with `origin/main`, else `main`
(`git fetch origin main` first: a stale `origin/main` makes everything main gained since look like part of your change).
The CI job and the paths that trigger it are declared in `scripts/checks/map.ts`, not here.

| Ratchet | Looks at | Flags |
|---|---|---|
| `svelte` (rule `hardcoded`) | `apps/desktop/src`, `site/src` `.svelte` files, parsed with `svelte/compiler` | Text nodes with letters and static `title`, `placeholder`, `aria-label`, `aria-description`, `alt`, `label`, `tooltip`, `description`, `message`. Not `<code>`, `<pre>`, `<kbd>`, `<script>`, `<style>`, or the node after `<!-- i18n-ignore -->`. |
| `rust` (rule `hardcoded`) | `src` of `atlas-cli`, `atlas-tui`, `atlas-desktop`, `atlas-host` | The format string of `println!`, `eprintln!`, `print!`, `eprint!`, `bail!`, `anyhow!` with letters outside `{..}`. Not test code (`mod tests` tail, `tests.rs`, `tests/`), machine output (`{}` only), `step:` lines, ANSI-only strings, or a line with `// i18n-ignore` (or the line above). |
| `shortcut` | the Svelte, TypeScript, Astro and Rust sources above | A modifier glyph or `Cmd+` style chord on a non-comment line. `keys.ts` and tests are exempt. |
| `platform-check` | the desktop and site sources | `navigator.platform` and `isMac =` anywhere but `apps/desktop/src/lib/keys.ts`. |
| `writing-*` | English docs, `README.md`, `skills/`, site content, `CHANGELOG.md` | Error-level hits of the writing lint. Translations under `docs/<locale>/` are not ratcheted: they are strict. |

#### Why the Rust scan is a text scanner and not clippy

Clippy's `disallowed-macros` is the obvious tool and the wrong one here:

- It has no baseline. Every one of the 575 user-facing call sites that exist today would fail `cargo clippy -D warnings`
  until it is migrated or carries `#[expect]`, so the lint could not be turned on before the migration it is meant to drive.
- It bans a macro, it does not read the format string. `println!("{}", json)`, the `step:` lines the desktop parses and
  every `--json` output are legitimate English and untranslated (823 call sites by raw count, about 30 % of them machine
  output). A macro ban cannot tell them apart.
- It cannot skip test code per macro without a `#[cfg(test)]` allow at each module, and it needs a full build of the crate
  to run, where the scanner is about half a second and needs no toolchain.

The price is a heuristic: it misses a user-facing string that reaches the user through `format!`, `write!`, a clap `about`,
or a ratatui `Span::raw`, and it reads text, not types. When a crate has migrated to catalog ids, turn on
`disallowed-macros` for it in its `clippy.toml` (no baseline needed once the count is zero) and drop it from `RUST_DIRS`.

## The writing lint

`style/flagged-terms.yaml` is the spec (its header explains every field); `scripts/i18n/writing.ts` only applies it.
It reads catalog values of every locale (scope `ui`) and Markdown prose (scope `docs`: code, URLs, link targets, HTML
comments and front matter are skipped). Catalog values and `docs/<locale>/**` fail at the rule's severity; existing
English prose is ratcheted. Waive one hit, with a reason:

```md
<!-- writing-lint: allow em-dash: quoting a heading verbatim -->
```

In a catalog, put `writing-lint: allow RULE_ID: reason` in the key's description (meta.json); it covers every locale of the key.

## Translating with a model

`bun run i18n:translate` sends only what is missing or outdated, area by area. It needs `I18N_MODEL` and either
`ANTHROPIC_API_KEY` or `I18N_BASE_URL` (any OpenAI-compatible endpoint, Ollama included) with `I18N_API_KEY`. The prompt
carries the glossary entries that occur in the batch (and the per-platform nouns, genders and advice), `style/<locale>.md`,
each key's description, the previous translation of an outdated key, and up to 20 existing translations of the same area.
The answer is validated with the same rules as `i18n:check`; if one key fails nothing is written for that area and locale.
`--dry-run` prints the prompts and touches neither the network nor a file. Translation never runs in pull request CI (fork
PRs get no secrets): the author runs it, or a maintainer pushes the translation to the branch.

## Adding a language

`bun run i18n:add-locale es --owner @someone` adds the tag to `locales.json`, creates an empty `es.json` in every area,
and edits `SUPPORTED`, `CFBundleLocalizations`, the project settings and two lines of CODEOWNERS; `supported-sync` then keeps
those lists equal. A new language starts as `community`: missing keys warn and the UI falls back to English until a native
speaker owns it. It prints what is left (translate, style guide, docs).

## Adding a checker

A checker is one file in `scripts/i18n/checks/`; the runner finds it by glob, so a new surface never edits a shared file.

```ts
// scripts/i18n/checks/example.ts
import type { Checker } from "../types.ts";

const checker: Checker = {
  name: "example",
  async run(ctx) {
    // ctx.root is the repo root, ctx.surface the --surface filter; ctx.locales and ctx.glossary are loaded.
    // Return [] when there is nothing to check.
    return [{ rule: "missing", severity: "error", file: "i18n/messages/x/pt-BR.json", line: 3, key: "k", message: "... run bun run i18n:fix" }];
  },
  async status(ctx) { return []; }, // optional: rows for --status
};
export default checker;
```

Per-area rules are `checkArea` in `engine.ts`; rules across areas are `checks/messages.ts`; a source scan is a `RatchetDef` in
`ratchets.ts`. Tests live next to the code (`scripts/i18n/*.test.ts`, run by `bun test scripts`): they build a small valid
tree in a temp dir (`testkit.ts`), break one thing, and assert the rule goes red; copy that shape.
