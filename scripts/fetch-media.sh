#!/usr/bin/env bash
# fetch-media.sh — download the heavy site media that is not kept in git into site/public/media/.
# Idempotent: files that already exist (and are non-empty) are skipped; `--force` re-downloads.
# `bun run site:deploy` uploads site/dist, which is built from site/public, so run this first on any
# machine that has not built the site before, or the deployed site loses the film.
#
# Usage: scripts/fetch-media.sh [--force]
#   MEDIA_BASE_URL   where to download from (default: the live site, https://atlas.codeatlas.com.br/media)
set -euo pipefail

base="${MEDIA_BASE_URL:-https://atlas.codeatlas.com.br/media}"
dest="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/site/public/media"
force=0; [[ "${1:-}" == "--force" ]] && force=1

# Everything .gitignore excludes under site/public/media (keep in sync with .gitignore).
files=(
  film.mp4 film.webm film-vertical.mp4
  loop-agents.mp4 loop-agents.webm loop-logs.mp4 loop-logs.webm
  loop-mcp.mp4 loop-mcp.webm loop-panel.mp4 loop-panel.webm
  loop-terminal.mp4 loop-terminal.webm loop-tui.mp4 loop-tui.webm
  loop-workspace.mp4 loop-workspace.webm
  demo.gif
)

mkdir -p "$dest"
fail=0
for f in "${files[@]}"; do
  if [[ $force == 0 && -s "$dest/$f" ]]; then
    echo "have  $f"
    continue
  fi
  # download to a temp name so an interrupted run never leaves a truncated file behind
  if curl -fsSL --retry 2 -o "$dest/$f.part" "$base/$f"; then
    mv "$dest/$f.part" "$dest/$f"
    echo "got   $f"
  else
    rm -f "$dest/$f.part"
    echo "FAIL  $f ($base/$f)" >&2
    fail=1
  fi
done
exit $fail
