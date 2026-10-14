---
name: atlas-workflow
description: Committing, pushing, branching, fixing CI, or cutting a release in the atlas repo. Read before any git write or `bun run release`.
---

# Working on Atlas: branches, gate, commits, releases

## Branches

```text
main ── alpha ── alpha ──────────────▶  everything lands here first
   \
    release/X.Y ── beta ── fix(cherry-pick) ── beta ── stable ── patch
```

- `main` and `release/*` are protected by a GitHub ruleset ("main and release branches"): pull request required, 2 approving human reviews (stale ones dismissed, last push approved by someone else), threads resolved, three required checks (`CI` from `ci.yml`, `Signed-off-by` and `Conventional title` from `pr.yml`), branch up to date, no deletion, no force-push. **Only maintainers bypass** (team `atlas-maintainers`: mvilacad, luancoleto), which is how they merge without approvals and how `bun run release` pushes the release commit and tag. Everyone else, including write-access contributors who are not maintainers, goes through a pull request with 2 approvals.
- Tags `v*` are immutable (ruleset "release tags are immutable"), same single bypass. Never move or delete a tag; cut the next version instead.
- Fixes land on `main` first. For a soaking release: `git switch release/X.Y && git cherry-pick -x <sha>`, then a PR to `release/X.Y` by a maintainer. Never commit a fix only to `release/X.Y`.
- The flow for every change: branch `type/short-desc` (fork or branch), draft PR to `main`, CI green, 2 human approvals from people other than the author (external contributors only; maintainers need none), a maintainer merges (squash). The canonical text is [CONTRIBUTING.md](../../../CONTRIBUTING.md#pull-request-flow-and-who-merges); do not restate or soften it here.
- Bypass is for maintainers: merging their own PR without approvals, a release, or an emergency hotfix (with a note saying what was bypassed and why). An agent never pushes to `main`, merges a PR or uses the bypass unless a maintainer is running it and asks for exactly that. If you are not sure whom you are working for, open a PR.

## The gate

1. `bun run check` on its own; read the exit code (`> /tmp/check.log 2>&1; echo rc=$?`). Never chain `git push` or a release after it with `;`.
2. Formatting failures: `bun run fmt`, never hand-format.
3. CI runs on PRs and on pushes to `main` and `release/**`. After a push, wait until the run is `completed` and read its `conclusion`. `gh run watch --exit-status` returns 1 on GitHub API 502s while the run is still in progress, so its exit code alone means nothing:
   ```bash
   id=$(gh run list -L 1 --workflow CI --branch main --json databaseId -q '.[0].databaseId')
   gh run view $id --json status,conclusion,jobs -q '.status,.conclusion,(.jobs[]|"\(.name) \(.conclusion)")'
   gh run view --job <job-id> --log-failed | grep -E "error|\(fail\)" -A5
   ```
4. A red CI job is a bug: regression test first (skill `real-tests`), then the fix.
5. On `main`, a push whose tree its pull request already tested shows the non-Rust jobs as skipped and the Rust jobs as build-only (`scripts/ci-tested.sh`); that is expected, and `CI` is still green. Workflow changes: `bun run lint:workflows` (actionlint + zizmor) before pushing.

## Commits

- `git commit -s` (DCO). Conventional, imperative subject: `feat:`, `fix:`, `test:`, `docs:`, `chore:`, scoped when useful (`fix(desktop): …`).
- Stage paths, not `git add -A`; check `git diff --cached --stat` for files you did not touch (other agents and the user work in the same tree).
- User-visible changes get a line under `## [Unreleased]` in `CHANGELOG.md` in the same commit.

## Releases

Only maintainers cut releases, and only from a green CI on the exact commit. `scripts/release.sh` picks the version, bumps every crate, tags, pushes and publishes through `act` (~40 min; run with a long timeout). Full details: `docs/release.md`, `docs/versioning.md`.

| Want | Branch | Command |
|---|---|---|
| Ship `main` to early adopters | `main` | `bun run release alpha` |
| Start stabilizing X.Y | `main` | `bun run release beta` (creates `release/X.Y`) |
| Another beta after cherry-picks | `release/X.Y` | `bun run release beta` |
| Promote after the 48 h soak | `release/X.Y` | `bun run release stable` |
| Hotfix a stable | `release/X.Y` | `bun run release patch` |

Default choice: `alpha` from `main` for new work; `beta` once a feature set should become the next stable; `stable` only after the soak (avoid `--force-soak`). Always `--dry-run` first. After publishing, verify with the commands under "Verify a release" in `docs/release.md`.

**GitHub "Latest" = the default channel.** While the default channel is Alpha (`crates/atlas-core/data/channels.json`), every alpha is published as a normal release and becomes GitHub's "Latest" (and `/releases/latest`); betas stay pre-releases. Clients built from 1.5.0-alpha.4 on resolve Stable as the newest release without a prerelease label, never from `/releases/latest`; 1.4.0 binaries follow `/releases/latest`, which moves them to Alpha on purpose. At GA, set the default back to `stable` in that one file (tests flag every copy that drifts) and the badge returns to Stable releases.

## Never

- Force-push, rewrite published history, delete branches or tags.
- Merge a pull request (yours or anyone's), push to `main`, or use the ruleset bypass, unless a maintainer is running you and asks for it.
- Release from a red or still-running CI.
- `docker compose down`, port-based kills, or showing an Atlas window on the user's screen (see `AGENTS.md`).
