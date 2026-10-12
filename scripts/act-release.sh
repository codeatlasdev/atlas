#!/usr/bin/env bash
# Run .github/workflows/release.yml locally via act (no GitHub-hosted minutes).
# macOS jobs run natively on this host (-self-hosted); ubuntu jobs in Docker.
#
# Usage: bun run release:act [vX.Y.Z] [--publish]
#   default tag: version from crates/atlas-cli/Cargo.toml
#   --publish:   also create the GitHub Release (token from GITHUB_TOKEN or `gh auth token`);
#                the tag must already be pushed to origin.
set -euo pipefail
cd "$(dirname "$0")/.."

tag="" publish=0
for a in "$@"; do
  case "$a" in
    --publish) publish=1 ;;
    v*) tag="$a" ;;
    *) echo "unknown arg: $a" >&2; exit 1 ;;
  esac
done
tag="${tag:-v$(sed -n 's/^version = "\(.*\)"/\1/p' crates/atlas-cli/Cargo.toml | head -1)}"
[[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]] || { echo "invalid tag: $tag" >&2; exit 1; }

command -v act >/dev/null || { echo "act not installed: brew install act" >&2; exit 1; }
docker info >/dev/null 2>&1 || { echo "docker not running (needed for ubuntu jobs)" >&2; exit 1; }

secrets=()
if [[ $publish == 1 ]]; then
  git ls-remote --exit-code --tags origin "refs/tags/$tag" >/dev/null || { echo "tag $tag not on origin" >&2; exit 1; }
  token="${GITHUB_TOKEN:-$(gh auth token 2>/dev/null || true)}"
  [[ -n "$token" ]] || { echo "no token: set GITHUB_TOKEN or run gh auth login" >&2; exit 1; }
  secrets=(-s GITHUB_TOKEN="$token" --env ATLAS_PUBLISH=1)
fi

event="$(mktemp -t act-tag).json"
trap 'rm -f "$event"' EXIT
printf '{"ref":"refs/tags/%s","ref_name":"%s","ref_type":"tag"}\n' "$tag" "$tag" >"$event"

rm -rf .act/artifacts
echo "▶ act: release.yml @ $tag$([[ $publish == 1 ]] && echo ' [publish]')"
act workflow_dispatch -W .github/workflows/release.yml -e "$event" \
  --env GITHUB_REF="refs/tags/$tag" --env GITHUB_REF_NAME="$tag" \
  ${secrets[@]+"${secrets[@]}"}

if [[ $publish == 1 ]]; then
  echo "✓ published https://github.com/codeatlasdev/atlas/releases/tag/$tag"
else
  echo "✓ artifacts in .act/artifacts (not published)"
fi
