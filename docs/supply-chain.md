---
title: "Supply chain and provenance"
description: "What Atlas does today to protect its dependencies and releases, what it does not claim, and the maintainer's checklist for more."
---

# Supply chain and provenance

This page is the honest version of "how much can you trust an Atlas download". It lists what is done today, what is **deliberately not claimed**, and the steps the maintainer can take next. Reporting a vulnerability and how to verify a download are in [SECURITY.md](https://github.com/codeatlasdev/atlas/blob/main/SECURITY.md); how releases are cut is in [Releasing](release.md).

## What Atlas does today

| Area | What is in place | Where |
|---|---|---|
| Dependency policy | `cargo deny check` (advisories from the RustSec database, licenses on an allow-list, duplicate and banned crates) runs in `bun run check`, in CI (`cargo-deny` job) and before every release. The one ignored advisory has a written reason. | [`deny.toml`](https://github.com/codeatlasdev/atlas/blob/main/deny.toml) |
| Pinned inputs | `Cargo.lock` and `bun.lock` are committed. CI, the gate that every release runs first (`bun run check`) and the release build use `--locked`, the desktop build uses `bun install --frozen-lockfile`, and the Rust toolchain is pinned. | `Cargo.lock`, `rust-toolchain.toml` |
| Update PRs | Dependabot opens weekly, grouped updates for Cargo, the two npm projects and GitHub Actions, so a pending dependency change is a reviewable pull request. A new version waits 7 days (`cooldown`) before its pull request, so a compromised or yanked release is usually caught first. | [`.github/dependabot.yml`](https://github.com/codeatlasdev/atlas/blob/main/.github/dependabot.yml) |
| Pinned actions | Every `uses:` in the workflows is pinned to a full commit SHA with the exact version in a comment (`# v7.0.1`). | `.github/workflows/*.yml`, `.github/actions/*` |
| Workflow audit | actionlint (syntax, expressions, shellcheck of `run:` blocks) and [zizmor](https://docs.zizmor.sh) (template injection, credential persistence, cache poisoning, excessive permissions, unpinned actions) run in `bun run check` and the `Workflow lint` CI job. Every suppression is in `.github/zizmor.yml` with its reason. | `scripts/lint-workflows.sh` |
| Least privilege CI | Every workflow sets `permissions: contents: read`; the `Plan` job adds `actions: read` and `pull-requests: read`, the `CI` aggregate job has none, and only the `publish` job of `release.yml` raises it (`contents: write`). Every checkout uses `persist-credentials: false`. Pull requests from forks run with a read-only token and no secrets; untrusted text (the pull request title) reaches scripts only through `env`. | `.github/workflows/*.yml` |
| Contribution provenance | Every commit needs a `Signed-off-by` ([DCO](https://developercertificate.org/)), checked by the `Signed-off-by` job. | `.github/workflows/pr.yml`, `scripts/check-dco.sh` |
| Release integrity | Each release asset has a `.sha256` and `manifest.json` lists the checksum of every file. `atlas update`, `atlas desktop` and the installer verify the checksum before installing anything. | [Installation](installation.md) |
| SBOM | The release job attaches a CycloneDX SBOM of the CLI's dependency tree (`atlas-<version>-sbom.cdx.json`) when `cargo-cyclonedx` is available, and warns loudly when it is not, so a release can ship without one. | `release.yml` |
| Code signing | Binaries and the app are **ad-hoc** code signed (`codesign --sign -`), which satisfies the Apple silicon "must be signed" rule. | `release.yml` |
| No hosted build | Releases are built on the maintainer's machine with `act`; no secret is stored in GitHub. | [Releasing](release.md) |
| Network surface | Atlas makes no network request other than update checks and downloads from GitHub; there is no telemetry. | [PRIVACY.md](https://github.com/codeatlasdev/atlas/blob/main/PRIVACY.md) |

## What is deliberately not claimed

- **No SLSA level.** SLSA Build L2 and above require the build to run on a hosted platform and not on an individual's workstation. Atlas is built on a maintainer's laptop, so even a signed provenance document would be at most Build L1. Atlas publishes none and claims none.
- **No GitHub artifact attestations.** They need the GitHub Actions OIDC token of a hosted run (`id-token: write`); `act` does not provide it. A hosted job that only attests files already uploaded would prove "these bytes were uploaded", not "these bytes were built from this commit", so it is not used.
- **Not Developer ID signed or notarized.** Gatekeeper may ask for confirmation on first open of a downloaded app. The installer and `atlas update` fetch with `curl`, which is the path that works without a prompt.
- **Not reproducible bit for bit.** Code signing and the macOS bundle embed timestamps; nothing here promises two builds are byte identical.
- **No signed checksum file yet** (it is in the list below). The SBOM is best effort, see the table above.
- **No third-party audit.** Dependencies are screened by `cargo deny`, not reviewed line by line.

Provenance in one sentence: *maintainer-built locally, ad-hoc code signed, checksummed.*

## Release checklist addition

`CITATION.cff` carries the version and release date of the latest release. The release tool should bump `version` and `date-released` there together with the Cargo versions and `CHANGELOG.md`, and `scripts/release.sh` is the place to do it. Until it does, bump both by hand in the release commit.

## Maintainer recommendations

None of this is applied; each item needs repository-owner permissions or a purchase, so it is the maintainer's decision. They are ordered by value for effort.

### 1. Branch ruleset for `main` and `release/*`

Settings → Rules → Rulesets → New branch ruleset.

1. Name `protect main and release`, enforcement **Evaluate** first (it logs what would be blocked), then **Active**.
2. Target branches: the default branch, and the pattern `release/*`.
3. Rules:
   - Restrict deletions; block force pushes.
   - Require linear history.
   - Require signed commits.
   - Require a pull request before merging, with **0** required approvals (there is one maintainer).
   - Require status checks to pass, using the real check names from `.github/workflows`:
     `CI` (the aggregate job of `ci.yml`: it passes only when every other job passed, so jobs can change without touching the ruleset), and `Signed-off-by` and `Conventional title` (from `pr.yml`).
4. Bypass list: add the maintainer's role with **For pull requests only**, so a direct push is impossible but the maintainer can still merge a pull request when a hosted check is unavailable.

A required check has to be reported by something. CI runs on GitHub-hosted runners for pull requests; releases do not need them. If the maintainer prefers not to spend those minutes, drop the hosted checks from the list and keep `Signed-off-by` only.

### 2. Tag ruleset for `v*`

New tag ruleset, target `v*`, rules: restrict deletions and restrict updates (a published tag must never move; the release flow in [Releasing](release.md) creates tags, it never re-points them). Add the maintainer as the only one who may create them.

### 3. Signed commits and tags

```bash
git config --global gpg.format ssh
git config --global user.signingkey ~/.ssh/id_ed25519.pub
git config --global commit.gpgsign true
git config --global tag.gpgsign true
```

Add the same public key to GitHub as a **signing key** (Settings → SSH and GPG keys → New SSH key → type *Signing Key*). Turn on **Vigilant mode** (Settings → SSH and GPG keys) so unsigned commits and tags attributed to the maintainer show as *Unverified*. Verify a tag with `git tag -v vX.Y.Z`.

### 4. Secret scanning, push protection, private vulnerability reporting

Settings → Advanced Security (Code security): enable **Secret scanning**, **Push protection** and **Private vulnerability reporting**. `SECURITY.md` already tells reporters to use private vulnerability reporting, which works only once it is enabled; check that `https://github.com/codeatlasdev/atlas/security/advisories/new` opens a form.

### 5. Immutable releases

Settings → General → Releases → **Enable release immutability**. A published release then cannot have assets added, replaced or removed, and its tag cannot move. Two consequences to decide on first:

- The release flow must create the release as a **draft**, upload every asset, then publish. Today the `publish` job of `release.yml` creates it already published (`draft: false`) with its assets in one step, so it needs `draft: true`, then a publish step, before this is turned on.
- A future moving "channels" pointer release (a design option for channel discovery that is not built) would conflict with it: an immutable release cannot be re-uploaded with `--clobber`. The channel logic in Atlas reads `releases.atom` (and the `releases/latest` redirect as one more candidate), not a moving release, so there is no conflict today.

### 6. Dependabot cooldown

Dependabot already waits 3 days before opening a version update when nothing is configured. Make it explicit and longer for the ecosystems where a compromised release is most likely to be yanked within a week. For each `updates:` entry of `.github/dependabot.yml`:

```yaml
    cooldown:
      default-days: 7
      semver-major-days: 14
```

Security updates are not delayed by a cooldown.

### 7. OpenSSF Scorecard without Actions minutes

Do not add the Scorecard Action: it runs on hosted runners. Instead:

```bash
brew install scorecard
GITHUB_AUTH_TOKEN="$(gh auth token)" scorecard --repo=github.com/codeatlasdev/atlas
```

or read the weekly public scan at `https://api.scorecard.dev/projects/github.com/codeatlasdev/atlas` (no setup). Expected low scores by design for a one-person project: Code-Review, Contributors, Signed-Releases (until item 9), SAST, Fuzzing. A Scorecard *badge* requires the Action with `publish_results: true`, so Atlas does not show one.

### 8. OpenSSF Best Practices badge ("passing")

Apply at <https://www.bestpractices.dev> with the repository URL. The table maps the *passing* criteria categories to what Atlas already satisfies. **Met** means a file in the repository proves it; **Unmet** means a gap to close before applying; **N/A** is allowed by the criteria.

| Category | Criteria (ids) | Status | Evidence or gap |
|---|---|---|---|
| Basics | `description_good`, `interact`, `contribution`, `contribution_requirements` | Met | [README.md](https://github.com/codeatlasdev/atlas/blob/main/README.md), [CONTRIBUTING.md](https://github.com/codeatlasdev/atlas/blob/main/CONTRIBUTING.md) |
| Basics | `floss_license`, `floss_license_osi`, `license_location` | Met | `LICENSE-MIT`, `LICENSE-APACHE` at the root |
| Basics | `documentation_basics`, `documentation_interface` | Met | [`docs/`](https://github.com/codeatlasdev/atlas/tree/main/docs), [CLI reference](cli.md) |
| Basics | `sites_https`, `english`, `maintained` | Met | `https://atlas.codeatlas.com.br`; docs in English; releases and commits continue |
| Basics | `discussion` | Met | GitHub issues and pull requests are searchable, addressable by URL and open to anyone; Discussions are not needed (see [labels and triage](labels.md)) [INFERENCE: confirm against the criterion text when applying] |
| Change control | `repo_public`, `repo_track`, `repo_interim`, `repo_distributed` | Met | public Git repository on GitHub |
| Change control | `version_unique`, `version_semver`, `version_tags` | Met | [Versioning](versioning.md); tags `vX.Y.Z` |
| Change control | `release_notes`, `release_notes_vulns` | Met | [CHANGELOG.md](https://github.com/codeatlasdev/atlas/blob/main/CHANGELOG.md); `SECURITY.md` promises credit and a **Security** entry for every fixed vulnerability |
| Reporting | `report_process`, `report_tracker`, `report_responses`, `enhancement_responses`, `report_archive` | Met | issue forms in `.github/ISSUE_TEMPLATE/`, [triage targets](labels.md#triage-policy) |
| Reporting | `vulnerability_report_process`, `vulnerability_report_private`, `vulnerability_report_response` | Met once item 4 is done | [SECURITY.md](https://github.com/codeatlasdev/atlas/blob/main/SECURITY.md) (private reporting, acknowledgement in 7 days, assessment in 14) |
| Quality | `build`, `build_common_tools`, `build_floss_tools` | Met | `cargo build`, `rust-toolchain.toml` |
| Quality | `test`, `test_invocation`, `test_most`, `test_continuous_integration`, `test_policy`, `tests_are_added`, `tests_documented_added` | Met | `cargo test --workspace`, `.github/workflows/ci.yml`, tests-first rule in [AGENTS.md](https://github.com/codeatlasdev/atlas/blob/main/AGENTS.md) and the PR template |
| Quality | `warnings`, `warnings_fixed`, `warnings_strict` | Met | `clippy -D warnings`, `[workspace.lints]` in `Cargo.toml` |
| Security | `know_secure_design`, `know_common_errors` | Met (self-attested) | threat model in `SECURITY.md`, runtime safety rules in `AGENTS.md` |
| Security | `crypto_*` | N/A | Atlas implements no cryptography of its own; TLS comes from `rustls` through `reqwest` |
| Security | `delivery_mitm`, `delivery_unsigned` | Met for HTTPS; **Unmet for signatures** until item 9 | downloads are HTTPS and verified against a `.sha256` published beside them, but that file comes from the same place as the asset |
| Security | `vulnerabilities_fixed_60_days`, `vulnerabilities_critical_fixed` | Met by policy | 30 day fix or mitigation target in `SECURITY.md` |
| Security | `no_leaked_credentials` | Met | no secrets stored in the repository; secret scanning (item 4) enforces it |
| Analysis | `static_analysis`, `static_analysis_fixed` | Met if clippy is accepted as the analyzer | `clippy -D warnings` in CI; CodeQL or Semgrep would remove the doubt but run on hosted minutes |
| Analysis | `dynamic_analysis*` | N/A or suggested | the criteria are SUGGESTED, not required, for passing |

The criteria text is on the badge site and changes; re-read each answer when you apply.

### 9. A signed `SHA256SUMS`

Today a `.sha256` next to an asset protects against a corrupted download, not against a tampered release, because both come from the same place. Publish one `SHA256SUMS` per release and sign it with one of:

```bash
# keyless, bound to the maintainer's identity, logged in Rekor; opens a browser
shasum -a 256 atlas-*.tar.gz Atlas-*.dmg Atlas-*.app.tar.gz > SHA256SUMS
cosign sign-blob SHA256SUMS --bundle SHA256SUMS.sigstore.json

# or a plain key pair, no network needed
minisign -Sm SHA256SUMS
```

Verify with `cosign verify-blob --bundle SHA256SUMS.sigstore.json --certificate-identity <maintainer email> --certificate-oidc-issuer <issuer> SHA256SUMS`, or `minisign -Vm SHA256SUMS -P <public key>`. Put the public key or identity in `SECURITY.md`. The Scorecard *Signed-Releases* check looks for `*.sigstore.json`, `*.minisig`, `*.sig` and `*.asc` among the release assets, so this also improves that score. Teach `atlas update` to verify the signature only once it ships with a pinned public key; until then it is documentation, not a guarantee.

### 10. Developer ID signing and notarization

An Apple Developer Program membership costs 99 USD a year. With a Developer ID Application certificate in the maintainer's keychain (builds are local, so no secret needs to be stored anywhere):

```bash
security find-identity -v -p codesigning        # find the identity
codesign --sign "Developer ID Application: <name> (<team id>)" --options runtime --timestamp --force dist/atlas
xcrun notarytool submit Atlas.dmg --keychain-profile "<profile>" --wait
xcrun stapler staple Atlas.dmg
```

A bare CLI binary cannot be stapled; the app and the dmg can. This replaces the ad-hoc `codesign --sign -` in `release.yml` and is one of the [general availability](versioning.md#general-availability) criteria.

### 11. SBOM

The release already attaches a CycloneDX SBOM when the tool installs. To make it a guarantee, make a missing `cargo-cyclonedx` fail the release instead of warning. `cargo-auditable` is an alternative that embeds the dependency list in the binary so `cargo audit bin` can scan an installed `atlas`; it is not wired in.

### 12. Organization-level community files

If CodeAtlas gets more repositories, a `codeatlasdev/.github` repository (it does not exist today) can hold shared `CODE_OF_CONDUCT.md`, `SECURITY.md` and `SUPPORT.md` that every repository without its own copy inherits.
