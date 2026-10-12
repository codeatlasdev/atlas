#!/usr/bin/env bash
# Atlas CLI — bootstrap installer (no GitHub login, no `gh`: public release assets over curl).
# Usage: bash <(curl -fsSL https://raw.githubusercontent.com/codeatlasdev/atlas/main/scripts/bootstrap.sh)
#   or:  curl -fsSL https://atlas.codeatlas.com.br/install.sh | sh   (same download + `atlas install`, without the agent prompts)
#
# Downloads the latest release, verifies its sha256, runs `atlas install`
# (binary to ~/.atlas/bin, PATH entry, bundled skills) and offers to wire up
# any detected coding agent (omp, Claude Code, Codex).
set -euo pipefail

REPO="codeatlasdev/atlas"

# ── Output ────────────────────────────────────────────────────────────────────

BOLD="\033[1m"
DIM="\033[2m"
GREEN="\033[32m"
YELLOW="\033[33m"
RED="\033[31m"
RESET="\033[0m"

step()    { echo; echo -e "  ${BOLD}$1${RESET}"; }
ok()      { echo -e "  ${GREEN}✓${RESET}  $1"; }
info()    { echo -e "  ${DIM}·${RESET}  $1"; }
warn()    { echo -e "  ${YELLOW}!${RESET}  $1"; }
die()     { echo; echo -e "  ${RED}✗${RESET}  $1" >&2; exit 1; }

echo
echo -e "  ${BOLD}Atlas${RESET}  ${DIM}local-first dev environments${RESET}"
echo -e "  ${DIM}──────────────────────────────────${RESET}"

# ── Preconditions ─────────────────────────────────────────────────────────────

step "Checking dependencies"

command -v curl >/dev/null || die "curl is required"
command -v tar  >/dev/null || die "tar is required"
if command -v shasum >/dev/null; then
  sha256() { shasum -a 256 "$1" | cut -d' ' -f1; }
elif command -v sha256sum >/dev/null; then
  sha256() { sha256sum "$1" | cut -d' ' -f1; }
else
  die "shasum or sha256sum is required"
fi

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) TARGET="aarch64-apple-darwin" ;;
  *) die "unsupported platform $(uname -s)/$(uname -m) (published: macOS Apple Silicon; build from source otherwise)" ;;
esac
ok "curl, tar, sha256 · $TARGET"

# ── Download ──────────────────────────────────────────────────────────────────

step "Installing Atlas"

meta="$(curl -fsSL -H 'Accept: application/vnd.github+json' "https://api.github.com/repos/$REPO/releases/latest")" \
  || die "Could not read the latest release of $REPO (is it published yet?). https://github.com/$REPO/releases"
tag="$(printf '%s' "$meta" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1)"
version="${tag#v}"
case "$version" in *[!0-9A-Za-z._-]*|"") die "unexpected release tag '$tag'" ;; esac

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

asset="atlas-$version-$TARGET.tar.gz"
base="https://github.com/$REPO/releases/download/$tag"
info "Downloading $asset..."
curl -fsSL --proto '=https' --tlsv1.2 -o "$TMP/$asset"        "$base/$asset"        || die "download failed: $asset"
curl -fsSL --proto '=https' --tlsv1.2 -o "$TMP/$asset.sha256" "$base/$asset.sha256" || die "download failed: $asset.sha256"

want="$(cut -d' ' -f1 < "$TMP/$asset.sha256")"
got="$(sha256 "$TMP/$asset")"
[ -n "$want" ] && [ "$want" = "$got" ] || die "sha256 mismatch for $asset (expected $want, got $got)"
ok "sha256 verified"

tar xzf "$TMP/$asset" -C "$TMP" atlas
chmod +x "$TMP/atlas"

# ── Install ───────────────────────────────────────────────────────────────────

"$TMP/atlas" install

# ── Agent setup (omp, Claude Code, Codex) ─────────────────────────────────────

ATLAS_BIN="$HOME/.atlas/bin/atlas"
if [ -x "$ATLAS_BIN" ]; then
  # agent name (atlas setup <name>) : executable on PATH
  for pair in omp:omp claude-code:claude codex:codex; do
    agent="${pair%%:*}"; bin="${pair##*:}"
    command -v "$bin" &>/dev/null || continue
    step "Agent setup: $agent"
    info "$bin detected — plug it into Atlas (MCP server, skills, hooks)"
    if [ -t 0 ] && [ -t 1 ]; then
      read -r -p "  Run 'atlas setup $agent --apply' now? [Y/n] " reply || reply=""
      case "${reply:-y}" in
        [Yy]*) PATH="$HOME/.atlas/bin:$PATH" "$ATLAS_BIN" setup "$agent" --apply || warn "atlas setup $agent failed — run it again later" ;;
        *)     info "Skipped. Run later: atlas setup $agent --apply" ;;
      esac
    else
      info "Non-interactive shell — skipped. Run: atlas setup $agent --apply"
    fi
  done
fi

# ── Done ──────────────────────────────────────────────────────────────────────

echo
echo -e "  ${DIM}──────────────────────────────────${RESET}"
echo -e "  Restart your shell or run:"
echo -e "  ${BOLD}export PATH=\"\$HOME/.atlas/bin:\$PATH\"${RESET}"
echo
