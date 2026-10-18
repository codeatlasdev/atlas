#!/usr/bin/env bash
# Checks bootstrap.sh's pure tag selection (`select_tag`) against the shared vectors in
# testdata/channels/resolve.json, for all three channels. Standalone:
#   scripts/test-bootstrap-channel.sh
# Needs bun or python3 only to read the JSON.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VECTORS="$ROOT/testdata/channels/resolve.json"

ATLAS_BOOTSTRAP_SOURCE_ONLY=1
# shellcheck source=scripts/bootstrap.sh
. "$ROOT/scripts/bootstrap.sh"
set +e # the sourced script turns `-e` on; this runner counts failures instead

# One TSV line per (case, channel): name, channel, stable_tag, expected, comma-joined tags.
# `-` stands for null / empty.
emit() {
  if command -v bun >/dev/null 2>&1; then
    bun -e '
      const d = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"));
      for (const c of d.cases) for (const ch of ["stable", "beta", "alpha"])
        console.log([c.name, ch, c.stable_tag ?? "-", c.expect[ch] ?? "-", c.tags.join(",") || "-"].join("\t"));
    ' "$VECTORS"
  else
    python3 - "$VECTORS" <<'PY'
import json, sys
for c in json.load(open(sys.argv[1]))["cases"]:
    for ch in ("stable", "beta", "alpha"):
        print("\t".join([c["name"], ch, c["stable_tag"] or "-", c["expect"][ch] or "-", ",".join(c["tags"]) or "-"]))
PY
  fi
}

pass=0
fail=0
check() { # name, got, want
  if [ "$2" = "$3" ]; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
    echo "FAIL: $1: got '$2', want '$3'" >&2
  fi
}

while IFS=$'\t' read -r name channel stable want tags; do
  [ "$stable" = "-" ] && stable=""
  [ "$want" = "-" ] && want=""
  [ "$tags" = "-" ] && tags=""
  got="$(printf '%s\n' "$tags" | tr ',' '\n' | select_tag "$channel" "$stable")"
  check "$name [$channel]" "$got" "$want"
done < <(emit)

# `sort -V` is not SemVer: it ranks 1.4.0-rc.1 above 1.4.0. The selection must not rely on it.
check "release outranks its rc" \
  "$(printf 'v1.4.0-rc.1\nv1.4.0\n' | select_tag alpha '')" "v1.4.0"
check "alpha.10 beats alpha.9" \
  "$(printf 'v1.4.0-alpha.9\nv1.4.0-alpha.10\n' | select_tag alpha '')" "v1.4.0-alpha.10"
check "numeric identifiers rank below alphanumeric" \
  "$(printf 'v1.4.0-alpha.1\nv1.4.0-alpha.beta\n' | select_tag alpha '')" "v1.4.0-alpha.beta"
check "invalid channel selects nothing" \
  "$(printf 'v1.4.0\n' | select_tag nightly 'v1.3.0')" ""

# The channel nobody chose. Pre-GA it is alpha, like `#[default]` on atlas_core::channel::Channel:
# the two must agree, and an explicit value (even stable) must win over it.
rust_default="$(sed -n '/#\[default\]/{n;s/^ *\([A-Za-z]*\).*/\1/p;q;}' "$ROOT/crates/atlas-core/src/channel.rs" | tr 'A-Z' 'a-z')"
check "default channel matches atlas-core" "$DEFAULT_CHANNEL" "$rust_default"
check "default channel is alpha pre-GA" "$DEFAULT_CHANNEL" "alpha"
check "unset ATLAS_CHANNEL follows the default" "$(chosen_channel "")" "alpha"
check "explicit stable is kept" "$(chosen_channel stable)" "stable"
check "explicit beta is kept" "$(chosen_channel beta)" "beta"
check "an unknown channel is not silently the default" "$(chosen_channel nightly)" "nightly"
# With nothing newer than stable published, the default channel installs stable (never nothing).
check "default channel falls back to stable when nothing newer exists" \
  "$(printf 'v1.4.0\n' | select_tag "$DEFAULT_CHANNEL" 'v1.4.0')" "v1.4.0"
# A 1.4.0 stable user with a newer alpha out: the default channel offers the alpha.
check "default channel installs the newest alpha" \
  "$(printf 'v1.5.0-alpha.2\nv1.5.0-alpha.1\nv1.4.0\n' | select_tag "$DEFAULT_CHANNEL" 'v1.4.0')" "v1.5.0-alpha.2"

# While alpha is the default, /releases/latest is the newest alpha (release.yml publishes the default
# channel's builds as non-prerelease). A client that chose stable must still get the newest real stable:
# `stable_from` takes the redirect only when it carries no prerelease label, and finds the rest in the feed.
check "stable ignores an alpha /releases/latest" \
  "$(printf 'v1.5.0-alpha.4\nv1.5.0-alpha.3\nv1.4.0\n' | stable_from 'v1.5.0-alpha.4')" "v1.4.0"
check "stable still trusts a real stable redirect" \
  "$(printf 'v1.4.0\n' | stable_from 'v1.4.0')" "v1.4.0"
check "no stable in the feed and an alpha redirect: nothing, never the alpha" \
  "$(printf 'v1.5.0-alpha.2\n' | stable_from 'v1.5.0-alpha.2')" ""
check "a stable hotfix wins on precedence, not on feed order" \
  "$(printf 'v1.4.1\nv1.5.0-alpha.1\nv1.4.0\n' | stable_from 'v1.5.0-alpha.1')" "v1.4.1"

# site/public/install.sh (the curl | sh one-liner): which channel the installed atlas moves to.
# Sourced in a subshell so its helpers do not leak into the checks above.
step() { (ATLAS_BOOTSTRAP_SOURCE_ONLY=1; . "$ROOT/site/public/install.sh"; channel_step "$1"); }
check "install.sh: no ATLAS_CHANNEL means the installed atlas follows its default" "$(step "")" "default"
check "install.sh: explicit stable is saved as stable" "$(step stable)" "stable"
check "install.sh: explicit beta" "$(step beta)" "beta"
check "install.sh: explicit alpha" "$(step alpha)" "alpha"
# install.sh bootstraps with the newest REAL stable, whatever /releases/latest says (it is an alpha pre-GA),
# then the installed atlas moves to the channel: no ATLAS_CHANNEL -> its default (alpha); stable -> saved.
istable() { (ATLAS_BOOTSTRAP_SOURCE_ONLY=1; . "$ROOT/site/public/install.sh"; printf '%s\n' "$1" | sed 's/,/\n/g' | install_stable_tag "$2"); }
check "install.sh: /releases/latest is an alpha: the bootstrap binary is still the newest stable" \
  "$(istable 'v1.5.0-alpha.4,v1.4.0,v1.4.0-beta.1' 'v1.5.0-alpha.4')" "v1.4.0"
check "install.sh: /releases/latest is a stable: used as is" "$(istable 'v1.4.0' 'v1.4.0')" "v1.4.0"
check "install.sh: nothing stable anywhere: no stable tag to bootstrap from" \
  "$(istable 'v1.5.0-alpha.4' 'v1.5.0-alpha.4')" ""
check "install.sh: an unknown channel is rejected" "$(step nightly)" "invalid"
check "install.sh: channel names are exact (no case folding)" "$(step Alpha)" "invalid"
check "install.sh: default channel matches bootstrap.sh" \
  "$( (ATLAS_BOOTSTRAP_SOURCE_ONLY=1; . "$ROOT/site/public/install.sh"; printf '%s' "$DEFAULT_CHANNEL") )" "$DEFAULT_CHANNEL"

# install.sh downloads the first candidate whose release has a build for the platform (#91: the
# newest Stable, 1.4.0, predates the Linux builds, so `curl | sh` died on a 404 on Linux x86_64).
# boot WITH STEP STABLE LATEST: the tag install.sh downloads for x86_64 Linux, with `has_asset`
# stubbed to the comma-separated releases in WITH that carry atlas-<version>-<target>.tar.gz.
boot() {
  (
    ATLAS_BOOTSTRAP_SOURCE_ONLY=1
    . "$ROOT/site/public/install.sh"
    with="$1"
    has_asset() { case ",$with," in *",$1,"*) [ "$2" = "atlas-${1#v}-x86_64-unknown-linux-gnu.tar.gz" ] ;; *) return 1 ;; esac; }
    bootstrap_candidates "$2" "$3" "$4" | first_with_asset x86_64-unknown-linux-gnu
  )
}
check "install.sh #91: the newest Stable has no build for the platform: the default channel's newest build" \
  "$(boot 'v1.5.0-alpha.13' default v1.4.0 v1.5.0-alpha.13)" "v1.5.0-alpha.13"
check "install.sh #91: same with ATLAS_CHANNEL=alpha" \
  "$(boot 'v1.5.0-alpha.13' alpha v1.4.0 v1.5.0-alpha.13)" "v1.5.0-alpha.13"
check "install.sh: ATLAS_CHANNEL=stable never bootstraps from an alpha" \
  "$(boot 'v1.5.0-alpha.13' stable v1.4.0 v1.5.0-alpha.13)" ""
check "install.sh: ATLAS_CHANNEL=beta never bootstraps from an alpha" \
  "$(boot 'v1.5.0-alpha.13' beta v1.4.0 v1.5.0-alpha.13)" ""
check "install.sh: a Stable with the build still wins over a newer alpha" \
  "$(boot 'v1.4.0,v1.5.0-alpha.13' default v1.4.0 v1.5.0-alpha.13)" "v1.4.0"
check "install.sh: no Stable published: the default channel's newest build" \
  "$(boot 'v1.5.0-alpha.13' default '' v1.5.0-alpha.13)" "v1.5.0-alpha.13"
check "install.sh: no release has the build: nothing, so the script dies" \
  "$(boot '' default v1.4.0 v1.5.0-alpha.13)" ""
check "install.sh: a malformed redirect tag is never probed or installed" \
  "$(boot 'v1.5.0;x' default v1.4.0 'v1.5.0;x')" ""

echo "bootstrap channel selection: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
