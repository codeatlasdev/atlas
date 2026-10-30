#!/usr/bin/env bash
# A disposable Linux "ship" for testing Atlas Bot end to end on this Mac (Docker, systemd, sshd).
#
#   scripts/test-ship.sh up      # build if needed, start it, print what to type in the onboarding
#   scripts/test-ship.sh reset   # throw it away and start a clean one (and forget it in Atlas Desktop)
#   scripts/test-ship.sh down    # stop and remove it
#
# The ship is reachable as `ubuntu@atlas-test-ship` on the default port: a managed block in ~/.ssh/config maps
# that name to the container, so the onboarding needs no port and both `ubuntu@` and `atlas-bot@` resolve.
# `ubuntu` has passwordless sudo and accepts this Mac's SSH key, like a fresh cloud server.
set -euo pipefail

NAME=atlas-test-ship
PORT=${ATLAS_TEST_SHIP_PORT:-2222}
DIR=$(cd "$(dirname "$0")" && pwd)/test-ship
SSH_CONFIG=$HOME/.ssh/config
BEGIN="# >>> atlas-test-ship (scripts/test-ship.sh) >>>"
END="# <<< atlas-test-ship <<<"

key() {
	for k in "$HOME/.ssh/id_ed25519.pub" "$HOME/.ssh/id_rsa.pub"; do
		[ -f "$k" ] && { cat "$k"; return; }
	done
	echo "no SSH public key in ~/.ssh (id_ed25519.pub or id_rsa.pub)" >&2
	exit 1
}

ssh_block() {
	mkdir -p "$HOME/.ssh"
	touch "$SSH_CONFIG"
	if ! grep -qF "$BEGIN" "$SSH_CONFIG"; then
		# Prepended: ssh uses the first match, and a later `Host *` must not override these.
		tmp=$(mktemp)
		{
			echo "$BEGIN"
			echo "Host $NAME"
			echo "    HostName 127.0.0.1"
			echo "    Port $PORT"
			echo "    StrictHostKeyChecking no"
			echo "    UserKnownHostsFile /dev/null"
			echo "    LogLevel ERROR"
			echo "$END"
			echo
			cat "$SSH_CONFIG"
		} >"$tmp"
		cat "$tmp" >"$SSH_CONFIG"
		rm -f "$tmp"
	fi
}
forget_in_desktop() {
	# Both lists: the Bots ship registry and Atlas's SSH hosts (which keeps reconnecting to a ship that is gone).
	python3 - "$HOME/.atlas/desktop" "$NAME" <<'PY'
import json, os, sys
base, name = sys.argv[1], sys.argv[2]
ours = lambda alias: alias.endswith("@" + name)
bots = os.path.join(base, "bot-hosts.json")
if os.path.exists(bots):
    hosts = json.load(open(bots))
    json.dump([h for h in hosts if not ours(h.get("host", ""))], open(bots, "w"), indent=2)
settings = os.path.join(base, "settings.json")
if os.path.exists(settings):
    s = json.load(open(settings))
    s["sshHosts"] = [h for h in s.get("sshHosts", []) if not ours(h.get("alias", ""))]
    json.dump(s, open(settings, "w"), indent=2)
PY
	rm -f "$HOME/.atlas/remote/"*"@$NAME.sock"
}

build() {
	mkdir -p "$DIR"
	key >"$DIR/key.pub"
	docker build -q --platform linux/amd64 -t "$NAME" "$DIR" >/dev/null
}

start() {
	# --restart unless-stopped: an OrbStack/Docker restart or a Mac sleep brings the ship back (it once sat stopped
	# and the app could only say "not connected"); `down` still stops it for good.
	docker run -d --restart unless-stopped --name "$NAME" --platform linux/amd64 --privileged --cgroupns=host \
		-v /sys/fs/cgroup:/sys/fs/cgroup:rw -p "127.0.0.1:$PORT:22" "$NAME" >/dev/null
	for _ in $(seq 1 30); do
		ssh -o BatchMode=yes -o ConnectTimeout=2 "ubuntu@$NAME" 'sudo -n true' 2>/dev/null && return 0
		sleep 1
	done
	echo "the ship did not answer over SSH in 30 s (docker logs $NAME)" >&2
	exit 1
}

remove() { docker rm -f "$NAME" >/dev/null 2>&1 || true; }

case "${1:-up}" in
up)
	ssh_block
	docker image inspect "$NAME" >/dev/null 2>&1 || build
	docker ps --format '{{.Names}}' | grep -qx "$NAME" || { remove; start; }
	;;
reset)
	ssh_block
	remove
	forget_in_desktop
	build
	start
	;;
down)
	remove
	forget_in_desktop
	echo "removed $NAME"
	exit 0
	;;
*)
	echo "usage: $0 up|reset|down" >&2
	exit 2
	;;
esac

echo "ship ready. In the onboarding type:"
echo "  SSH address: ubuntu@$NAME"
echo "  Port:        22 (leave the default)"
