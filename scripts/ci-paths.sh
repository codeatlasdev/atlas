#!/usr/bin/env bash
# ci-paths.sh — reads changed paths on stdin, prints `area=true|false` lines for the CI plan job, so a change
# only runs the jobs it can affect. Anything not matched by a known docs-only pattern counts as source for
# every area (unknown paths never skip a check). Docs feed the site build, so docs/ still runs `site`.
set -uo pipefail
rust=false front=false mobile=false site=false relay=false workflows=false installer=false i18n=false any=false
while IFS= read -r f || [[ -n $f ]]; do
  [[ -z $f ]] && continue
  any=true
  case "$f" in
    *.md|LICENSE*|.github/ISSUE_TEMPLATE/*|assets/*)
      case "$f" in docs/*) site=true ;; esac
      # bun run i18n:check reads prose: translations (front matter, source sha) and the writing lint of English docs,
      # README, CHANGELOG, skills, site content
      case "$f" in docs/*|skills/*|README.md|CHANGELOG.md|i18n/*|site/src/content/*) i18n=true ;; esac
      case "$f" in skills/*) front=true; rust=true ;; esac   # skills are embedded in the CLI and checked
      case "$f" in apps/mobile/*) mobile=true ;; esac
      ;;
    # The i18n job (bun run i18n:check): Fluent catalogs, and every Rust file its hardcoded-text ratchet scans
    # (scripts/i18n/hardcoded/rust.ts RUST_DIRS). Scanning by directory, not by `t!(`: the ratchet exists to catch the
    # new println!/bail! that has no `t!(` in it, and names alone cannot tell which file contains what.
    crates/*/locales/*|crates/atlas-cli/src/*|crates/atlas-tui/src/*|crates/atlas-desktop/src/*|crates/atlas-host/src/*) rust=true; i18n=true ;;
    crates/*|Cargo.toml|Cargo.lock|rust-toolchain.toml|deny.toml|.cargo/*|.config/nextest.toml) rust=true ;;
    apps/desktop/*) front=true; rust=true; i18n=true ;;   # the desktop shell embeds the frontend build; catalogs and ratchets
    apps/mobile/*) mobile=true ;;
    site/messages/*|site/project.inlang/*|site/src/*) site=true; i18n=true ;;   # catalogs and the source ratchets
    site/public/install.ps1) site=true; installer=true ;;   # the Windows installer has its own job (PowerShell 5.1 and 7)
    site/*) site=true ;;
    services/push-relay/*) relay=true ;;
    .github/*) workflows=true; rust=true; front=true; mobile=true; site=true; installer=true; i18n=true ;;
    scripts/install-ps1.test.ts) front=true; installer=true ;;
    *) rust=true front=true mobile=true site=true relay=true workflows=true installer=true i18n=true ;;
  esac
done
# No diff to compare (a new branch, an API error): run everything.
if [[ $any == false ]]; then rust=true front=true mobile=true site=true relay=true workflows=true installer=true i18n=true; fi
for k in rust front mobile site relay workflows installer i18n; do echo "$k=${!k}"; done
