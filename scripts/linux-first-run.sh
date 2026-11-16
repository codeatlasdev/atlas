#!/usr/bin/env bash
# A first Linux user, with the builds you have in hand instead of whatever install.sh would download: a throwaway
# HOME, the CLI installed the way `install.sh` installs it (`atlas install`), `atlas doctor`, then Atlas Desktop
# installed by `atlas desktop --from <your build>` and OPENED on a private headless sway
# (scripts/linux-launch-check.sh: it fails on a crash, a blank page or an error in the app's log).
#
#   scripts/linux-first-run.sh --cli target/debug/atlas --desktop target/debug/atlas-desktop [--out DIR] [--skel DIR] [launch-check options]
#
# --desktop is an atlas-desktop binary or an .AppImage. --skel is the new account's skeleton (default this machine's
# /etc/skel: Arch's has .bash_profile, which a non-login terminal never reads; Ubuntu's has .profile and .bashrc). No sudo, nothing outside the throwaway HOME (the shell's
# own config dirs are unset so fish/nushell files never land in yours), no network: install.sh's download is the
# only step not exercised, because it is the only one that fetches a published build.
# Exit codes: 0 pass; 1 a step failed (the last line says which); 2 usage; 3 as linux-launch-check.sh.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"

cli="" desktop="" out=first-run-out skel=/etc/skel
while [ $# -gt 0 ]; do
  case "$1" in
    --cli) cli="$2"; shift 2 ;;
    --desktop) desktop="$2"; shift 2 ;;
    --out) out="$2"; shift 2 ;;
    --skel) skel="$2"; shift 2 ;;
    *) break ;;
  esac
done
[ -x "$cli" ] && [ -x "$desktop" ] || { echo "usage: linux-first-run.sh --cli ATLAS --desktop ATLAS_DESKTOP|APPIMAGE [--out DIR] [--skel DIR] [launch-check options]" >&2; exit 2; }
cli="$(realpath "$cli")" desktop="$(realpath "$desktop")"
mkdir -p "$out"
out="$(realpath "$out")"

home="$(mktemp -d "${TMPDIR:-/tmp}/atlas-first-run.XXXXXX")"
trap 'rm -rf "$home"' EXIT
# A new account starts from the distro's skeleton (.bashrc, .bash_profile): `atlas install` adds its PATH line to the
# startup files that exist, so an empty HOME is not what a first user has.
cp -a "$skel"/. "$home/" 2>/dev/null || true
[ -e "$home/.bashrc" ] || [ -e "$home/.bash_profile" ] || [ -e "$home/.profile" ] || : > "$home/.bashrc"
# A first user's shell and PATH: nothing of the caller's, nothing of Atlas.
user() { env -u XDG_CONFIG_HOME -u XDG_DATA_HOME -u XDG_STATE_HOME -u ZDOTDIR -u ATLAS_BIN -u ATLAS_HOSTD_DIR \
  HOME="$home" SHELL=/bin/bash PATH=/usr/local/bin:/usr/bin:/bin "$@"; }
step() { echo "▶ $*"; }
die() { echo "FAIL first-run: $*" >&2; exit 1; }

step "atlas install (what install.sh runs after the download)"
user "$cli" install >"$out/install.txt" 2>&1 || { cat "$out/install.txt" >&2; die "atlas install failed"; }
[ -x "$home/.atlas/bin/atlas" ] || die "atlas install left no ~/.atlas/bin/atlas"

step "atlas --version, atlas doctor --json"
user "$home/.atlas/bin/atlas" --version | tee "$out/version.txt"
user "$home/.atlas/bin/atlas" doctor --json >"$out/doctor.json" 2>"$out/doctor.err" || { cat "$out/doctor.json" "$out/doctor.err" >&2; die "atlas doctor reports a failing check"; }

step "atlas desktop --from $desktop (installs, does not open)"
user "$home/.atlas/bin/atlas" desktop --from "$desktop" --no-open >"$out/desktop-install.txt" 2>&1 || { cat "$out/desktop-install.txt" >&2; die "atlas desktop --from failed"; }
app="$home/.local/bin/atlas-desktop"
[ -x "$app" ] || die "atlas desktop --from left no $app"

step "open the installed app"
ATLAS_HEADLESS_HOME="$home" "$here/linux-launch-check.sh" --out "$out" "$@" "$app" || exit $?
