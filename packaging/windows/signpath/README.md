# SignPath code signing

Windows releases are signed through [SignPath Foundation](https://signpath.org) (free for open-source projects). `.github/workflows/release.yml` submits three signing requests, in the order the files are embedded in one another:

| Order | Artifact configuration slug | File here | What is signed | Input zip (job) |
| --- | --- | --- | --- | --- |
| 1 | `binaries` | `binaries.xml` | `atlas.exe`, `atlas-desktop.exe` | `windows-binaries` |
| 2 | `installer` | `installer.xml` | the NSIS `*-setup.exe`, which carries the signed binaries | `windows-installer` |
| 3 | `setup` | `setup.xml` | `Atlas-Setup.exe`, which embeds the signed NSIS installer | `windows-setup` |

Each configuration takes a `version` parameter (the tag without `v`, set by the workflow) and restricts the files to `product-name="Atlas"` and `product-version=<version>`, which `crates/atlas-cli/build.rs` (`atlas.exe`), `tauri-build` (`atlas-desktop.exe`) and `crates/atlas-setup/build.rs` (`Atlas-Setup.exe`) write into the version resource. The NSIS installer's metadata comes from Tauri: check `product-name` against a real installer when you first see a request rejected.

The files are valid against SignPath's schema (`https://app.signpath.io/Web/artifact-configuration/v1.xsd`).

## One-time setup on signpath.org (maintainer)

1. Apply at <https://signpath.org/apply.html> for `codeatlasdev/atlas`. Review is manual; the page documents no turnaround.
2. In the SignPath organization, add the **GitHub.com** trusted build system and link it to the project.
3. Create the project (repository URL `https://github.com/codeatlasdev/atlas`) and paste each XML above as an artifact configuration with the slug in the table (the first one can be the project's default).
4. Create the **release-signing** policy: manual approval by the approvers, origin verification on, and only the repository `codeatlasdev/atlas` (releases build from `v*` tags, and a re-run through `workflow_dispatch` runs the workflow from `main` but builds the tag, so check that the policy accepts both; test it on a test-signing policy first).
5. Create a CI user with permission to submit to that policy and an API token for it.
6. Turn on multi-factor authentication for every maintainer account (GitHub and SignPath): a Foundation requirement.

## Repository secrets and variable

Settings › Secrets and variables › Actions:

| Name | Kind | Value |
| --- | --- | --- |
| `SIGNPATH_API_TOKEN` | secret | the CI user's API token |
| `SIGNPATH_ORGANIZATION_ID` | secret | the organization id (SignPath, organization page) |
| `SIGNPATH_PROJECT_SLUG` | secret | the project slug |
| `SIGNPATH_SIGNING_POLICY_SLUG` | secret | the signing policy slug (for example `release-signing`) |
| `SIGNPATH_ENABLED` | **variable** | `true` to sign; anything else (or unset) keeps the unsigned release |

`SIGNPATH_ENABLED` is the switch. Until it is `true` the three `sign-*` jobs are skipped and the release ships the same unsigned files under the same names. Set it only after the Foundation has accepted the project and a test request has passed: with it on, every release waits for three manual approvals in SignPath (the jobs wait up to 6 hours each).

## After signing

A signed file has new bytes, so the `.sha256` files, `SHA256SUMS` and `manifest.json` are produced after the last signature (`build-windows` and `publish`). The Windows publisher shown by SmartScreen and in the file's Properties is the Foundation's certificate subject. SmartScreen reputation still builds with downloads, so the first signed releases may warn; the "Windows protected your PC" note in `docs/installation.md` stays until that stops happening.
