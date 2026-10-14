#!/usr/bin/env bash
# Reclaim target/: intermediate artifacts live in target/build (.cargo/config.toml build-dir) and
# the next build recreates them. Final binaries (target/debug/atlas, ...) stay. --dry-run: sizes only.
set -euo pipefail
cd "$(dirname "$0")/.."

[[ -d target ]] || { echo "no target/"; exit 0; }
sizes() { du -sh target target/build target/rust-analyzer 2>/dev/null || true; }

sizes
[[ ${1:-} == --dry-run ]] && exit 0
rm -rf target/build target/rust-analyzer
echo "after:"
sizes
