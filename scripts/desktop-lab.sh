#!/bin/sh
# bun run desktop:lab [--name N] [--port P] [--stop]
# Atlas Lab: the desktop frontend in your own browser (Vite + HMR) against the real backend of an isolated,
# invisible agent instance. No Atlas window ever appears; open the printed URL when you want to look, click
# yourself, or leave it to the agent. See docs/contributing.md "Atlas Lab".
set -eu
cd "$(dirname "$0")/.."
name=lab
port=1421
stop=0
while [ $# -gt 0 ]; do
	case "$1" in
	--name) name=$2; shift ;;
	--port) port=$2; shift ;;
	--stop) stop=1 ;;
	*) echo "unknown option $1" >&2; exit 2 ;;
	esac
	shift
done
if [ "$stop" = 1 ]; then exec scripts/desktop-agent.sh --stop --name "$name"; fi

# Reuse a running instance of that name, else start one; either way read its socket from the ready line.
line=$(scripts/desktop-agent.sh --name "$name" 2>&1 | grep '^atlas-agent ready' || true)
sock=${line##*ATLAS_DEBUG_SOCK=}
if [ -z "$line" ]; then
	sock=$(ls -d /private/tmp/atlas-agent-"$name"/debug.sock /tmp/atlas-agent-"$name"/debug.sock 2>/dev/null | head -1)
	[ -S "$sock" ] || { echo "could not start agent instance $name" >&2; exit 1; }
fi
echo "Atlas Lab: http://localhost:$port  (backend: agent instance $name; stop with: bun run desktop:lab --stop --name $name)"
cd apps/desktop
ATLAS_DEBUG_SOCK=$sock ATLAS_LAB_INSTANCE=$name ATLAS_LAB_PORT=$port exec bunx vite dev
