# Roadmap

A short, honest list. It is a direction, not a promise; dates are not committed. Discuss or vote with a reaction in [Discussions](https://github.com/codeatlasdev/atlas/discussions).

## Now

- **Open the project up**: public repository, CI on every pull request, DCO, contributor docs.
- **Release trust**: Developer ID signing and notarization for `Atlas.app` and the CLI (today binaries are ad-hoc signed), build provenance attestations, an independent signature for updates.
- **Stability of the public surfaces**: write down what is stable (`atlas.yaml`, CLI flags, MCP tools) and a breaking-change policy.

## Next

- **Linux**: the CLI, TUI, runner and MCP server build on Linux and run in CI; prebuilt Linux binaries and a tested install path are not there yet. Atlas Desktop stays macOS-only for now.
- **Homebrew** formula and cask next to the install script.
- **Changelog automation** from conventional commits.

## Later / ideas

- A non-TUI `atlas-runtime` crate split out of `atlas-tui`, so the desktop does not depend on the terminal UI crate.
- One shared text/log module (ANSI stripping, level detection, link finding) between the TUI and the desktop, instead of parallel Rust and TypeScript copies. Today both sides are checked against one shared fixture (`testdata/log-levels.json`) so they cannot silently drift.
- Narrower `pub` surface per crate (facade modules), consistent test layout.
- SBOM per release, coverage tooling.

## Structure follow-ups (mechanical)

Splitting the largest files, behavior unchanged and test counts kept: `tui/app/logview.rs`, `runner.rs`, `tui/app/update.rs`, `runtime/logs.rs`, `atlas-host/src/pty.rs`, `atlas-desktop/src/term.rs`, `atlas-term/src/vt.rs`, `Logs.svelte`.

## Not planned

Hosted services, accounts or telemetry. Atlas stays local-first (see [PRIVACY.md](PRIVACY.md)).
