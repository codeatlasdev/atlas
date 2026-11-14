#!/usr/bin/env bash
# py.sh — runs Python 3 with its arguments. On Windows `python3` is often the Microsoft Store stub, which exits
# without running anything, so each candidate is tried for real first.
set -euo pipefail
# The Windows launcher first: in Git Bash `python3` may be the Store stub or an MSYS Python without pip.
for py in "py -3" python3 python; do
  if $py -c 'import sys; sys.exit(sys.version_info < (3, 8))' >/dev/null 2>&1; then
    exec $py "$@"
  fi
done
echo "✗ Python 3.8+ not found (https://www.python.org/downloads/)" >&2
exit 1
