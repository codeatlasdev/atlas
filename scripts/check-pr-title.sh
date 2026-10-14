#!/usr/bin/env bash
# check-pr-title.sh — a pull request title must be a Conventional Commits subject
# (https://www.conventionalcommits.org/en/v1.0.0/): PRs are squash-merged and the title becomes the
# commit subject on main. GitHub's own `Revert "<title>"` is accepted.
# Usage: scripts/check-pr-title.sh "<title>"
set -euo pipefail

title="${1?usage: check-pr-title.sh <title>}"
types='feat|fix|perf|refactor|docs|test|build|ci|chore|revert|style'
re="^($types)(\([a-z0-9][a-z0-9._/-]*\))?!?: [^[:space:]].*$"

if [[ $title =~ $re || $title =~ ^Revert\ \".+\"$ ]]; then
  echo "PR title: ok"
  exit 0
fi
cat >&2 <<EOF
::error::PR title is not a conventional commit subject: $title
Use "<type>(<optional scope>): <imperative summary>", type one of: ${types//|/, }.
Examples: "fix(desktop): keep the sidebar width", "feat!: drop the v1 config". See CONTRIBUTING.md.
EOF
exit 1
