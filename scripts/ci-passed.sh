#!/usr/bin/env bash
# ci-passed.sh — waits for the CI run (ci.yml) of <sha> and exits 0 only when it completed with success.
# A failed/cancelled run, no completed run within the poll budget, or nothing but API errors exits 1:
# the Release workflow and `bun run release` publish nothing that CI did not pass.
# Usage: scripts/ci-passed.sh <owner/repo> <sha>
#   GH overrides the gh binary (tests); CI_PASSED_POLL_SECS (default 30) and CI_PASSED_MAX_POLLS
#   (default 240, i.e. 2 h) control waiting.
set -uo pipefail

repo="${1:?owner/repo}"; sha="${2:?sha}"
gh="${GH:-gh}"
secs="${CI_PASSED_POLL_SECS:-30}"; max="${CI_PASSED_MAX_POLLS:-240}"

for ((i = 1; i <= max; i++)); do
  if out="$("$gh" api "repos/$repo/actions/workflows/ci.yml/runs?head_sha=$sha&per_page=1" \
    --jq '.workflow_runs[0] // empty | "\(.status) \(.conclusion) \(.html_url)"')"; then
    read -r status conclusion url <<<"$out"
    if [[ ${status:-} == completed ]]; then
      if [[ $conclusion == success ]]; then echo "CI passed for $sha"; exit 0; fi
      echo "::error::CI $conclusion for $sha: $url" >&2
      exit 1
    fi
  fi
  ((i < max)) && sleep "$secs"
done
echo "::error::no completed CI run for $sha after $max polls" >&2
exit 1
