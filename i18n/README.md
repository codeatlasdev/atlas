# i18n

Atlas is written in English and translated into pt-BR (required) and any language a native speaker owns later. This
directory holds what is shared by every surface; the tools are in `scripts/i18n/`.

| Path | What |
|---|---|
| `locales.json` | The languages and their tier: `source` (English), `required` (a missing or outdated key fails the gate), `community` (it warns, the UI falls back to English), with owners. |
| `glossary.yaml` | Product terms: never translated (`dnt`), fixed renderings, grammatical gender, wording advice, and the per-platform nouns (`os`). |
| `style/` | The writing guide (`README.md`, `en.md`, `pt-BR.md`) and `flagged-terms.yaml`, the data behind the writing lint. |
| `lock/<surface>/<locale>/<area>.json` | Which English text each translation was made from (see `lock/README.md`). Written only by the tools. |
| `hardcoded-baseline.json` | Today's offenders per file for each ratchet (below). It only shrinks. |
| `platform-keys.json` | Optional: `{ "key": "mac" }` marks a Paraglide key as being about one platform. |

## Commands

```bash
bun run i18n:check                       # the gate: offline, seconds; part of bun run check and check:changed
bun run i18n:check -- --json             # the same findings for an agent: { ok, errors, warnings, info, findings }
bun run i18n:check -- --surface rust     # one surface: desktop, site, rust, docs, writing
bun run i18n:check -- --status           # per locale and area: fresh, outdated, missing
bun run i18n:check -- --info             # also the informational lines (docs not translated yet, tolerated offenders)
bun run i18n:translate [--locale pt-BR] [--surface x] [--dry-run]
bun run i18n:fix [--restamp <key>] [--baseline]
bun run i18n:add-locale <tag> [--owner @user] [--tier community|required]
```

Each finding is one line, `file:line rule key message`, and any `error` makes the exit code 1. `warn` is for the
reviewer and `info` is hidden unless asked for.

**Change English text:** edit `<app>/messages/en.json` (or the `.ftl`) and the key's description, run
`bun run i18n:translate` (your own key, see below) or write the pt-BR yourself, then `bun run i18n:fix` and
`bun run i18n:check`. A typo-only English edit does not need a retranslation: `bun run i18n:fix --restamp <key>`.

## What `i18n:check` enforces

Paraglide apps (`apps/desktop`, `site`; every `**/project.inlang/settings.json` is a surface named after its app
directory) and Fluent (`crates/*/locales/<lang>/*.ftl`, surface `rust`) share one rule engine.

| Rule | Meaning |
|---|---|
| `missing` | A key of the source the locale lacks, or an empty translation. Error in `required` locales, warning in `community`. |
| `outdated` / `unstamped` | The lock hash differs from the current English hash / the translation has no lock entry yet (`i18n:fix` stamps it). |
| `orphan` | A key in a translation that is not in the source. Always an error. |
| `unused` | A Paraglide key no source file references (`m.key`, `m["key"]`, `messages_key`, a named import). Dynamic use needs `// i18n-keys: prefix_*`. |
| `key` | Paraglide keys are `area_snake_name`; Fluent ids are kebab-case. |
| `placeholder`, `verbatim`, `plural` | Variables, markup tags and attributes must match; code spans and URLs must be identical; plural and select arms must match, and pt-BR needs `one` and `other`. Add an explicit zero arm where "0 arquivo" reads wrong. |
| `description` | Every source key has a description (Paraglide: `messages/descriptions.json`, Fluent: the `#` comment above the message) that is not just the text. |
| `dnt`, `glossary`, `pt-pt` | Do-not-translate terms kept verbatim (error), glossary renderings (warning), pt-PT words in pt-BR (error). Waive one hit in the key's description: `i18n-allow dnt:log`. |
| `shortcut` | A modifier glyph or chord (`⌘K`, `Ctrl+S`) in a catalog value. A shortcut is a `{key}` placeholder filled by `keys.ts` `kbd()`/`label()` (Rust: the shortcut helper). |
| `platform-noun` | `Mac`, `macOS`, `Finder`, `Windows` or `Explorer` in a message with no `os` select. Select on `os` (`$os ->` in Fluent, a selector `os` in Paraglide), or flag the key (`# platform: mac`, `platform-keys.json`). |
| `format` | Catalogs are sorted, one key per line, LF, trailing newline (`i18n:fix` repairs it). |
| `lock`, `config`, `catalog`, `syntax`, `unique` | Lock files that are stale or not canonical; a locale that `locales.json` and `settings.json` disagree on; unreadable JSON or a malformed variants array; Fluent junk; an id defined twice. |
| `hardcoded`, `shortcut`, `platform-check`, `writing-*` | The ratchets below. |
| `docs-frontmatter`, `docs-outdated`, `docs-structure`, `docs-missing` | Translated docs, below. |

### Translated docs

`docs/<locale>/<page>.md` carries front matter with `title`, `source` (the English file in `docs/`) and `source_sha`,
the **git blob sha** of that file when it was translated (`git hash-object docs/<source>`, quoted: an all-digit sha
unquoted is a YAML number). A different sha today is a warning (a typo fix must not block a PR); invalid front matter or
a source that is gone is an error; a page without a translation is `info`; a changed heading count or code block is a
warning. pt-PT words in prose warn.

### Ratchets: a change may not add offenders

A ratchet is a scanner for something that exists today and must not spread (hardcoded text, shortcuts spelled by hand,
platform sniffing, flagged phrases in existing prose). It judges the **change**, not a snapshot:

- **With a diff base** (the normal case: a git checkout with `I18N_BASE`, `origin/main` or `main`), for every file the
  change touches (committed, staged, unstaged, untracked; renames followed) the offenders of the file at the
  merge-base are counted with the same detector, and only the ones the change **adds** are errors, reported with
  `file:line`. Moving, reordering or re-wrapping old code adds nothing; one more copy of a string that was already there
  does. A new file starts at zero. Files the change does not touch are not looked at at all, so when main adds screens
  there is nothing to re-snapshot, and an old file with a hundred offenders never fails someone else's PR.
- **`hardcoded-baseline.json`** is then the shrinking floor: today's count per file, kept by `bun run i18n:fix`, which
  only lowers it (`--baseline` starts a section that does not exist yet). A change that **raises** a count fails with
  `baseline-grew` ("fix the offenders, do not snapshot them"). A baseline that is lower or higher than reality for a
  file does not fail in diff mode.
- **Without a diff base** (not a git checkout, a shallow clone, no such ref) the committed baseline is the judge, as it
  was before: a file may not exceed its count, and a count above reality is an error that `i18n:fix` clears.

`bun run i18n:check` says which mode it used (stderr; `ratchets` in `--json`). Locally, `git fetch origin main` first: a
stale `origin/main` makes everything main gained since look like part of your change.

In CI the `i18n` job fetches the full history (`fetch-depth: 0`) and passes `I18N_BASE`: on a pull request `HEAD^1`
(resolved to a sha), on a push the commit it replaced. GitHub checks a pull request out as the merge of its branch into
the **current** main, and `HEAD^1` is exactly the main tip that merge was built on, so the diff is the PR's own. The base sha
the event records (where the branch started) would blame the PR for main code merged since, which is why it is not used.
An unknown base (the all-zero `before` of a new branch push, a missing ref) is skipped and the next candidate is tried:
`origin/main`, then `main`. The job runs whenever a catalog, a doc, a file under `i18n/` or `scripts/i18n/`, or any Rust
source of the four scanned crates changes (`scripts/ci-paths.sh`; a test fails if a scanned directory is not classified).

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

In Fluent put `# writing-lint: allow RULE_ID: reason` above the message; in a Paraglide catalog put the same text in the
key's description.

## Translating with a model

`bun run i18n:translate` sends only what is missing or outdated. It needs `I18N_MODEL` and either `ANTHROPIC_API_KEY`
or `I18N_BASE_URL` (any OpenAI-compatible endpoint, Ollama included) with `I18N_API_KEY`. The prompt carries the
glossary entries that occur in the batch (and the per-platform nouns, genders and advice), `style/<locale>.md`, each
key's description, the previous translation of an outdated key, and up to 20 existing translations of the same area.
The answer is validated with the same rules as `i18n:check`; if one key fails nothing is written for that surface and
locale. `--dry-run` prints the prompts and touches neither the network nor a file. Translation never runs in pull
request CI (fork PRs get no secrets): the author runs it, or a maintainer pushes the translation to the branch.

## Adding a language

`bun run i18n:add-locale es --owner @someone` edits `locales.json`, three lines of `.github/CODEOWNERS`, every
`project.inlang` `locales` list (and creates an empty catalog), the `SUPPORTED` list of
`crates/atlas-core/src/lang.rs`, `CFBundleLocalizations` of `crates/atlas-desktop/Info.plist`, and creates an empty
`crates/atlas-i18n/locales/es/`. A new language starts as `community`; it becomes `required` when a native speaker owns it.
It prints the steps that are left (translate, style guide, docs).

## Adding a checker

A checker is one file in `scripts/i18n/checks/`; the runner finds it by glob, so a new surface (the Flutter `arb`
catalogs, for example) never edits a shared file.

```ts
// scripts/i18n/checks/arb.ts
import type { Checker } from "../types.ts";

const checker: Checker = {
  name: "arb",
  async run(ctx) {
    // ctx.root is the repo root, ctx.surface the --surface filter, ctx.locales and ctx.glossary are loaded.
    // Return [] when the surface has no files yet.
    return [{ rule: "missing", severity: "error", file: "apps/mobile/lib/l10n/app_pt.arb", line: 3, key: "k", message: "..." }];
  },
  // optional: rows for --status
  async status(ctx) { return []; },
};
export default checker;
```

For a catalog format, build a `Surface` of `Entry` values (see `formats/paraglide.ts` and `formats/fluent.ts`) and call
`checkSurface(ctx, surface)` from `engine.ts`: missing, outdated, orphan, placeholders, plurals, descriptions, glossary,
pt-PT and the lock come with it. For a source scan, add a `RatchetDef` to `ratchets.ts`.

Tests live next to the code (`scripts/i18n/*.test.ts`, run by `bun test scripts`). They build a small valid repository
in a temp dir (`testkit.ts`), break one thing, and assert the rule goes red; copy that shape.
