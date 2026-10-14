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
- Crash reports stay on your machine and contain the panic message, location, arguments and a backtrace. Review one before you attach it to an issue.
- Agent hooks post to a loopback HTTP server inside Atlas Desktop with a token that changes on every run.
- Nothing secret should be loaded into the embedded browser. URLs and attributes sent to an agent by Design Mode are redacted.

## Verifying downloads

Each release has a `.sha256` next to every asset and a `manifest.json` with the checksum of each file. The installer, `atlas update` and `atlas desktop` verify the checksum before installing anything. To check by hand:

```bash
shasum -a 256 -c atlas-<version>-aarch64-apple-darwin.tar.gz.sha256
```

What that does and does not prove, so you can decide how much to trust it:

- **Provenance: built on GitHub-hosted runners from the tag, ad-hoc code signed, checksummed.** Releases are built by GitHub Actions from the version tag, with no long-lived secret (the publish job uses the run's `GITHUB_TOKEN`), and the binaries are ad-hoc signed (not Developer ID signed or notarized, so macOS may ask you to confirm the first launch of a downloaded app).
- The checksum protects against a corrupted or truncated download. It comes from the same place as the file, so it does not on its own protect against someone who can replace both on GitHub. A signed `SHA256SUMS` is planned, not shipped.
- **No SLSA level is claimed**, and GitHub artifact attestations are not enabled yet (possible now that builds are hosted). See [docs/supply-chain.md](docs/supply-chain.md) for what is done today, what is not claimed and why.
