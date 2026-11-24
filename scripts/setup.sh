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
TAURI_CLI_VERSION=2.12.6
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
  Debian/Ubuntu: sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libasound2-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
  Fedora:        sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file libappindicator-gtk3-devel librsvg2-devel libxdo-devel alsa-lib-devel
  (the CLI alone builds without them: cargo build --release -p atlas-cli)"
fi

# Agent guards (docs/contributing.md "Agent guards"): the git-level backstop. core.hooksPath is shared by every worktree of
# this clone and replaces .git/hooks, so a stale lefthook shim there (no lefthook config exists) stops warning on every commit.
# Never overwrites a hooks path somebody else chose.
if git rev-parse --git-dir >/dev/null 2>&1; then
  hooks_path="$(git config --get core.hooksPath || true)"
  if [[ -z $hooks_path ]]; then
    git config core.hooksPath scripts/git-hooks
    say "✓ git hooks: core.hooksPath = scripts/git-hooks (pre-push refuses main, release/*, force-push, deletes; override: docs/contributing.md)"
  elif [[ $hooks_path != scripts/git-hooks ]]; then
    say "ℹ core.hooksPath is '$hooks_path': the pre-push guard is not active. Call scripts/git-hooks/pre-push from it, or: git config core.hooksPath scripts/git-hooks"
  fi
  # In the primary checkout of a clone that has worktrees, agents belong in the worktrees: say how to enforce it.
  if [[ "$(git rev-parse --git-dir)" == "$(git rev-parse --git-common-dir)" ]] \
    && [[ "$(git worktree list --porcelain | grep -c '^worktree ')" -gt 1 ]] \
    && [[ "$(git config --get atlas.agentWrites || true)" != deny ]]; then
    say "ℹ This is the primary checkout of a clone with worktrees. To keep agents out of it: git config atlas.agentWrites deny"
  fi
fi

front=apps/desktop
if [[ ! -d $front/node_modules || $front/bun.lock -nt $front/node_modules ]]; then
  say "▶ bun install ($front)"
  bun install --frozen-lockfile --cwd "$front" >/dev/null
  touch "$front/node_modules"
fi

# Atlas Mobile (apps/mobile) needs Flutter; everything else works without it.
command -v flutter >/dev/null || say "ℹ Flutter not found: apps/mobile is skipped by the gate (https://docs.flutter.dev/get-started/install)"
# release:test (`bun run check`) runs the release manifest and label checks in Python 3.
scripts/py.sh -c "" 2>/dev/null || say "ℹ Python 3 not found: release:test needs it (https://www.python.org/downloads/; on Windows not the Microsoft Store alias)"
# services/push-relay needs Go; everything else works without it.
command -v go >/dev/null || say "ℹ Go not found: services/push-relay is skipped by the gate (https://go.dev/doc/install)"

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
# Workflow linters for `bun run check` (scripts/lint-workflows.sh). Homebrew on macOS (and Linuxbrew);
# otherwise zizmor through cargo and actionlint through Go.
if ! command -v zizmor >/dev/null; then
  say "▶ install zizmor"
  if command -v brew >/dev/null; then brew install zizmor; else cargo install zizmor --locked; fi
fi
if ! command -v actionlint >/dev/null; then
  say "▶ install actionlint"
  if command -v brew >/dev/null; then brew install actionlint
  elif command -v go >/dev/null; then go install github.com/rhysd/actionlint/cmd/actionlint@v1.7.12   # same as ci.yml
  else die "actionlint not found (brew install actionlint, or go install github.com/rhysd/actionlint/cmd/actionlint@v1.7.12)"
  fi
fi

say "✓ toolchain ready ($(cargo tauri --version))"

# Advice only: target/ has no GC (docs/build-perf.md); never delete here.
target="$(dirname "$0")/../target"
if [[ -d $target ]] && (( $(du -sk "$target" | cut -f1) > 20971520 )); then
  say "target/ is $(du -sh "$target" | cut -f1); run: bun run sweep"
fi
