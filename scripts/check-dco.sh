#!/usr/bin/env bash
# check-dco.sh — every commit in <base>..<head> must carry a Signed-off-by trailer
# (https://developercertificate.org/). Merge commits and the bots listed below are exempt.
# Usage: scripts/check-dco.sh <base-sha> <head-sha>
set -euo pipefail

base="${1:?base sha}"; head="${2:?head sha}"
fail=0
while read -r sha; do
  [[ -n "$sha" ]] || continue
  author_email="$(git show -s --format='%ae' "$sha")"
  case "$author_email" in
    *dependabot*|*'[bot]@users.noreply.github.com') continue ;;
  esac
  if ! git show -s --format='%B' "$sha" | grep -Eq '^Signed-off-by: .+ <[^<>[:space:]]+@[^<>[:space:]]+>[[:space:]]*$'; then
    echo "::error::commit $(git show -s --format='%h %s' "$sha") has no Signed-off-by (use: git commit -s / git rebase --signoff)"
    fail=1
  fi
done < <(git rev-list --no-merges "$base..$head")

if [[ $fail == 1 ]]; then
  echo "DCO: sign off every commit, see CONTRIBUTING.md#developer-certificate-of-origin-dco" >&2
  exit 1
fi
echo "DCO: all commits signed off"
