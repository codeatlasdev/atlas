#!/bin/sh
# scripts/heavy.sh <cmd...> — run a heavy job (cargo build, the gate, a bench) as the only one of its kind on this machine.
# With an atlas that has the machine-wide queue (`atlas run --heavy`, probed with `atlas heavy status`), the job goes through
# it: at most N heavy jobs at once across every worktree and agent (default min(cores/2, GB of memory/8); ATLAS_HEAVY_SLOTS or heavy.slots
# in ~/.atlas/config.json), first come first served, under `nice`, after the load
# and memory pressure drop (ATLAS_HEAVY_MAXLOAD, ATLAS_HEAVY_MINMEM, ATLAS_HEAVY_WAIT), with CARGO_BUILD_JOBS capped.
# `atlas heavy status` shows who runs and who waits (crates/atlas-core/src/heavy.rs).
# An installed atlas that predates the queue, or none at all, gets the previous per-user implementation below: one job at a
# time per user (a lock), started when the 5-minute load is under ATLAS_HEAVY_MAXLOAD (default: 0.75 * the number of
# CPUs) and, on macOS and Linux, at least ATLAS_HEAVY_MINMEM percent of memory is free (default 20), under `nice`. Several
# worktrees or agents building at once froze the machine and made timing tests flaky (2026-10-15, PR #106; again 2026-11-13).
for atlas in "${ATLAS_BIN:-}" "$(command -v atlas 2>/dev/null)" "$HOME/.atlas/bin/atlas"; do
	if [ -n "$atlas" ] && [ -x "$atlas" ] && "$atlas" heavy status --json >/dev/null 2>&1; then
		exec "$atlas" run --heavy --label "$(basename "$1")" -- "$@"
	fi
done

# --- Fallback: per-user lock + load wait (an atlas without `run --heavy`) ---
set -u
lock=${ATLAS_HEAVY_LOCK:-${TMPDIR:-/tmp}/atlas-heavy-$(id -u).lock}
cores=$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 8)
default_max=$((cores * 3 / 4)) # 0.75 * cores, whole numbers (the load below is truncated too), at least 1
[ "$default_max" -ge 1 ] || default_max=1
max=${ATLAS_HEAVY_MAXLOAD:-$default_max}
minmem=${ATLAS_HEAVY_MINMEM:-20} # percent of memory that must be free; 0 turns the gate off
patience=${ATLAS_HEAVY_WAIT:-600} # seconds to wait for the load and memory; then run anyway, warned
waited=0
# Percent of memory free: macOS kern.memorystatus_level, Linux MemAvailable/MemTotal; empty when unknown (gate open).
# ATLAS_HEAVY_MEMFREE injects a reading (tests).
memfree() {
	if [ -n "${ATLAS_HEAVY_MEMFREE:-}" ]; then echo "$ATLAS_HEAVY_MEMFREE"
	elif [ -r /proc/meminfo ]; then awk '/^MemTotal:/{t=$2} /^MemAvailable:/{a=$2} END{if (t>0 && a!="") print int(a*100/t)}' /proc/meminfo
	else sysctl -n kern.memorystatus_level 2>/dev/null
	fi
}
# shlock (macOS, BSD) or a mkdir lock elsewhere; both treat a lock whose pid is gone as stale.
take() {
	if command -v shlock >/dev/null; then
		shlock -p $$ -f "$lock"
	elif mkdir "$lock.d" 2>/dev/null; then
		echo $$ >"$lock.d/pid"
	else
		owner=$(cat "$lock.d/pid" 2>/dev/null)
		if [ -n "$owner" ] && ! kill -0 "$owner" 2>/dev/null; then rm -rf "$lock.d"; fi
		return 1
	fi
}
release() { rm -f "$lock"; rm -rf "$lock.d"; }
while ! take; do
	[ $waited = 0 ] && echo "heavy: waiting for $(cat "$lock" "$lock.d/pid" 2>/dev/null | head -1) to finish ($*)" >&2
	waited=1
	sleep 2
done
trap release EXIT INT TERM
start=$(date +%s)
while :; do
	load=$(uptime | sed 's/.*load averages*: *//; s/,/ /g' | awk '{print int($2)}')
	free=$(memfree)
	why=
	# Memory first: it freezes a machine, a busy CPU only slows it. An unreadable figure leaves its gate open.
	if [ "$minmem" -gt 0 ] 2>/dev/null && [ -n "$free" ] && [ "$free" -lt "$minmem" ]; then
		why="only $free% of memory free (< $minmem%)"
	elif [ "$load" -ge "$max" ]; then
		why="load $load >= $max"
	fi
	[ -z "$why" ] && break
	if [ $(($(date +%s) - start)) -ge "$patience" ]; then
		echo "heavy: $why after ${patience}s; running anyway, timing-sensitive tests may flake" >&2
		break
	fi
	[ "$waited" != "$why" ] && echo "heavy: $why, waiting up to ${patience}s" >&2
	waited=$why
	sleep 5
done
# One job at a time here, so it may use every core; a cap the user set stays theirs.
: "${CARGO_BUILD_JOBS:=$cores}"
export CARGO_BUILD_JOBS
nice -n 10 "$@"
