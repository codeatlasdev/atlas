#!/usr/bin/env bash
# What a RUNNING Atlas Desktop costs in the states a user leaves it in, judged against the budgets of
# scripts/linux-idle-bench.ts (GATE_BUDGET): the CI half of #289 (docs/perf/linux-webkit-study.md). Same private display
# as scripts/linux-launch-check.sh (own sway on pixman = llvmpipe, own D-Bus, throwaway HOME; nothing on your screen), with
# the app started as an agent instance (ATLAS_AGENT=1 + its debug socket) so the bench can put it in each state.
#
#   scripts/linux-idle-gate.sh [--out DIR] [--env K=V]... [--seconds S] <atlas-desktop binary>
#
#   --out DIR      report.jsonl (one window per scenario, with the per-thread split), app.log, sway.log (default idle-gate-out/)
#   --env K=V      extra environment for the app (repeatable; CI passes WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1)
#   --seconds S    length of each CPU window (default 30)
#   ATLAS_BIN      the atlas CLI the app and the bench use (default target/debug/atlas)
#
# Exit codes: 0 every gated scenario is inside its budget; 1 over budget or a scenario did not run; 2 usage error or a
# missing tool; 3 the private display or the app could not start. Needs: sway, dbus-run-session, bun, python3.
# `wtype` (when present) gives the instance's seat a keyboard so the window has focus, as a user's does.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(dirname "$here")"
. "$here/linux-headless.sh"

out=idle-gate-out seconds=30 envs=()
while [ $# -gt 1 ]; do
  case "$1" in
    --out) out="$2"; shift 2 ;;
    --env) envs+=("$2"); shift 2 ;;
    --seconds) seconds="$2"; shift 2 ;;
    *) echo "unknown option $1" >&2; exit 2 ;;
  esac
done
bin="${1:?usage: linux-idle-gate.sh [options] <atlas-desktop binary>}"
[ -x "$bin" ] || { echo "not executable: $bin" >&2; exit 2; }
bin="$(realpath "$bin")"
atlas="$(realpath "${ATLAS_BIN:-$root/target/debug/atlas}")"
[ -x "$atlas" ] || { echo "no atlas CLI at $atlas (ATLAS_BIN)" >&2; exit 2; }
command -v bun >/dev/null || { echo "missing: bun" >&2; exit 2; }
mkdir -p "$out"
out="$(realpath "$out")"
rm -f "$out"/report.jsonl "$out"/app.log "$out"/sway.log
headless_prepare 1440x900

set +e
headless_session bash -s "$here" "$root" "$work" "$bin" "$atlas" "$out" "$seconds" "${#envs[@]}" "${envs[@]}" <<'INNER'
set -uo pipefail
here=$1; root=$2; work=$3; bin=$4; atlas=$5; out=$6; seconds=$7; shift 7; nenv=$1; shift
envs=("${@:1:nenv}")
. "$here/linux-headless.sh"
headless_up || exit 3

# The contract of an agent instance (crates/atlas-core/src/agent_display.rs): the debug socket sits next to `run/`.
sock="$work/debug.sock"
# (the bus address exactly as the app expects it: dbus-run-session appends a guid)
env -u DISPLAY ATLAS_AGENT=1 ATLAS_DEBUG_SOCK="$sock" ATLAS_BIN="$atlas" DBUS_SESSION_BUS_ADDRESS="unix:path=$work/run/bus" GDK_BACKEND=wayland "${envs[@]}" setsid "$bin" </dev/null >"$work/app.out" 2>&1 & app=$!
cleanup() {
  [ -n "${kb:-}" ] && kill "$kb" 2>/dev/null
  kill -- -"$app" 2>/dev/null || kill "$app" 2>/dev/null
  sleep 0.5; kill -9 -- -"$app" 2>/dev/null
  # hostd detaches itself from the app: it is found by the instance's own ATLAS_HOSTD_DIR in its environment
  for p in $(grep -lxz "ATLAS_HOSTD_DIR=$ATLAS_HOSTD_DIR" /proc/[0-9]*/environ 2>/dev/null | cut -d/ -f3); do [ "$p" = "$$" ] || kill "$p" 2>/dev/null; done
  headless_down
  cp "$work/sway.log" "$out/sway.log" 2>/dev/null
  cp "$HOME/.atlas/desktop/app.log" "$out/app.log" 2>/dev/null
  return 0
}
trap cleanup EXIT

for _ in $(seq 120); do
  kill -0 "$app" 2>/dev/null || { echo "FAIL: the app ended before its debug bridge answered"; tail -20 "$work/app.out"; exit 3; }
  [ -S "$sock" ] && "$atlas" desktop debug info >/dev/null 2>&1 && break
  sleep 1
done
export ATLAS_DEBUG_SOCK="$sock"
"$atlas" desktop debug info >/dev/null 2>&1 || { echo "FAIL: no debug bridge after 120 s"; tail -20 "$work/app.out"; exit 3; }

# A seat with no keyboard never gives a window focus (document.hasFocus() stays false: no caret blink, no focus-only paths).
# `wtype -k Shift_R` presses and releases one modifier once, then `-s` keeps its virtual keyboard alive: the seat has a keyboard
# and no key stays held (a held Shift would shift every key the app sees). Only after the display is proven the gate's own, and
# with the instance's variables spelled out on the command lines, never inherited by luck.
if command -v wtype >/dev/null; then
  headless_assert_private || exit 3
  priv=(XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" WAYLAND_DISPLAY="$WAYLAND_DISPLAY" SWAYSOCK="$SWAYSOCK")
  env -u ATLAS_HOSTD_DIR "${priv[@]}" wtype -k Shift_R -s 7200000 >/dev/null 2>&1 & kb=$!
  sleep 1
  env "${priv[@]}" swaymsg '[app_id=".*"] focus' >/dev/null 2>&1 || true
  sleep 2
  focus=$("$atlas" desktop debug eval 'return document.hasFocus()' 2>/dev/null || true)
  echo "page has focus: $focus"
fi

# The bench and wtype run without the instance's ATLAS_HOSTD_DIR: the bench finds the instance's processes by it, and must not count itself.
rc=0
for s in $(bun "$root/scripts/linux-idle-bench.ts" --gate-scenarios); do
  env -u ATLAS_HOSTD_DIR bun "$root/scripts/linux-idle-bench.ts" --sock "$sock" --atlas "$atlas" --label ci --max-load 0 --scenario "$s" --seconds "$seconds" --out "$out/report.jsonl" \
    | bun -e 'const r = JSON.parse(await Bun.stdin.text()); console.log(`${r.label}: ${r.totalCpuPct}% of a core over ${r.seconds.toFixed(0)} s ${JSON.stringify(r.cpuPct)}, PSS ${r.pssMb} MB`)' \
    || { echo "FAIL: scenario $s did not run"; rc=1; }
done
bun "$root/scripts/linux-idle-bench.ts" --judge "$out/report.jsonl" --gate || rc=1
exit $rc
INNER
code=$?
exit "$code"
