---
title: "Releasing"
description: "How maintainers cut, promote and publish an Atlas release on the Stable, Beta and Alpha channels."
---

# Releasing Atlas (maintainers)

Releases are built and published **from a maintainer's machine**. The `Release` workflow is manual-only (`workflow_dispatch`); `CI` runs on pull requests and pushes to `main`. [`act`](https://github.com/nektos/act) runs the real workflow files locally: macOS jobs run natively on the host (`-self-hosted`), Ubuntu jobs in Docker.

What the channels mean for users, and what Atlas promises on each, is in [Versioning and release channels](versioning.md). This page is how to cut them.

## The flow

```text
main        ── alpha ── alpha ── alpha ──────────────────────────▶   bun run release alpha
   \
    release/1.4 ── beta ── fix ── beta ── (rc) ── stable ── fix ── patch
```

| Step | Run on | Command | Result |
|---|---|---|---|
| Next alpha | `main` | `bun run release alpha` | `v1.4.0-alpha.1`, `.2`, … |
| First beta | `main` | `bun run release beta` | creates `release/1.4` from `HEAD`, tags `v1.4.0-beta.1` on it |
| Next beta | `release/1.4` | `bun run release beta` | `v1.4.0-beta.2`, … |
| Release candidate (optional) | `release/1.4` | `bun run release rc` | `v1.4.0-rc.1`, … (needs a beta first) |
| Promote to Stable | `release/1.4` | `bun run release stable` | `v1.4.0`, the soaked beta plus version files |
| Hotfix | `release/1.4` | `bun run release patch` | `v1.4.1` |

Every command takes `--dry-run`, `--skip-checks`, `--force-soak` and `--bump major`. An explicit version works too (`bun run release 1.4.0-beta.2`) and follows the rules of its channel; it must be valid SemVer with an `alpha`, `beta` or `rc` label and a **dot before the counter**, and newer than every existing tag.

## Commands

All from the repository root (`package.json` scripts, run with `bun run <name>`):

| Command | What it does |
|---|---|
| `bun run check` | `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo nextest run --workspace --locked`, `cargo test --workspace --locked --doc`, `cargo deny check`, the skills check, `bun run desktop:check`, then `bun run release:test`. The same gate as CI. |
| `bun run release <alpha\|beta\|rc\|stable\|patch\|X.Y.Z[-pre.N]>` | Cut and publish a release. |
| `bun run release:dry` | `scripts/release.sh --dry-run`: prints every step, changes and pushes nothing. Pass the request after it: `bun run release:dry alpha`. |
| `bun run release:test` | The release tooling's own tests: `bun test scripts` (version, branch, soak and diff-guard rules) and `python3 -m unittest scripts/test_generate_manifest.py`. |
| `bun run release:act [vX.Y.Z[-pre.N]] [--publish] [-j <job>]...` | Run `.github/workflows/release.yml` through `act`. Default tag: `v` + the version in `crates/atlas-cli/Cargo.toml`. Without `--publish` it builds artifacts into `.act/artifacts` and publishes nothing. `-j build-linux` runs only that job (its `needs` outside the selection are dropped), which proves the Linux packaging without the macOS jobs; not combinable with `--publish`. |
| `bun run ci:act` | Run the macOS job of `.github/workflows/ci.yml` locally through `act`. |

You can ask the rules directly, without changing anything:

```bash
bun scripts/release-version.ts next --request alpha --json   # {"version":"1.4.0-alpha.1","tag":…,"channel":"alpha","errors":[]}
bun scripts/release-version.ts next --request stable --json  # on main: {"errors":["stable releases are cut from release/X.Y …"]}
bun scripts/release-version.ts diff-guard --from v1.4.0-beta.2   # lists files a promotion may not change
```

## Prerequisites

- The right branch, working tree clean, in sync with its `origin` branch (the script checks; `--dry-run` relaxes the clean/sync checks and a missing `CHANGELOG.md` section becomes a warning).
- `gh` logged in with write access to `codeatlasdev/atlas` (`gh auth login`); its token is used to publish.
- `act` (`brew install act`) and a running Docker (OrbStack or Docker Desktop) for the Ubuntu `publish` job.
- Rust toolchain pinned in `rust-toolchain.toml`, `bun`, `gh`. `scripts/setup.sh` (run by `release.sh`, `desktop:dev` and `desktop:build`) verifies them and installs frontend deps, `cargo-tauri` and `cargo-deny` when missing. Under `act`, the `Swatinem/rust-cache` steps are skipped (`if: ${{ !env.ACT }}`): the local `target/` is the cache.
- `.actrc` is committed and configures act: `macos-14` and `macos-latest` → `-self-hosted`, `ubuntu-latest` → `catthehacker/ubuntu:act-latest`, container architecture `linux/arm64`, artifact server path `.act/artifacts` (gitignored).
- Write the `## [X.Y.Z[-pre.N]]` section of `CHANGELOG.md` **before** you run the command: the release notes come from it.

## What `bun run release` does

1. **Resume check.** If `HEAD` is already a pushed `vX.Y.Z[-pre.N]` tag whose GitHub Release does not exist (a previous publish failed), it only runs the publish step for that tag and exits. It never bumps again. A promoted stable keeps its provenance: the `Promoted from:` line in the tag message is passed on again.
2. **Plan** (`scripts/release-version.ts`, unit-tested). Reads `Cargo.toml`, every `v*` tag (local and on `origin`), the branches and the current branch, then decides the next version or says why it cannot:
   - `alpha`: only on `main`. The base is the next minor after the current stable; it moves up a minor once `release/<base>` or any `beta`, `rc` or stable tag of that base exists (`--bump major` starts a new major). The counter is the highest existing `-alpha.N` **in the tags** plus one, not whatever `Cargo.toml` says.
   - `beta`: on `main` it creates `release/X.Y` and tags `X.Y.0-beta.1` there; on `release/X.Y` it increments `beta.N`, and is refused once an `rc` exists.
   - `rc`: on `release/X.Y`, after at least one beta.
   - `stable`: on `release/X.Y`, needs a beta or `rc` tag of that base, the soak below, and no existing stable tag.
   - `patch`: only on `release/X.Y`, `X.Y.(Z+1)`, and only after `X.Y.0` is out. On `main` it fails and explains the hotfix flow. `minor` and `major` no longer exist: use `alpha` / `beta`.
   - Explicit versions: same branch rules as their channel; counters glued to the label (`alpha10`), unknown labels, leading zeros and build metadata are rejected.
3. **Preflight.** Tree clean; the branch equals its `origin` branch (for a first beta, `main` equals `origin/main`); the tag exists neither locally nor on `origin`; `CHANGELOG.md` has a `## [<version>]` section; for `stable`, the **promotion guard** below.
4. **Checks.** `bun run check` (unless `--skip-checks`).
5. **Branch.** A first beta runs `git switch -c release/X.Y`.
6. **Bump.** Every `crates/*/Cargo.toml` (including `atlas-desktop`; `tauri.conf.json` has no version of its own and reads it from the crate) gets the same version; `cargo update --workspace --offline` refreshes `Cargo.lock`.
7. **Commit, tag, push.** Commit `chore(release): vX.Y.Z[-pre.N]`, annotated tag, then `git push --atomic origin <main|release/X.Y> <tag>`: the branch and the tag land together or not at all.
8. **Build and publish** via `scripts/act-release.sh <tag> --publish`, then the site deploy (kept; it never fails the release).

### Promotion guard (stable)

Stable is **the soaked prerelease plus version metadata**. Before bumping, `release.sh stable` diffs the newest `beta`/`rc` tag of the line against `HEAD` and allows only `Cargo.toml` files (the root and `crates/<name>/Cargo.toml`), `Cargo.lock` and `CHANGELOG.md`. Anything else is refused with the file list:

```text
✗ stable must be v1.4.0-beta.2 plus version metadata, but these files changed since it:
crates/atlas-core/src/lib.rs
Re-cut a beta/rc that includes them and let it soak, or revert them from release/1.4.
```

The tag message records `Promoted from: v1.4.0-beta.2` and the manifest gets `promoted_from` (through `ATLAS_PROMOTED_FROM`), so anyone can re-run the same diff later.

### Soak

During the alpha era the minimum is **48 hours** between the newest beta/`rc` tag of the line and `stable` (measured from the tag's creation date). `stable` inside the window fails with the age and the override:

```text
✗ soak not met: the last 1.4.0 prerelease is 10h old, stable needs at least 48h (override with --force-soak; it is recorded in the release notes)
```

`--force-soak` lets it through and the tool adds a `> Soak override: promoted after 10h …` line under the version's `CHANGELOG.md` heading, so it ships in the release notes. A date that cannot be read also needs `--force-soak`; the soak is never assumed met. Use it for security fixes, not for impatience.

## Hotfixes

Fix on `main` first, then bring the commit to the release branch with provenance:

```bash
git switch main && git pull           # land the fix on main through a normal PR
git switch release/1.4 && git pull
git cherry-pick -x <sha>              # -x records "(cherry picked from commit …)"
# write the "## [1.4.1]" section in CHANGELOG.md, commit it
bun run release patch                 # v1.4.1, stable
```

Only the newest stable line gets fixes. If a hotfix touches code the next beta also carries, it reaches Beta and Alpha through `main`.

## What the release workflow does

`.github/workflows/release.yml`, three jobs in order:

1. **Test** (`macos-14`): `bun run check` (the full gate: every workspace crate, deny, skills, desktop frontend, release tooling).
2. **Build** (`macos-14`): `cargo build --release --locked --target aarch64-apple-darwin -p atlas-cli` with `SOURCE_DATE_EPOCH` set to the commit time, ad-hoc `codesign` (so Gatekeeper does not SIGKILL the binary), package `atlas-<version>-aarch64-apple-darwin.tar.gz` plus a `.sha256`. A CycloneDX SBOM of the CLI's dependencies (`cargo cyclonedx`, installed with `--locked` when missing; the step warns and carries on if it cannot be) becomes `atlas-<version>-sbom.cdx.json`. Then Atlas Desktop: `cargo tauri build --bundles app,dmg` (ad-hoc signed through `bundle.macOS.signingIdentity: "-"`), packaged as `Atlas-<version>-aarch64-apple-darwin.app.tar.gz` (what `atlas desktop` and `atlas update` install) and `Atlas-<version>-aarch64-apple-darwin.dmg` (manual download), each with a `.sha256`.
3. **Publish** (`ubuntu-latest`): `scripts/generate-manifest.py` writes `manifest.json` (`version`, `date`, `assets`, plus `channel`, `prerelease`, `commit` and, for a promotion, `promoted_from`); a `SHA256SUMS` file covers every asset and the manifest; then the GitHub Release (`softprops/action-gh-release`). The body is the version's `CHANGELOG.md` section followed by the install/update instructions; for a prerelease it opens with a line saying which channel the build is on (and, for a default-channel build that GitHub marks "Latest", that it is still not offered on the Stable channel).

The GitHub flags come from the tag's channel against the **default channel** (`"default"` in `crates/atlas-core/data/channels.json`, pinned to `#[default]` on `Channel` by a test), through `scripts/release-version.ts github-flags` (unit-tested; the workflow has no `contains('-')` guess of its own):

| Tag | Default `alpha` (pre-GA) | Default `stable` (GA) |
|---|---|---|
| `vX.Y.Z` | normal release, **Latest** | normal release, **Latest** |
| `vX.Y.Z-alpha.N` | normal release, **Latest** (`prerelease: false`, `make_latest: true`) | pre-release, never Latest |
| `vX.Y.Z-beta.N` / `-rc.N` | pre-release, never Latest | pre-release, never Latest |

So pre-GA GitHub's "Latest" badge and `releases/latest` show the newest build of the default channel: what a user who never chose a channel gets. **That redirect is therefore not the Stable channel any more.** Clients never use it as "the stable tag": Stable is the newest tag without a prerelease label among `releases.atom` (older pages with `?after=<tag>` when a run of alphas fills the 10-entry window) and the redirect, which is only one more candidate. The release notes header says so for a default-channel alpha. The manifest's `prerelease` is the SemVer label, not GitHub's flag. At GA the default flips back to Stable (see [versioning](versioning.md#general-availability)) and the table's right column applies with no other edit.

Publishing is skipped under `act` unless `ATLAS_PUBLISH=1` is set, which `act-release.sh --publish` does. The version comes from the tag name (`GITHUB_REF_NAME`), which the script passes to `act` through a synthetic event file.

### Prerelease versions in the macOS bundle

Tauri writes the crate version unchanged into `CFBundleShortVersionString` and `CFBundleVersion`, and into the `.dmg` file name (`Atlas_<version>_aarch64.dmg`), so a prerelease version such as `1.4.0-alpha.1` appears there as is. The bundler source (`tauri-bundler`, `macos/app.rs` and `dmg/mod.rs`) does no SemVer parsing for the plist. On a copy of the installed `Atlas.app` with that value, `plutil -lint`, ad-hoc `codesign --verify --deep --strict` and `hdiutil create` all succeed, and the one reader in this repo (`app_version` in `atlas-cli`, through `plutil -extract`) treats it as a string. The release job's `Atlas_*_aarch64.dmg` glob still matches. A full `cargo tauri build` with a prerelease version has **not** been run end to end here: the first alpha is that test. If it misbehaves, set `bundle.macOS.bundleVersion` in `tauri.conf.json` to the core `X.Y.Z` and keep the full version in the asset names and `atlas --version`.

## Publishing with a token

`scripts/act-release.sh --publish` needs a token with permission to create releases. It uses `$GITHUB_TOKEN` if set, otherwise `gh auth token`, and passes it to `act` as the `GITHUB_TOKEN` secret. It refuses to publish when the tag is not on `origin`, `act` is missing, Docker is not running, or no token is available. Without `--publish` nothing is uploaded.

## If something fails

- **Plan or preflight fails** (wrong branch, soak, guard, tag exists, changelog section): nothing has changed yet; fix and rerun.
- **Checks fail**: nothing has changed yet.
- **Push succeeded but build or publish failed**: rerun the same `bun run release <request>` on that commit; the resume check sees the pushed tag without a Release and only republishes. Or run `bun run release:act vX.Y.Z --publish` directly.
- **Push failed halfway**: `--atomic` means neither the branch nor the tag moved on `origin`. A local commit and tag remain: delete the tag (`git tag -d vX.Y.Z`), reset the release commit, and rerun.
- **First beta failed after the branch was created**: `release/X.Y` exists locally; switch back to `main`, `git branch -D release/X.Y` if nothing was pushed, rerun.
- **Tag exists**: pick another version, or delete the tag only if no Release was created from it.

## Rehearse

```bash
bun run release:dry alpha     # prints the version it would cut and every step; changes nothing
bun run release:act           # builds artifacts into .act/artifacts, publishes nothing
bun run release:test          # the rules themselves
```

## Verify a release

None of these use the GitHub API, which is rate limited and which Atlas clients do not call either:

```bash
# What GitHub shows as "Latest": the newest build of the default channel (an alpha pre-GA), NOT the Stable channel
curl -sI https://github.com/codeatlasdev/atlas/releases/latest | grep -i '^location:'

# Stable, beta and alpha: the feed clients read, newest first (Stable = the newest tag with no prerelease label;
# older entries: releases.atom?after=<oldest tag shown>)
curl -fsSL https://github.com/codeatlasdev/atlas/releases.atom | grep -o '<title>[^<]*</title>' | head

# The assets and what each channel offers right now, as the clients compute it
curl -fsSIL https://github.com/codeatlasdev/atlas/releases/download/vX.Y.Z/manifest.json | head -1
atlas update channel --json
atlas update --check
```

Check the checksums of a download:

```bash
curl -fsSLO https://github.com/codeatlasdev/atlas/releases/download/vX.Y.Z/SHA256SUMS
shasum -a 256 -c SHA256SUMS --ignore-missing
```

Users get the new version with `atlas update`, which downloads `atlas-<version>-aarch64-apple-darwin.tar.gz` from the release and then the matching `Atlas-<version>-…app.tar.gz` when Atlas Desktop is installed. Or they click **Update CLI & app** in the app. Check the app asset with `atlas desktop --install --latest --no-open && atlas desktop --status`.

## Yanking a bad release

A release that must not be installed is handled in the open and never rewritten:

1. Mark it in `CHANGELOG.md`: `## [1.4.0-beta.2] — 2026-10-20 [YANKED]` with one line saying why and which version to use instead. Do not delete the section.
2. Ship the fix as the next version (`beta.3`, or `patch` for a stable). Clients move up to it by SemVer precedence.
3. Only the maintainer may then demote or remove the GitHub release: `gh release edit vX.Y.Z --prerelease` (a release stops being "Latest" once something newer is marked latest; demoting a default-channel alpha hands "Latest" back to the previous build only when you also `gh release edit <tag> --latest`), or `gh release delete vX.Y.Z` for a build that is dangerous to keep downloadable. Never delete or move the **tag**: other clones and the resume logic rely on it, and a deleted version number is never reused (v1.2.0 was deleted and stays skipped).

## Provenance and signing: what is true today

| Property | Status |
|---|---|
| Binaries are ad-hoc signed (`codesign --sign -`) | yes; Gatekeeper still warns on a downloaded `.dmg` |
| Developer ID signature and notarization | **no**; a maintainer decision (Apple Developer Program, USD 99 per year) |
| Checksums | `.sha256` per asset, `SHA256SUMS` over everything, `manifest.json` |
| Signed `SHA256SUMS` | **no**; a maintainer decision, commands below |
| SBOM | CycloneDX JSON of the CLI's dependency tree, when `cargo-cyclonedx` is available |
| Reproducible builds | partial: `--locked`, `SOURCE_DATE_EPOCH`; `--remap-path-prefix` is not set because it has not been verified against the Tauri build |
| GitHub artifact attestations (SLSA provenance) | **no**: they need Actions' hosted OIDC identity and do not work under `act` |

**What a local build can claim:** the artifacts were built from the tagged commit by the maintainer's machine, from locked dependencies, and the manifest records that commit. That is at most [SLSA](https://slsa.dev) build level 1 (provenance exists, is not signed by a build service, and is self-asserted). Do not describe the builds as higher than that.

If the maintainer decides to sign, these are the commands (run after the release is published, on the downloaded `SHA256SUMS`, then attach the result with `gh release upload`):

```bash
# Keyless-or-key signature of the checksum file, verifiable offline from the bundle
cosign sign-blob --bundle SHA256SUMS.sigstore.json SHA256SUMS
# or, with a minisign key pair
minisign -Sm SHA256SUMS

# Developer ID + notarization of the app and dmg (needs the Developer ID certificate in the keychain)
xcrun notarytool submit Atlas-X.Y.Z-aarch64-apple-darwin.dmg --keychain-profile atlas-notary --wait
xcrun stapler staple Atlas-X.Y.Z-aarch64-apple-darwin.dmg
```

Until then the install docs say honestly that the app is ad-hoc signed.

## Contrast audit (before every release)

`bun run check` covers the token level (every text, syntax, indicator, focus-ring and field-border token on every surface and tint of the six bundled themes: `apps/desktop/src/lib/theme/theme.test.ts`, `debug/contrast.test.ts`). The runtime audit needs a debug app build, so it is a release step, not part of the gate:

```bash
bun run desktop:build:app                 # debug .app only (never a DMG)
bun scripts/contrast-audit.ts --json /tmp/contrast.json --md /tmp/contrast.md
```

It starts its own agent instance (never your screen), switches through the six themes and walks ~35 screens (sidebar, panel, logs, editor with find and completion, every Settings tab, palette, What's new incl. an update on offer, confirm/prompt/discover/theme-import dialogs, toasts of every severity, row/project/new-tab menus, tooltip, focus ring, tour, checklist, shortcuts sheet). Per screen it runs the text audit (4.5 body, 3 large/disabled/placeholder/gutter) and the non-text audit (icons, field borders and the focused outline, 3:1). Exit 0 means **0 failures**; fix anything it lists before tagging. Decisions: syntax colours and inlay hints are text (4.5); disabled controls are exempt from AA by WCAG but held to 3 so they stay visible; a text field's 3:1 boundary is the box that wraps the field itself (its own border/fill, or a `label` around it); a container that also holds other controls (the command palette, a dialog) is a surface, and a bare input there is identified by its placeholder and caret; a ring/spinner is judged by its strongest part (the arc, not the faint track); file-type icons repeat the adjacent filename and are marked `data-contrast-decorative` (skipped by the non-text audit, never recoloured: they are brand colours); the GPU terminal's `terminal.minimum_contrast` defaults to 4.5 (the ANSI table is untouched, cells are lifted at draw time).
