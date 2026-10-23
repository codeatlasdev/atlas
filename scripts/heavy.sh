#!/bin/sh
# scripts/heavy.sh <cmd...> — run a heavy job (cargo build, the gate, a bench) as the only one on this machine.
# Jobs queue on one lock per user, start only when the 5-minute load is under ATLAS_HEAVY_MAXLOAD (default: the
# number of CPUs), and run under `nice`. Several worktrees or agents building at once froze the machine and made
# timing tests flaky (2026-10-15, PR #106). atlas.yaml actions go through it.
set -u
lock=${ATLAS_HEAVY_LOCK:-${TMPDIR:-/tmp}/atlas-heavy-$(id -u).lock}
max=${ATLAS_HEAVY_MAXLOAD:-$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 8)}
patience=${ATLAS_HEAVY_WAIT:-600} # seconds to wait for the load to drop; then run anyway, warned
waited=0
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
	[ "$load" -lt "$max" ] && break
	if [ $(($(date +%s) - start)) -ge "$patience" ]; then
		echo "heavy: load still $load after ${patience}s; running anyway, timing-sensitive tests may flake" >&2
		break
	fi
	[ $waited != 2 ] && echo "heavy: load $load >= $max, waiting up to ${patience}s" >&2
	waited=2
	sleep 5
done
nice -n 10 "$@"
