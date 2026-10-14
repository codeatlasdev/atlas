# Contributing to Atlas

Thanks for wanting to help. This is the short version; the full guide, with the code layout, test conventions and desktop debugging tips, is [docs/contributing.md](docs/contributing.md).

By taking part you agree to the [Code of Conduct](CODE_OF_CONDUCT.md). Atlas is dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option, and so are your contributions (see [License of contributions](#license-of-contributions)).

## Report a bug or ask for a feature

Open an [issue](https://github.com/codeatlasdev/atlas/issues/new/choose). For a bug include `atlas version`, the [release channel](docs/versioning.md) you are on, your OS, what you ran, what you expected, and the relevant lines of `.logs/<service>.log` (redact secrets). A minimal `atlas.yaml` that reproduces it is the fastest way to get it fixed. Check that it still happens on the newest build of your channel first. There is a separate form for update, installer and channel problems. How issues are labeled and triaged, and what to expect, is in [docs/labels.md](docs/labels.md).

**Security problems do not go in public issues.** Follow [SECURITY.md](SECURITY.md).

## Send a change

For anything bigger than a small fix, open an issue first so we can agree on the approach.

```bash
git clone https://github.com/codeatlasdev/atlas.git
cd atlas
scripts/setup.sh          # rustup, bun, frontend deps, cargo-tauri, cargo-deny, cargo-nextest
bun run check             # the only gate: fmt, clippy -D warnings, tests, cargo deny, skills, desktop checks
```

1. Branch from `main` (see [the flow](#pull-request-flow-and-who-merges)).
2. Change the code, add or update tests, and update the docs and `CHANGELOG.md` (`## [Unreleased]`) in the same pull request. **Tests come first**: for a bug fix, write the regression test, watch it fail for the right reason, then fix it (see [AGENTS.md](AGENTS.md)). A change to `atlas.yaml`, the CLI, the MCP tools or the settings files needs a **Changed** or **Removed** changelog entry that says what to do instead, and follows the [deprecation policy](docs/deprecation.md).
3. Run `bun run check` on its own and read its exit code.
4. Open a pull request (a draft until it is ready) and fill in the template.

Rules of thumb:

- Prefer behavior tests (boundaries, failures, state transitions) over getters.
- Fix warnings; do not add global `allow`s. Workspace lints apply to every crate.
- Keep duplication at or below 1.5% (`bun run dup`).
- Commit subjects are short and imperative with a conventional prefix (`feat:`, `fix:`, `perf:`, `refactor:`, `docs:`, `test:`, `build:`, `ci:`, `chore:`, `revert:`, `style:`), scoped when useful (`fix(desktop): ...`), `!` for a breaking change (`feat!: ...`). The pull request **title** must have this form: it becomes the commit subject on squash, and the `Conventional title` check (`scripts/check-pr-title.sh`) fails otherwise.
- Do not add a dependency without saying why; `cargo deny check` must pass.
- Respect the runtime safety rules in [AGENTS.md](AGENTS.md) (exact-PID orphan cleanup, never `docker compose down`, and so on). They come from real incidents.

### Pull request flow and who merges

This is the one flow. [GOVERNANCE.md](GOVERNANCE.md), [AGENTS.md](AGENTS.md), `docs/contributing.md`, the pull request template and the `atlas-workflow` skill link here instead of restating it.

1. **Branch.** Fork (outside contributors) or branch (people with write access) from `main`, named `type/short-desc`, where `type` is a conventional prefix: `feat/`, `fix/`, `docs/`, `chore/`, `test/`, `refactor/` (for example `fix/empty-ready-log`).
2. **Open a pull request to `main`**, as a **draft** until it is ready for review. Every commit carries a [DCO](#developer-certificate-of-origin-dco) sign-off. Fill in the template.
3. **CI is green.** Three checks are required: `CI` (the aggregate of every job in `ci.yml`), `Signed-off-by` and `Conventional title` (`pr.yml`). The branch must be up to date with `main`.
4. **Approvals depend on who wrote it.**
   - **External contributors** (anyone who is not a maintainer, including people with write access such as Richards0nd): **two approvals** from humans other than the author. New pushes dismiss earlier approvals, and the most recent push has to be approved by someone other than whoever pushed it. Every review thread is resolved before merging.
   - **Maintainers** (the GitHub team `atlas-maintainers`: [@mvilacad](https://github.com/mvilacad), the owner, and [@luancoleto](https://github.com/luancoleto)) may merge their own pull requests **without approvals**, through the ruleset bypass, once CI is green. Reviews are welcome and encouraged for anything non-trivial, but not required.
5. **Who merges.** An external contributor does not merge their own pull request: a maintainer merges it, **squash by default** (the pull request title becomes the commit subject, so it follows the conventional-commit form). Maintainers merge their own and other people's pull requests.
6. **Releases are cut only by maintainers** (`atlas-maintainers`), from `main`, with `bun run release alpha` (see [docs/release.md](docs/release.md)). Nobody else tags or publishes.

These rules are enforced by a GitHub ruleset on `main` and `release/*` (pull request required, 2 approving reviews, stale reviews dismissed, last push approved by someone else, threads resolved, the required checks above, branch up to date, no deletion, no force-push), and `v*` tags are immutable. **Only the `atlas-maintainers` team bypasses it** (the ruleset bypass list is that team). That bypass is how maintainers merge without approvals and how `bun run release` pushes the version commit and the tag directly. Required checks still apply to the normal merge path; a maintainer who bypasses them for an **emergency hotfix** (a broken `main`, a security fix that cannot wait) leaves a note on the commit or the linked issue saying what was bypassed and why.

**Agents** never push to `main`, merge a pull request or use the bypass on their own initiative. An agent run by a maintainer may merge, or use the bypass, when that maintainer says so for that change. An agent working for an external contributor opens a branch and a pull request like that contributor would, and stops there; it is not one of the two approvals. Why the approval rule exists is in [GOVERNANCE.md](GOVERNANCE.md#why-external-contributors-do-not-merge-their-own-pull-requests).

CI runs on every pull request (fmt, clippy `-D warnings`, tests, `cargo deny`, the desktop frontend checks, the site type-check and build, and the duplication gate) on macOS and Linux; the same gate is `bun run check` locally, plus `bun run site:check` and `bun run dup`. What Stable, Beta and Alpha mean is in [docs/versioning.md](docs/versioning.md).

### Release branches

Pull requests always target `main`. When a release is being soaked it lives on a short-lived `release/X.Y` branch (see [docs/versioning.md](docs/versioning.md#how-a-release-moves-between-channels)). Fixes land on `main` first and are then cherry-picked onto the release branch with `git cherry-pick -x <sha>`, which records the original commit in the message. Only a maintainer opens a pull request against `release/X.Y`, it goes through the same flow, and it links the commit that already landed on `main`.

### Commands

Everything is a root `package.json` script, so `bun run <name>` from the repo root is the one entry point (`scripts/setup.sh` first, once).

| Command | What it does |
|---|---|
| `bun run setup` | the one preflight: rustup, bun, frontend deps, `cargo-tauri`, `cargo-deny`, `cargo-nextest` (idempotent) |
| `bun run check` | **the gate**: fmt, clippy `-D warnings`, workspace tests (nextest + doctests), `cargo deny`, skills, desktop checks |
| `bun run sweep` | free `target/build` (intermediate artifacts; the next build recreates them). See `docs/build-perf.md` |
| `bun run fmt` | `cargo fmt` and prettier for the desktop frontend |
| `bun run dup` | jscpd duplication over Rust, TypeScript, Svelte and Astro; fails above 1.5% (config: `.jscpd.json`) |
| `bun run desktop:dev` / `desktop:build` / `desktop:check` / `desktop:link` | run the app with hot reload, build the bundle, check the frontend, link the debug bundle |
| `bun run site:dev` / `site:build` / `site:check` / `site:deploy` | the website: dev server, static build, `astro check` plus script/function type-checks, Cloudflare deploy |
| `bun run studio` / `studio:check` | the film studio CLI, and its checks (`studio/` is not part of `bun run check`) |
| `bun run icons` | regenerate the app icons from `assets/*.svg` |
| `bun run deps:update` | update JS dependencies everywhere (see below) |
| `bun run release <alpha\|beta\|rc\|stable\|patch>` / `release:dry` / `release:act` | owner only: cut a release (see [docs/release.md](docs/release.md)) |
| `bun run ci:act` | run the macOS CI job locally through `act` |

### Updating JS dependencies

`bun run deps:update` runs `npm-check-updates -u` in the root, `apps/desktop`, `site` and `studio` (config: `.ncurc.json`) and then reinstalls. `typescript` is rejected on purpose: it stays on 6.x because TypeScript 7's native compiler drops the JS API that `svelte-kit sync`, `svelte-check` and `astro check` rely on. Move it by hand once those tools support 7. `studio` pins `hyperframes` exactly; after an update run `bun studio check` and keep the old pin if it fails.

### Platforms and toolchain

- The prebuilt product targets macOS on Apple silicon. Atlas Desktop and the GPU terminal (`atlas-desktop`, `atlas-term`) are macOS-only. Everything else builds on Linux: `cargo build`/`cargo test` use the workspace `default-members` (all crates except those two), and CI runs that set on Ubuntu. Use `--workspace` on a Mac for the full gate.
- **MSRV policy:** the Rust version is pinned in `rust-toolchain.toml` and `rust-version` in `Cargo.toml` is the same version (currently 1.99). We only support building with that toolchain or newer stable; bumping it is a normal change, called out in the changelog.

## Developer Certificate of Origin (DCO)

Every commit must carry a `Signed-off-by` trailer. It is your statement, under the [Developer Certificate of Origin 1.1](https://developercertificate.org/), that you wrote the change or have the right to submit it under this project's license. There is no CLA and you keep your copyright.

```bash
git commit -s -m "fix: handle empty ready_log"     # adds: Signed-off-by: Your Name <you@example.com>
git rebase --signoff main                          # sign every commit of a branch you already made
git commit --amend -s --no-edit                    # sign the last one
```

The name and email must match your git identity (a real name, not a handle you want to hide). The `DCO` check on pull requests fails when a commit lacks the trailer.

## License of contributions

Unless you say otherwise, any contribution you submit is dual-licensed as MIT or Apache-2.0, at the user's option, without additional terms (the same as the project).

## AI-assisted contributions

AI tools are fine to use; the rules protect reviewers' time and the codebase.

- **A human is accountable.** You must understand every line you submit, be able to explain it in review, and have run it. "The model wrote it" is not an answer to a review comment.
- **No autonomous PRs.** Pull requests opened by an agent without a person driving, reading and standing behind the change are closed. Disclose substantial AI assistance in the pull request description (tool and what it did); you do not need to mark autocomplete.
- **Do not paste unverified output**: invented APIs, fabricated test results, or issue/PR text generated at volume. Low-effort, unreviewed AI submissions are closed without detailed review and repeat ones lead to a block.
- **Licensing and secrets.** Do not submit code you cannot license under the terms above (for example, output that reproduces copyleft code verbatim) and never put secrets or private data into a prompt or into the diff. The DCO sign-off still applies and is yours, never the tool's.
- **Open an issue first** for anything non-trivial, as for any contribution.

[AGENTS.md](AGENTS.md) holds the notes coding agents need for this repo: the gate, how to deploy a local build, how to debug the desktop app without the mouse, and the pitfalls. Agents working in your checkout should follow it, and they follow [the same flow](#pull-request-flow-and-who-merges) as you: a branch and a pull request, and no merge unless a maintainer is running the agent and says so.
