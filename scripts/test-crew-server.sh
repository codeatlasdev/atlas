#!/usr/bin/env bash
# A disposable Linux crew server for testing Atlas Crew end to end on this Mac (Docker, systemd, sshd).
#
#   scripts/test-crew-server.sh up      # build if needed, start it, print what to type in the onboarding
#   scripts/test-crew-server.sh reset   # throw it away and start a clean one (and forget it in Atlas Desktop)
#   scripts/test-crew-server.sh down    # stop and remove it
#
# The server is reachable as `ubuntu@atlas-test-crew` on the default port: a managed block in ~/.ssh/config maps
# that name to the container, so the onboarding needs no port and `ubuntu@`, `atlas-crew@` (and, on a server
# installed before the rename, `atlas-bot@`) resolve.
# `ubuntu` has passwordless sudo and accepts this Mac's SSH key, like a fresh cloud server.
set -euo pipefail

NAME=atlas-test-crew
PORT=${ATLAS_TEST_CREW_PORT:-2222}
DIR=$(cd "$(dirname "$0")" && pwd)/test-crew-server
SSH_CONFIG=$HOME/.ssh/config
BEGIN="# >>> atlas-test-crew (scripts/test-crew-server.sh) >>>"
END="# <<< atlas-test-crew <<<"

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
	# Both lists: the crew-server registry and Atlas's SSH hosts (which keeps reconnecting to a server that is gone).
	# Only entries for this test server are removed; a file is rewritten only when one was there, and atomically
	# (temp file + rename), since these are the user's real settings. `bot-hosts.json` is the name before the rename.
	python3 - "$HOME/.atlas/desktop" "$NAME" <<'PY'
import json, os, sys
base, name = sys.argv[1], sys.argv[2]
ours = lambda alias: alias.endswith("@" + name)

def save(path, data):
    tmp = path + ".atlas-test-tmp"
    with open(tmp, "w") as f:
        json.dump(data, f, indent=2)
    os.replace(tmp, path)

for registry in ("crew-servers.json", "bot-hosts.json"):
    path = os.path.join(base, registry)
    if os.path.exists(path):
        hosts = json.load(open(path))
        kept = [h for h in hosts if not ours(h.get("host", ""))]
        if len(kept) != len(hosts):
            save(path, kept)
settings = os.path.join(base, "settings.json")
if os.path.exists(settings):
    s = json.load(open(settings))
    hosts = s.get("sshHosts", [])
    kept = [h for h in hosts if not ours(h.get("alias", ""))]
    if len(kept) != len(hosts):
        s["sshHosts"] = kept
        save(settings, s)
PY
	rm -f "$HOME/.atlas/remote/"*"@$NAME.sock"
}

build() {
	mkdir -p "$DIR"
	key >"$DIR/key.pub"
	docker build -q --platform linux/amd64 -t "$NAME" "$DIR" >/dev/null
}

start() {
	# --restart unless-stopped: an OrbStack/Docker restart or a Mac sleep brings the server back (it once sat stopped
	# and the app could only say "not connected"); `down` still stops it for good.
	docker run -d --restart unless-stopped --name "$NAME" --platform linux/amd64 --privileged --cgroupns=host \
		-v /sys/fs/cgroup:/sys/fs/cgroup:rw -p "127.0.0.1:$PORT:22" "$NAME" >/dev/null
	for _ in $(seq 1 30); do
		ssh -o BatchMode=yes -o ConnectTimeout=2 "ubuntu@$NAME" 'sudo -n true' 2>/dev/null && return 0
		sleep 1
	done
	echo "the test crew server did not answer over SSH in 30 s (docker logs $NAME)" >&2
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

echo "crew server ready. In the onboarding type:"
echo "  SSH address: ubuntu@$NAME"
echo "  Port:        22 (leave the default)"
