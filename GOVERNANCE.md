# Governance

Atlas is maintained by [CodeAtlas](https://github.com/codeatlasdev). This is a small project with a simple model; it will be revisited when there are more maintainers.

## Roles

| Role | Who | What they do |
|---|---|---|
| **Owner** | [@mvilacad](https://github.com/mvilacad) (member of `atlas-maintainers`) | Roadmap, final say on disputes. A maintainer in every other respect |
| **Maintainer** | The GitHub team `atlas-maintainers`: [@mvilacad](https://github.com/mvilacad) and [@luancoleto](https://github.com/luancoleto) | Review, triage issues, merge pull requests (squash by default) including their own, without approvals, through the ruleset bypass (the bypass list is this team). Cut releases with `bun run release`. Enforce the [Code of Conduct](CODE_OF_CONDUCT.md) |
| **Reviewer** | Contributors invited to review | Their approval counts toward the two required for an external contributor's pull request, and `CODEOWNERS` can route requests to them. They cannot merge |
| **Contributor** | Anyone else who sends an issue, a pull request or a review, including people with write access who are not in `atlas-maintainers` | Follows [CONTRIBUTING.md](CONTRIBUTING.md). Their pull requests need two approvals and a maintainer merges them |

The one flow, step by step, lives in [CONTRIBUTING.md](CONTRIBUTING.md#pull-request-flow-and-who-merges); this page does not repeat it. `CODEOWNERS` routes review requests to `@codeatlasdev/atlas-maintainers` on everything and on the sensitive paths (`.github/`, `scripts/release*`, `crates/atlas-host`, `AGENTS.md`).

## Why external contributors do not merge their own pull requests

Pull request #3 (Linux support) was merged by its author with no reviews. CI was green and nothing was lost, but the repository's settings allowed it, and a process that depends on everyone remembering to ask is not a process. So the rule for external contributors is enforced by GitHub instead of by memory: a pull request needs two approvals from people other than the author, every required check green, resolved review threads and an up-to-date branch, and a maintainer merges it. The maintainers (`atlas-maintainers`) are the people accountable for the repository and are on the ruleset's bypass list, so they can merge their own changes without approvals; that trust is a property of the role, not of any person, and anyone who joins the team gets it (see [CONTRIBUTING.md](CONTRIBUTING.md#pull-request-flow-and-who-merges)).

## How decisions are made

- Small fixes: a green CI and, for external contributors, two human approvals, as in the flow above.
- Features, config-schema changes, CLI flag changes, MCP tool changes and anything touching the runtime safety rules in [AGENTS.md](AGENTS.md): discuss in an issue first; the owner decides when there is no consensus.
- Public surfaces (CLI flags, `atlas.yaml`, MCP tools) follow [SemVer](https://semver.org); breaking changes are called out in [CHANGELOG.md](CHANGELOG.md). The release channels (Stable, Beta, Alpha) and what each promises are in [docs/versioning.md](docs/versioning.md), and how something is deprecated and removed is in [docs/deprecation.md](docs/deprecation.md). Releases are cut by maintainers.
- Issues are triaged by a maintainer using the labels and targets in [docs/labels.md](docs/labels.md).
- Pull requests opened by AI agents without a person driving them are closed; see [AI-assisted contributions](CONTRIBUTING.md#ai-assisted-contributions).

## Becoming a reviewer or maintainer

- **Reviewer.** A contributor who has landed a few non-trivial changes and leaves useful, correct reviews can be asked by a maintainer, or can ask, to be added as a reviewer. A maintainer confirms. Reviewing a change you do not understand well is a reason to hand it to someone else, not to approve it.
- **Maintainer.** Reviewers who have landed several substantial changes, review others' work well and follow the Code of Conduct can be invited by the owner and the existing maintainers, which adds them to the `atlas-maintainers` team (and so to the bypass list). Maintainers who are inactive for a long time may be moved to emeritus.
- Access is granted and removed by the owner in the GitHub organization. Write access alone does not make someone a maintainer or give them the bypass.

## Licensing and contributions

Atlas is dual-licensed under MIT or Apache-2.0. Contributions are accepted under the [Developer Certificate of Origin](https://developercertificate.org/) (`Signed-off-by`), with no CLA; see [CONTRIBUTING.md](CONTRIBUTING.md#developer-certificate-of-origin-dco). The name and logo are CodeAtlas's: forks are welcome under the licenses, but please rename them so they are not confused with this project.
