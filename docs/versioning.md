---
title: "Versioning and release channels"
description: "How Atlas versions releases, what Stable, Beta and Alpha mean, how to switch, and what is supported."
---

# Versioning and release channels

> **Atlas is in alpha.** Every channel, including Stable, can have rough edges and breaking changes until the first release that meets the [general availability criteria](#general-availability) below. Channels describe how thoroughly a build was soaked, not how finished Atlas is.

Atlas ships the CLI and the desktop app together with the same version. Every release is built and published by GitHub Actions from the version tag (see [Releasing](release.md)).

## Channels

You choose what you receive. **Until the first Stable that meets the [general availability criteria](#general-availability), the default is Alpha**: an install or update that never picked a channel follows the newest build from `main`. Anyone who chose a channel (the installer, Settings, `atlas update channel`, `ATLAS_UPDATE_CHANNEL`) keeps it, and Stable stays one command away. Once a real Stable exists the default goes back to **Stable**.

| Channel | Version looks like | Cut from | Risk |
|---|---|---|---|
| **Stable** | `1.4.0` | a `release/X.Y` branch, after a beta | lowest |
| **Beta** | `1.4.0-beta.2`, `1.4.0-rc.1` | a `release/X.Y` branch | medium |
| **Alpha** | `1.5.0-alpha.3` | `main` | highest |

### Stable

Only builds that already ran as a beta, with nothing changed since but the version number.

- **Risk:** lowest risk of the three.
- **Cadence:** when a soaked beta is promoted, usually every few weeks.
- **For:** anyone who uses Atlas for real work and wants only soaked builds.
- **Switching to it:** you move to the next Stable release that is newer than the version you run. Atlas never installs an older build by itself.
- While Atlas is in alpha, Stable means soaked through alpha and beta, not finished.

### Beta

Feature-complete builds that are being soaked before they become Stable. Only fixes land between betas.

- **Risk:** mostly works; a bug can slip through.
- **Cadence:** a new beta about every one to two weeks while a release is soaking.
- **For:** people who want next release's features a few weeks early and will report what breaks.
- **Switching to it:** you get the newest beta or release candidate, and Stable as soon as it is newer. Going back to Stable later waits for a Stable newer than your build, or you can reinstall Stable explicitly.

### Alpha

The default while Atlas is in alpha. Fresh builds cut straight from main, with little soak. Features can be unfinished and behavior can change between builds.

- **Risk:** expect rough edges and breaking changes.
- **Cadence:** whenever the maintainer cuts one, often several times a week.
- **For:** contributors and testers who accept breakage in exchange for the newest work.
- **Switching to it:** you get the newest build of any channel, so a settings or config change may not be readable by an older Atlas. Going back to Beta or Stable later waits for a newer build there, or you can reinstall it explicitly.

### Switching channels

| Where | How |
|---|---|
| Desktop | Settings → General → Updates → **Update channel** |
| CLI | `atlas update channel` (shows each channel and what it offers now), `atlas update channel beta` (sets it), `atlas update --channel beta` (sets it and updates) |
| Scripts / CI | `ATLAS_UPDATE_CHANNEL=beta` overrides the saved choice |
| Installer | `curl -fsSL https://atlas.codeatlas.com.br/install.sh \| ATLAS_CHANNEL=stable bash` (or `beta`, `alpha`); without it the installer follows the default channel |

The choice is one word in `~/.atlas/update-channel`, shared by the CLI and the app.

**Atlas never downgrades by itself.** If you move to a more conservative channel (Alpha → Stable) while running a newer build than that channel offers, nothing is installed. Atlas says so and you either wait for a release newer than your build, or choose **Install X now** (CLI: `atlas update --downgrade`). A downgrade can leave settings or data written by the newer build; keep that in mind before using it.

## Version numbers

Atlas uses [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html).

- **Releases:** `MAJOR.MINOR.PATCH`, tag `vMAJOR.MINOR.PATCH`.
- **Prereleases:** `-alpha.N`, `-beta.N`, `-rc.N`, with a **dot before the counter** (`1.4.0-alpha.10` is newer than `1.4.0-alpha.9`; `alpha10` would compare as text). Precedence is `alpha < beta < rc < release`, and a prerelease of the next minor is newer than the previous release (`1.5.0-alpha.1 > 1.4.0`).
- Build metadata (`+…`) is never used.
- Any other label (`nightly`, `dev`, …) is not offered on any channel.
- **What GitHub's "Latest" means.** While the default channel is Alpha (pre-GA), a release of the default channel is published as a normal release and marked **Latest**, so the badge, the releases page and `releases/latest` show the newest build a user who never chose gets: the newest alpha. Beta and rc builds stay marked **pre-release**. This is *not* the Stable channel: Atlas, the installer and the site never read Stable from `releases/latest`; Stable is the newest tag without a prerelease label in `releases.atom`. At GA the default is Stable again, every `-` tag is a pre-release again, and "Latest" is the newest Stable. The flags are decided by `scripts/release-version.ts github-flags` from the `default` in `crates/atlas-core/data/channels.json`.

A channel offers the newest version, by SemVer precedence, among the releases it accepts: Stable accepts releases only; Beta accepts releases, `beta` and `rc`; Alpha accepts everything above plus `alpha`. A channel is therefore never worse than the one more conservative than it.

## How a release moves between channels

```text
main        ── 1.5.0-alpha.1 ── 1.5.0-alpha.2 ──▶            (Alpha)
   \
    release/1.4 ── 1.4.0-beta.1 ── fix ── 1.4.0-beta.2 ── 1.4.0 ── fix ── 1.4.1    (Beta, then Stable)
```

1. Work lands on `main`, which is always releasable as the next alpha.
2. When a release is feature-complete, the first beta creates `release/X.Y` from `main`. From then on only fixes reach it: a fix lands on `main` first and is cherry-picked.
3. **Promotion to Stable** takes a beta (or release candidate) that has soaked, and the Stable commit may differ from it **only in version files and `CHANGELOG.md`**. The release tool refuses otherwise, so Stable is a soaked beta plus a version number. During the alpha era the minimum soak is 48 hours on beta; the maintainer can override it and the release notes then say so.
4. Hotfixes of the newest Stable are `X.Y.(Z+1)` releases from `release/X.Y`.

Release notes live in [`CHANGELOG.md`](https://github.com/codeatlasdev/atlas/blob/main/CHANGELOG.md): a prerelease section lists what changed since the previous prerelease, and the Stable section is the complete summary since the previous Stable. *What's new* hides a prerelease section once its Stable is in range.

## Support

While Atlas is in alpha, **only the newest Stable gets fixes**, and fixes reach Beta and Alpha through the normal flow. There is no long-term support line. Security fixes follow [SECURITY.md](https://github.com/codeatlasdev/atlas/blob/main/SECURITY.md).

| | Fixes | Security fixes |
|---|---|---|
| Newest Stable | yes | yes |
| Current Beta | yes, in the next beta | yes |
| Current Alpha | next alpha | yes |
| Anything older | no | no |

## What Atlas promises, and when

| Surface | During alpha | After general availability |
|---|---|---|
| `atlas.yaml` keys and meaning | may change between any two releases, called out in the changelog | no breaking change inside a major version; a key is deprecated for at least one minor release (and warned about) before removal |
| CLI commands, flags, exit codes | may change, called out in the changelog | same rule |
| MCP tool names and arguments | may change, called out in the changelog | same rule |
| Settings files (`~/.atlas`) | forward-only; an older build may not read a newer file | forward-only migrations; an upgrade from the previous Stable never loses data |
| Upgrade path | `atlas update` from any earlier release | from the previous Stable and from its betas |

Every breaking change, in any phase, is listed under **Changed** or **Removed** in the changelog with what to do instead.

## General availability

Atlas leaves alpha with a deliberate release, not a version-number event. The first Stable release that meets **all** of these is declared generally available, and the alpha notices are removed in the same release:

1. `atlas.yaml`, the CLI surface (commands, flags, exit codes) and the MCP tool names are documented and covered by tests that fail on an unintended break.
2. The deprecation rule in the table above is in force and the release tool enforces a changelog entry for every breaking change.
3. An upgrade from the previous Stable has been exercised on a real install (config, desktop settings, terminal host), including a downgrade message.
4. The release has spent at least 30 days as a beta with no open data-loss, crash-on-start or security issue.
5. The support window and the security response targets in `SECURITY.md` are published and met for the last two releases.
6. macOS builds are signed with a Developer ID and notarized, or the install docs say honestly why not.

The version number of that release is whatever the next minor would be; Atlas does not jump to 2.0 to mark it.

In that same release the default channel goes back to Stable: `#[default]` on `Channel` in `crates/atlas-core/src/channel.rs`, `"default"` in `crates/atlas-core/data/channels.json` (what `release.yml` reads: from then on `-` tags are pre-releases again and only a Stable is "Latest"; a test fails if it differs from the attribute), `DEFAULT_CHANNEL` in `site/functions/_lib/channels.ts` (a test fails if the two differ), `CHANNEL=${ATLAS_CHANNEL:-…}` in `scripts/bootstrap.sh`, the `The default while Atlas is in alpha.` sentence in the Alpha promise (`crates/atlas-core/data/channels.json`, repeated above) and `site/public/install.sh`. Installs that chose a channel are not touched; an install that only ever followed the default moves to Stable as soon as Stable is newer than its build, and is never downgraded.

## Verifying a download

Each release lists a `.sha256` next to every asset and a `manifest.json` with the version, date, size and checksum of each file. `atlas update` and the installer verify the checksum before installing.
