#!/bin/sh
# scripts/migration-e2e.sh — end-to-end check of the app-id migration (id_migrate.rs) with real WebKit.
# Two throwaway bundle ids stand in for the old and the new app, so nobody's real store is read or written:
#   1. build as $OLD, start an agent instance on its default store, write a localStorage key, stop
#   2. build as $NEW, start it with ATLAS_MIGRATE_E2E_FROM=$OLD: the app copies $OLD's store before its window
#   3. the key must read back; then both stores are deleted and the normal dev app is rebuilt
# Needs macOS and a debug build. Runs heavy builds through scripts/heavy.sh.
set -eu
cd "$(dirname "$0")/.."
OLD=br.com.codeatlas.atlas.e2eold
NEW=br.com.codeatlas.atlas.e2enew
WK="$HOME/Library/WebKit"
ATLAS=target/debug/atlas
cleanup() {
	scripts/desktop-agent.sh --stop --name mige2e >/dev/null 2>&1 || true
	rm -rf "${WK:?}/$OLD" "${WK:?}/$NEW"
}
trap cleanup EXIT INT TERM
build() { # $1 = bundle id
	scripts/heavy.sh sh -c "scripts/setup.sh --quiet && cd crates/atlas-desktop && cargo tauri build --debug --bundles app --ci --config tauri.dev.conf.json --config '{\"identifier\":\"$1\"}'" >/tmp/migration-e2e-build.log 2>&1 ||
		{ echo "build as $1 failed: /tmp/migration-e2e-build.log" >&2; exit 1; }
}
start() { # $1 = value for ATLAS_MIGRATE_E2E_FROM
	line=$(ATLAS_MIGRATE_E2E_FROM=$1 scripts/desktop-agent.sh --name mige2e 2>&1 | grep '^atlas-agent ready') ||
		{ echo "agent instance did not start" >&2; exit 1; }
	ATLAS_DEBUG_SOCK=${line##*ATLAS_DEBUG_SOCK=}
	export ATLAS_DEBUG_SOCK
}
cleanup
token="migrated-$(date +%s)"

echo "1/3 old app ($OLD) writes its preferences"
build "$OLD"
start "br.com.codeatlas.atlas.e2e-nothing" # forces the default store; nothing to migrate from
"$ATLAS" desktop debug eval "localStorage.setItem('ca-e2e-probe', '$token'); return localStorage.getItem('ca-e2e-probe')" >/dev/null
scripts/desktop-agent.sh --stop --name mige2e >/dev/null
[ -d "$WK/$OLD" ] || { echo "FAIL: the old app left no store in $WK/$OLD" >&2; exit 1; }

echo "2/3 new app ($NEW) starts and migrates"
build "$NEW"
[ ! -e "$WK/$NEW" ] || { echo "FAIL: $WK/$NEW existed before the first start" >&2; exit 1; }
start "$OLD"
got=$("$ATLAS" desktop debug eval "return localStorage.getItem('ca-e2e-probe')")
scripts/desktop-agent.sh --stop --name mige2e >/dev/null

echo "3/3 restore the normal dev app"
scripts/heavy.sh bun run desktop:build:app >/tmp/migration-e2e-build.log 2>&1
if [ "$got" = "\"$token\"" ]; then
	echo "PASS: the new app read the old app's localStorage after migrating ($got)"
else
	echo "FAIL: expected \"$token\", the new app read $got" >&2
	exit 1
fi
