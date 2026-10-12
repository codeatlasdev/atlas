# Governance

Atlas is maintained by [CodeAtlas](https://github.com/codeatlasdev). This is a small project with a simple model; it will be revisited when there are more maintainers.

## Maintainers

| Maintainer | Role |
|---|---|
| [@mvilacad](https://github.com/mvilacad) | Lead maintainer: roadmap, releases, final say on merges |

Maintainers review and merge pull requests, triage issues, cut releases (see [docs/release.md](docs/release.md)) and enforce the [Code of Conduct](CODE_OF_CONDUCT.md). `CODEOWNERS` routes review requests.

## How decisions are made

- Small fixes: one maintainer approval and a green `bun run check` / CI.
- Features, config-schema changes, CLI flag changes, MCP tool changes and anything touching the runtime safety rules in [AGENTS.md](AGENTS.md): discuss in an issue or Discussion first; the lead maintainer decides when there is no consensus.
- Public surfaces (CLI flags, `atlas.yaml`, MCP tools) follow [SemVer](https://semver.org); breaking changes are called out in [CHANGELOG.md](CHANGELOG.md).

## Becoming a maintainer

Contributors who have landed several substantial changes, review others' work well and follow the Code of Conduct can be invited by the existing maintainers. Maintainers who are inactive for a long time may be moved to emeritus.

## Licensing and contributions

Atlas is dual-licensed under MIT or Apache-2.0. Contributions are accepted under the [Developer Certificate of Origin](https://developercertificate.org/) (`Signed-off-by`), with no CLA; see [CONTRIBUTING.md](CONTRIBUTING.md#developer-certificate-of-origin-dco). The name and logo are CodeAtlas's: forks are welcome under the licenses, but please rename them so they are not confused with this project.
