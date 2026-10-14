---
title: "Installation"
description: "Install Atlas from a release, update it, or build it from source."
---

# Installation

## Requirements

| | |
|---|---|
| OS | macOS 13+ on Apple silicon, Linux x86_64 with glibc 2.35+ (Arch, Manjaro, Fedora, Debian 12, Ubuntu 22.04 and newer), or Windows 10 (1809+) / 11 on x64, for the prebuilt CLI and Atlas Desktop. Intel Macs and Linux on ARM are coming; meanwhile [build from source](#build-from-source). |
| `docker` | Optional. Needed only when your `atlas.yaml` has an `infra:` block (OrbStack and Docker Desktop both work). |
| `git`, `gh` | Optional. Atlas Desktop uses `git` for worktrees and the `gh` CLI for pull requests and checks. |

## Install a release

The installer downloads the latest stable release binary from GitHub, verifies its SHA-256 and runs the binary's own `atlas install`: it copies `atlas` to `~/.atlas/bin/atlas`, adds that directory to `PATH` for every detected shell (fish, zsh, bash) and installs the bundled agent skills. It needs `curl`, `tar` and `sha256sum` (or `shasum`), never `sudo`, and exits with a message on anything other than macOS on Apple silicon or Linux x86_64.

```bash
curl -fsSL https://atlas.codeatlas.com.br/install.sh | sh
```

On Windows, in PowerShell (see [Windows](#windows)):

```powershell
irm https://atlas.codeatlas.com.br/install.ps1 | iex
```

`scripts/bootstrap.sh` in the repository goes further: it also runs `atlas install` (PATH entries and bundled skills) and offers to set up any agent it finds (omp, Claude Code, Codex). It also needs only `curl`, no `gh`:

```bash
curl -fsSL https://raw.githubusercontent.com/codeatlasdev/atlas/main/scripts/bootstrap.sh | bash
```

### Install a beta or alpha

`ATLAS_CHANNEL=stable|beta|alpha` picks the [release channel](versioning.md#channels). Without it you get the **default channel, which is Alpha while Atlas is in alpha** (the newest build from `main`; Stable once a real Stable exists), so the plain one-liner above already installs the newest alpha. Both installers accept it:

```bash
curl -fsSL https://atlas.codeatlas.com.br/install.sh | ATLAS_CHANNEL=stable sh   # only soaked builds
curl -fsSL https://raw.githubusercontent.com/codeatlasdev/atlas/main/scripts/bootstrap.sh | ATLAS_CHANNEL=beta bash
```

`install.sh` installs the latest Stable and then runs `atlas update` (the default channel, nothing saved) or `atlas update --channel <name>` (an explicit choice, saved), so the newer build comes through the same verified update path. `bootstrap.sh` picks the right release itself.

Beta installs the newest beta or release candidate, Alpha the newest alpha, beta or release candidate; both fall back to the latest Stable when that is newer (or when nothing else is published), and Stable installs releases only. The installer reads only the public `releases.atom` feed (older pages with `?after=` when needed) and the `releases/latest` redirect (no GitHub API, so no rate limit) and picks the highest SemVer precedence. Stable is the newest release without a prerelease label, never the redirect alone: while Alpha is the default, GitHub's "Latest" is the newest alpha. An explicit `ATLAS_CHANNEL` is saved to `~/.atlas/update-channel`, so `atlas update` keeps following it; with none, nothing is saved and `atlas update` follows the default. What each channel promises: [Versioning and release channels](versioning.md).

Then check it worked (open a new shell if you changed your `PATH`):

```bash
atlas version
```

### Manual install

Download `atlas-<version>-<platform>.tar.gz` and its `.sha256` from the [releases page](https://github.com/codeatlasdev/atlas/releases) (`aarch64-apple-darwin` for macOS, `x86_64-unknown-linux-gnu` for Linux), then:

```bash
shasum -a 256 -c atlas-*.tar.gz.sha256      # Linux: sha256sum -c atlas-*.tar.gz.sha256
tar xzf atlas-*.tar.gz
./atlas install
```

`atlas install` copies the binary to `~/.atlas/bin/atlas` (override with `--dir`), adds that directory to `PATH` for every detected shell (fish: `~/.config/fish/conf.d/atlas.fish`; zsh: `~/.zshenv`; bash: the first of `~/.bash_profile`, `~/.bash_login`, `~/.bashrc` that exists) and installs the AI skills bundled in the binary into `~/.agents/skills/`.

The release binary is ad-hoc signed. If Gatekeeper complains about a binary you downloaded in a browser, run `xattr -d com.apple.quarantine ./atlas` first. The installer script does not need this.

## Install Atlas Desktop

```bash
atlas desktop
```

The first run downloads the app that matches your CLI version, verifies its checksum, installs it and opens it. Later runs just open it.

- **macOS:** `Atlas.app` goes into `/Applications`. You can also download `Atlas-<version>-aarch64-apple-darwin.dmg` from the release page by hand.
- **Linux:** the AppImage goes to `~/.local/bin/atlas-desktop`, with an "Atlas" entry in your application menu. `atlas update` keeps it on the CLI's version.
- **Windows:** the release's per-user installer runs silently into `%LOCALAPPDATA%\Atlas` (no admin rights), with a Start menu entry and an uninstaller in Settings › Apps. `atlas update` keeps it on the CLI's version.

See the [Desktop app](desktop.md) for the details.

## Linux packages

Every release also ships Atlas as native packages for Linux x86_64. Each one installs **both** the `atlas` CLI and Atlas Desktop (`/usr/bin/atlas`, `/usr/bin/atlas-desktop`, a menu entry), and pulls in WebKitGTK through your package manager. Download them from the [releases page](https://github.com/codeatlasdev/atlas/releases/latest):

| Distribution | Asset | Install |
|---|---|---|
| Arch Linux, Manjaro, EndeavourOS | `codeatlas-atlas-bin-<version>-1-x86_64.pkg.tar.zst` | `sudo pacman -U codeatlas-atlas-bin-*.pkg.tar.zst` |
| Debian, Ubuntu, Mint, Pop!_OS | `atlas_<version>_amd64.deb` | `sudo apt install ./atlas_*_amd64.deb` |
| Fedora, RHEL, openSUSE | `atlas-<version>-1.x86_64.rpm` | `sudo dnf install ./atlas-*.x86_64.rpm` (openSUSE: `sudo zypper install`) |
| Any distribution | `Atlas-<version>-x86_64-unknown-linux-gnu.AppImage` | `chmod +x Atlas-*.AppImage && ./Atlas-*.AppImage` |

Arch users can also build the package themselves: the release attaches the `PKGBUILD` it was built from (`makepkg -si` next to it); it repackages the release's `.deb`.

A package manager owns what it installed: `atlas update` does not touch `/usr/bin/atlas` and tells you to update with your package manager instead, and `atlas desktop` opens the packaged app instead of downloading an AppImage. Pick one install method per machine (packages, or `install.sh` + `atlas desktop`); with both, the `atlas` that comes first on your `PATH` wins.

AppImages need FUSE 2 to start (`fuse2` on Arch, `libfuse2` on Debian/Ubuntu). Without it, run `./Atlas-*.AppImage --appimage-extract-and-run`.

On Linux, Atlas Desktop draws terminals with xterm.js (WebGL) instead of the macOS GPU terminal, and the capture/recording tools (`atlas desktop capture`, `record`) and the agent test instances (`desktop:agent`) are macOS only.

## Windows

Windows 10 (1809 or newer) and 11 on x64 get the same CLI, TUI, MCP server and Atlas Desktop (ARM64 machines run the x64 build emulated). Install from PowerShell, no admin rights:

```powershell
irm https://atlas.codeatlas.com.br/install.ps1 | iex
$env:ATLAS_CHANNEL = 'stable'; irm https://atlas.codeatlas.com.br/install.ps1 | iex   # or pick a channel
```

It verifies the release checksum, puts `atlas.exe` in `%USERPROFILE%\.atlas\bin`, adds that folder to your user `PATH` (open a new terminal afterwards) and installs the bundled agent skills. `atlas desktop` then installs and opens the app. You can also run `Atlas-<version>-x86_64-pc-windows-msvc-setup.exe` from the release page by hand: it installs the app and an `atlas.exe` next to it, which the app uses until you install the CLI.

What differs from macOS and Linux:

- **Commands in `atlas.yaml` run through `cmd.exe`** (`cmd /d /s /c`, as npm runs scripts), so `&&`, `>` and `%VAR%` work; POSIX-only syntax (`FOO=1 cmd`, `$VAR`, `;`) does not. Write commands both shells understand (`bun run dev`, `docker compose up -d`), or wrap one in `bash -c '…'` if you use Git Bash. `worktree.setup` and `archive` run the same way; `worktree.shared` links folders with a symlink (Developer Mode) or a junction.
- **Terminals** run on ConPTY and are drawn with xterm.js (the GPU terminal is macOS only). A new tab starts PowerShell 7 (`pwsh`) when installed, else Windows PowerShell, else `cmd.exe`; pick another shell or program per tab as usual. npm-installed agents (`claude`, `codex`) are found through their `.cmd` shims.
- **The terminal host is not upgraded live.** Windows cannot hand a running console to a new process, so after an update the host restarts by itself once no terminal is running, and otherwise asks first, like an old host on any platform.
- Atlas keeps paths with forward slashes internally (`C:/Users/me/app`, as git prints them).
- Not on Windows: capture and recording (`atlas desktop capture`, `record`), agent test instances (`desktop:agent`), `atlas link --desktop`, and the `local` target of `atlas deploy` (deploy targets are Linux servers reached over SSH, which works from Windows). `atlas link` makes a symlink, which needs Developer Mode.

## Update

```bash
atlas update                     # download the newest release of your channel, replace the binary, refresh skills and agent setup
atlas update --check             # only report whether a newer version exists
atlas update --check --json      # the same as one JSON object, for scripts
atlas update channel             # list the channels and what each offers now (numbered picker in a terminal)
atlas update channel beta        # follow beta from now on
atlas update --channel alpha     # save alpha and update from it
atlas update --downgrade         # install the older build your channel offers (asks first; see below)
```

Atlas follows one of three [release channels](versioning.md#channels): `stable`, `beta` or `alpha` (the default while Atlas is in alpha). The choice lives in `~/.atlas/update-channel` and is shared with Atlas Desktop; `ATLAS_UPDATE_CHANNEL=beta` overrides it for one process. Updates never go backwards on their own: if you switch to a more conservative channel than the build you run, `atlas update` tells you so and waits until the channel passes your version, unless you run `atlas update --downgrade` (older builds may not read data newer ones wrote; it asks for confirmation, `--yes` skips it).

`atlas update` also brings an installed Atlas Desktop to the same version. Atlas checks for a new release of your channel in the background at most every 6 hours and prints a one-line notice after commands when one is available; `atlas dev` offers to update before the dashboard opens. Set `ATLAS_NO_UPDATE_PROMPT=1` to silence the prompt. It refuses to run while you are using a [linked dev build](#link-a-dev-build). Full reference: [`atlas update`](cli.md#atlas-update).

## Build from source

You need:

- the Rust toolchain pinned in [`rust-toolchain.toml`](https://github.com/codeatlasdev/atlas/blob/main/rust-toolchain.toml) (`rustup` installs it automatically on the first `cargo` command);
- [`bun`](https://bun.sh), for the Atlas Desktop frontend and the repo scripts;
- for the desktop app, Xcode command line tools on macOS (`xcode-select --install`), or WebKitGTK 4.1 and its build dependencies on Linux:

  ```bash
  sudo pacman -S --needed webkit2gtk-4.1 base-devel curl wget file openssl appmenu-gtk-module libappindicator-gtk3 librsvg xdotool   # Arch
  sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev   # Debian/Ubuntu
  sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file libappindicator-gtk3-devel librsvg2-devel libxdo-devel   # Fedora
  ```

  The CLI alone needs none of them (`cargo build --release -p atlas-cli`).

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
bun run desktop:build            # macOS: .app + .dmg; Linux: .deb, .rpm and .AppImage; Windows: the NSIS setup .exe; all in target/release/bundle/
atlas desktop --from target/release/bundle/macos/Atlas.app                 # macOS
atlas desktop --from target/release/bundle/appimage/Atlas_*.AppImage       # Linux
atlas desktop --from target/release/bundle/nsis/Atlas_*-setup.exe          # Windows
```

On Linux, build the CLI first (`cargo build --release -p atlas-cli`): the `.deb` and `.rpm` include `target/release/atlas`. On Windows you need the Visual Studio Build Tools (C++ workload) and WebView2 (preinstalled on Windows 11); the release build also puts `atlas.exe` in the installer (`release.yml`, `build-windows`).

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

`atlas uninstall` deletes the whole `~/.atlas/` directory: the binary, desktop settings and projects, worktrees under `~/.atlas/worktrees`, terminal scrollback and the omp plugin. It lists what it will remove and asks before doing anything. Delete `/Applications/Atlas.app` yourself to remove the desktop app (Linux: `~/.local/bin/atlas-desktop`, `~/.local/share/applications/dev.codeatlas.atlas.desktop` and the icon under `~/.local/share/icons/hicolor/128x128/apps/`; a distro package: remove it with your package manager, e.g. `sudo pacman -R codeatlas-atlas-bin`, `sudo apt remove atlas`; Windows: uninstall **Atlas** in Settings › Apps, and `atlas uninstall` also takes `~\.atlas\bin` off your user `PATH`). Skills copied to `~/.agents/skills/` and `~/.claude/skills/` are left in place; remove them with your agent's tooling if you want them gone.
