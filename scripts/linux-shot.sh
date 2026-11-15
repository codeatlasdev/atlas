#!/usr/bin/env bash
# Run Atlas Desktop on a private, invisible Wayland display and screenshot it: the Linux stand-in for the
# macOS agent instances (#280). Nothing appears on your screen, nothing touches your HOME, hostd or D-Bus.
#
#   scripts/linux-shot.sh [--bin PATH] [--size WxH] [--wait SECS] [--env K=V]... [--times N] OUT.png
#
# Headless sway (wlroots `headless` backend, pixman renderer) with its own XDG_RUNTIME_DIR, a private
# `dbus-run-session`, a throwaway HOME/ATLAS_HOSTD_DIR. The app gets a Wayland socket and XWayland, like a real
# session. `--times N` writes OUT-1.png … OUT-N.png, one every 250 ms from the moment the window maps:
# a film of the first frames, to catch a flash of the wrong theme or a layout shift.
# Needs: sway, grim, dbus-run-session (Arch: `pacman -S sway grim`, Debian/Ubuntu: `apt install sway grim`).
set -euo pipefail

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
for t in sway grim dbus-run-session; do command -v "$t" >/dev/null || { echo "missing: $t" >&2; exit 2; }; done
[ -x "$bin" ] || { echo "not executable: $bin" >&2; exit 2; }
bin="$(realpath "$bin")"
out="$(realpath -m "$out")"

work="$(mktemp -d "${TMPDIR:-/tmp}/atlas-shot.XXXXXX")"
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/run" "$work/home"
chmod 700 "$work/run"
printf 'output HEADLESS-1 resolution %s\ndefault_border none\nexec_always true\n' "$size" > "$work/sway.conf"

# Everything below runs in its own D-Bus session; the inner script exits with the app.
exec dbus-run-session -- bash -s "$work" "$bin" "$out" "$wait_s" "$times" "${envs[@]}" <<'INNER'
set -euo pipefail
work=$1; bin=$2; out=$3; wait_s=$4; times=$5; shift 5
unset WAYLAND_DISPLAY DISPLAY SWAYSOCK HYPRLAND_INSTANCE_SIGNATURE GDK_BACKEND XDG_CONFIG_HOME XDG_DATA_HOME XDG_STATE_HOME
export HOME="$work/home" XDG_RUNTIME_DIR="$work/run" ATLAS_HOSTD_DIR="$work/home/.atlas"
export WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman XDG_SESSION_TYPE=wayland
sway -c "$work/sway.conf" >"$work/sway.log" 2>&1 & sway_pid=$!
for _ in $(seq 100); do ls "$XDG_RUNTIME_DIR"/sway-ipc.*.sock >/dev/null 2>&1 && break; sleep 0.05; done
export SWAYSOCK="$(ls "$XDG_RUNTIME_DIR"/sway-ipc.*.sock)"
export WAYLAND_DISPLAY="$(cd "$XDG_RUNTIME_DIR" && ls | grep -m1 '^wayland-[0-9]*$')"
env "$@" setsid "$bin" </dev/null >"$work/app.out" 2>&1 & app=$!
# The moment the window maps (sway's tree has a view of our pid), then the frames.
for _ in $(seq 200); do swaymsg -t get_tree 2>/dev/null | grep -q "\"pid\": $app" && break; sleep 0.05; done
if [ "$times" -gt 1 ]; then
  for i in $(seq "$times"); do grim "${out%.png}-$i.png"; sleep 0.25; done
else
  sleep "$wait_s"; grim "$out"
fi
kill -- -"$app" 2>/dev/null || kill "$app" 2>/dev/null || true
sleep 0.5; kill "$sway_pid" 2>/dev/null || true; wait 2>/dev/null || true
# What the app printed and logged, next to the picture.
cp "$work/app.out" "${out%.png}.stderr.txt" 2>/dev/null || true
cp "$work/home/.atlas/desktop/app.log" "${out%.png}.app.log" 2>/dev/null || true
echo "${out}"
INNER
