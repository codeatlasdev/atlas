#!/usr/bin/env bash
# Run Atlas Desktop on a private, invisible Wayland display and screenshot it: the Linux stand-in for the
# macOS agent instances (#280). Nothing appears on your screen, nothing touches your HOME, hostd or D-Bus.
#
#   scripts/linux-shot.sh [--bin PATH] [--size WxH] [--wait SECS] [--env K=V]... [--times N] OUT.png
#
# Headless sway (scripts/linux-headless.sh) with its own XDG_RUNTIME_DIR, a private `dbus-run-session`, a throwaway
# HOME/ATLAS_HOSTD_DIR. The app gets a Wayland socket and XWayland, like a real session. `--times N` writes
# OUT-1.png … OUT-N.png, one every 250 ms from the moment the window maps: a film of the first frames, to catch a
# flash of the wrong theme or a layout shift. To pass or fail a launch (crashes, blank page, errors) use
# scripts/linux-launch-check.sh.
# Needs: sway, grim, dbus-run-session (Arch: `pacman -S sway grim`, Debian/Ubuntu: `apt install sway grim`).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/linux-headless.sh"

bin="target/debug/atlas-desktop"
size="1440x900"
wait_s=12
times=1
envs=()
while [ $# -gt 1 ]; do
  case "$1" in
    --bin) bin="$2"; shift 2 ;;
    --size) size="$2"; shift 2 ;;
    --wait) wait_s="$2"; shift 2 ;;
    --env) envs+=("$2"); shift 2 ;;
    --times) times="$2"; shift 2 ;;
    *) echo "unknown option $1" >&2; exit 2 ;;
  esac
done
out="${1:?usage: linux-shot.sh [options] OUT.png}"
[ -x "$bin" ] || { echo "not executable: $bin" >&2; exit 2; }
bin="$(realpath "$bin")"
out="$(realpath -m "$out")"
headless_prepare "$size"

# Everything below runs in its own D-Bus session (no activatable services) and exits with the app. Not `exec`: the EXIT trap of
# headless_prepare must run to remove the throwaway dir.
headless_session bash -s "$here" "$work" "$bin" "$out" "$wait_s" "$times" "${envs[@]}" <<'INNER'
set -euo pipefail
here=$1; work=$2; bin=$3; out=$4; wait_s=$5; times=$6; shift 6
. "$here/linux-headless.sh"
headless_up
env "$@" setsid "$bin" </dev/null >"$work/app.out" 2>&1 & app=$!
# The moment the window maps, then the frames.
for _ in $(seq 200); do headless_mapped "$app" && break; sleep 0.05; done
if [ "$times" -gt 1 ]; then
  for i in $(seq "$times"); do grim "${out%.png}-$i.png"; sleep 0.25; done
else
  sleep "$wait_s"; grim "$out"
fi
kill -- -"$app" 2>/dev/null || kill "$app" 2>/dev/null || true
sleep 0.5; headless_down
# What the app printed and logged, next to the picture.
cp "$work/app.out" "${out%.png}.stderr.txt" 2>/dev/null || true
cp "$work/home/.atlas/desktop/app.log" "${out%.png}.app.log" 2>/dev/null || true
echo "${out}"
INNER
