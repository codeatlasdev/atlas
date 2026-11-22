---
title: "Labels and triage"
description: "The issue and pull request labels, who applies them, and how new issues are triaged."
---

# Labels and triage

Atlas has one small label set, kept in [`.github/labels.yml`](https://github.com/codeatlasdev/atlas/blob/main/.github/labels.yml). Each label is `family: value`, so the families sort together in the label picker. This page says what each family means, who applies it and what you can expect after you open an issue.

## Label families

| Family | Values | Applied by | Rule |
|---|---|---|---|
| `type:` | `bug`, `feature`, `docs`, `chore`, `security`, `question` | the issue form sets it; a maintainer fixes it | exactly one |
| `area:` | `cli`, `runtime`, `tui`, `desktop`, `terminal`, `editor`, `mcp`, `deploy`, `installer`, `site`, `ci`, `crew` | a maintainer during triage | one or more |
| `priority:` | `0 data loss or security`, `1 high`, `2 normal`, `3 low` | a maintainer during triage | at most one; contributors do not set it |
| `status:` | `needs-triage`, `needs-repro`, `confirmed`, `blocked`, `upstream`, `stale`, `wontfix`, `duplicate` | issue forms set `needs-triage`; a maintainer moves it | one at a time |
| `channel:` | `stable`, `beta`, `alpha` | a maintainer, from the **Release channel** answer in the bug form | the channel the problem was seen on, see [Versioning](versioning.md) |

Labels without a family:

| Label | Meaning |
|---|---|
| `good first issue` | Small, well described, no hidden context. GitHub lists these for new contributors, so the issue says where to look and what done means. |
| `help wanted` | A maintainer would welcome a pull request; it may need more experience than a first issue. |
| `breaking change` | The change affects `atlas.yaml`, the CLI, MCP tools or settings files. It needs a **Changed** or **Removed** changelog entry that says what to do instead, and follows the [deprecation policy](deprecation.md). |
| `skip-changelog` | A pull request with no user-visible change, so no `CHANGELOG.md` line is expected. |
| `dependencies` | Dependency updates, applied by Dependabot. |
| `ci: rerun-after-stall` | Set by `ci-rerun-lost.yml` when CI reran a job after a lost or stalled runner. A rerun that passes could hide a real hang: read the comment (job, kind, tests running at the last sample) and #520, which collects one line per occurrence, before trusting the green. |

### Priorities

| Label | Means | Example |
|---|---|---|
| `priority: 0 data loss or security` | Data loss, crash on start, or a security impact. Fixed first, see [SECURITY.md](https://github.com/codeatlasdev/atlas/blob/main/SECURITY.md) for vulnerabilities | An update that deletes `~/.atlas` settings |
| `priority: 1 high` | A main workflow is broken and there is no reasonable workaround | `atlas dev` never reaches a ready service |
| `priority: 2 normal` | A real problem with a workaround, or a clear improvement | A flag behaves differently from the docs |
| `priority: 3 low` | Cosmetic, rare, or nice to have | A misaligned column in one theme |

## Triage policy

Atlas is maintained by one person in spare time ([GOVERNANCE.md](https://github.com/codeatlasdev/atlas/blob/main/GOVERNANCE.md)). These are targets, not guarantees; the [support page](https://github.com/codeatlasdev/atlas/blob/main/SUPPORT.md) says there is no support SLA. Vulnerabilities have their own, firmer targets in [SECURITY.md](https://github.com/codeatlasdev/atlas/blob/main/SECURITY.md).

| Step | Target | What happens |
|---|---|---|
| First response | within 7 days | A human reads the issue, sets `type:` and `area:`, and either asks for what is missing or moves it to `status: confirmed`. |
| Reproduction | the same pass | An issue that cannot be acted on without more detail gets `status: needs-repro` and one comment listing exactly what is missing (version, channel, a minimal `atlas.yaml`, logs). |
| Priority | at confirmation | A maintainer sets `priority:`. |
| Pull requests | first review within 7 days | A maintainer reviews, or says when they will. |

### Closing stale issues

There are no bots: Actions minutes are kept for CI. Stale handling is done by hand, with `gh`, so every closure has a person behind it.

- An issue in `status: needs-repro` with no answer for **30 days** gets `status: stale` and a comment saying it will close in 14 days.
- With no answer after those **14 days** it is closed as *not planned*, with a comment that it can be reopened with the missing information.
- Confirmed bugs, accepted features and `help wanted` issues are **never** closed for inactivity.
- `status: wontfix` and `status: duplicate` issues are closed at once, with the reason or the link to the other issue.

A maintainer can list the candidates with:

```bash
gh issue list --label "status: needs-repro" --state open --json number,title,updatedAt \
  --jq '.[] | select(.updatedAt < (now - 30*86400 | todate)) | "#\(.number) \(.title)"'
```

## Keeping the labels in sync

`.github/labels.yml` is the source of truth; the repository's labels are made to match it from a maintainer's machine, with no Actions minutes:

```bash
scripts/sync-labels.sh --dry-run   # prints the `gh label create` commands, runs nothing
scripts/sync-labels.sh             # creates missing labels, updates color and description (gh label create --force)
scripts/sync-labels.sh --prune     # also lists labels on GitHub that are not in the file; it never deletes
scripts/test-labels.sh             # checks the file: unique names, 6-digit colors, descriptions of at most 100 characters
```

Changing a label means editing `.github/labels.yml` in a pull request; a maintainer runs the sync after merge. Renaming a label in the file creates a new one: rename it on GitHub first (Issues → Labels → Edit), then change the file, so existing issues keep it. Release notes are written by hand from [`CHANGELOG.md`](https://github.com/codeatlasdev/atlas/blob/main/CHANGELOG.md), so labels do not generate them.
