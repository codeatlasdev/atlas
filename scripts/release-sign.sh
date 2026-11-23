#!/usr/bin/env bash
# Sign a release's SHA256SUMS with a maintainer's OFFLINE key and publish it (#279, docs/release.md "Release signing").
# release.yml builds every release as a DRAFT; nothing is public, and no client can fetch it, until this runs:
#   gh release download SHA256SUMS -> ssh-keygen -Y sign -n atlas-release -> verify it back against the embedded
#   keys (crates/atlas-core/data/release-keys.json) -> gh release upload SHA256SUMS.sig -> gh release edit --draft=false.
#
# Usage: scripts/release-sign.sh vX.Y.Z [--key FILE] [--yes]
#   --key FILE   private key, or the public key of one held by ssh-agent / a gpg-agent card (default $ATLAS_RELEASE_KEY)
#   --yes        do not ask before signing (the SHA256SUMS is still printed)
# `bun run release` calls it after the release run, for every channel including a stable promotion. It is
# safe to re-run: a release that already carries a valid signature is only published.
#
# While no key is configured (release-keys.json has none) there is nothing to sign with: the draft is published
# as before and says so. Once a key is configured an unsigned release can no longer be published by this script.
set -euo pipefail
cd "$(dirname "$0")/.."

repo=codeatlasdev/atlas
die() { echo "✗ $*" >&2; exit 1; }

tag="${1:-}"; [[ "$tag" == v* ]] || die "usage: scripts/release-sign.sh vX.Y.Z [--key FILE] [--yes]"
shift
key="${ATLAS_RELEASE_KEY:-}" yes=0
while (($#)); do
  case "$1" in
    --key) shift; key="${1:?--key needs a file}" ;;
    --yes) yes=1 ;;
    *) die "unknown arg: $1" ;;
  esac
  shift
done

for t in gh bun; do command -v "$t" >/dev/null || die "$t not found"; done

is_draft="$(gh release view "$tag" --repo "$repo" --json isDraft -q .isDraft)" || die "no release $tag on $repo (did the Release run finish?)"

publish() {
  if [[ "$is_draft" != true ]]; then echo "✓ $tag is already published"; return 0; fi
  # Same flags release.yml used to pass at creation: "Latest" follows the default channel.
  local latest
  latest="$(bun scripts/release-version.ts github-flags --tag "$tag" | sed -n 's/^make_latest=//p')"
  echo "▶ publishing $tag (draft -> public, latest=$latest)"
  gh release edit "$tag" --repo "$repo" --draft=false --latest="$latest"
}

if [[ "$(bun scripts/release-keys.ts count)" == 0 ]]; then
  echo "⚠ no release key is configured (crates/atlas-core/data/release-keys.json): publishing $tag unsigned" >&2
  publish
  exit 0
fi

command -v ssh-keygen >/dev/null || die "ssh-keygen not found (install openssh)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
bun scripts/release-keys.ts allowed-signers > "$tmp/allowed_signers"

# 0 when "$2" is a valid signature of "$1" by a key in release-keys.json (valid today).
verify() {
  local p
  while read -r p; do
    [[ -n "$p" ]] || continue
    ssh-keygen -Y verify -f "$tmp/allowed_signers" -I "$p" -n atlas-release -s "$2" < "$1" >/dev/null 2>&1 && return 0
  done < <(ssh-keygen -Y find-principals -s "$2" -f "$tmp/allowed_signers" 2>/dev/null)
  return 1
}

gh release download "$tag" --repo "$repo" -p SHA256SUMS -D "$tmp"
if gh release view "$tag" --repo "$repo" --json assets -q '.assets[].name' | grep -qx SHA256SUMS.sig; then
  gh release download "$tag" --repo "$repo" -p SHA256SUMS.sig -D "$tmp" --clobber
  verify "$tmp/SHA256SUMS" "$tmp/SHA256SUMS.sig" || die "$tag already has a SHA256SUMS.sig that does not verify against release-keys.json; not publishing"
  echo "✓ $tag already carries a valid signature"
else
  [[ -n "$key" ]] || die "no signing key: pass --key FILE or set ATLAS_RELEASE_KEY (your offline release key, docs/release.md)"
  cat "$tmp/SHA256SUMS"
  if [[ $yes == 0 ]]; then
    [[ -t 0 ]] || die "not a terminal: review SHA256SUMS above and re-run with --yes"
    read -r -p "Sign $tag with these hashes? [y/N] " ans
    [[ "$ans" == [yY]* ]] || die "not signed"
  fi
  ssh-keygen -Y sign -f "$key" -n atlas-release "$tmp/SHA256SUMS"
  # Never upload what clients would reject (wrong key, expired key, wrong namespace).
  verify "$tmp/SHA256SUMS" "$tmp/SHA256SUMS.sig" || die "the signature does not verify against release-keys.json (wrong key, or one outside its validity); nothing uploaded"
  echo "✓ signature verifies against release-keys.json"
  gh release upload "$tag" --repo "$repo" "$tmp/SHA256SUMS.sig"
fi
publish
