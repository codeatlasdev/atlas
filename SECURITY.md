# Security policy

## Supported versions

Security fixes go into the latest release. Atlas follows [SemVer](https://semver.org); update with `atlas update`.

| Version | Supported |
|---|---|
| latest release | yes |
| older releases | no, please update |

## Reporting a vulnerability

**Do not open a public issue for a security problem.**

Report it privately with GitHub's [private vulnerability reporting](https://github.com/codeatlasdev/atlas/security/advisories/new) on this repository. Only the maintainers can see the report. Please include:

- what is affected (`atlas` CLI, the TUI, `atlas mcp`, `atlas hostd`, `atlas deploy`, Atlas Desktop, the installer, the website);
- the version (`atlas version`) and your OS;
- steps to reproduce, or a proof of concept;
- the impact as you understand it.

You can expect an acknowledgement within a few days and a fix or a mitigation plan after we have reproduced the issue. We will credit you in the release notes unless you prefer to stay anonymous. Please give us a reasonable chance to ship a fix before you disclose publicly.

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
