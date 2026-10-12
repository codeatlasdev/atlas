#!/usr/bin/env bash
# Cut a release: bump all crate versions, run checks, commit, tag, push, then
# build + publish locally by running .github/workflows/release.yml through act.
# GitHub Actions is manual-only (no hosted minutes spent).
#
# Usage: bun run release <patch|minor|major|X.Y.Z> [--dry-run] [--skip-checks]
set -euo pipefail
cd "$(dirname "$0")/.."

bump="" dry=0 checks=1
for a in "$@"; do
  case "$a" in
    --dry-run) dry=1 ;;
    --skip-checks) checks=0 ;;
    patch|minor|major|[0-9]*) bump="$a" ;;
    *) echo "unknown arg: $a" >&2; exit 1 ;;
  esac
done
bump="${bump:-patch}"

die() { echo "✗ $*" >&2; exit 1; }
run() { echo "+ $*"; [[ $dry == 1 ]] || "$@"; }

crates=(crates/*/Cargo.toml)
current="$(sed -n 's/^version = "\(.*\)"/\1/p' crates/atlas-cli/Cargo.toml | head -1)"
IFS=. read -r ma mi pa <<<"${current%%-*}"
case "$bump" in
  patch) next="$ma.$mi.$((pa + 1))" ;;
  minor) next="$ma.$((mi + 1)).0" ;;
  major) next="$((ma + 1)).0.0" ;;
  *) next="$bump" ;;
esac
[[ "$next" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]] || die "invalid version: $next"
tag="v$next"

# Resume: HEAD already is a pushed release tag whose GitHub Release is missing
# (e.g. publish failed last run) → only publish; never bump again.
head_tag="$(git describe --exact-match --tags --match 'v[0-9]*' HEAD 2>/dev/null || true)"
if [[ -n "$head_tag" ]] && ! gh release view "$head_tag" --repo codeatlasdev/atlas >/dev/null 2>&1; then
  echo "▶ resuming publish of $head_tag (tag exists, release missing)"
  run scripts/act-release.sh "$head_tag" --publish
  exit 0
fi

# Preflight
# The macOS build job runs natively here (act -self-hosted) and bundles Atlas Desktop.
scripts/setup.sh --quiet
command -v act >/dev/null || die "act not found"
branch="$(git rev-parse --abbrev-ref HEAD)"
[[ "$branch" == main ]] || die "must be on main (on $branch)"
[[ -z "$(git status --porcelain)" ]] || [[ $dry == 1 ]] || die "working tree dirty"
git fetch --quiet --tags origin main
[[ "$(git rev-parse HEAD)" == "$(git rev-parse origin/main)" ]] || [[ $dry == 1 ]] || die "main not in sync with origin/main"
git rev-parse -q --verify "refs/tags/$tag" >/dev/null && die "tag $tag already exists"
git ls-remote --exit-code --tags origin "refs/tags/$tag" >/dev/null && die "tag $tag already exists on origin"
grep -q "^## \[$next\]" CHANGELOG.md || die "CHANGELOG.md has no '## [$next]' section (the release notes come from it)"

echo "▶ release $current → $next ($tag)$([[ $dry == 1 ]] && echo ' [dry-run]')"

if [[ $checks == 1 ]]; then
  bun run check
fi

# Bump every crate to the same version (single release version).
for f in "${crates[@]}"; do
  echo "+ bump $f"
  [[ $dry == 1 ]] || sed -i '' "1,/^version = /s/^version = \".*\"/version = \"$next\"/" "$f"
done
run cargo update --workspace --offline --quiet

run git add Cargo.lock "${crates[@]}"
run git commit -m "chore(release): $tag"
run git tag -a "$tag" -m "Atlas $tag"
run git push --atomic origin main "$tag"

run scripts/act-release.sh "$tag" --publish
