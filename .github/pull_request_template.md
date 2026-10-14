## What and why

<!-- What does this change, and why? Link the issue it closes: "Closes #123". Open this as a draft until it is ready for review. -->

## How it was tested

<!-- Tests added or changed, and anything you ran by hand. -->

## Screenshots or recording

<!-- Required for any UI change (desktop app, TUI, website). Delete this section otherwise. -->

## AI assistance

<!-- Tool and what it did, or "none". See the AI policy: https://github.com/codeatlasdev/atlas/blob/main/CONTRIBUTING.md#ai-assisted-contributions -->

## Target branch

<!-- `main`, always, unless this is a cherry-pick to a release branch. A cherry-pick targets `release/X.Y` and only after the fix landed on `main` first: link that commit here and use `git cherry-pick -x`. -->

## Checklist

The flow is in [CONTRIBUTING.md](../blob/main/CONTRIBUTING.md#pull-request-flow-and-who-merges).

- [ ] Branch is named `type/short-desc` and targets `main`
- [ ] Every commit is signed off (`git commit -s`), see [DCO](../blob/main/CONTRIBUTING.md#developer-certificate-of-origin-dco)
- [ ] `bun run check` passes
- [ ] **Tests first:** for a bug fix, the regression test was written and seen failing before the fix (see [AGENTS.md](../blob/main/AGENTS.md))
- [ ] Tests cover the behavior (boundaries, failures, state transitions)
- [ ] `CHANGELOG.md` has a line under `## [Unreleased]` for user-visible changes
- [ ] Docs updated in `docs/` when a flag, key, config field or behavior changed
- [ ] Screenshots or a recording are attached for UI changes
- [ ] **Breaking change?** (`atlas.yaml`, CLI flags or exit codes, MCP tool names or arguments, settings files): the changelog entry is under **Changed** or **Removed** and says what to do instead, and the [deprecation policy](../blob/main/docs/deprecation.md) is followed
- [ ] No secrets, private URLs or personal data in the diff
- [ ] I understand every line of this change; AI assistance, if any, is disclosed above
- [ ] External contributors: I will not merge my own PR; it needs 2 approvals from people other than me, and a maintainer merges it (maintainers may merge their own, see the flow)
