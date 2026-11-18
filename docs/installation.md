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

The installer downloads the latest stable release binary from GitHub, verifies its SHA-256 and runs the binary's own `atlas install`: it copies `atlas` to `~/.atlas/bin/atlas`, adds that directory to `PATH` for every detected shell (fish, zsh, bash) and installs the bundled agent skills. When that stable has no build for your platform (no Stable has a Linux build yet: Linux ships from 1.5.0-alpha.7 on), it starts from the newest build of the default channel (Alpha) instead, and with `ATLAS_CHANNEL=stable` or `beta` it stops and says so. It needs `curl`, `tar` and `sha256sum` (or `shasum`), never `sudo`, and exits with a message on anything other than macOS on Apple silicon or Linux x86_64.

```bash
curl -fsSL https://atlas.codeatlas.com.br/install.sh | sh
```

On Linux with a graphical session (`WAYLAND_DISPLAY` or `DISPLAY` set), the script also installs Atlas Desktop (`atlas desktop --no-open`: the AppImage in `~/.local/bin/atlas-desktop` and a launcher entry), so the app is in your launcher when it finishes. Skip it with `ATLAS_NO_DESKTOP=1`; a failed download never fails the CLI install and says to run `atlas desktop` later. Servers (no display) and macOS get only the CLI, as before.

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

`install.sh` installs the latest Stable (or, when it has no build for your platform and you follow Alpha, the default channel's newest build) and then runs `atlas update` (the default channel, nothing saved) or `atlas update --channel <name>` (an explicit choice, saved), so the newer build comes through the same verified update path. `bootstrap.sh` picks the right release itself.

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

`atlas install` copies the binary to `~/.atlas/bin/atlas` (override with `--dir`), puts that directory on `PATH` for every shell it finds and installs the AI skills bundled in the binary into `~/.agents/skills/`. Where each shell gets it:

| Shell | File | Why there |
|---|---|---|
| bash (Linux) | `~/.bashrc`, created if missing | a terminal starts a non-login bash, which reads only `~/.bashrc`; distros' `~/.bash_profile` sources it |
| bash (macOS) | the first of `~/.bash_profile`, `~/.bash_login`, `~/.bashrc` that exists | Terminal starts login shells |
| zsh | `$ZDOTDIR/.zshenv`, else `~/.zshenv` | every zsh reads it |
| fish | `$XDG_CONFIG_HOME/fish/conf.d/atlas.fish` (`fish_add_path -g`) | global, not universal: nothing outlives the file |
| nushell 0.102+ | `$XDG_CONFIG_HOME/nushell/autoload/atlas.nu` | loaded on every interactive start |

When `~/.local/bin` is already on your `PATH` (most Linux desktops), install also links `~/.local/bin/atlas` to the binary: `atlas` then works in the shell you ran the installer from, in non-interactive `bash -c` (agents, scripts; `~/.bashrc` returns early for those) and in apps started from the launcher. A file of yours under any of these names is never overwritten; the installer says so and prints the line to add yourself. It ends with the command that puts `atlas` on `PATH` in the current shell, in your login shell's syntax. Do not run it with `sudo`: that installs for root.

Release binaries are signed with CODEATLAS LTDA's Developer ID (Team ID `58UYCA3ZYA`) and notarized by Apple, so Gatekeeper opens the downloaded `.dmg` and `Atlas.app` without the "cannot verify" warning, and the macOS firewall remembers `atlas` across updates instead of asking again. A bare binary cannot carry Apple's ticket, so Gatekeeper checks one you downloaded in a browser online the first time it runs; offline, run `xattr -d com.apple.quarantine ./atlas` first. A build from a fork, or a release made before signing was configured (see [Releasing](release.md#macos-signing-and-notarization)), is ad-hoc signed instead and always needs that command. The installer script does not need it.

## Install Atlas Desktop

```bash
atlas desktop
```

The first run downloads the app that matches your CLI version, verifies its checksum, installs it and opens it. Later runs just open it.

- **macOS:** `Atlas.app` goes into `/Applications`. You can also download `Atlas-<version>-aarch64-apple-darwin.dmg` from the release page by hand.
- **Linux:** on a distro with a release package, `atlas desktop` installs **that package** and not the AppImage: Arch and its derivatives (pacman package), Debian, Ubuntu and Mint (`.deb`), Fedora and RHEL (`.rpm`), openSUSE (`.rpm`), chosen from `ID` / `ID_LIKE` in `/etc/os-release` and only when that package manager is on the machine and the CPU is x86_64. It downloads the asset, verifies its checksum, shows the command (for example `sudo pacman -U …`), asks `[Y/n]` and runs it as root through `sudo` in a terminal or `pkexec` without one; `--yes` skips the question. The package installs `atlas-desktop` in `/usr/bin` with a menu entry, and your package manager updates it. Without a terminal and without `--yes`, when you answer no, when the install fails (a distro whose WebKitGTK is older than the package needs), on Manjaro (it holds libraries back from Arch's) and on every other distro, the AppImage goes to `~/.local/bin/atlas-desktop` instead, with an "Atlas" entry in your application menu, and `atlas update` keeps it on the CLI's version. `atlas desktop --appimage` forces the AppImage. `install.sh` gives `atlas desktop` your terminal (`/dev/tty`) so that it can ask. **aarch64 Linux has no Atlas Desktop build yet** (the release carries the CLI there, for crew servers): `atlas desktop` says so and points at the releases page instead of asking for a file that does not exist; `atlas desktop --from <AppImage>` installs a build of your own, and `atlas update` leaves it as it is.
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

On Arch the same package is on the AUR as `codeatlas-atlas-bin` (`yay -S codeatlas-atlas-bin`), published by the release workflow from that `PKGBUILD` (maintainer setup: [Release process](release.md#aur-codeatlas-atlas-bin)). Every package declares what Atlas Desktop needs at run time, and CI reads it from the built package before installing it: WebKitGTK 4.1, GTK 3, the GStreamer base and good plugins (sounds and media), the tray library, `git` and `ripgrep` (project search).

A package manager owns what it installed, and `atlas update` works through it (it never overwrites `/usr/bin/atlas` itself). It asks `pacman -Qo`, `dpkg -S` or `rpm -qf` who owns the binary (a `/usr/bin` binary no database claims uses the first manager it finds), downloads the matching release asset (`codeatlas-atlas-bin-*.pkg.tar.zst`, `atlas_*_amd64.deb`, `atlas-*.x86_64.rpm`) and its `.sha256`, **verifies it before anything asks for privileges**, shows the exact command, asks `[y/N]` (`--yes` skips it, and outside a terminal it is required) and runs it as root through `sudo` (or `pkexec` when there is no terminal; the in-app Update shows the command with a Copy button, and an "Install now" that uses `pkexec` after you confirm): `pacman -U`, `apt-get install`, `dnf install` or `zypper install`. It holds the update lock while it runs and afterwards refreshes skills and the OMP / Claude Code / Codex setups with the **installed** binary. `atlas update --check` always reports installed and latest. `atlas desktop` opens the packaged app instead of downloading an AppImage. Pick one install method per machine (packages, or `install.sh` + `atlas desktop`); with both, the `atlas` that comes first on your `PATH` wins.

AppImages need FUSE 2 to start (`fuse2` on Arch, `libfuse2` on Debian/Ubuntu). Without it, run `./Atlas-*.AppImage --appimage-extract-and-run`.

On Linux, Atlas Desktop draws terminals with xterm.js (WebGL) instead of the macOS GPU terminal, and the capture/recording tools (`atlas desktop capture`, `record`) and the agent test instances (`desktop:agent`) are macOS only.

## Windows

Windows 10 (1809 or newer) and 11 on x64 get the same CLI, TUI, MCP server and Atlas Desktop (ARM64 machines run the x64 build emulated). Install from PowerShell, no admin rights:

```powershell
irm https://atlas.codeatlas.com.br/install.ps1 | iex
$env:ATLAS_CHANNEL = 'stable'; irm https://atlas.codeatlas.com.br/install.ps1 | iex   # or pick a channel
```

The installer runs in the Windows PowerShell 5.1 that ships with Windows as well as in PowerShell 7, and it needs the `curl.exe` and `tar.exe` that Windows 10 1803 and newer include. If the newest Stable has no Windows build (1.4.0 has none), it says so and installs the newest alpha; `ATLAS_CHANNEL=stable` refuses instead and tells you how to take the alpha. `atlas` works in the same window right after it finishes; terminals that were already open need to be reopened. `irm | iex` does not run a script file, so the execution policy does not get in its way.

It verifies the release checksum, puts `atlas.exe` in `%USERPROFILE%\.atlas\bin`, adds that folder to your user `PATH` (open a new terminal afterwards) and installs the bundled agent skills. `atlas desktop` then installs and opens the app. You can also download `Atlas-Setup.exe` from the release page and double-click it: a small window installs Atlas Desktop and, if you have no `atlas` command yet, the `atlas` command too (`%USERPROFILE%\.atlas\bin`, added to your PATH), then opens Atlas. No admin rights, no wizard. The Windows files are **not code-signed yet** (signing through SignPath Foundation is pending its approval, see [Code signing policy](#code-signing-policy)), so SmartScreen may say "Windows protected your PC": click **More info**, then **Run anyway**. Once releases are signed, SmartScreen can still warn about a file few people have downloaded: reputation builds with downloads, and a signature does not switch the warning off by itself. Uninstall it from Settings › Apps (Windows 11: Settings › Apps › Installed apps) like any program: for an install made with `Atlas-Setup.exe` the same window asks **Keep your Atlas data?**. Choosing to delete it removes `%USERPROFILE%\.atlas` (settings, your project list, worktrees Atlas created with any uncommitted changes in them, and the `atlas` command) and asks a second time before it does; your project folders are never touched, and keeping is the default. An install made by `atlas desktop` or `atlas update` shows the plain Windows uninstaller instead. If something fails the window says what and offers **Copy details** (also Ctrl+C); the log is `%TEMP%\atlas-setup.log`. For scripts, `Atlas-Setup.exe --silent` runs without a window (and `--uninstall --silent` removes the app, keeping the data unless `--remove-data` is given); the plain NSIS installer `Atlas-<version>-x86_64-pc-windows-msvc-setup.exe` (`/S`) is what `atlas desktop` and `atlas update` run.

What differs from macOS and Linux:

- **Commands in `atlas.yaml` run through `cmd.exe`** (`cmd /d /s /c`, as npm runs scripts), so `&&`, `>` and `%VAR%` work; POSIX-only syntax (`FOO=1 cmd`, `$VAR`, `;`) does not. Write commands both shells understand (`bun run dev`, `docker compose up -d`), or wrap one in `bash -c '…'` if you use Git Bash. `worktree.setup` and `archive` run the same way; `worktree.shared` links folders with a symlink (Developer Mode) or a junction.
- **Terminals** run on ConPTY and are drawn with xterm.js (the GPU terminal is macOS only). A new tab starts PowerShell 7 (`pwsh`) when installed, else Windows PowerShell, else `cmd.exe`; pick another shell or program per tab as usual. npm-installed agents (`claude`, `codex`) are found through their `.cmd` shims.
- **The terminal host is not upgraded live.** Windows cannot hand a running console to a new process, so after an update the host restarts by itself once no terminal is running, and otherwise asks first, like an old host on any platform.
- Atlas keeps paths with forward slashes internally (`C:/Users/me/app`, as git prints them).
- Not on Windows: capture and recording (`atlas desktop capture`, `record`), agent test instances (`desktop:agent`), `atlas link --desktop`, and the `local` target of `atlas deploy` (deploy targets are Linux servers reached over SSH, which works from Windows). `atlas link` makes a symlink, which needs Developer Mode.

### Code signing policy

Free code signing provided by [SignPath.io](https://about.signpath.io), certificate by [SignPath Foundation](https://signpath.org).

- **Scope:** `atlas.exe`, `atlas-desktop.exe`, the NSIS installer and `Atlas-Setup.exe` of each release, built by GitHub Actions from the version tag and signed by SignPath after a maintainer approves the request. Signing is **pending the Foundation's approval**; until then these files are unsigned.
- **Team roles:** Committers and reviewers are the repository maintainers ([GOVERNANCE.md](https://github.com/codeatlasdev/atlas/blob/main/GOVERNANCE.md)). Approvers are the members of the GitHub team [`atlas-maintainers`](https://github.com/orgs/codeatlasdev/teams/atlas-maintainers).
- **Privacy:** Atlas does not transfer data to other systems unless you ask it to. No telemetry, analytics, crash reporting or accounts. The one request it makes on its own is an update check that reads the public releases of `codeatlasdev/atlas` on GitHub (Atlas Desktop has a *Check for updates automatically* switch in Settings); installing or updating downloads the release from GitHub. Nothing is sent to CodeAtlas or SignPath. The full list is in [PRIVACY.md](https://github.com/codeatlasdev/atlas/blob/main/PRIVACY.md).

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
atlas desktop --from target/release/bundle/nsis/Atlas_*-setup.exe          # Windows (the NSIS installer; Atlas-Setup.exe wraps it: docs/release.md)
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
atlas uninstall --dry-run             # list what would go; changes nothing
atlas uninstall                       # remove Atlas, keep your data (asks first; -y skips)
atlas uninstall --purge               # also delete ~/.atlas: settings, projects, worktrees
```

`atlas uninstall` removes what Atlas installed: the `atlas` command (`~/.atlas/bin`), the desktop app (macOS `Atlas.app`, or only the link when it is a dev build from `atlas link --desktop`; Linux the AppImage and its launcher entry and icon), the `# Atlas CLI` lines it added to your shell startup files (`~/.zshenv` or `$ZDOTDIR/.zshenv`, `~/.bashrc` or your bash login file, fish's `conf.d/atlas.fish` and the universal `fish_user_paths` entry versions up to 1.5.0-alpha.25 stored, nushell's `autoload/atlas.nu`; a line of your own that merely mentions `.atlas` is never touched), the `~/.local/bin/atlas` link when it points at this install, the update cache and lock, and the terminal host's sockets (it asks a running host to stop first). It **keeps** `~/.atlas/desktop` (settings and the project list), `~/.atlas/worktrees` (worktrees can hold uncommitted work), the update channel, themes and the omp plugin, and says so; `--purge` deletes all of `~/.atlas` too. It lists everything first and asks before changing anything (`--yes` skips the question; without a terminal and without `--yes` it refuses). A step that cannot be done (a file in use) is reported and the rest still runs; running it again finishes the job. A distro package is removed with its package manager (`sudo pacman -R codeatlas-atlas-bin`, `sudo apt remove atlas`); on Windows uninstall **Atlas** in Settings › Apps (it asks whether to keep your data), and `atlas uninstall` also takes `~\.atlas\bin` off your user `PATH`. Skills copied to `~/.agents/skills/` and `~/.claude/skills/` are left in place; remove them with your agent's tooling if you want them gone.
