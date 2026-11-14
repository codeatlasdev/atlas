#!/usr/bin/env bash
# push-relay-check.sh — gofmt, go vet, and go test for services/push-relay.
# Exits 0 with a notice when Go is not installed (mirrors mobile-check.sh behaviour).
set -euo pipefail
command -v go >/dev/null || { echo "ℹ Go not installed: skipping services/push-relay"; exit 0; }
cd "$(dirname "$0")/../services/push-relay"
bad=$(gofmt -l .)
if [[ -n "$bad" ]]; then
  echo "gofmt: the following files need formatting:" >&2
  echo "$bad" >&2
  exit 1
fi
go vet ./...
go test ./...
