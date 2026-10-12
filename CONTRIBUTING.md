# Contributing to Atlas

Thanks for wanting to help. This is the short version; the full guide, with the code layout, test conventions and desktop debugging tips, is [docs/contributing.md](docs/contributing.md).

By taking part you agree to the [Code of Conduct](CODE_OF_CONDUCT.md). Atlas is dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option, and so are your contributions (see [License of contributions](#license-of-contributions)).

## Report a bug or ask for a feature

Open an [issue](https://github.com/codeatlasdev/atlas/issues/new/choose). For a bug include `atlas version`, your OS, what you ran, what you expected, and the relevant lines of `.logs/<service>.log` (redact secrets). A minimal `atlas.yaml` that reproduces it is the fastest way to get it fixed.

**Security problems do not go in public issues.** Follow [SECURITY.md](SECURITY.md).

## Send a change

For anything bigger than a small fix, open an issue first so we can agree on the approach.

```bash
git clone https://github.com/codeatlasdev/atlas.git
cd atlas
scripts/setup.sh          # rustup, bun, frontend deps, cargo-tauri, cargo-deny
bun run check             # the only gate: fmt, clippy -D warnings, tests, cargo deny, skills, desktop checks
```

1. Branch from `main`.
2. Change the code, add or update tests, and update the docs and `CHANGELOG.md` (`## [Unreleased]`) in the same pull request.
3. Run `bun run check` on its own and read its exit code.
4. Open a pull request and fill in the template.

Rules of thumb:

- Prefer behavior tests (boundaries, failures, state transitions) over getters.
- Fix warnings; do not add global `allow`s. Workspace lints apply to every crate.
- Keep duplication at or below 1.5% (`bun run dup`).
- Commit subjects are short and imperative with a conventional prefix (`feat:`, `fix:`, `docs:`, `chore:`).
- Do not add a dependency without saying why; `cargo deny check` must pass.
- Respect the runtime safety rules in [AGENTS.md](AGENTS.md) (exact-PID orphan cleanup, never `docker compose down`, and so on). They come from real incidents.

CI runs on every pull request (fmt, clippy `-D warnings`, tests, `cargo deny`, the desktop frontend checks and the site build) on macOS and Linux; the same gate is `bun run check` locally. Releases are cut by maintainers (see [docs/release.md](docs/release.md)).

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

[AGENTS.md](AGENTS.md) holds the notes coding agents need for this repo: the gate, how to deploy a local build, how to debug the desktop app without the mouse, and the pitfalls. Agents working in your checkout should follow it.
