#!/usr/bin/env bash
# lint-workflows.sh — actionlint (syntax, expressions, shellcheck of `run:` blocks) and zizmor (security
# audit; config .github/zizmor.yml) over .github/. Used by `bun run check` and the CI "Workflow lint" job.
# Install: scripts/setup.sh (brew/cargo locally), taiki-e/install-action in CI.
set -euo pipefail
cd "$(dirname "$0")/.."

for tool in actionlint zizmor; do
  command -v "$tool" >/dev/null || { echo "lint-workflows: $tool is missing; run scripts/setup.sh" >&2; exit 1; }
done

actionlint
# --offline: the audits that need the GitHub API (known-vulnerable actions, impostor commits) are covered
# by dependabot and the SHA pins; offline keeps the gate deterministic and free of token/rate limits.
zizmor --offline --quiet .github
