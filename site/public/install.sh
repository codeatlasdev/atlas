#!/bin/sh
# Atlas installer: https://atlas.codeatlas.com.br
#   curl -fsSL https://atlas.codeatlas.com.br/install.sh | sh
#
# Downloads the latest GitHub Release of codeatlasdev/atlas, verifies its sha256
# and runs the binary's own `atlas install`, which puts `atlas` in ~/.atlas/bin,
# adds it to PATH for fish/zsh/bash and installs the bundled agent skills.
# No sudo. Prebuilt binaries exist for macOS on Apple silicon only.
set -eu

REPO="codeatlasdev/atlas"

say() { printf '  %s\n' "$1"; }
die() { printf '  x %s\n' "$1" >&2; exit 1; }

command -v curl >/dev/null 2>&1 || die "curl is required"
command -v tar >/dev/null 2>&1 || die "tar is required"
command -v shasum >/dev/null 2>&1 || die "shasum is required"

os="$(uname -s)"
arch="$(uname -m)"
case "$os-$arch" in
  Darwin-arm64) target="aarch64-apple-darwin" ;;
  *)
    die "Atlas ships prebuilt binaries for macOS on Apple silicon only; this is $os/$arch.
    Build from source instead: https://github.com/$REPO/blob/main/docs/installation.md#build-from-source"
    ;;
esac

meta="$(curl -fsSL --proto '=https' --tlsv1.2 -H 'Accept: application/vnd.github+json' \
  "https://api.github.com/repos/$REPO/releases/latest")" \
  || die "could not read the latest release: https://github.com/$REPO/releases"
tag="$(printf '%s' "$meta" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"
version="${tag#v}"
case "$version" in
  "" | *[!0-9A-Za-z._-]*) die "unexpected release tag '$tag'" ;;
esac

asset="atlas-$version-$target.tar.gz"
base="https://github.com/$REPO/releases/download/$tag"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

say "Atlas $version ($target)"
curl -fsSL --proto '=https' --tlsv1.2 -o "$tmp/$asset" "$base/$asset" || die "download failed: $asset"
curl -fsSL --proto '=https' --tlsv1.2 -o "$tmp/$asset.sha256" "$base/$asset.sha256" || die "download failed: $asset.sha256"

want="$(cut -d' ' -f1 < "$tmp/$asset.sha256")"
got="$(shasum -a 256 "$tmp/$asset" | cut -d' ' -f1)"
[ -n "$want" ] && [ "$want" = "$got" ] || die "sha256 mismatch for $asset (expected $want, got $got)"
say "sha256 verified"

tar -xzf "$tmp/$asset" -C "$tmp" atlas
chmod +x "$tmp/atlas"

"$tmp/atlas" install

say ""
say "Next:"
say "  open a new shell, then:  atlas version"
say "  atlas desktop            install the desktop app"
say "  atlas setup omp          wire an agent: omp | claude-code | codex (add --apply to write)"
say "  docs: https://atlas.codeatlas.com.br/docs/getting-started/"
