#!/usr/bin/env bash
# Release Atlas Mobile: tag main as mobile-vX.Y.Z and push the tag. The tag runs the `Mobile release`
# workflow (.github/workflows/mobile-release.yml): testers first (TestFlight, Play internal testing),
# then the stores only after a maintainer approves the `mobile-store` environment. docs/mobile.md,
# "Releases".
#
# Usage: bun run mobile:release X.Y.Z [--dry-run]
set -euo pipefail
cd "$(dirname "$0")/.."

version="" dry=0
for a in "$@"; do
  case "$a" in
    --dry-run) dry=1 ;;
    -*) echo "unknown arg: $a" >&2; exit 1 ;;
    *) [[ -z "$version" ]] || { echo "unexpected extra argument: $a" >&2; exit 1; }; version="$a" ;;
  esac
done

die() { echo "✗ $*" >&2; exit 1; }
run() { echo "+ $*"; [[ $dry == 1 ]] || "$@"; }

[[ -n "$version" ]] || die "usage: bun run mobile:release X.Y.Z [--dry-run]"
# The workflow accepts only mobile-vX.Y.Z; anything else would build pubspec.yaml's version.
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "version must be X.Y.Z (got '$version')"
tag="mobile-v$version"

branch=$(git branch --show-current)
[[ "$branch" == main ]] || die "release from main (you are on '$branch')"
[[ -z "$(git status --porcelain --untracked-files=no)" ]] || die "uncommitted changes; commit or stash them first"
git fetch -q origin main --tags
[[ "$(git rev-parse HEAD)" == "$(git rev-parse origin/main)" ]] ||
  die "main is not origin/main; run git pull (or push) first"
! git rev-parse -q --verify "refs/tags/$tag" >/dev/null || die "$tag already exists; pick the next version"

# Not lower than the newest mobile release: the stores refuse a version below the live one.
last=$(git tag -l 'mobile-v*' | sed 's/^mobile-v//' | grep -E '^[0-9]+\.[0-9]+\.[0-9]+$' | sort -V | tail -1 || true)
if [[ -n "$last" ]]; then
  [[ "$(printf '%s\n%s\n' "$last" "$version" | sort -V | tail -1)" == "$version" ]] ||
    die "$version is lower than the last release ($last)"
fi

run git tag -a "$tag" -m "Atlas Mobile $version"
run git push origin "$tag"
[[ $dry == 1 ]] && exit 0

cat <<EOF
✓ $tag pushed. Next:
  1. testers get it in TestFlight and Play internal testing (about 20 minutes)
  2. to send it to the stores: GitHub → Actions → Mobile release → this run → Review deployments → Approve
     https://github.com/codeatlasdev/atlas/actions/workflows/mobile-release.yml
EOF
