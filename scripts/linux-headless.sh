#!/usr/bin/env bash
# Sourced by scripts/linux-shot.sh and scripts/linux-launch-check.sh: a private, invisible Wayland display for a
# desktop app. Never the caller's session (Hyprland, GNOME, ...): own XDG_RUNTIME_DIR, own sway on wlroots'
# `headless` backend with the pixman renderer (no GPU needed), a throwaway HOME. Nothing appears on any screen.
#
#   headless_prepare WxH     outside: makes $work (run/, home/, sway.conf, session.conf), removes it when the script exits
#   headless_session CMD...  runs CMD in a private D-Bus session with NOTHING activatable (see below)
#   headless_up              inside the private session: starts sway, exports HOME (the throwaway, or $ATLAS_HEADLESS_HOME),
#                            XDG_*, SWAYSOCK, WAYLAND_DISPLAY
#   headless_mapped PID      true once sway's tree has a window of that pid
#   headless_down            stops sway

# The private bus has no <servicedir>: a client of it can never auto-activate Nautilus, xdg-desktop-portal(-gtk/-hyprland)
# or any other service, which (started with the caller's WAYLAND_DISPLAY) would open windows on the user's screen
# (incident 2026-11-14). Never plain `dbus-run-session` with the distro's session.conf. dbus-run-session ends the
# daemon with its program; the daemon starts without the caller's display variables.
headless_session() {
  env -u WAYLAND_DISPLAY -u DISPLAY -u SWAYSOCK -u HYPRLAND_INSTANCE_SIGNATURE -u GDK_BACKEND -u DBUS_SESSION_BUS_ADDRESS \
    HOME="${ATLAS_HEADLESS_HOME:-$work/home}" XDG_RUNTIME_DIR="$work/run" \
    dbus-run-session --config-file="$work/session.conf" -- "$@"
}

headless_prepare() {
  for t in sway grim dbus-run-session; do command -v "$t" >/dev/null || { echo "missing: $t" >&2; exit 2; }; done
  work="$(mktemp -d "${TMPDIR:-/tmp}/atlas-headless.XXXXXX")"
  trap 'rm -rf "$work"' EXIT
  mkdir -p "$work/run" "$work/home"
  chmod 700 "$work/run"
  cat > "$work/session.conf" <<CONF
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN" "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:path=$work/run/bus</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
CONF
  printf 'output HEADLESS-1 resolution %s\ndefault_border none\nexec_always true\n' "$1" > "$work/sway.conf"
}

headless_up() {
  unset WAYLAND_DISPLAY DISPLAY SWAYSOCK HYPRLAND_INSTANCE_SIGNATURE GDK_BACKEND XDG_CONFIG_HOME XDG_DATA_HOME XDG_STATE_HOME
  # HOME is a fresh throwaway unless the caller brings one (ATLAS_HEADLESS_HOME: scripts/linux-first-run.sh installs into it first).
  export HOME="${ATLAS_HEADLESS_HOME:-$work/home}" XDG_RUNTIME_DIR="$work/run"
  export ATLAS_HOSTD_DIR="$HOME/.atlas"
  # The app reads the desktop it runs on (tiling, title row): ours is sway, not the caller's.
  # NO_AT_BRIDGE/GTK_A11Y: no at-spi launcher, which would bind an accessibility bus (an orphan once rebound the user's).
  export NO_AT_BRIDGE=1 GTK_A11Y=none
  export XDG_CURRENT_DESKTOP=sway XDG_SESSION_TYPE=wayland WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman
  sway -c "$work/sway.conf" >"$work/sway.log" 2>&1 & sway_pid=$!
  for _ in $(seq 100); do ls "$XDG_RUNTIME_DIR"/sway-ipc.*.sock >/dev/null 2>&1 && break; sleep 0.05; done
  SWAYSOCK="$(ls "$XDG_RUNTIME_DIR"/sway-ipc.*.sock 2>/dev/null | head -1)"
  WAYLAND_DISPLAY="$(cd "$XDG_RUNTIME_DIR" && ls | grep -m1 '^wayland-[0-9]*$' || true)"
  if [ -z "$SWAYSOCK" ] || [ -z "$WAYLAND_DISPLAY" ]; then
    echo "the private sway did not start:" >&2; cat "$work/sway.log" >&2
    kill "$sway_pid" 2>/dev/null || true
    return 3
  fi
  export SWAYSOCK WAYLAND_DISPLAY
  # The X display of sway's own XWayland, as a real session exports it: the AppImage's AppRun forces GDK_BACKEND=x11.
  # sway sets DISPLAY in its own environment when it starts XWayland (lazily), so a process sway starts shows it.
  rm -f "$work/display"
  swaymsg exec "sh -c 'printf %s \"\$DISPLAY\" > $work/display'" >/dev/null 2>&1 || true
  for _ in $(seq 100); do [ -s "$work/display" ] && break; sleep 0.05; done
  if [ -s "$work/display" ]; then DISPLAY="$(cat "$work/display")"; export DISPLAY; fi
}

# Before any input is sent to the display (wtype, swaymsg focus): the runtime dir, the Wayland socket and sway's socket are
# all inside this session's own $work, never the caller's (/run/user/...). Exit 3 with the variable named; nothing was sent.
headless_assert_private() {
  local run="${XDG_RUNTIME_DIR:-}" wl="${WAYLAND_DISPLAY:-}" sw="${SWAYSOCK:-}"
  case "$run" in "$work"/run) ;; *) echo "REFUSED: XDG_RUNTIME_DIR=$run is not this session's $work/run" >&2; return 3 ;; esac
  case "$run" in /run/user/*) echo "REFUSED: XDG_RUNTIME_DIR=$run is a user session" >&2; return 3 ;; esac
  case "$wl" in ""|*/*|*..*) echo "REFUSED: WAYLAND_DISPLAY=$wl is not a socket name inside $run" >&2; return 3 ;; esac
  [ -S "$run/$wl" ] || { echo "REFUSED: WAYLAND_DISPLAY=$wl is not a socket inside $run" >&2; return 3; }
  case "$sw" in "$run"/*) ;; *) echo "REFUSED: SWAYSOCK=$sw is outside $run" >&2; return 3 ;; esac
}

headless_mapped() { swaymsg -t get_tree 2>/dev/null | grep -q "\"pid\": $1"; }

# Stops sway and whatever the app left behind in this session: hostd detaches itself and would outlive the app. Found by
# the throwaway ATLAS_HOSTD_DIR in its environment, so nothing of anyone else's can match.
headless_down() {
  kill "$sway_pid" 2>/dev/null || true; wait "$sway_pid" 2>/dev/null || true
  local p
  for p in $(grep -lz "^ATLAS_HOSTD_DIR=$ATLAS_HOSTD_DIR\$" /proc/[0-9]*/environ 2>/dev/null | cut -d/ -f3); do
    [ "$p" = "$$" ] || kill "$p" 2>/dev/null || true
  done
}
