#!/usr/bin/env bash
# Every dependency of the repo, one command per job (docs/contributing.md, "Updating dependencies"):
#   scripts/deps.sh outdated   read-only: what is behind its latest release, per ecosystem
#   scripts/deps.sh update     move everything to its latest release (majors included), minus the holds below
# The set of manifests is the same as .github/dependabot.yml. A missing tool skips its ecosystem with a note.
# After `update`: `bun run check:changed`, fix what a major broke, or add a hold here with its reason.
set -euo pipefail
cd "$(dirname "$0")/.."

BUN_DIRS=(. apps/desktop site studio scripts/i18n)
# Cargo holds: crates whose version is dictated by another crate, never by us. Each has its reason next to it in the
# crate's Cargo.toml. JS holds live in .ncurc.json (`reject`).
CARGO_HOLDS=(windows windows-core webkit2gtk gtk gdkwayland-sys)

has() { command -v "$1" >/dev/null; }
skip() { echo "ℹ $1 not installed: skipping $2"; }
head_() { printf '\n== %s\n' "$1"; }

outdated() {
  head_ "cargo (workspace, apps/mobile/native)"
  if has cargo-upgrade; then
    cargo upgrade --dry-run --incompatible --pinned 2>&1 | grep -v '^warning: aborting' || true
    cargo upgrade --dry-run --incompatible --pinned --manifest-path apps/mobile/native/Cargo.toml 2>&1 | grep -v '^warning: aborting' || true
  else skip cargo-edit "cargo (cargo install cargo-edit)"; fi
  for d in "${BUN_DIRS[@]}"; do head_ "bun ($d)"; (cd "$d" && bun outdated) || true; done
  head_ "go (services/push-relay)"
  if has go; then (cd services/push-relay && go list -u -m -f '{{if and .Update (not .Indirect)}}{{.Path}} {{.Version}} -> {{.Update.Version}}{{end}}' all); else skip go go; fi
  head_ "flutter (apps/mobile)"
  if has flutter; then (cd apps/mobile && flutter pub outdated) || true; else skip flutter flutter; fi
  head_ "bundler (apps/mobile)"
  if has bundle; then (cd apps/mobile && bundle outdated --only-explicit) || true; else skip bundle bundler; fi
  head_ "toolchains (pinned -> latest stable)"
  local rust go flutter
  rust=$(curl -fsS https://static.rust-lang.org/dist/channel-rust-stable.toml | sed -n '/^\[pkg.rustc\]/,/^version/s/^version = "\([0-9.]*\).*/\1/p' || true)
  go=$(curl -fsS 'https://go.dev/dl/?mode=json' | sed -n 's/.*"version": *"go\([0-9.]*\)".*/\1/p' | head -1 || true)
  flutter=$(curl -fsS https://storage.googleapis.com/flutter_infra_release/releases/releases_linux.json |
    bun -e 'const j=await Bun.stdin.json();console.log(j.releases.find(r=>r.hash===j.current_release.stable).version)' || true)
  echo "rust     $(sed -n 's/^channel = "\(.*\)"/\1/p' rust-toolchain.toml) -> ${rust:-?}  (rust-toolchain.toml)"
  echo "go       $(sed -n 's/^go //p' services/push-relay/go.mod) -> ${go:-?}  (go.mod and services/push-relay/Dockerfile)"
  echo "flutter  $(sed -n 's/^  flutter: *//p' apps/mobile/pubspec.yaml) -> ${flutter:-?}  (apps/mobile/pubspec.yaml)"
  echo "tauri-cli $(scripts/setup.sh --tauri-version) -> $(curl -fsS -A atlas-deps https://crates.io/api/v1/crates/tauri-cli | sed -n 's/.*"max_stable_version":"\([^"]*\)".*/\1/p' || true)  (scripts/setup.sh)"
  echo "github actions: Dependabot (.github/dependabot.yml) moves the SHA pins"
}

update() {
  if has cargo-upgrade; then
    local ex=(); for c in "${CARGO_HOLDS[@]}"; do ex+=(--exclude "$c"); done
    cargo upgrade --incompatible "${ex[@]}"
    cargo upgrade --incompatible --manifest-path apps/mobile/native/Cargo.toml
  else skip cargo-edit "cargo majors (cargo install cargo-edit); lockfiles only"; fi
  cargo update
  cargo update --manifest-path apps/mobile/native/Cargo.toml
  local root=$PWD
  for d in "${BUN_DIRS[@]}"; do (cd "$d" && bunx npm-check-updates -u --configFilePath "$root" && bun install); done
  if has go; then (cd services/push-relay && go get -u ./... && go mod tidy); else skip go go; fi
  if has flutter; then (cd apps/mobile && flutter pub upgrade --major-versions); else skip flutter flutter; fi
  if has bundle; then (cd apps/mobile && bundle update); else skip bundle bundler; fi
  echo
  echo "Next: bun run check:changed. Not moved by this script: rust-toolchain.toml, the Flutter pin in apps/mobile/pubspec.yaml,"
  echo "the Go builder image (services/push-relay/Dockerfile, same minor as go.mod), TAURI_CLI_VERSION (scripts/setup.sh)."
}

case "${1:-}" in
  outdated) outdated ;;
  update) update ;;
  *) echo "usage: scripts/deps.sh outdated|update" >&2; exit 2 ;;
esac
