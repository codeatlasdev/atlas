#!/usr/bin/env bash
# ci-tested.sh — prints `true` when <sha>, a push to main or release/*, has exactly the tree of a merged
# pull request head whose CI run passed. Then the push needs no second test run: the ruleset requires
# the branch to be up to date, so the squash commit's tree is the tree CI already tested.
# Anything else prints `false` and the full CI runs: no PR, a tree that differs (merged while behind,
# a direct push), no green run, or any API error. It never fails the job.
# Usage: scripts/ci-tested.sh <owner/repo> <sha>     (GH overrides the gh binary, for tests)
set -uo pipefail

repo="${1:?owner/repo}"; sha="${2:?sha}"
gh="${GH:-gh}"

tested() {
  local tree head head_tree runs
  tree="$("$gh" api "repos/$repo/git/commits/$sha" --jq .tree.sha)" || return 1
  head="$("$gh" api "repos/$repo/commits/$sha/pulls" --jq '[.[] | select(.merged_at != null)][0].head.sha // empty')" || return 1
  [[ -n $head ]] || return 1
  head_tree="$("$gh" api "repos/$repo/git/commits/$head" --jq .tree.sha)" || return 1
  [[ -n $tree && $tree == "$head_tree" ]] || return 1
  runs="$("$gh" api "repos/$repo/actions/workflows/ci.yml/runs?head_sha=$head&event=pull_request&status=success" --jq .total_count)" || return 1
  [[ $runs =~ ^[0-9]+$ ]] && ((runs > 0))
}

if tested; then echo true; else echo false; fi
