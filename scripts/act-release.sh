#!/usr/bin/env bash
# Run .github/workflows/release.yml locally via act (no GitHub-hosted minutes).
# macOS jobs run natively on this host (-self-hosted); ubuntu jobs (Linux build, Windows cross-build, Arch package,
# publish) in Docker.
#
# Usage: bun run release:act [vX.Y.Z[-pre.N]] [--publish] [-j <job>]...
#   default tag: version from crates/atlas-cli/Cargo.toml
#   -j <job>:    run only that job (repeatable; ids from `act -l -W .github/workflows/release.yml`), e.g.
#                `-j build-linux` proves the Linux build without macOS. act would still run the job's `needs`
#                (the macOS `test`), so the dependencies outside the selected jobs are dropped from a temp copy
#                of the workflow. Incompatible with --publish.
#   --publish:   also create the GitHub Release (token from GITHUB_TOKEN or `gh auth token`);
#                the tag must already be pushed to origin.
#   ATLAS_PROMOTED_FROM=<tag> in the environment is recorded in manifest.json (set by release.sh stable).
set -euo pipefail
cd "$(dirname "$0")/.."

tag="" publish=0 jobs=() only=()
while [[ $# -gt 0 ]]; do
  a="$1"; shift
  case "$a" in
    --publish) publish=1 ;;
    -j) [[ $# -gt 0 ]] || { echo "-j needs a job id" >&2; exit 1; }; jobs+=(-j "$1"); only+=("$1"); shift ;;
    v*) tag="$a" ;;
    *) echo "unknown arg: $a" >&2; exit 1 ;;
  esac
done
tag="${tag:-v$(sed -n 's/^version = "\(.*\)"/\1/p' crates/atlas-cli/Cargo.toml | head -1)}"
[[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]] || { echo "invalid tag: $tag" >&2; exit 1; }

[[ $publish == 0 || ${#jobs[@]} == 0 ]] || { echo "--publish needs the whole workflow, not -j" >&2; exit 1; }

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
workflow=.github/workflows/release.yml
trap 'rm -f "$event" "${subset:-}"' EXIT
if [[ ${#only[@]} -gt 0 ]]; then
  # Same directory as the real file: act resolves the workflow relative to it. Removed on exit.
  subset=.github/workflows/.release-subset.yml
  python3 - "$workflow" "$subset" "${only[@]}" <<'PY'
import re, sys
src, dst, *keep = sys.argv[1:]
out = []
for line in open(src):
    m = re.match(r"^    needs: (.*)$", line.rstrip("\n"))
    if m:
        deps = [d for d in re.findall(r"[\w-]+", m.group(1)) if d in keep]
        if not deps:
            continue
        line = "    needs: [%s]\n" % ", ".join(deps)
    out.append(line)
open(dst, "w").writelines(out)
PY
  workflow=$subset
fi
printf '{"ref":"refs/tags/%s","ref_name":"%s","ref_type":"tag"}\n' "$tag" "$tag" >"$event"

extra_env=()
[[ -z "${ATLAS_PROMOTED_FROM:-}" ]] || extra_env=(--env ATLAS_PROMOTED_FROM="$ATLAS_PROMOTED_FROM")

rm -rf .act/artifacts
echo "▶ act: release.yml @ $tag$([[ $publish == 1 ]] && echo ' [publish]')"
# linux/amd64: the Linux release assets are x86_64 (Docker runs them under Rosetta/QEMU on Apple silicon).
act workflow_dispatch -W "$workflow" -e "$event" --container-architecture linux/amd64 \
  --env GITHUB_REF="refs/tags/$tag" --env GITHUB_REF_NAME="$tag" \
  ${jobs[@]+"${jobs[@]}"} \
  ${extra_env[@]+"${extra_env[@]}"} \
  ${secrets[@]+"${secrets[@]}"}

if [[ $publish == 1 ]]; then
  echo "✓ published https://github.com/codeatlasdev/atlas/releases/tag/$tag"
else
  echo "✓ artifacts in .act/artifacts (not published)"
fi
