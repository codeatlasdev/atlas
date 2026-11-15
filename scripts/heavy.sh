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
# The label is the command itself (`bun run check`, not `bun`): the queue's run history is keyed by it, and
# `atlas_estimate` reads that history to tell one action's duration from another's. ATLAS_HEAVY_LABEL overrides it.
for atlas in "${ATLAS_BIN:-}" "$(command -v atlas 2>/dev/null)" "$HOME/.atlas/bin/atlas"; do
	if [ -n "$atlas" ] && [ -x "$atlas" ] && "$atlas" heavy status --json >/dev/null 2>&1; then
		label=${ATLAS_HEAVY_LABEL:-$(basename "$1")}
		if [ -z "${ATLAS_HEAVY_LABEL:-}" ] && [ $# -gt 1 ]; then
			rest=$(shift; printf '%s ' "$@")
			label=$(printf '%s %s' "$label" "$rest" | cut -c1-60 | sed 's/ *$//')
		fi
		exec "$atlas" run --heavy --label "$label" -- "$@"
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
# The lock is a pid file the shell itself creates with noclobber (`set -C`: open with O_CREAT|O_EXCL), so of two jobs
# exactly one wins on every OS and shell (dash, bash, Git Bash). Never an external tool's word: uutils `mkdir`
# (Ubuntu 26.04) reports success to both racers of a `mkdir`, which once let two heavy jobs run at once.
# A lock whose pid is gone is stale: it is removed, after checking it still names that pid (another job may have
# replaced it meanwhile).
take() {
	(set -C; echo $$ >"$lock") 2>/dev/null && return 0
	owner=$(cat "$lock" 2>/dev/null)
	if [ -n "$owner" ] && ! kill -0 "$owner" 2>/dev/null && [ "$(cat "$lock" 2>/dev/null)" = "$owner" ]; then
		rm -f "$lock"
	fi
	return 1
}
release() { rm -f "$lock"; }
while ! take; do
	[ $waited = 0 ] && echo "heavy: waiting for $(cat "$lock" 2>/dev/null) to finish ($*)" >&2
	waited=1
	sleep 2
done
trap release EXIT INT TERM
start=$(date +%s)
while :; do
	load=$(uptime 2>/dev/null | sed 's/.*load averages*: *//; s/,/ /g' | awk '{print int($2)}')
	free=$(memfree)
	why=
	# Memory first: it freezes a machine, a busy CPU only slows it. An unreadable figure (no `uptime` in Git Bash)
	# leaves its gate open.
	if [ "$minmem" -gt 0 ] 2>/dev/null && [ -n "$free" ] && [ "$free" -lt "$minmem" ]; then
		why="only $free% of memory free (< $minmem%)"
	elif [ -n "$load" ] && [ "$load" -ge "$max" ]; then
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
