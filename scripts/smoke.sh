#!/usr/bin/env bash
# smoke.sh — end-to-end check that `atlas dev --ci` brings a project up healthy.
# Usage: scripts/smoke.sh [project-dir]   (default: demo/, which needs nothing but a shell)
# Exit 0 = every service healthy, 1 = failure.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROJECT="${1:-$ROOT/demo}"
ATLAS="$ROOT/target/debug/atlas"

echo "=== Atlas smoke test ==="
echo "  project: $PROJECT"
echo "  atlas:   $ATLAS"

echo "[1/2] Building atlas..."
(cd "$ROOT" && cargo build -q -p atlas-cli)

echo "[2/2] Running atlas dev --ci..."
status=0
"$ATLAS" dev --ci --timeout 60 -d "$PROJECT" 2>&1 || status=$?

if [ "$status" -eq 0 ]; then
  echo "=== PASS ==="
else
  echo "=== FAIL (exit $status) ==="
fi
exit "$status"
