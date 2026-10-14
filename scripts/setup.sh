#!/usr/bin/env bash
# One-stop dev preflight: verifies tools, installs frontend deps and cargo
# subcommands when missing. Idempotent; used by desktop:dev/build and release.sh.
# Needs only rustup/cargo and bun (the GitHub CLI is only used by maintainers to publish).
#
# Usage: scripts/setup.sh [--quiet | --tauri-version]
#   --tauri-version  print the pinned tauri-cli version (also used by .github/workflows/release.yml)
set -euo pipefail
cd "$(dirname "$0")/.."

# The one place the tauri-cli version is pinned.
TAURI_CLI_VERSION=2.11.4
[[ "${1:-}" == "--tauri-version" ]] && { echo "$TAURI_CLI_VERSION"; exit 0; }

quiet=0
[[ "${1:-}" == "--quiet" ]] && quiet=1
say() { [[ $quiet == 1 ]] || echo "$*"; }
die() { echo "✗ $*" >&2; exit 1; }

hint() {
  case "$1" in
    rustup|cargo) echo "install Rust: https://rustup.rs" ;;
    bun)          echo "install Bun: https://bun.sh" ;;
  esac
}
for tool in rustup cargo bun; do
  command -v "$tool" >/dev/null || die "$tool not found ($(hint "$tool"))"
done

# Linux: Atlas Desktop (Tauri) links WebKitGTK; say which packages provide it instead of failing deep in cargo.
if [[ "$(uname -s)" == Linux ]] && ! pkg-config --exists webkit2gtk-4.1 2>/dev/null; then
  die "WebKitGTK 4.1 development files not found (needed by Atlas Desktop). Install them:
  Arch:          sudo pacman -S --needed webkit2gtk-4.1 base-devel curl wget file openssl appmenu-gtk-module libappindicator-gtk3 librsvg xdotool
  Debian/Ubuntu: sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
  Fedora:        sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file libappindicator-gtk3-devel librsvg2-devel libxdo-devel
  (the CLI alone builds without them: cargo build --release -p atlas-cli)"
fi

front=apps/desktop
if [[ ! -d $front/node_modules || $front/bun.lock -nt $front/node_modules ]]; then
  say "▶ bun install ($front)"
  bun install --frozen-lockfile --cwd "$front" >/dev/null
  touch "$front/node_modules"
fi

if ! command -v cargo-tauri >/dev/null; then
  say "▶ cargo install tauri-cli"
  cargo install tauri-cli --version "$TAURI_CLI_VERSION" --locked
fi
if ! command -v cargo-deny >/dev/null; then
  say "▶ cargo install cargo-deny"
  cargo install cargo-deny --locked
fi
if ! command -v cargo-nextest >/dev/null; then
  say "▶ cargo install cargo-nextest"
  cargo install cargo-nextest --locked
fi

say "✓ toolchain ready ($(cargo tauri --version))"

# Advice only: target/ has no GC (docs/build-perf.md); never delete here.
target="$(dirname "$0")/../target"
if [[ -d $target ]] && (( $(du -sk "$target" | cut -f1) > 20971520 )); then
  say "target/ is $(du -sh "$target" | cut -f1); run: bun run sweep"
fi
