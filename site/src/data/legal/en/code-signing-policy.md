Free code signing provided by [SignPath.io](https://about.signpath.io), certificate by [SignPath Foundation](https://signpath.org).

<h2 id="scope">Scope</h2>

The Windows files of a release: `atlas.exe`, `atlas-desktop.exe`, the NSIS installer (`Atlas-<version>-x86_64-pc-windows-msvc-setup.exe`) and `Atlas-Setup.exe`. They are built by GitHub Actions from the version tag ([`.github/workflows/release.yml`](https://github.com/codeatlasdev/atlas/blob/main/.github/workflows/release.yml)) and signed by SignPath after a maintainer approves the signing request. Nothing is signed from a developer's machine. macOS and Linux files are not part of it.

<h2 id="status">Status</h2>

Signing is **pending the Foundation's approval**. Until the first signed release, the Windows files are unsigned, so SmartScreen may say "Windows protected your PC": click **More info**, then **Run anyway**. See the [Windows guide](/docs/installation/#windows).

<h2 id="roles">Team roles</h2>

- **Committers and reviewers:** the repository maintainers, see [GOVERNANCE.md](https://github.com/codeatlasdev/atlas/blob/main/GOVERNANCE.md).
- **Approvers** (release a signing request): the members of the GitHub team [`atlas-maintainers`](https://github.com/orgs/codeatlasdev/teams/atlas-maintainers), currently [@mvilacad](https://github.com/mvilacad) and [@luancoleto](https://github.com/luancoleto).

<h2 id="privacy">Privacy</h2>

Atlas does not transfer data to other systems unless you ask it to. It has no telemetry, analytics, crash reporting or accounts. The one request it makes on its own is an update check that reads the public releases of `codeatlasdev/atlas` on GitHub (Atlas Desktop has a *Check for updates automatically* switch in Settings); installing or updating downloads the release from GitHub. Nothing is sent to CodeAtlas or SignPath. Every request is listed in [PRIVACY.md](https://github.com/codeatlasdev/atlas/blob/main/PRIVACY.md).
