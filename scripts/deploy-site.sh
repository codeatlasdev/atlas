#!/usr/bin/env bash
# Build site/ and deploy it to Cloudflare Pages (project atlas-site, https://atlas.codeatlas.com.br).
#   scripts/deploy-site.sh              # production (branch main)
#   scripts/deploy-site.sh --branch x   # preview deployment
# Needs CLOUDFLARE_API_TOKEN and CLOUDFLARE_ACCOUNT_ID in the environment; the token is never printed.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root/site"

branch="main"
if [[ "${1:-}" == "--branch" ]]; then
  branch="${2:?--branch needs a name}"
fi

: "${CLOUDFLARE_API_TOKEN:?CLOUDFLARE_API_TOKEN is not set}"
: "${CLOUDFLARE_ACCOUNT_ID:?CLOUDFLARE_ACCOUNT_ID is not set}"
command -v bun >/dev/null || { echo "bun is required" >&2; exit 1; }

bun install --frozen-lockfile
bun run build

# wrangler picks up site/functions/ (Pages Functions) and site/dist/_headers automatically.
bunx wrangler pages deploy dist \
  --project-name atlas-site \
  --branch "$branch" \
  --commit-dirty=true
