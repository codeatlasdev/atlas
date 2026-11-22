# Security policy

## Supported versions

Atlas ships on three [release channels](docs/versioning.md): Stable, Beta and Alpha (the default while Atlas is in alpha). Atlas is in alpha, so there is no long-term support line: **security fixes go into the newest Stable release**, and reach Beta and Alpha through the normal flow. Update with `atlas update`.

| Version | Security fixes |
|---|---|
| Newest Stable release | yes |
| Current Beta (`X.Y.Z-beta.N`, `-rc.N`) | yes, in the next beta build |
| Current Alpha (`X.Y.Z-alpha.N`) | yes, in the next alpha build |
| Anything older | no, please update |

A fix for a vulnerability in released versions is published as a Stable release (and the matching prerelease builds) and listed under **Security** in the [changelog](CHANGELOG.md) with the advisory (GHSA) id and credit to the reporter.

## Reporting a vulnerability

**Do not open a public issue for a security problem.**

Report it privately with GitHub's [private vulnerability reporting](https://github.com/codeatlasdev/atlas/security/advisories/new) on this repository. Only the maintainers can see the report. Please include:

- what is affected (`atlas` CLI, the TUI, `atlas mcp`, `atlas hostd`, `atlas deploy`, Atlas Desktop, the installer, the website);
- the version (`atlas version`) and your OS;
- steps to reproduce, or a proof of concept;
- the impact as you understand it.

### What to expect

Atlas has one maintainer, so these are targets, counted from the day you send the report:

| Step | Target |
|---|---|
| Acknowledgement | within **7 days** |
| Initial assessment (is it a vulnerability, how severe, which versions) | within **14 days** |
| Fix, or a mitigation plan with a date, where a fix is feasible | within **30 days** |
| Public disclosure | coordinated with you, **at most 90 days** after the report; sooner when a fix is out, later only by agreement |

If a step is going to be missed, we tell you why. We credit you in the release notes and in the GitHub security advisory (GHSA) unless you prefer to stay anonymous. Please give us the time above to ship a fix before you disclose publicly. There is no bug bounty.

## Threat model

Atlas is a local developer tool. It runs as **you**, on your machine, and does what your project's files tell it to.

**Trusted, by design:**

- `atlas.yaml`, `atlas.local.yaml`, the scripts and `pre_command`/`post_command` hooks they name, and agent hooks you configure. Atlas runs these commands with your privileges, exactly as `make` or `npm run` would. Opening a repository you do not trust and running `atlas dev` in it is as risky as running its `Makefile`; Atlas does not sandbox it.
- The environment Atlas inherits and the env files you point it at: they are handed to your own services unscrubbed.
- Your own user account and the files you own under `~/.atlas` and `.logs/`.

**Not trusted (a flaw here is a vulnerability):**

- Another local user, another process of a different user, or a web page trying to reach Atlas: the control sockets (`.logs/atlas.sock`, `~/.atlas/desktop.sock`, the host daemon socket) are Unix sockets with mode `0600`, and the hook server is bound to loopback only and requires a token that changes on every run.
- Data that is not yours: a PR or issue URL, a worktree or branch name, a hook payload, a log line, a downloaded release, a language server's output, or text sent by a website in the embedded browser. None of it may become a command, a path outside its scope or a signal to a process Atlas did not start.
- The network between you and GitHub when you install or update (checksums are verified, see [Verifying downloads](#verifying-downloads)).

**Out of the model:** an attacker who already runs code as your user (they can read your files without Atlas), a malicious `atlas.yaml` you chose to run, physical access, and vulnerabilities in a dependency that Atlas does not use in an exploitable way. Reports outside the model are still welcome as hardening ideas but do not get an advisory or CVE.

## What is in scope

Atlas runs on a developer's machine, starts processes for them, and exposes local sockets. Things we treat as security issues include:

- another local user, a webpage or a remote host being able to control a stack, a terminal or the desktop app (the control sockets are `0600` unix sockets and the hook server is loopback-only with a per-run token);
- command injection through `atlas.yaml`, `atlas.local.yaml`, a worktree name, a PR or issue URL or a hook payload;
- secrets leaking into logs, crash reports, the embedded browser or an agent prompt;
- the installer or `atlas update` installing something that is not the published release (downloads are verified against the release's SHA-256);
- Atlas signalling a process it did not start;
- a deploy that can leave a host in an unrecoverable state.

Out of scope: running `atlas dev` on an `atlas.yaml` you do not trust (it runs the commands in it, by design, exactly like a `Makefile` or a `package.json` script); problems that need an attacker who already has your user account; and bugs in third-party dependencies that are already public, unless Atlas uses them in an exploitable way (please report those upstream).

## How Atlas handles secrets

- Atlas never ships, reads or prints your env files except to inject them into your own services. The environment it inherits reaches your services unscrubbed, by design.
- `atlas deploy` never uploads env files; shared secrets live on the host and are referenced with `link`.
- Crash reports stay on your machine and contain the panic message, location, arguments and a backtrace, with tokens redacted and your home directory written as `~` (the newest 20 are kept). Review one before you attach it to an issue.
- Agent hooks post to a loopback HTTP server inside Atlas Desktop with a token that changes on every run.
- Nothing secret should be loaded into the embedded browser. URLs and attributes sent to an agent by Design Mode are redacted.

## Verifying downloads

Each release has a `.sha256` next to every asset, a `SHA256SUMS` over all of them, and a `manifest.json`. `atlas update`, `atlas desktop` and `atlas crew install` verify before installing anything. To check an asset by hand:

```bash
shasum -a 256 -c atlas-<version>-aarch64-apple-darwin.tar.gz.sha256
```

**Signed releases.** Starting with the first signed release (announced in its release notes; earlier releases are not signed), a maintainer signs `SHA256SUMS` offline with an ed25519 SSH key (namespace `atlas-release`) before the release is public, and Atlas takes every asset hash from that signed file, never from the `.sha256` beside the asset. `atlas update`, `atlas desktop` and the download `atlas crew install` makes on your own machine refuse a signed-era release whose signature is missing or invalid, and an asset that does not match the signed file; there is no override. The public keys are in [`crates/atlas-core/data/release-keys.json`](crates/atlas-core/data/release-keys.json). To verify by hand:

```bash
# the keys, as an OpenSSH allowed_signers file (from a checkout at the release tag)
bun scripts/release-keys.ts allowed-signers > allowed_signers
curl -fsSLO https://github.com/codeatlasdev/atlas/releases/download/vX.Y.Z/SHA256SUMS
curl -fsSLO https://github.com/codeatlasdev/atlas/releases/download/vX.Y.Z/SHA256SUMS.sig
ssh-keygen -Y verify -f allowed_signers -I <key name> -n atlas-release -s SHA256SUMS.sig < SHA256SUMS   # prints: Good "atlas-release" signature
shasum -a 256 -c SHA256SUMS --ignore-missing
```

Each release is also attested by GitHub (a Sigstore attestation made by the release workflow), and, once the repository has immutable releases on, GitHub signs the release itself. These need the `gh` CLI and do not replace the maintainer signature:

```bash
gh attestation verify atlas-<version>-aarch64-apple-darwin.tar.gz --repo codeatlasdev/atlas
gh release verify vX.Y.Z --repo codeatlasdev/atlas
gh release verify-asset vX.Y.Z atlas-<version>-aarch64-apple-darwin.tar.gz --repo codeatlasdev/atlas
```

What that does and does not prove, so you can decide how much to trust it:

- **Provenance: built on GitHub-hosted runners from the tag, code signed, checksummed.** Releases are built by GitHub Actions from the version tag. The publish job uses only the run's `GITHUB_TOKEN`; the macOS build job also reads the Developer ID certificate and the App Store Connect key for notarization from repository secrets, into a temporary keychain it deletes ([Releasing](docs/release.md#macos-signing-and-notarization)). The macOS binaries are Developer ID signed and notarized when those secrets are configured, and ad-hoc signed otherwise (forks). Every release is built as a draft; once the maintainers' public keys are configured it is published only after a maintainer signs `SHA256SUMS` with an offline key ([Releasing](docs/release.md#release-signing)). A GitHub attestation of the assets is recorded too.
- A `.sha256` file protects against a corrupted or truncated download only: it comes from the same place as the file, so anyone who can replace the release's assets can replace both. The signed `SHA256SUMS` is what protects against that (a stolen upload token, a compromised GitHub account or workflow can publish assets but cannot produce a signature the clients accept). It does not stop someone who suppresses new releases or serves an old signed one, and a maintainer who signs a poisoned build signs it.
- **Not yet:** the shell installers (`install.sh`, `install.ps1`) and the script `atlas crew install` runs on the server to download the CLI still check only the `.sha256`; they move to the signature in the release that makes signed releases the only bootstrap source ([docs/release.md](docs/release.md#transition)).
- **No SLSA level is claimed** beyond what GitHub's attestation states. See [docs/supply-chain.md](docs/supply-chain.md) for what is done today, what is not claimed and why.
