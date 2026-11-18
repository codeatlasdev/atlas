---
title: "Releasing"
description: "How maintainers cut, promote and publish an Atlas release on the Stable, Beta and Alpha channels."
---

# Releasing Atlas (maintainers)

Releases are built and published by GitHub Actions on hosted runners when a maintainer pushes a `v*` tag, which `bun run release` does. CI runs on pull requests and on pushes to `main`/`release/**`; the Release workflow waits for the CI run of the tagged commit to pass before building. It pushes `main` (or `release/X.Y`) and the tag, which the agent guards refuse (docs/contributing.md "Agent guards"): run it as `ATLAS_MAINTAINER_OK=push bun run release <channel>`.

What the channels mean for users, and what Atlas promises on each, is in [Versioning and release channels](versioning.md). This page is how to cut them.

Atlas Mobile ships separately, from its own `mobile-v*` tags: `bun run mobile:release X.Y.Z`, then a maintainer's approval sends the tested build to the stores. Steps: [Atlas Mobile, "Releases"](mobile.md#releases-testers-then-the-stores).

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
| `bun run check` | The local gate (see [Contributing](contributing.md#the-gate)); CI runs the same checks. |
| `bun run release <alpha\|beta\|rc\|stable\|patch\|X.Y.Z[-pre.N]>` | Cut and publish a release: requires a green CI on `HEAD`, bumps, commits, tags and pushes; the tag push starts the Release workflow, which the script watches. |
| `bun run release:dry` | `scripts/release.sh --dry-run`: prints every step, changes and pushes nothing. Pass the request after it: `bun run release:dry alpha`. |
| `bun run release:test` | The release tooling's own tests: `bun test scripts` (version, branch, soak and diff-guard rules, CI scripts) and `python3 -m unittest scripts/test_generate_manifest.py`. |
| `bun run release:publish vX.Y.Z[-pre.N]` | (Re)build and publish an existing tag: dispatches `release.yml` from `main` with that tag and watches the run, then deploys the site. |

You can ask the rules directly, without changing anything:

```bash
bun scripts/release-version.ts next --request alpha --json   # {"version":"1.4.0-alpha.1","tag":…,"channel":"alpha","errors":[]}
bun scripts/release-version.ts next --request stable --json  # on main: {"errors":["stable releases are cut from release/X.Y …"]}
bun scripts/release-version.ts diff-guard --from v1.4.0-beta.2   # lists files a promotion may not change
```

## Prerequisites

- The right branch, working tree clean, in sync with its `origin` branch (the script checks; `--dry-run` relaxes the clean/sync checks and a missing `CHANGELOG.md` section becomes a warning).
- `gh` logged in with write access to `codeatlasdev/atlas` (`gh auth login`): it reads the CI result, dispatches and watches the Release run. The publish itself uses the workflow's own `GITHUB_TOKEN`, so no token is stored anywhere.
- `bun`, `cargo` and `git`. Nothing is built on your machine.
- A **green `CI` run on `HEAD`**: push the branch (or merge the pull request) and let CI finish first. `--skip-checks` skips this requirement.
- **A full run, not a skipped one.** On a push, CI skips what the change does not touch: a docs-only `HEAD` (the release-notes commit) reports success with every job skipped, and a run on `main` is cancelled when the next push arrives, so the code merged just before may never have had a full run on `main`. Before releasing, run `gh workflow run CI --ref main` (a dispatch has no base, so every job runs on every OS) and read every job. On 2026-11-15 this found three bugs that per-PR runs had missed (#375).
- Write the `## [X.Y.Z[-pre.N]]` section of `CHANGELOG.md` **before** you run the command: the release notes come from it.

## What `bun run release` does

1. **Resume check.** If `HEAD` is already a pushed `vX.Y.Z[-pre.N]` tag whose GitHub Release does not exist (a previous publish failed), it only dispatches and watches the Release workflow for that tag (the same as `bun run release:publish vX.Y.Z`) and exits. It never bumps again. A promoted stable keeps its provenance: the workflow reads the `Promoted from:` line from the tag message again.
2. **Plan** (`scripts/release-version.ts`, unit-tested). Reads `Cargo.toml`, every `v*` tag (local and on `origin`), the branches and the current branch, then decides the next version or says why it cannot:
   - `alpha`: only on `main`. The base is the next minor after the current stable; it moves up a minor once `release/<base>` or any `beta`, `rc` or stable tag of that base exists (`--bump major` starts a new major). The counter is the highest existing `-alpha.N` **in the tags** plus one, not whatever `Cargo.toml` says.
   - `beta`: on `main` it creates `release/X.Y` and tags `X.Y.0-beta.1` there; on `release/X.Y` it increments `beta.N`, and is refused once an `rc` exists.
   - `rc`: on `release/X.Y`, after at least one beta.
   - `stable`: on `release/X.Y`, needs a beta or `rc` tag of that base, the soak below, and no existing stable tag.
   - `patch`: only on `release/X.Y`, `X.Y.(Z+1)`, and only after `X.Y.0` is out. On `main` it fails and explains the hotfix flow. `minor` and `major` no longer exist: use `alpha` / `beta`.
   - Explicit versions: same branch rules as their channel; counters glued to the label (`alpha10`), unknown labels, leading zeros and build metadata are rejected.
3. **Preflight.** Tree clean; the branch equals its `origin` branch (for a first beta, `main` equals `origin/main`); the tag exists neither locally nor on `origin`; `CHANGELOG.md` has a `## [<version>]` section; for `stable`, the **promotion guard** below.
4. **CI green on HEAD** (`scripts/ci-passed.sh`, skip with `--skip-checks`). It waits for the `CI` run of `HEAD` and refuses to go on unless it passed; the release does not re-run the gate locally.
5. **Branch.** A first beta runs `git switch -c release/X.Y`.
6. **Bump.** Every `crates/*/Cargo.toml` (including `atlas-desktop`; `tauri.conf.json` has no version of its own and reads it from the crate) gets the same version; `cargo update --workspace --offline` refreshes `Cargo.lock`.
7. **Commit, tag, push.** Commit `chore(release): vX.Y.Z[-pre.N]`, annotated tag, then `git push --atomic origin <main|release/X.Y> <tag>`: the branch and the tag land together or not at all.
8. **Push the tag; GitHub runs `release.yml`** (gate → macOS, Linux and Windows builds, Arch package → Linux verification in containers → publish). The script watches the run (`gh run watch`), then deploys the site (kept; it never fails the release).

### Promotion guard (stable)

Stable is **the soaked prerelease plus version metadata**. Before bumping, `release.sh stable` diffs the newest `beta`/`rc` tag of the line against `HEAD` and allows only `Cargo.toml` files (the root and `crates/<name>/Cargo.toml`), `Cargo.lock` and `CHANGELOG.md`. Anything else is refused with the file list:

```text
✗ stable must be v1.4.0-beta.2 plus version metadata, but these files changed since it:
crates/atlas-core/src/lib.rs
Re-cut a beta/rc that includes them and let it soak, or revert them from release/1.4.
```

The tag message records `Promoted from: v1.4.0-beta.2` and the `publish` job copies it into the manifest as `promoted_from`, so anyone can re-run the same diff later.

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

`.github/workflows/release.yml`, a handful of jobs on GitHub-hosted runners (the Windows build is a chain of seven, three of them optional). It runs on a `v*` tag push, or by hand (`workflow_dispatch` with the `tag` input, which is what `bun run release:publish` does). A third way, [build only](#build-only-test-main-end-to-end-before-a-release), builds and verifies everything and publishes nothing. Every job that builds checks out `refs/tags/<tag>`, and the version comes from the tag name (`TAG`, which is `inputs.tag` or the pushed ref). Every build job starts with the composite action `.github/actions/release-toolchain` (its Linux packages, the Rust toolchain and cross targets, and `SOURCE_DATE_EPOCH` = the commit time), so no job has its own copy of that setup.

1. **`gate`** (`ubuntu-latest`): `scripts/ci-passed.sh` waits for the `CI` run of the tagged commit and fails the release unless it finished `success`. The release never re-runs the tests; it trusts the one CI run of that exact commit.
2. **`build`** (`macos-latest`, after `gate`; `macos-14` was retired by GitHub): `cargo build --release --locked --target aarch64-apple-darwin -p atlas-cli` with `SOURCE_DATE_EPOCH` set to the commit time, ad-hoc `codesign` (so Gatekeeper does not SIGKILL the binary; with the Developer ID secrets set, the [Developer ID signature](#macos-signing-and-notarization) instead), package `atlas-<version>-aarch64-apple-darwin.tar.gz` plus a `.sha256`. A CycloneDX SBOM of the CLI's dependencies (`cargo cyclonedx`, installed with `--locked` when missing; the step warns and carries on if it cannot be) becomes `atlas-<version>-sbom.cdx.json`. Then Atlas Desktop: `cargo tauri build --bundles app,dmg` (ad-hoc signed through `bundle.macOS.signingIdentity: "-"`, or Developer ID signed and notarized, see [macOS signing and notarization](#macos-signing-and-notarization)), packaged as `Atlas-<version>-aarch64-apple-darwin.app.tar.gz` (what `atlas desktop` and `atlas update` install) and `Atlas-<version>-aarch64-apple-darwin.dmg` (manual download), each with a `.sha256`.
3. **`build-linux`** (`ubuntu-22.04`, after `gate`): the CLI (`atlas-<version>-x86_64-unknown-linux-gnu.tar.gz`) and Atlas Desktop through `cargo tauri build` with `tauri.linux.conf.json`: an AppImage, a `.deb` and an `.rpm`, each with a `.sha256`.
   **`build-linux-arm64`** (`ubuntu-22.04-arm`, a native GitHub arm64 runner, after `gate`, in parallel with `build-linux`): only the CLI, `atlas-<version>-aarch64-unknown-linux-gnu.tar.gz` plus its `.sha256`, built with `cargo build --release --locked -p atlas-cli` (no desktop app, no packages). It exists for crew servers on arm64 (AWS Graviton, Oracle A1, Hetzner CAX): `atlas crew install` and `atlas crew update` pick the asset from the server's `uname -m`. `ubuntu-22.04-arm`, not 24.04, for the same reason `build-linux` uses 22.04: the binary then runs on any glibc >= 2.35 (Ubuntu 22.04, Debian 12). `manifest.json` lists it under `aarch64-unknown-linux-gnu` with `cli` and `daemon` only, and `publish` waits for it.
4. **`build-arch`** (`archlinux:base-devel` container, after `build-linux`): repackages the `.deb` with `makepkg` (`packaging/arch/`) into a `.pkg.tar.zst` plus the rendered `PKGBUILD`.
5. **`verify-linux`** (`ubuntu-latest`, four containers in parallel: `archlinux:latest`, `fedora:latest`, `ubuntu:24.04`, `debian:stable`; after `build-linux` and `build-arch`): nothing Linux is published unless it was executed first (#283, #312). In a fresh container the distro's own tool installs its package (`pacman -U` the `.pkg.tar.zst`, `dnf install` the `.rpm`, `apt install` the `.deb`, so a missing runtime dependency fails here), then `atlas --version` (it must contain the tag's version) and `atlas doctor --json` run, and then **both the AppImage and the package's `atlas-desktop` are opened** by `scripts/linux-launch-check.sh` on a private headless sway with Mesa llvmpipe (no GPU, no display). The script fails on a crash or exit of the app, a window that never maps, a page that stays one flat colour, a missing or replaced `WebKitWebProcess`, a crash/EGL/CSP/console marker on stderr, any error line in the app's own `~/.atlas/desktop/app.log` and a coredump (exit codes and reasons: the header of the script). Screenshots and logs are uploaded as the `verify-linux-<distro>` artifacts, pass or fail. One red container blocks `publish`.
6. **`windows-binaries` → … → `build-windows`** (`ubuntu-22.04`, after `gate`, `cargo xwin`): the CLI, Atlas Desktop's binary, the NSIS installer (`Atlas-<version>-x86_64-pc-windows-msvc-setup.exe`) and `Atlas-Setup.exe` with that installer embedded (see [Windows installer](#windows-installer-atlas-setupexe)), as a chain of jobs with the optional signing jobs between them ([Windows code signing](#windows-code-signing-signpath-foundation)); the last one, `build-windows`, packs `atlas-<version>-x86_64-pc-windows-msvc.tar.gz` and the installers, each with a `.sha256`.
7. **`verify-windows`** (`windows-latest`, after `build-windows`): the Windows files are executed, not only cross-built (#312). `scripts/windows-launch-check.ps1` checks every `.sha256`, unpacks the CLI archive and runs `atlas.exe --version`, installs `Atlas-Setup.exe --silent`, opens the installed `atlas-desktop.exe` (a window, still up after a settle, a screen that is not one colour, `~\.atlas\desktop\app.log` empty), uninstalls with `Atlas Uninstall.exe --uninstall --silent --remove-data`, then does the same with the plain NSIS installer `/S` (what `atlas desktop` and `atlas update` run) and its `uninstall.exe /S`. A first step runs the check on a program that is not an app and requires it to fail. Screenshots and logs are the `verify-windows` artifact.
8. **`verify-macos`** (`macos-latest`, after `build`): `scripts/macos-launch-check.sh` checks every `.sha256`, runs the CLI tarball's `atlas --version`, mounts the `.dmg` and verifies the code signature of the `Atlas.app` in it, unpacks the `.app.tar.gz` (what `atlas desktop` installs) and requires the same bundle version, then opens the app from each (a window on screen, still up after a settle, `~/.atlas/desktop/app.log` empty). Same negative control; the evidence is the `verify-macos` artifact. The app runs from a copy in a temporary directory, never from `/Applications`.
9. **`publish`** (`ubuntu-latest`, after the builds and the three `verify-*` jobs): `scripts/generate-manifest.py` writes `manifest.json` (`version`, `date`, `assets`, plus `channel`, `prerelease`, `commit` and, for a promotion, `promoted_from`); a `SHA256SUMS` file covers every asset and the manifest; then the GitHub Release (`softprops/action-gh-release`, with the run's own `GITHUB_TOKEN`). `promoted_from` comes from the `Promoted from: …` line in the tag message, which `release.sh stable` writes. The body is the version's `CHANGELOG.md` section followed by the install/update instructions; for a prerelease it opens with a line saying which channel the build is on (and, for a default-channel build that GitHub marks "Latest", that it is still not offered on the Stable channel).

A release is built cold on purpose: there is no cargo cache, because a release is never built from a restored one.

**The crew server end-to-end test** (`.github/workflows/crew-e2e.yml`, [what it checks](crew.md#end-to-end-test-on-real-systemd)) installs a crew server on real systemd and runs tasks on it. It runs nightly and on pull requests that touch the crew, and it is a reusable workflow. To make a release wait for it, add a job to `release.yml` and put it in `publish`'s `needs` and `if`:

```yaml
  crew-e2e:
    name: Crew server E2E
    needs: gate
    uses: ./.github/workflows/crew-e2e.yml
```

Before releasing, `gh workflow run crew-e2e.yml --ref main` (it is also on the nightly schedule) and read the result: a crew release is the one thing no other job installs on a server.

The GitHub flags come from the tag's channel against the **default channel** (`"default"` in `crates/atlas-core/data/channels.json`, pinned to `#[default]` on `Channel` by a test), through `scripts/release-version.ts github-flags` (unit-tested; the workflow has no `contains('-')` guess of its own):

| Tag | Default `alpha` (pre-GA) | Default `stable` (GA) |
|---|---|---|
| `vX.Y.Z` | normal release, **Latest** | normal release, **Latest** |
| `vX.Y.Z-alpha.N` | normal release, **Latest** (`prerelease: false`, `make_latest: true`) | pre-release, never Latest |
| `vX.Y.Z-beta.N` / `-rc.N` | pre-release, never Latest | pre-release, never Latest |

So pre-GA GitHub's "Latest" badge and `releases/latest` show the newest build of the default channel: what a user who never chose a channel gets. **That redirect is therefore not the Stable channel any more.** Clients never use it as "the stable tag": Stable is the newest tag without a prerelease label among `releases.atom` (older pages with `?after=<tag>` when a run of alphas fills the 10-entry window) and the redirect, which is only one more candidate. The release notes header says so for a default-channel alpha. The manifest's `prerelease` is the SemVer label, not GitHub's flag. At GA the default flips back to Stable (see [versioning](versioning.md#general-availability)) and the table's right column applies with no other edit.

### Windows installer (`Atlas-Setup.exe`)

The Windows build produces two installers on purpose:

| Asset | What it is | Who runs it |
|---|---|---|
| `Atlas-Setup.exe` | `crates/atlas-setup`: a small borderless window (drawn by us, **no WebView2**, which may not exist yet) with the Atlas globe, a progress bar and a status line; it embeds the NSIS installer below, runs it silently and opens Atlas. `--uninstall` is the same window for removal. | People (the download link, a stable name). |
| `Atlas-<version>-x86_64-pc-windows-msvc-setup.exe` | The Tauri NSIS installer (per-user, `%LOCALAPPDATA%\Atlas`). Listed in `manifest.json` as `desktop`. | `atlas desktop` and `atlas update` (`/S /D=`), scripts. |

The setup is built after the NSIS bundle, with the bundle as its payload: `ATLAS_SETUP_PAYLOAD=<nsis exe> cargo xwin build --release --locked --target x86_64-pc-windows-msvc -p atlas-setup` (without the variable it still compiles, so `cargo build --workspace` works, and reports at run time that it carries no installer). The release builds the NSIS installer from the already built (and, with signing on, signed) binaries with `cargo tauri bundle` and the setup from that (signed) installer, one job each; see [Windows code signing](#windows-code-signing-signpath-foundation). Progress is real: the NSIS installer is silent, so the window watches the install folder grow against the installer's unpacked size; the bar stops short of the end until the exit code is 0 **and** `atlas-desktop.exe` is in place. Errors never close silently: the window shows the reason, **Copy details** and the log (`%TEMP%\atlas-setup.log`).

After a successful install the setup copies itself into the install folder as `Atlas Uninstall.exe` and points the `UninstallString`/`DisplayIcon` of the Settings › Apps entry at it (`QuietUninstallString` stays NSIS's `uninstall.exe /S`). Tauri's installer rewrites that entry on every run, including the silent one of `atlas update`, so `packaging/windows/installer-hooks.nsh` (`NSIS_HOOK_POSTINSTALL`) puts it back when the copy exists. `atlas update` leaves `Atlas Uninstall.exe` in place while it swaps the other files, so the hook finds it. The uninstall window asks **Keep your Atlas data?** and says what deleting removes (`%USERPROFILE%\.atlas`: settings, project list, worktrees Atlas created with any uncommitted changes, the `atlas` command; never project folders). Keeping is the default (`--silent` never removes it unless `--remove-data`); deleting asks a second time, and closing the window or pressing Esc on the question cancels before anything is touched. It then runs `uninstall.exe /S _?=<dir>`, runs `atlas uninstall --yes` only when removing data, and deletes what NSIS leaves. The copy that is running (Settings › Apps starts it from the install folder) cannot delete itself, so a detached `cmd` deletes it and the empty folder a few seconds after the window closes. The plain NSIS installer, when it upgrades over an install made by `Atlas-Setup.exe`, runs the registered uninstaller as `"Atlas Uninstall.exe" --uninstall /UPDATE /P _?=<dir>`: `/UPDATE`, `/P` and `_?=` are accepted and make it a silent uninstall that keeps the data.

Upgrade repair (`crates/atlas-setup/src/repair.rs`): 1.5.0-alpha.21–24 left known bad state behind, so after the NSIS step succeeds the setup reads the machine, plans and performs the fixes, one log line each in `%TEMP%\atlas-setup.log`, and shows "Updating your setup…" when there is something to do. A failed repair is logged and never fails the install. (1) An `%USERPROFILE%\.atlas\bin\atlas.exe` older than the bundled one (SemVer precedence, so `alpha.9 < alpha.24`) or unreadable is replaced by the bundled `atlas.exe install --upgrade`, which renames a running copy aside instead of killing the processes holding it; a symlink (`atlas link`) or a newer CLI is left alone. The NSIS hook runs the same `atlas install --upgrade` whenever that CLI exists, so `atlas update` and winget `/S` repair it too, with no second implementation. (2) A terminal host that is not the bundled version goes through the bundled `atlas hostd --reconcile`: no live upgrade exists on Windows, so an empty host is restarted and one with terminals is kept (the app asks, as before). (3) `%APPDATA%\br.com.codeatlas.atlas\.window-state.json` loses `decorated` and every entry whose rectangle is outside all monitors. (4) `~/.atlas/desktop/settings.json` gets `onboarding.done: true` when `projects.json` lists projects. (5) `%TEMP%\atlas-uninstalled-*.exe`, `atlas-setup-*`, `.atlas.*.__relocated__.exe` and `~nsu*.tmp`, and (6) a `~/.atlas/.update.lock` whose pid is dead, are deleted. Nothing else is touched: other settings, projects, worktrees and sessions are never rewritten or deleted (`the_plan_never_touches_user_data`).

Try one locally (Windows, after `cargo tauri build --ci --bundles nsis --config '{"bundle":{"resources":{"../../target/release/atlas.exe":"atlas.exe"}}}'` in `crates/atlas-desktop`): `ATLAS_SETUP_PAYLOAD=$PWD/target/release/bundle/nsis/Atlas_*_x64-setup.exe cargo build --release -p atlas-setup`, then run `target\release\Atlas-Setup.exe`. `ATLAS_SETUP_DUMP=<dir> cargo test -p atlas-setup dump_frames` writes every screen as raw RGBA for design review.

### Windows code signing (SignPath Foundation)

Windows files are signed by [SignPath Foundation](https://signpath.org) (free for open source; policy text in the [README](../README.md#code-signing-policy) and [Installation](installation.md#code-signing-policy)). The signature order is forced by what embeds what: the NSIS installer packs `atlas.exe` and `atlas-desktop.exe`, and `Atlas-Setup.exe` embeds the NSIS installer at build time. So `build-windows` is a chain of jobs, three of them signing requests:

```text
windows-binaries ─▶ sign-binaries ─▶ windows-installer ─▶ sign-installer ─▶ windows-setup ─▶ sign-setup ─▶ build-windows
 atlas.exe,           (SignPath          NSIS from the        (SignPath         Atlas-Setup      (SignPath      archive, renamed copy,
 atlas-desktop.exe     `binaries`)        signed binaries      `installer`)      around it        `setup`)       .sha256 of the final bytes
```

- Each `sign-*` job uploads nothing but a zip to SignPath (`signpath/github-action-submit-signing-request`, pinned by SHA) and waits for the result. A request waits for a manual **approval** in SignPath, so each is its own job: the wait (up to 6 hours) does not count against a build job's 90-minute limit, and a release with signing on needs three approvals.
- `build-windows` keeps its name: it packs the final bytes (`atlas-<version>-x86_64-pc-windows-msvc.tar.gz`, `…-setup.exe`, `Atlas-Setup.exe`) and writes the `.sha256` files **after** the last signature, which is why checksums come from this job and not from the build jobs. `publish` and the artifact name `atlas-x86_64-pc-windows-msvc` are unchanged.
- **The switch:** the sign jobs run only when the repository variable `SIGNPATH_ENABLED` is `true` (Settings › Secrets and variables › Actions › Variables). Unset or anything else: the three jobs are skipped and the same chain ships the unsigned files under the same names, so nothing about a release depends on SignPath until you turn it on. The unsigned path also goes through the split jobs (`cargo tauri build --no-bundle`, then `cargo tauri bundle`), so the first release after this change is the first to exercise the split; read its Windows jobs before trusting it.
- Secrets: `SIGNPATH_API_TOKEN`, `SIGNPATH_ORGANIZATION_ID`, `SIGNPATH_PROJECT_SLUG`, `SIGNPATH_SIGNING_POLICY_SLUG`. The artifact configurations to paste into SignPath, and the one-time setup on signpath.org, are in `packaging/windows/signpath/README.md` (`binaries.xml`, `installer.xml`, `setup.xml`).
- If a signing request is rejected or times out, the Windows jobs after it are skipped and `publish` does not run; fix the cause in SignPath and re-run the failed jobs, or `bun run release:publish vX.Y.Z` for the same tag. Turning `SIGNPATH_ENABLED` off and re-running ships unsigned files.

### macOS signing and notarization

The macOS files carry a Developer ID signature of team CODEATLAS LTDA (Team ID `58UYCA3ZYA`) and are notarized by Apple, **when the six secrets below exist**. Without them (a fork, or before they are configured) the build is ad-hoc signed exactly as before, and the `macOS signing setup` step prints a notice saying so. Why it matters: with the macOS application firewall on, an ad-hoc identity is the binary's hash, so every update asked again "allow incoming connections" for `atlas hostd` (it opens the phone gateway on all interfaces while a phone is paired); a Developer ID identity is stable, so the firewall and TCC (Screen Recording, Input Monitoring, Microphone) remember it across updates, and a downloaded `.dmg` opens without Gatekeeper's "cannot verify" warning.

| File | Signed | Notarized | Entitlements |
|---|---|---|---|
| `atlas` (inside `atlas-<version>-aarch64-apple-darwin.tar.gz`) | by the release job: `codesign --options runtime --timestamp` | yes, submitted as a zip. A bare binary cannot be stapled, so Gatekeeper looks the ticket up online the first time a downloaded copy runs (`atlas install` and `atlas update` never quarantine it) | `crates/atlas-desktop/Entitlements-cli.plist` (empty on purpose) |
| `Atlas.app` (inside `….app.tar.gz` and the dmg) | by Tauri (`cargo tauri build`), hardened runtime, secure timestamp | yes, by Tauri, then stapled | `crates/atlas-desktop/Entitlements.plist` (`bundle.macOS.entitlements`) |
| `Atlas-<version>-aarch64-apple-darwin.dmg` | by Tauri | yes, by the release job, then `xcrun stapler staple` **before** `Package` writes the `.sha256` (stapling changes the bytes) | none (a disk image) |

**How the `build` job does it.**

1. `macOS signing setup` (`id: sign`): decides, imports, or fails. All six secrets present and the run is a release or a build-only run of `main` → on. None present → ad-hoc plus a notice. **Some but not all → the job fails** naming the missing ones, so a half-configured repository cannot silently ship ad-hoc. It decodes the `.p12`, re-encrypts it under a throwaway password (`security import -P` has no stdin form, so only that throwaway ever appears on a command line, never the real one), imports it into a temporary keychain (created, opened with `security unlock-keychain` and kept open for 6 hours, put first in the search list, partition list set for `codesign`), checks that the keychain holds a *valid* code-signing identity named exactly `MACOS_SIGNING_IDENTITY`, and writes the App Store Connect `.p8` to `$RUNNER_TEMP/notary.p8` (mode 600).
2. `Build desktop app`: only when on, it exports `APPLE_SIGNING_IDENTITY`, `APPLE_API_KEY` (key id), `APPLE_API_ISSUER` and `APPLE_API_KEY_PATH` (Tauri 2's names; `APPLE_CERTIFICATE` is not used, the keychain above already holds the identity). They are exported only then because Tauri reads an empty `APPLE_SIGNING_IDENTITY` as an identity named `""`. A `codesign` wrapper first on `PATH` adds `--timestamp`: Tauri 2.11 calls `codesign` without it, `codesign` adds none by itself (checked: no `Timestamp=` in `codesign -dvv`), and notarization refuses a signature without a secure timestamp.
3. `Developer ID signature, notarization and checks`: signs the CLI, notarizes its zip and the dmg (`xcrun notarytool submit --wait --output-format json`; anything but `Accepted` prints Apple's log and fails), staples the dmg, then checks the real files: `codesign --verify --deep --strict` on the app and CLI, `xcrun stapler validate` on app and dmg, `spctl -a -vvv -t exec` on the app and `spctl -a -vvv -t open --context context:primary-signature` on the dmg (both must say `source=Notarized Developer ID`), `TeamIdentifier`, `Authority=Developer ID Application` and a `Timestamp=` in `codesign -dvv` of all three, the `runtime` flag on app and CLI, the microphone entitlement in the app, and no `get-task-allow`. `spctl` on a bare CLI is not meaningful (Gatekeeper assesses apps and images), so the CLI is covered by the other checks.
4. `Package` signs ad-hoc only when the Developer ID is off, then packs as before.
5. `Remove the signing keychain and keys` (`if: always()`): deletes the keychain (which also drops it from the search list) and every `.p12`, `.pem` and `.p8`. The runner is a fresh VM anyway; this is for the job's own failures.

**Secrets** (repository secrets, Settings › Secrets and variables › Actions; set by `scripts/macos-signing-secrets.sh`, never by hand): `MACOS_CERTIFICATE_P12_BASE64` (certificate and private key), `MACOS_CERTIFICATE_PASSWORD` (random, made by the script), `MACOS_SIGNING_IDENTITY` (`Developer ID Application: CODEATLAS LTDA (58UYCA3ZYA)`, read from the certificate), `APPLE_API_KEY_ID`, `APPLE_API_ISSUER`, `APPLE_API_KEY_P8` (the App Store Connect API key the mobile release already uses, copied from `~/.config/stores/apple.env`: `ASC_KEY_ID`, `ASC_ISSUER_ID`, the file at `ASC_KEY_PATH`). They are repository-level, not an environment's, so any workflow run on the repository can read them; that is why a build-only run of a branch is refused the certificate and only `main` (protected by the ruleset) and release tags get it. Values reach steps only through `env`, never a command line or a log.

**One-time setup (a maintainer, 5 minutes after the certificate exists).**

1. The private key and CSR already exist in `~/.config/stores/devid/` (`devid.key`, `devid.csr`); the key never leaves that Mac. The Account Holder creates a **Developer ID Application** certificate from `devid.csr` at developer.apple.com › Certificates (an Admin cannot) and sends the downloaded `.cer`.
2. `scripts/macos-signing-secrets.sh ~/Downloads/<file>.cer --dry-run` checks every file and that the certificate belongs to `devid.key`, builds the `.p12`, reads it back, and prints only the six secret names. Without `--dry-run` it sets them with `gh secret set` (value on stdin, `--repo` defaults to `codeatlasdev/atlas`) and prints each name it set. It needs `gh` authenticated with permission to set secrets.
3. `gh workflow run release.yml --ref main -f build_only=true`, then read the `build` job: the checks above run for real, and the artifact `atlas-aarch64-apple-darwin` holds the signed files. That run is the first time notarization is exercised; run it before cutting a release.

**Rotation.** A Developer ID Application certificate lasts five years; files signed before it expires stay valid (they carry a secure timestamp). To replace it (expiry, revocation, a leaked `.p12`): new CSR if the key is suspect (`devid.key` otherwise stays), the Account Holder issues the new certificate, rerun the script with the new `.cer`: it overwrites `MACOS_CERTIFICATE_P12_BASE64`, `MACOS_CERTIFICATE_PASSWORD` and `MACOS_SIGNING_IDENTITY` (the password changes on every run). To rotate the API key, update `apple.env` and rerun the script; the mobile release reads its own environment secrets and is untouched. To go back to ad-hoc, delete all six secrets (deleting some fails the build on purpose).

**Entitlements, one by one.** The hardened runtime (required for notarization) turns every capability off unless it is listed, so each entry was checked against the code, and the files carry the reasoning in a comment. Present: `com.apple.security.device.audio-input` in the app (Settings › Capture › Microphone: `AVCaptureDevice` access and ScreenCaptureKit's `captureMicrophone`; under the hardened runtime without it the system never prompts and the recording is silent). Deliberately absent: `allow-jit` and `allow-unsigned-executable-memory` (WebKit's JIT runs in the WebContent process, which is Apple's; Atlas has no JIT, no wasm and no scripting engine of its own), `disable-library-validation` (no `dlopen`, no bundled dylib: `otool -L` of the CLI lists system libraries only), `allow-dyld-environment-variables` (nothing relies on `DYLD_*` reaching Atlas itself; shells and agents are separate processes, which the hardened runtime does not constrain), Apple Events (none in the codebase), `get-task-allow` (refused by notarization). Screen Recording, Input Monitoring and Accessibility are TCC permissions the user grants, and network listening needs no entitlement outside the App Sandbox, which Atlas does not use. **If a signed build shows a WebKit or Metal crash naming JIT or executable memory, add `allow-jit` to `Entitlements.plist` and say why there.**

**What was proven locally and what only Apple can prove.** On a Mac, with a throwaway self-signed code-signing identity in a throwaway keychain (deleted afterwards), a debug `atlas` signed with `--options runtime` and `Entitlements-cli.plist` ran `atlas --version`, `atlas hostd --dir <tmp>` (daemon up, `--status` answered over its socket, `--stop`) with no leftover process, so the hardened runtime does not break the CLI or hostd; and the keychain import used by the workflow (re-encrypt, `security import`, `set-key-partition-list`) works with this machine's `openssl` (LibreSSL, the runner's too). Not provable without Apple's certificate: that notarization is accepted, `spctl` saying `Notarized Developer ID`, that the dmg and app staple, that the hardened Atlas.app opens its window and WebKit works (the entitlement list above is reasoned from the code, not from a signed run of the app), and that the App Store Connect key may notarize. The first `build_only` run of `main` with the secrets is that proof.

### Prerelease versions in the macOS bundle

Tauri writes the crate version unchanged into `CFBundleShortVersionString` and `CFBundleVersion`, and into the `.dmg` file name (`Atlas_<version>_aarch64.dmg`), so a prerelease version such as `1.4.0-alpha.1` appears there as is. The bundler source (`tauri-bundler`, `macos/app.rs` and `dmg/mod.rs`) does no SemVer parsing for the plist. On a copy of the installed `Atlas.app` with that value, `plutil -lint`, ad-hoc `codesign --verify --deep --strict` and `hdiutil create` all succeed, and the one reader in this repo (`app_version` in `atlas-cli`, through `plutil -extract`) treats it as a string. The release job's `Atlas_*_aarch64.dmg` glob still matches. A full `cargo tauri build` with a prerelease version has **not** been run end to end here: the first alpha is that test. If it misbehaves, set `bundle.macOS.bundleVersion` in `tauri.conf.json` to the core `X.Y.Z` and keep the full version in the asset names and `atlas --version`.

## If something fails

- **Plan or preflight fails** (wrong branch, soak, guard, tag exists, changelog section): nothing has changed yet; fix and rerun.
- **Checks fail**: nothing has changed yet.
- **Push succeeded but the Release run failed**: fix the cause (`gh run view <id> --log-failed`), then rerun `bun run release:publish vX.Y.Z`. It dispatches `release.yml` for the existing tag and watches it. Rerunning the same `bun run release <request>` on that commit does the same (the resume check sees the pushed tag without a Release). Failed jobs can also be re-run from the Actions page.
- **Push failed halfway**: `--atomic` means neither the branch nor the tag moved on `origin`. A local commit and tag remain: delete the tag (`git tag -d vX.Y.Z`), reset the release commit, and rerun.
- **First beta failed after the branch was created**: `release/X.Y` exists locally; switch back to `main`, `git branch -D release/X.Y` if nothing was pushed, rerun.
- **Tag exists**: pick another version, or delete the tag only if no Release was created from it.

## Rehearse

```bash
bun run release:dry alpha     # prints the version it would cut and every step; changes nothing
bun run release:test          # the rules themselves
```

### Build only: test main end to end before a release

```bash
gh workflow run release.yml --ref main -f build_only=true     # or any branch: --ref <branch>
gh run watch                                                  # then download the artifacts from the run page
```

`build_only` runs the same jobs on the dispatched commit (not a tag) and stops before `publish`: it builds the macOS, Linux (x86_64, aarch64), Arch and Windows artifacts, runs `verify-linux`, `verify-windows` and `verify-macos` against them, and uploads everything as workflow artifacts (kept 90 days by GitHub's default). It never creates a tag or a release and never reads the repository's publishing secrets: `publish` is skipped by its `if`, the `gate` does not wait for CI (there is no tag to look up), and the SignPath jobs are skipped too, so the Windows files are unsigned. macOS is Developer ID signed and notarized only when the run is of `main` and the secrets exist (a branch never gets the certificate); otherwise it is ad-hoc and the job says so. Files are named after a throwaway version, `0.0.0-alpha.<run number>`; the version inside the binaries is the one in `Cargo.toml`, and `verify-linux` skips its "`atlas --version` is the tag" check. `bun run release` is unchanged: it pushes a tag, and the tag run is the real one. Dispatching without `build_only` needs `-f tag=vX.Y.Z`, as before.

Run it before a release whenever Linux or packaging changed, and read the `verify-linux-*` artifacts: each has `screenshot.png`, the app's stderr and its `app.log`; `verify-windows` and `verify-macos` upload their screenshots and the app's `app.log` the same way.

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

## Symbols

Every release publishes the debug info of what it ships, so a crash report from a user can be turned into functions and lines (#318). The `*-symbols.tar.gz` assets, one per platform next to the CLI tarball:

| Platform | Archive holds | Matched by |
|---|---|---|
| Linux (x86_64, aarch64) | `<build-id>.debug` per binary (`atlas`, and `atlas-desktop` on x86_64), made by `scripts/symbols.sh extract` | the GNU build id (`atlas version` prints it as `build:`; every crash report has `build_id`) |
| macOS | the `.dSYM` bundles (`split-debuginfo=packed`) | the Mach-O UUID |
| Windows | the `.pdb` files | the PDB signature (GUID + age) of the exe, which is the `build_id`; `load_bias` in a report is the image base |

A crash report (`crash-*.toml` or `crash-native-*.toml`) names `version`, `channel`, `commit`, `build_id`, `load_bias` and `frames` (raw return addresses). On Linux:

```bash
tar xzf atlas-X.Y.Z-x86_64-unknown-linux-gnu-symbols.tar.gz -C symbols/
scripts/symbols.sh resolve crash-native-<ms>-<pid>.toml symbols/   # one line per frame: address function file:line
```

The shipped binaries, the AppImage, `.deb`, `.rpm`, `.dmg` and the NSIS installer carry **no** debug info: the workflow takes it out right after the build (`scripts/symbols.sh split` on Linux, `strip -S` after the `.dSYM` on macOS, a PDB that is already separate on Windows) and checks it (`symbols.sh verify-stripped`) before any package is made. Symbol names stay in the binary, so a plain backtrace still has function names; file and line come from the archive.

`resolve` looks the frame up as `frame - load_bias - 1` in `symbols/<build_id>.debug`, so a report from a build whose archive was not published says so instead of guessing. The reports stay on the user's machine until he attaches one; nothing is uploaded.

## Yanking a bad release

A release that must not be installed is withdrawn in the open and never rewritten. The tag and the assets stay (other clones and the resume logic rely on them); what changes is that every client stops offering it, **without a new release**:

1. Add it to [`data/yanked.json`](../data/yanked.json) on `main` (a normal PR; a maintainer may merge it as an emergency):

   ```json
   { "yanked": [ { "version": "1.5.0-alpha.3", "reason": "Corrupts the workspace file on first launch.", "replacement": "1.5.0-alpha.4" } ] }
   ```

   `version` has no leading `v`; `reason` is one line the user reads; `replacement` is optional and must not be yanked itself. `cargo test -p atlas-core yanked` checks the committed file.
2. From then on, `atlas update`, the app's Settings → Updates and the channel cards, hostd's phone check and `atlas desktop` skip that version: a channel offers the newest published version that is not yanked (Stable falls back to the previous Stable, even when `releases/latest` still names the yanked one). Clients read the file from `raw.githubusercontent.com/codeatlasdev/atlas/main/data/yanked.json` at every update check, so it applies within the check interval (6 h for the cached banner) of the merge. A client that cannot read it (offline, GitHub down, a file that does not parse) logs that and offers every release: the list only removes offers, so not knowing it never blocks an update.
3. A user who **runs** the yanked version is told why (`atlas update`, `atlas update --check --json` as `yanked`, Settings → Updates) and is offered the newest version of their channel that is not yanked, even when that is older than what they run (a confirmed rollback, like `--downgrade`).
4. Mark the section in `CHANGELOG.md` as well: `## [1.4.0-beta.2] — 2026-10-20 [YANKED]` with one line saying why and which version to use instead. Do not delete it.
5. Ship the fix as the next version (`beta.3`, or `patch` for a stable); clients move up to it by SemVer precedence.
6. Only the maintainer may also demote or remove the GitHub release: `gh release edit vX.Y.Z --prerelease` (a release stops being "Latest" once something newer is marked latest; demoting a default-channel alpha hands "Latest" back to the previous build only when you also `gh release edit <tag> --latest`), or `gh release delete vX.Y.Z` for a build that is dangerous to keep downloadable. Never delete or move the **tag**, and a deleted version number is never reused (v1.2.0 was deleted and stays skipped).

The manifest (`manifest.json`) has no kill-switch fields: it holds `version`, `channel`, `prerelease`, `commit`, `date`, `assets`, `changelog` and, for a promotion, `promoted_from`. `scripts/test_generate_manifest.py` pins that set, so a key nothing reads cannot come back. The yank list is not signed yet; it is as trustworthy as `main` (signing waits for #279).

## Provenance and signing: what is true today

| Property | Status |
|---|---|
| Binaries are ad-hoc signed (`codesign --sign -`) | only when the Developer ID secrets are absent (forks, before they are configured); Gatekeeper then warns on a downloaded `.dmg` |
| Developer ID signature and notarization (CLI, `Atlas.app`, dmg) | **decided and implemented**; on once the six secrets exist ([macOS signing and notarization](#macos-signing-and-notarization)). Until a release has run with them, treat releases as ad-hoc |
| Windows Authenticode signature (SignPath Foundation) | **not yet**: the workflow is ready and off; it turns on with the repository variable `SIGNPATH_ENABLED` once the Foundation has accepted the project ([Windows code signing](#windows-code-signing-signpath-foundation)) |
| Checksums | `.sha256` per asset, `SHA256SUMS` over everything, `manifest.json` |
| Signed `SHA256SUMS` | **no**; a maintainer decision, commands below |
| SBOM | CycloneDX JSON of the CLI's dependency tree, when `cargo-cyclonedx` is available |
| Reproducible builds | partial: `--locked`, `SOURCE_DATE_EPOCH`; `--remap-path-prefix` is not set because it has not been verified against the Tauri build |
| Built on GitHub-hosted runners from the tag | yes (`release.yml`, triggered by the `v*` tag push) |
| GitHub artifact attestations (SLSA provenance) | **not enabled**; possible now that builds are hosted |

**What a hosted build can claim:** the artifacts were built by GitHub Actions from the tagged commit, from locked dependencies, and the manifest records that commit. Without attestations that is at most [SLSA](https://slsa.dev) build level 1 (provenance exists, is not signed by a build service, and is self-asserted). Do not describe the builds as higher than that.

If the maintainer decides to sign, these are the commands (run after the release is published, on the downloaded `SHA256SUMS`, then attach the result with `gh release upload`):

```bash
# Keyless-or-key signature of the checksum file, verifiable offline from the bundle
cosign sign-blob --bundle SHA256SUMS.sigstore.json SHA256SUMS
# or, with a minisign key pair
minisign -Sm SHA256SUMS
```

The install docs say which of the two signatures a release carries.

## Contrast audit (before every release)

`bun run check` covers the token level (every text, syntax, indicator, focus-ring and field-border token on every surface and tint of the six bundled themes: `apps/desktop/src/lib/theme/theme.test.ts`, `debug/contrast.test.ts`). The runtime audit needs a debug app build, so it is a release step, not part of the gate:

```bash
bun run desktop:build:app                 # debug .app only (never a DMG)
bun scripts/contrast-audit.ts --json /tmp/contrast.json --md /tmp/contrast.md
```

It starts its own agent instance (never your screen), switches through the six themes and walks ~35 screens (sidebar, panel, logs, editor with find and completion, every Settings tab, palette, What's new incl. an update on offer, confirm/prompt/discover/theme-import dialogs, toasts of every severity, row/project/new-tab menus, tooltip, focus ring, tour, checklist, shortcuts sheet). Per screen it runs the text audit (4.5 body, 3 large/disabled/placeholder/gutter) and the non-text audit (icons, field borders and the focused outline, 3:1). Exit 0 means **0 failures**; fix anything it lists before tagging. Decisions: syntax colours and inlay hints are text (4.5); disabled controls are exempt from AA by WCAG but held to 3 so they stay visible; a text field's 3:1 boundary is the box that wraps the field itself (its own border/fill, or a `label` around it); a container that also holds other controls (the command palette, a dialog) is a surface, and a bare input there is identified by its placeholder and caret; a ring/spinner is judged by its strongest part (the arc, not the faint track); file-type icons repeat the adjacent filename and are marked `data-contrast-decorative` (skipped by the non-text audit, never recoloured: they are brand colours); the GPU terminal's `terminal.minimum_contrast` defaults to 4.5 (the ANSI table is untouched, cells are lifted at draw time).
