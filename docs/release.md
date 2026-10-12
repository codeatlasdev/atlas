---
title: "Releasing"
description: "How maintainers cut and publish an Atlas release."
---

# Releasing Atlas (maintainers)

Releases are built and published **from a maintainer's machine**. The `Release` workflow is manual-only (`workflow_dispatch`); `CI` runs on pull requests and pushes to `main`. [`act`](https://github.com/nektos/act) runs the real workflow files locally: macOS jobs run natively on the host (`-self-hosted`), Ubuntu jobs in Docker.

## Commands

All from the repository root (`package.json` scripts, run with `bun run <name>`):

| Command | What it does |
|---|---|
| `bun run check` | `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo deny check`, the skills check, then `bun run desktop:check` (svelte-check, prettier, bun test). The same gate as CI. |
| `bun run release <patch\|minor\|major\|X.Y.Z>` | Cut and publish a release (default bump: `patch`). |
| `bun run release:dry` | `scripts/release.sh --dry-run`: prints every step, changes and pushes nothing. |
| `bun run release:act [vX.Y.Z] [--publish]` | Run `.github/workflows/release.yml` through `act`. Default tag: `v` + the version in `crates/atlas-cli/Cargo.toml`. Without `--publish` it builds artifacts into `.act/artifacts` and publishes nothing. |
| `bun run ci:act` | Run the macOS job of `.github/workflows/ci.yml` locally through `act`. |

`scripts/release.sh` also accepts `--skip-checks` (skip `bun run check`).

## Prerequisites

- On `main`, working tree clean, in sync with `origin/main` (the script checks; `--dry-run` relaxes the clean/sync checks).
- `gh` logged in with write access to `codeatlasdev/atlas` (`gh auth login`); its token is used to publish.
- `act` (`brew install act`) and a running Docker (OrbStack or Docker Desktop) for the Ubuntu `publish` job.
- Rust toolchain pinned in `rust-toolchain.toml`, `bun`, `gh`. `scripts/setup.sh` (run by `release.sh`, `desktop:dev` and `desktop:build`) verifies them and installs frontend deps, `cargo-tauri` and `cargo-deny` when missing. Under `act`, the `Swatinem/rust-cache` steps are skipped (`if: ${{ !env.ACT }}`): the local `target/` is the cache.
- `.actrc` is committed and configures act: `macos-14` and `macos-latest` → `-self-hosted`, `ubuntu-latest` → `catthehacker/ubuntu:act-latest`, container architecture `linux/arm64`, artifact server path `.act/artifacts` (gitignored).

## What `bun run release patch` does

1. **Resume check.** If `HEAD` is already a pushed `vX.Y.Z` tag whose GitHub Release does not exist (a previous publish failed), it only runs the publish step for that tag and exits. It never bumps again.
2. **Preflight.** Branch is `main`; tree is clean; `main` equals `origin/main`; the new tag exists neither locally nor on `origin`; `CHANGELOG.md` has a `## [X.Y.Z]` section.
3. **Checks.** `bun run check` (unless `--skip-checks`).
4. **Bump.** Every `crates/*/Cargo.toml` (including `atlas-desktop`; `tauri.conf.json` has no version of its own and reads it from the crate) gets the same new version (taken from `atlas-cli`'s current version); `cargo update --workspace --offline` refreshes `Cargo.lock`.
5. **Commit, tag, push.** Commit `chore(release): vX.Y.Z`, annotated tag `vX.Y.Z` ("Atlas vX.Y.Z"), `git push --atomic origin main vX.Y.Z`.
6. **Build and publish** via `scripts/act-release.sh vX.Y.Z --publish`.

Version rules: `patch`/`minor`/`major` bump the current version; an explicit `X.Y.Z` (optionally with a `-pre` suffix such as `-beta.1`) is used as is. Tags containing `beta` or `alpha` are published as pre-releases.

## What the release workflow does

`.github/workflows/release.yml`, three jobs in order:

1. **Test** (`macos-14`): `bun run check` (the full gate: every workspace crate, deny, skills, desktop frontend).
2. **Build** (`macos-14`): `cargo build --release --target aarch64-apple-darwin -p atlas-cli`, ad-hoc `codesign` (so Gatekeeper does not SIGKILL the binary), package `atlas-<version>-aarch64-apple-darwin.tar.gz` plus a `.sha256`. Then Atlas Desktop: `cargo tauri build --bundles app,dmg` (ad-hoc signed through `bundle.macOS.signingIdentity: "-"`), packaged as `Atlas-<version>-aarch64-apple-darwin.app.tar.gz` (what `atlas desktop` and `atlas update` install) and `Atlas-<version>-aarch64-apple-darwin.dmg` (manual download), each with a `.sha256`.
3. **Publish** (`ubuntu-latest`): generate `manifest.json` with `scripts/generate-manifest.py` (its `desktop` and `desktop_dmg` entries point at the app assets), then create the GitHub Release (`softprops/action-gh-release`) with the tarball, checksum and manifest. The body is the version's `CHANGELOG.md` section followed by the install/update instructions.

Publishing is skipped under `act` unless `ATLAS_PUBLISH=1` is set, which `act-release.sh --publish` does. The version comes from the tag name (`GITHUB_REF_NAME`), which the script passes to `act` through a synthetic event file.

## Publishing with a token

`scripts/act-release.sh --publish` needs a token with permission to create releases. It uses `$GITHUB_TOKEN` if set, otherwise `gh auth token`, and passes it to `act` as the `GITHUB_TOKEN` secret. It refuses to publish when the tag is not on `origin`, `act` is missing, Docker is not running, or no token is available. Without `--publish` nothing is uploaded.

## If something fails

- **Checks or tests fail**: nothing has changed yet; fix and rerun.
- **Push succeeded but build or publish failed**: rerun `bun run release patch`; the resume check sees the pushed tag without a Release and only republishes. Or run `bun run release:act vX.Y.Z --publish` directly.
- **Want to rehearse**: `bun run release:dry`, then `bun run release:act` (builds artifacts locally, publishes nothing).
- **Tag exists** error: pick another version, or delete the tag only if no Release was created from it.

## Verify a release

```bash
gh release view vX.Y.Z --repo codeatlasdev/atlas
atlas update --check
curl -fsSL https://api.github.com/repos/codeatlasdev/atlas/releases/latest | grep tag_name   # what users' installers see: no login
```

Users get the new version with `atlas update`, which downloads `atlas-<version>-aarch64-apple-darwin.tar.gz` from the release and then the matching `Atlas-<version>-…app.tar.gz` when Atlas Desktop is installed. Or they click **Update CLI & app** in the app. Check the app asset with `atlas desktop --install --latest --no-open && atlas desktop --status`.
