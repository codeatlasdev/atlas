---
title: "Installation"
description: "Install Atlas from a release, update it, or build it from source."
---

# Installation

## Requirements

| | |
|---|---|
| OS | macOS 13+ on Apple silicon for the prebuilt CLI and Atlas Desktop. Other platforms: [build from source](#build-from-source). |
| `docker` | Optional. Needed only when your `atlas.yaml` has an `infra:` block (OrbStack and Docker Desktop both work). |
| `git`, `gh` | Optional. Atlas Desktop uses `git` for worktrees and the `gh` CLI for pull requests and checks. |

## Install a release

The installer downloads the latest release binary from GitHub, verifies its SHA-256 and runs the binary's own `atlas install`: it copies `atlas` to `~/.atlas/bin/atlas`, adds that directory to `PATH` for every detected shell (fish, zsh, bash) and installs the bundled agent skills. It needs `curl`, `tar` and `shasum`, never `sudo`, and exits with a message on anything other than macOS on Apple silicon.

```bash
curl -fsSL https://atlas.codeatlas.com.br/install.sh | sh
```

`scripts/bootstrap.sh` in the repository goes further: it also runs `atlas install` (PATH entries and bundled skills) and offers to set up any agent it finds (omp, Claude Code, Codex). It also needs only `curl`, no `gh`:

```bash
curl -fsSL https://raw.githubusercontent.com/codeatlasdev/atlas/main/scripts/bootstrap.sh | bash
```

Then check it worked (open a new shell if you changed your `PATH`):

```bash
atlas version
```

### Manual install

Download `atlas-<version>-aarch64-apple-darwin.tar.gz` and its `.sha256` from the [releases page](https://github.com/codeatlasdev/atlas/releases), then:

```bash
shasum -a 256 -c atlas-*-aarch64-apple-darwin.tar.gz.sha256
tar xzf atlas-*-aarch64-apple-darwin.tar.gz
./atlas install
```

`atlas install` copies the binary to `~/.atlas/bin/atlas` (override with `--dir`), adds that directory to `PATH` for every detected shell (fish: `~/.config/fish/conf.d/atlas.fish`; zsh: `~/.zshenv`; bash: the first of `~/.bash_profile`, `~/.bash_login`, `~/.bashrc` that exists) and installs the AI skills bundled in the binary into `~/.agents/skills/`.

The release binary is ad-hoc signed. If Gatekeeper complains about a binary you downloaded in a browser, run `xattr -d com.apple.quarantine ./atlas` first. The installer script does not need this.

## Install Atlas Desktop

```bash
atlas desktop
```

The first run downloads the `Atlas.app` that matches your CLI version, verifies its checksum, installs it into `/Applications` and opens it. Later runs just open it. You can also download `Atlas-<version>-aarch64-apple-darwin.dmg` from the release page by hand. See the [Desktop app](desktop.md) for the details.

## Update

```bash
atlas update           # download the latest release, replace the binary, refresh skills and agent setup
atlas update --check   # only report whether a newer version exists
```

`atlas update` also brings an installed Atlas Desktop to the same version. Atlas checks for a new release in the background at most every 6 hours and prints a one-line notice after commands when one is available; `atlas dev` offers to update before the dashboard opens. Set `ATLAS_NO_UPDATE_PROMPT=1` to silence the prompt. It refuses to run while you are using a [linked dev build](#link-a-dev-build).

## Build from source

You need:

- the Rust toolchain pinned in [`rust-toolchain.toml`](https://github.com/codeatlasdev/atlas/blob/main/rust-toolchain.toml) (`rustup` installs it automatically on the first `cargo` command);
- [`bun`](https://bun.sh), for the Atlas Desktop frontend and the repo scripts;
- for the desktop app, Xcode command line tools (`xcode-select --install`).

```bash
git clone https://github.com/codeatlasdev/atlas.git
cd atlas
scripts/setup.sh                 # checks tools, installs frontend deps, cargo-tauri and cargo-deny
```

### The CLI

```bash
cargo build --release -p atlas-cli
codesign -f -s - target/release/atlas      # macOS: ad-hoc sign, or Gatekeeper kills the binary
target/release/atlas install               # copies it to ~/.atlas/bin and wires PATH
```

Build `atlas-cli`, not `atlas-tui` alone: `atlas-tui` is a library, and `atlas-cli` is the crate that produces the `atlas` binary.

### Atlas Desktop

```bash
bun run desktop:build            # release bundle + dmg in target/release/bundle/
atlas desktop --from target/release/bundle/macos/Atlas.app
```

For day-to-day work on the app use the hot-reload loop instead: `bun run desktop:dev`. More in [Contributing](contributing.md#develop-the-desktop-app).

### Link a dev build

To make the installed `atlas` (and `/Applications/Atlas.app`) run your checkout:

```bash
atlas link                       # build + symlink the debug CLI
atlas link --desktop             # also build a debug Atlas.app and link it
atlas unlink                     # back to the release build
```

While linked, `atlas update` refuses to overwrite your build.

## Uninstall

```bash
atlas setup omp --uninstall --apply   # and claude-code / codex, if you set them up
atlas uninstall                       # removes ~/.atlas and the PATH lines it added (asks first; -y skips)
```

`atlas uninstall` deletes the whole `~/.atlas/` directory: the binary, desktop settings and projects, worktrees under `~/.atlas/worktrees`, terminal scrollback and the omp plugin. It lists what it will remove and asks before doing anything. Delete `/Applications/Atlas.app` yourself to remove the desktop app. Skills copied to `~/.agents/skills/` and `~/.claude/skills/` are left in place; remove them with your agent's tooling if you want them gone.
