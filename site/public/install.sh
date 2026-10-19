#!/bin/sh
# Atlas installer: https://atlas.codeatlas.com.br
#   curl -fsSL https://atlas.codeatlas.com.br/install.sh | sh
#   curl -fsSL https://atlas.codeatlas.com.br/install.sh | ATLAS_CHANNEL=stable sh   # stable, beta or alpha
#
# Downloads the newest Stable GitHub Release (no prerelease label) of codeatlasdev/atlas that has a
# build for this platform (else, on the alpha channel, the newest build of the default channel),
# verifies its sha256 and runs the binary's own `atlas install`, which puts `atlas` in ~/.atlas/bin,
# adds it to PATH for fish/zsh/bash and installs the bundled agent skills. Then the installed
# atlas moves to the channel you follow: the default (alpha while Atlas is in alpha, so the
# newest build from main) or the one in ATLAS_CHANNEL, which is remembered.
# No sudo. Prebuilt binaries: macOS on Apple silicon and Linux x86_64 (glibc 2.35+: Arch, Fedora,
# Debian 12, Ubuntu 22.04 and newer). Linux packages (.deb, .rpm, Arch, AppImage) are on the release page.
set -eu

# The channel nobody chose. Same value as DEFAULT_CHANNEL in scripts/bootstrap.sh and `#[default]`
# on atlas_core::channel::Channel (docs/versioning.md lists every copy to flip at GA).
DEFAULT_CHANNEL=alpha

# What to do after the install for an ATLAS_CHANNEL value. Prints `default` (no value: the
# installed atlas follows its own default channel; nothing is saved, so changing the default
# later still applies), the channel name (an explicit choice: saved with `--channel`), or
# `invalid`. Pure: sourced by scripts/test-bootstrap-channel.sh.
channel_step() {
  case "${1:-}" in
    "") echo default ;;
    stable | beta | alpha) echo "$1" ;;
    *) echo invalid ;;
  esac
}

# install_stable_tag LATEST < tags
#   The newest Stable among the feed tags on stdin and LATEST (the /releases/latest redirect, may be
#   empty): releases only (X.Y.Z, no prerelease label), highest SemVer precedence. Pre-GA the
#   redirect is the newest alpha (the default channel's builds are normal releases), so it is a
#   candidate, never "the stable". Prints nothing when none exists. Pure: sourced by
#   scripts/test-bootstrap-channel.sh, which pins it to bootstrap.sh's `stable_from`.
install_stable_tag() {
  { cat; printf '%s\n' "${1:-}"; } | tr -d '\r' | awk '
    { t = $0; v = t; sub(/^v/, "", v); sub(/\+.*/, "", v) }
    v !~ /^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$/ { next }
    { split(v, n, ".") }
    !have || n[1] + 0 > a + 0 || (n[1] + 0 == a + 0 && (n[2] + 0 > b + 0 || (n[2] + 0 == b + 0 && n[3] + 0 > c + 0))) {
      have = 1; a = n[1]; b = n[2]; c = n[3]; best = t
    }
    END { if (have) print best }
  '
}

# bootstrap_candidates STEP STABLE LATEST
#   The tags to bootstrap from, best first, one per line: STABLE (the newest Stable, may be empty),
#   then LATEST (the /releases/latest redirect: the default channel's newest build, may be empty)
#   when the channel being installed is alpha, which accepts any build. STEP is channel_step's
#   output. Stable and beta never get an alpha. Pure: sourced by scripts/test-bootstrap-channel.sh.
bootstrap_candidates() {
  channel="$1"
  [ "$channel" = default ] && channel="$DEFAULT_CHANNEL"
  [ -z "$2" ] || printf '%s\n' "$2"
  [ "$channel" != alpha ] || [ -z "$3" ] || [ "$3" = "$2" ] || printf '%s\n' "$3"
}

# first_with_asset TARGET < tags
#   The first well-formed tag whose release has atlas-<version>-TARGET.tar.gz (`has_asset TAG
#   ASSET`). A Stable can predate a platform: 1.4.0 has no Linux build (#91). Prints nothing when
#   none has it. Sourced by scripts/test-bootstrap-channel.sh with `has_asset` stubbed.
first_with_asset() {
  while IFS= read -r t; do
    v="${t#v}"
    case "$v" in "" | *[!0-9A-Za-z._-]*) continue ;; esac
    if has_asset "$t" "atlas-$v-$1.tar.gz"; then
      printf '%s\n' "$t"
      return 0
    fi
  done
  return 0
}

# has_asset TAG ASSET: whether the release TAG has the downloadable file ASSET (HEAD, follows the redirect).
has_asset() {
  curl -fsSIL --proto '=https' --tlsv1.2 -o /dev/null "https://github.com/$REPO/releases/download/$1/$2" 2>/dev/null
}

[ "${ATLAS_BOOTSTRAP_SOURCE_ONLY:-}" = 1 ] && return 0 2>/dev/null

REPO="codeatlasdev/atlas"

say() { printf '  %s\n' "$1"; }
die() { printf '  x %s\n' "$1" >&2; exit 1; }

command -v curl >/dev/null 2>&1 || die "curl is required"
command -v tar >/dev/null 2>&1 || die "tar is required"
if command -v sha256sum >/dev/null 2>&1; then
  sha256() { sha256sum "$1" | cut -d' ' -f1; }
elif command -v shasum >/dev/null 2>&1; then
  sha256() { shasum -a 256 "$1" | cut -d' ' -f1; }
else
  die "sha256sum or shasum is required"
fi
[ "$(channel_step "${ATLAS_CHANNEL:-}")" != invalid ] || die "unknown ATLAS_CHANNEL '$ATLAS_CHANNEL' (expected stable, beta or alpha)"

os="$(uname -s)"
arch="$(uname -m)"
case "$os-$arch" in
  Darwin-arm64) target="aarch64-apple-darwin" ;;
  Linux-x86_64 | Linux-amd64) target="x86_64-unknown-linux-gnu" ;;
  MINGW* | MSYS* | CYGWIN*)
    die "On Windows, install from PowerShell instead:
    irm https://atlas.codeatlas.com.br/install.ps1 | iex"
    ;;
  *)
    die "Atlas ships prebuilt binaries for macOS on Apple silicon and Linux x86_64; this is $os/$arch.
    Build from source instead: https://github.com/$REPO/blob/main/docs/installation.md#build-from-source"
    ;;
esac

# Newest Stable tag. /releases/latest (no API call, so no 60 requests/hour limit) is the newest build of
# the default channel, an alpha pre-GA, so it is only a hint for Stable: the feed (10 newest published
# releases, older pages with ?after=, bounded) and the redirect together give the newest tag without a
# prerelease label. On the alpha channel the redirect is also the fallback when that Stable has no build
# for this platform.
atom_tags() { sed -n 's|.*href="[^"]*/releases/tag/\([^"]*\)".*|\1|p' | sed 's/%2B/+/g; s/%2b/+/g'; }
latest="$(curl -fsSI --proto '=https' --tlsv1.2 "https://github.com/$REPO/releases/latest" 2>/dev/null \
  | tr -d '\r' | sed -n 's|^[Ll]ocation:.*/releases/tag/\(.*\)$|\1|p' | head -n 1)" || latest=""
seen="$(curl -fsSL --proto '=https' --tlsv1.2 "https://github.com/$REPO/releases.atom" 2>/dev/null | atom_tags)" || seen=""
stable="$(printf '%s\n' "$seen" | install_stable_tag "$latest")"
pages=0
last="$(printf '%s\n' "$seen" | grep -c .)" || last=0
while [ -z "$stable" ] && [ "$pages" -lt 5 ] && [ "$last" -ge 10 ]; do
  oldest="$(printf '%s\n' "$seen" | grep . | tail -n 1)"
  page="$(curl -fsSL --proto '=https' --tlsv1.2 "https://github.com/$REPO/releases.atom?after=$oldest" 2>/dev/null | atom_tags)" || break
  [ -n "$page" ] || break
  last="$(printf '%s\n' "$page" | grep -c .)"
  seen="$(printf '%s\n%s\n' "$seen" "$page")"
  stable="$(printf '%s\n' "$seen" | install_stable_tag "$latest")"
  pages=$((pages + 1))
done
step="$(channel_step "${ATLAS_CHANNEL:-}")"
tag="$(bootstrap_candidates "$step" "$stable" "$latest" | first_with_asset "$target")"
if [ -z "$tag" ]; then
  case "$step" in
    stable | beta)
      die "no Stable release has a build for $target yet (newest Stable: ${stable:-none}). Install the newest alpha instead:
    curl -fsSL https://atlas.codeatlas.com.br/install.sh | ATLAS_CHANNEL=alpha sh"
      ;;
    *) die "could not find a release with a build for $target: https://github.com/$REPO/releases" ;;
  esac
fi
version="${tag#v}"
[ "$tag" = "$stable" ] || say "no Stable build for $target yet (newest Stable: ${stable:-none}); starting from $tag"

asset="atlas-$version-$target.tar.gz"
base="https://github.com/$REPO/releases/download/$tag"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

say "Atlas $version ($target)"
curl -fsSL --proto '=https' --tlsv1.2 -o "$tmp/$asset" "$base/$asset" || die "download failed: $asset"
curl -fsSL --proto '=https' --tlsv1.2 -o "$tmp/$asset.sha256" "$base/$asset.sha256" || die "download failed: $asset.sha256"

want="$(cut -d' ' -f1 < "$tmp/$asset.sha256")"
got="$(sha256 "$tmp/$asset")"
[ -n "$want" ] && [ "$want" = "$got" ] || die "sha256 mismatch for $asset (expected $want, got $got)"
say "sha256 verified"

tar -xzf "$tmp/$asset" -C "$tmp" atlas
chmod +x "$tmp/atlas"

"$tmp/atlas" install

# This script installs a bootstrap binary (the latest stable, or the default channel's newest build
# when that stable has no build for this platform), then the freshly installed atlas moves to the
# channel it follows (same verified download, one resolver: `atlas update`). Without ATLAS_CHANNEL
# that is the default channel, passed as ATLAS_UPDATE_CHANNEL (not saved): the stable binary just
# installed may predate the default and would otherwise stay on its own (1.4.0 follows stable). With
# one, `--channel` saves it (stable included).
case "$(channel_step "${ATLAS_CHANNEL:-}")" in
  default)
    say "updating to the newest build of the ${DEFAULT_CHANNEL} channel"
    ATLAS_UPDATE_CHANNEL="$DEFAULT_CHANNEL" "$HOME/.atlas/bin/atlas" update \
      || say "could not update now; run later: atlas update"
    ;;
  stable)
    "$HOME/.atlas/bin/atlas" update --channel stable \
      || say "could not save the stable channel now; run later: atlas update --channel stable"
    ;;
  beta | alpha)
    say "switching to the ${ATLAS_CHANNEL} channel"
    "$HOME/.atlas/bin/atlas" update --channel "$ATLAS_CHANNEL" \
      || say "could not switch to ${ATLAS_CHANNEL} now; run later: atlas update --channel ${ATLAS_CHANNEL}"
    ;;
esac

say ""
say "Next:"
say "  open a new shell, then:  atlas version"
say "  atlas desktop            install the desktop app"
say "  atlas setup omp          wire an agent: omp | claude-code | codex (add --apply to write)"
say "  docs: https://atlas.codeatlas.com.br/docs/getting-started/"
