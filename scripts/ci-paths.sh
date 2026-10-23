#!/usr/bin/env bash
# ci-paths.sh — reads changed paths on stdin, prints `area=true|false` lines for the CI plan job, so a change
# only runs the jobs it can affect. Anything not matched by a known docs-only pattern counts as source for
# every area (unknown paths never skip a check). Docs feed the site build, so docs/ still runs `site`.
set -uo pipefail
rust=false front=false mobile=false site=false relay=false workflows=false any=false
while IFS= read -r f || [[ -n $f ]]; do
  [[ -z $f ]] && continue
  any=true
  case "$f" in
    *.md|LICENSE*|.github/ISSUE_TEMPLATE/*|assets/*)
      case "$f" in docs/*) site=true ;; esac
      case "$f" in skills/*) front=true; rust=true ;; esac   # skills are embedded in the CLI and checked
      case "$f" in apps/mobile/*) mobile=true ;; esac
      ;;
    crates/*|Cargo.toml|Cargo.lock|rust-toolchain.toml|deny.toml|.cargo/*|.config/nextest.toml) rust=true ;;
    apps/desktop/*) front=true; rust=true ;;              # the desktop shell embeds the frontend build
    apps/mobile/*) mobile=true ;;
    site/*) site=true ;;
    services/push-relay/*) relay=true ;;
    .github/*) workflows=true; rust=true; front=true; mobile=true; site=true ;;
    *) rust=true front=true mobile=true site=true relay=true workflows=true ;;
  esac
done
# No diff to compare (a new branch, an API error): run everything.
if [[ $any == false ]]; then rust=true front=true mobile=true site=true relay=true workflows=true; fi
for k in rust front mobile site relay workflows; do echo "$k=${!k}"; done
