#!/usr/bin/env bash
# Atlas CLI — bootstrap installer (no GitHub login, no `gh`: public release assets over curl).
# Usage: bash <(curl -fsSL https://raw.githubusercontent.com/codeatlasdev/atlas/main/scripts/bootstrap.sh)
#   or:  curl -fsSL https://atlas.codeatlas.com.br/install.sh | sh   (same download + `atlas install`, without the agent prompts)
#
# Downloads the newest release of the chosen channel, verifies its sha256, runs `atlas install`
# (binary to ~/.atlas/bin, PATH entry, bundled skills) and offers to wire up
# any detected coding agent (omp, Claude Code, Codex).
#
# ATLAS_CHANNEL=stable|beta|alpha  pick the release channel (default: alpha while Atlas is in
#                                  alpha, like `atlas update`; DEFAULT_CHANNEL below). An explicit
#                                  value is remembered in ~/.atlas/update-channel, so
#                                  `atlas update` keeps following it. See docs/versioning.md.
set -euo pipefail

REPO="codeatlasdev/atlas"

# ── Channel selection (pure: no network, no output; sourced by the tests) ─────

# The channel nobody chose. Mirrors `#[default]` on atlas_core::channel::Channel (the test
# fails if they differ). Pre-GA it is alpha; flipping it back to stable is this one line.
DEFAULT_CHANNEL=alpha

# chosen_channel VALUE: the channel to install for an ATLAS_CHANNEL value (empty or unset is
# the default). An unknown value is returned as is, so the caller can reject it.
chosen_channel() {
  printf '%s\n' "${1:-$DEFAULT_CHANNEL}"
}

# select_tag CHANNEL STABLE_TAG < tags
#   Reads candidate release tags, one per line (what releases.atom lists; junk allowed), and
#   prints the one CHANNEL should install: the highest SemVer 2.0 precedence among the tags the
#   channel accepts, with STABLE_TAG (the releases/latest redirect, may be empty) always a
#   candidate so a channel is never worse than stable.
#     stable accepts releases only; beta adds -beta.N and -rc.N; alpha adds -alpha.N.
#   Prints nothing when there is no candidate or CHANNEL is unknown.
#   `sort -V` is NOT used: it is not SemVer (it ranks 1.4.0-rc.1 above 1.4.0, and compares
#   pre-release identifiers as text), so the comparison is done in awk. Same algorithm and
#   vectors as atlas_core::channel::resolve (testdata/channels/resolve.json).
select_tag() {
  { cat; printf '%s\n' "${2:-}"; } | awk -v channel="$1" '
    # a numeric identifier: digits, no leading zero (SemVer 2.0 §9)
    function isnum(s) { return s ~ /^(0|[1-9][0-9]*)$/ }
    # precedence of two pre-release strings: -1, 0, 1 (§11); empty pre-release is the highest
    function cmppre(a, b,    x, y, na, nb, i, p, q) {
      if (a == b) return 0
      if (a == "") return 1
      if (b == "") return -1
      na = split(a, x, "."); nb = split(b, y, ".")
      for (i = 1; i <= na && i <= nb; i++) {
        p = x[i]; q = y[i]
        if (p == q) continue
        if (isnum(p) && isnum(q)) return (p + 0 < q + 0) ? -1 : 1
        if (isnum(p)) return -1
        if (isnum(q)) return 1
        return (p < q) ? -1 : 1
      }
      return (na < nb) ? -1 : (na > nb) ? 1 : 0
    }
    # 1 when v is better than the best so far (precedence only: build metadata is ignored)
    function better(a1, a2, a3, ap) {
      if (!have) return 1
      if (a1 + 0 != b1 + 0) return a1 + 0 > b1 + 0
      if (a2 + 0 != b2 + 0) return a2 + 0 > b2 + 0
      if (a3 + 0 != b3 + 0) return a3 + 0 > b3 + 0
      return cmppre(ap, bp) > 0
    }
    BEGIN { have = 0 }
    {
      tag = $0
      sub(/\r$/, "", tag)
      v = tag
      sub(/^v/, "", v)
      sub(/\+.*/, "", v)                       # build metadata never counts
      pre = ""
      if (match(v, /-/)) { pre = substr(v, RSTART + 1); v = substr(v, 1, RSTART - 1) }
      if (v !~ /^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$/) next
      if (pre != "") {
        n = split(pre, ids, ".")
        ok = 1
        for (i = 1; i <= n; i++) {
          if (ids[i] !~ /^[0-9A-Za-z-]+$/) ok = 0
          if (ids[i] ~ /^[0-9]+$/ && !isnum(ids[i])) ok = 0   # leading zero: invalid
        }
        if (!ok) next
        label = ids[1]
        if (label == "alpha") { if (channel != "alpha") next }
        else if (label == "beta" || label == "rc") { if (channel != "alpha" && channel != "beta") next }
        else next                                # unknown labels are offered nowhere
      }
      if (channel != "stable" && channel != "beta" && channel != "alpha") next
      split(v, n3, ".")
      if (better(n3[1], n3[2], n3[3], pre)) {
        have = 1; b1 = n3[1]; b2 = n3[2]; b3 = n3[3]; bp = pre; best = tag
      }
    }
    END { if (have) print best }
  '
}

# Tags of the entries in a releases.atom feed (stdin), in feed order, percent-decoded enough
# for SemVer tags: the part after `/releases/tag/` of every `href`.
atom_tags() {
  sed -n 's|.*href="[^"]*/releases/tag/\([^"]*\)".*|\1|p' | sed 's/%2B/+/g; s/%2b/+/g'
}

# stable_from LATEST < tags
#   The newest Stable (no prerelease label) among the feed tags on stdin and LATEST, the
#   /releases/latest redirect (may be empty). Pre-GA the redirect is the newest alpha (release.yml
#   publishes the default channel's builds as normal releases), so it is only a candidate, never
#   "the stable tag". Prints nothing when none is a Stable. Same rule as
#   atlas_core::github::stable_from.
stable_from() { select_tag stable "${1:-}"; }

# stable_tag_of_repo REPO LATEST FIRST_PAGE_TAGS
#   The Stable tag: stable_from over the first feed page, reading older pages (`?after=` the
#   oldest tag seen, 10 per page, at most 5) while a run of prereleases hides every Stable.
stable_tag_of_repo() {
  local repo="$1" latest="$2" seen="$3" found pages=0 last_page_len oldest page
  last_page_len="$(printf '%s\n' "$seen" | grep -c .)" || last_page_len=0
  while :; do
    found="$(printf '%s\n' "$seen" | stable_from "$latest")"
    [ -z "$found" ] || { printf '%s\n' "$found"; return 0; }
    # A feed page is 10 long: a shorter one is the end of the history. Bounded either way.
    [ "$last_page_len" -ge 10 ] && [ "$pages" -lt 5 ] || return 0
    oldest="$(printf '%s\n' "$seen" | grep . | tail -n 1)"
    page="$(curl -fsSL --proto '=https' --tlsv1.2 "https://github.com/$repo/releases.atom?after=$oldest" 2>/dev/null | atom_tags)" || return 0
    [ -n "$page" ] || return 0
    last_page_len="$(printf '%s\n' "$page" | grep -c .)"
    seen="$(printf '%s\n%s\n' "$seen" "$page")"
    pages=$((pages + 1))
  done
}

# The script is also sourced by scripts/test-bootstrap-channel.sh for the functions above.
if [ "${ATLAS_BOOTSTRAP_SOURCE_ONLY:-}" = 1 ]; then
  # shellcheck disable=SC2317 # `return` works when sourced, `exit` covers direct execution
  return 0 2>/dev/null || exit 0
fi

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
  Linux-x86_64 | Linux-amd64) TARGET="x86_64-unknown-linux-gnu" ;;
  MINGW* | MSYS* | CYGWIN*) die "on Windows install from PowerShell instead: irm https://atlas.codeatlas.com.br/install.ps1 | iex" ;;
  *) die "unsupported platform $(uname -s)/$(uname -m) (published: macOS Apple Silicon, Linux x86_64, Windows x64; build from source otherwise)" ;;
esac
ok "curl, tar, sha256 · $TARGET"

# ── Download ──────────────────────────────────────────────────────────────────

step "Installing Atlas"

CHANNEL="$(chosen_channel "${ATLAS_CHANNEL:-}")"
case "$CHANNEL" in
  stable|beta|alpha) ;;
  *) die "unknown ATLAS_CHANNEL '$CHANNEL' (expected stable, beta or alpha)" ;;
esac

# /releases/latest is the newest build of the default channel (an alpha pre-GA), no API call so no
# 60 requests/hour rate limit. It is only a hint: Stable is the newest tag without a prerelease
# label in releases.atom (10 newest published releases, prereleases included) plus that hint.
latest_tag="$(curl -fsSI --proto '=https' --tlsv1.2 -o /dev/null -w '%{redirect_url}' "https://github.com/$REPO/releases/latest" 2>/dev/null)" || latest_tag=""
case "$latest_tag" in */releases/tag/*) latest_tag="${latest_tag##*/releases/tag/}" ;; *) latest_tag="" ;; esac
feed="$(curl -fsSL --proto '=https' --tlsv1.2 "https://github.com/$REPO/releases.atom" 2>/dev/null)" || feed=""
[ -n "$feed" ] || warn "could not read the release feed; using /releases/latest only"
feed_tags="$(printf '%s\n' "$feed" | atom_tags)"
stable_tag="$(stable_tag_of_repo "$REPO" "$latest_tag" "$feed_tags")"

if [ "$CHANNEL" = stable ]; then
  tag="$stable_tag"
else
  # The stable tag is a candidate too, so beta/alpha are never worse than stable.
  tag="$(printf '%s\n' "$feed_tags" | select_tag "$CHANNEL" "$stable_tag")"
  [ -n "$tag" ] || tag="$stable_tag"
fi
[ -n "$tag" ] || die "Could not find a release of $REPO (is it published yet?). https://github.com/$REPO/releases"
version="${tag#v}"
case "$version" in *[!0-9A-Za-z._-]*|"") die "unexpected release tag '$tag'" ;; esac
[ "$CHANNEL" = stable ] || info "channel: $CHANNEL → $tag"

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

# Remember an explicit channel so `atlas update` keeps following it (same file as the CLI).
if [ -n "${ATLAS_CHANNEL:-}" ]; then
  mkdir -p "$HOME/.atlas"
  printf '%s\n' "$CHANNEL" > "$HOME/.atlas/update-channel.tmp"
  mv "$HOME/.atlas/update-channel.tmp" "$HOME/.atlas/update-channel"
  info "update channel: $CHANNEL (change it with: atlas update channel)"
fi

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
