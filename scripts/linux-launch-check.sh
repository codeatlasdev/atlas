#!/usr/bin/env bash
# Launch Atlas Desktop on a private headless sway and FAIL when a first user would see a broken app (#283).
#
#   scripts/linux-launch-check.sh [options] <atlas-desktop binary | .AppImage>
#
#   --out DIR         where the evidence goes (default launch-check-out/): screenshot.png, app.stderr.txt,
#                     app.log (the app's own ~/.atlas/desktop/app.log), sway.log, result.txt
#   --size WxH        virtual display size (default 1440x900)
#   --map-timeout S   seconds for the window to map (default 90)
#   --render-timeout S  seconds, after it maps, for the page to paint something (default 60)
#   --settle S        seconds the app must stay up and quiet after the first paint (default 8)
#   --env K=V         extra environment for the app (repeatable)
#   --ignore REGEX    stderr lines matching REGEX are not failures (repeatable)
#
# What fails (each reason is printed as `FAIL <tag>: <detail>` and listed in result.txt):
#   app-exited       the app process ended before the checks finished (a crash, `Aborting`, a missing library)
#   no-window        no window mapped in sway within --map-timeout
#   blank-page       the screenshot is one flat colour (or almost): the page never painted, or painted nothing
#   webprocess-died  no WebKitWebProcess under the app at the end, or the one seen at first paint is gone
#   stderr-marker    the app/WebKit printed a crash, EGL/GBM/Mesa failure, GStreamer `appsrc not found` (a bundle
#                    without its plugins, #265; it shows only once a page plays audio or video, so a clean launch
#                    does not prove playback: scripts/appimage-patch.sh checks the plugins are in the bundle), GLib
#                    CRITICAL, panic, CSP refusal or console error (MARKERS below)
#   app-log-error    the app wrote anything but its startup line to ~/.atlas/desktop/app.log: every line there is
#                    an incident (panic, frontend error from the page's error/unhandledrejection handlers,
#                    host/terminal failure). See crates/atlas-desktop/src/diag.rs
#   coredump         coredumpctl (when present) lists a dump of the app or a WebKit process since the start
#
# Exit codes: 0 every check passed; 1 at least one check failed; 2 usage error or a missing tool (sway, grim,
# dbus-run-session, the binary); 3 the private display could not start.
#
# HOME is a throwaway, or $ATLAS_HEADLESS_HOME when set (scripts/linux-first-run.sh).
# An AppImage runs with APPIMAGE_EXTRACT_AND_RUN=1 (CI and containers have no FUSE). Everything is private: own
# sway, own D-Bus session, throwaway HOME; the user's session is never touched.
#
# Not covered without the debug bridge: console.error calls that are not uncaught errors, and CSP violations
# unless WebKit prints them (they land in MARKERS when it does). When `atlas desktop --agent` works on Linux,
# replace the "markers" and "app log" steps with `atlas desktop debug logs --level error` and `--level csp`;
# the launch/render/crash steps stay as they are.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/linux-headless.sh"

# stderr lines that mean a broken launch (extended regex, case-sensitive). Not `libEGL warning: failed to get driver name`
# or `MESA: error: ZINK: vkCreateInstance failed`: Mesa probing a GPU/Vulkan a headless machine does not have before it
# falls back to llvmpipe (seen on GitHub runners, with a page that rendered). A GL that really cannot start shows as
# WebKit's `Could not create ... EGL`, a missing window or a blank page.
MARKERS='Aborting|GStreamer element appsrc not found|Process crashed|[Ww]eb ?[Pp]rocess (crashed|terminated|died)|terminated unexpectedly|Segmentation fault|SIGSEGV|SIGABRT|SIGBUS|panicked at|EGL_BAD|EGL_NOT_INITIALIZED|Could not create (an )?(EGL|GL)|Failed to create (an )?(EGL|GBM|GL)|Cannot get default EGL|Error 71 \(Protocol error\)|CRITICAL \*\*|-CRITICAL|Refused to (load|execute|connect|apply|evaluate|frame|create)|Content[- ]Security[- ]Policy|CONSOLE (JS )?ERROR|Uncaught|error while loading shared libraries|cannot open shared object'

# Known, tracked, harmless today; each entry names why. Everything else that matches MARKERS fails.
#  - Gtk-CRITICAL gtk_widget_get_scale_factor: printed once at startup by the tray/dock window creation (GTK3 called on a
#    widget that is already gone); the window and page work. Remove the entry when the app stops printing it.
KNOWN='gtk_widget_get_scale_factor'

out=launch-check-out size=1440x900 map_s=90 render_s=60 settle_s=8
envs=() ignores=()
while [ $# -gt 1 ]; do
  case "$1" in
    --out) out="$2"; shift 2 ;;
    --size) size="$2"; shift 2 ;;
    --map-timeout) map_s="$2"; shift 2 ;;
    --render-timeout) render_s="$2"; shift 2 ;;
    --settle) settle_s="$2"; shift 2 ;;
    --env) envs+=("$2"); shift 2 ;;
    --ignore) ignores+=("$2"); shift 2 ;;
    *) echo "unknown option $1" >&2; exit 2 ;;
  esac
done
bin="${1:?usage: linux-launch-check.sh [options] <binary|AppImage>}"
[ -x "$bin" ] || { echo "not executable: $bin" >&2; exit 2; }
bin="$(realpath "$bin")"
mkdir -p "$out"
out="$(realpath "$out")"
rm -f "$out"/screenshot.png "$out"/app.stderr.txt "$out"/app.log "$out"/sway.log "$out"/result.txt
headless_prepare "$size"

# Runs in the private D-Bus session (headless_session); its exit status is the script's.
set +e
headless_session bash -s "$here" "$work" "$bin" "$out" "$map_s" "$render_s" "$settle_s" "$MARKERS" "$KNOWN" "${#envs[@]}" "${envs[@]}" "${ignores[@]}" <<'INNER'
set -uo pipefail
here=$1; work=$2; bin=$3; out=$4; map_s=$5; render_s=$6; settle_s=$7; markers=$8; known=$9; shift 9; nenv=$1; shift
envs=("${@:1:nenv}") ignores=("${@:nenv+1}")
. "$here/linux-headless.sh"
headless_up || exit 3
export APPIMAGE_EXTRACT_AND_RUN=${APPIMAGE_EXTRACT_AND_RUN:-1}

fails=()
fail() { fails+=("$1: $2"); echo "FAIL $1: $2"; }

# pids under $1 (children, grandchildren, ...) with their command name.
descendants() {
  ps -eo pid=,ppid=,comm= | awk -v root="$1" '
    { pid[NR] = $1; par[NR] = $2; name[NR] = $3 }
    END { want[root] = 1
      do { grew = 0
        for (i = 1; i <= NR; i++) if (!(pid[i] in want) && (par[i] in want)) { want[pid[i]] = 1; grew = 1 } } while (grew)
      for (i = 1; i <= NR; i++) if (pid[i] in want && pid[i] != root) print pid[i], name[i] }'
}
# True once sway shows a window owned by $1 or by something it started (an AppImage's AppRun, a wrapper script).
window_mapped() {
  local p
  for p in $(swaymsg -t get_tree 2>/dev/null | grep -o '"pid": [0-9]*' | cut -d' ' -f2); do
    [ "$p" = "$1" ] && return 0
    descendants "$1" | awk -v p="$p" '$1 == p { f = 1 } END { exit !f }' && return 0
  done
  return 1
}
webprocess() { descendants "$1" | awk '$2 ~ /^WebKitWebProces/ { print $1 }' | sort -n | tr '\n' ' '; }

# Flat-colour test on a 1/4 scale grab: "<distinct colours> <share of the commonest colour, per mille>".
paint() {
  grim -s 0.25 -t ppm - 2>/dev/null | { read -r _; read -r _; read -r _; od -An -v -tx1 -w3; } | sort | uniq -c | sort -rn |
    awk '{ n += $1; d++; if (d == 1) top = $1 } END { if (n) printf "%d %d\n", d, top * 1000 / n; else print "0 1000" }'
}
painted() { read -r d top < <(paint); echo "colours=$d top=${top}permille" >&2; [ "$d" -ge 12 ] && [ "$top" -le 985 ]; }

start=$(date +%s)
env "${envs[@]}" setsid "$bin" </dev/null >"$work/app.out" 2>&1 & app=$!
alive() { kill -0 "$app" 2>/dev/null; }

# 1. the window maps
mapped=0
for _ in $(seq $((map_s * 10))); do
  window_mapped "$app" && { mapped=1; break; }
  alive || break
  sleep 0.1
done
if ! alive; then
  wait "$app"; code=$?
  fail app-exited "the app ended with status $code before its window mapped"
elif [ $mapped = 0 ]; then
  fail no-window "no window of pid $app mapped in ${map_s}s"
fi

# 2. the page paints something
wp=""
if [ ${#fails[@]} = 0 ]; then
  ok=0
  for _ in $(seq "$render_s"); do
    alive || break
    painted 2>/dev/null && { ok=1; break; }
    sleep 1
  done
  alive || { wait "$app"; code=$?; fail app-exited "the app ended with status $code while the page was loading"; }
  [ $ok = 1 ] || { alive && fail blank-page "nothing but a flat colour after ${render_s}s ($(painted 2>&1 || true))"; }
  wp="$(webprocess "$app")"
fi

# 3. it stays up and quiet
if [ ${#fails[@]} = 0 ]; then
  for _ in $(seq "$settle_s"); do alive || break; sleep 1; done
  if alive; then
    painted 2>/dev/null || fail blank-page "the page went flat after painting ($(painted 2>&1 || true))"
    now="$(webprocess "$app")"
    [ -n "$wp" ] || fail webprocess-died "no WebKitWebProcess under the app although the page painted"
    [ -z "$wp" ] || [ "$now" = "$wp" ] || fail webprocess-died "WebKitWebProcess was [$wp] at first paint and is [$now] now"
  else
    wait "$app"; code=$?
    fail app-exited "the app ended with status $code after painting"
  fi
fi

grim "$out/screenshot.png" 2>/dev/null || true
kill -- -"$app" 2>/dev/null || kill "$app" 2>/dev/null || true
sleep 0.5; kill -9 -- -"$app" 2>/dev/null || true
headless_down

# 4. what the app printed (stderr) and wrote to its own log
cp "$work/app.out" "$out/app.stderr.txt"
cp "$work/sway.log" "$out/sway.log"
cp "$HOME/.atlas/desktop/app.log" "$out/app.log" 2>/dev/null || : > "$out/app.log"
hits="$(grep -E "$markers" "$out/app.stderr.txt" | grep -Ev "$known" || true)"
for re in ${ignores[@]+"${ignores[@]}"}; do hits="$(grep -Ev "$re" <<<"$hits" || true)"; done
[ -z "$hits" ] || fail stderr-marker "$(head -5 <<<"$hits" | cut -c1-240 | paste -sd'|')"
# The startup line (`linux graphics: …`) is information; every other line is an incident.
bad="$(grep -Ev '^[0-9]+ linux graphics: ' "$out/app.log" || true)"
[ -z "$bad" ] || fail app-log-error "$(head -5 <<<"$bad" | cut -c1-240 | paste -sd'|')"

# 5. a dump of the app or WebKit since the start, when the machine keeps them
if command -v coredumpctl >/dev/null; then
  dumps="$(coredumpctl list --no-pager --no-legend --since "@$start" 2>/dev/null | grep -E 'atlas|WebKit' || true)"
  [ -z "$dumps" ] || fail coredump "$(head -3 <<<"$dumps" | paste -sd'|')"
fi

{ echo "binary: $bin"; echo "seconds: $(( $(date +%s) - start ))"
  if [ ${#fails[@]} = 0 ]; then echo "result: pass"; else echo "result: fail"; printf 'FAIL %s\n' "${fails[@]}"; fi; } > "$out/result.txt"
if [ ${#fails[@]} = 0 ]; then echo "PASS: window mapped, page painted, no crash, no error ($out)"; exit 0; fi
exit 1
INNER
code=$?
exit "$code"
