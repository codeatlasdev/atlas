#!/bin/sh
# bun run desktop:agent [--name N] [--scratch] [--stop]
# An isolated Atlas Desktop for an agent: own HOME, hostd and debug socket, never on the user's desktop (macOS: virtual
# display, no Dock icon, no focus; Linux: its own headless sway and D-Bus session bus). Builds the debug app and the CLI
# when missing, then starts it through `atlas desktop --agent`, which prints `ATLAS_DEBUG_SOCK=<socket>`. See AGENTS.md
# "Debugging Atlas Desktop".
set -eu
cd "$(dirname "$0")/.."
ATLAS=target/debug/atlas
if [ "$(uname -s)" = Linux ]; then
	# The bare debug binary with the bundled frontend (`bun run desktop:build:debug`). A plain `cargo build` makes the
	# same file but loads the Vite dev URL: the stamp says the binary was built bundled, and is older than the binary
	# when something rebuilt it since.
	APP=target/debug/atlas-desktop
	STAMP=target/debug/.atlas-desktop-bundled
	build="bun run desktop:build:debug"
else
	APP=target/debug/bundle/macos/Atlas.app/Contents/MacOS/atlas-desktop
	STAMP=$APP
	build="bun run desktop:build:app"
fi
stopping=0
for a in "$@"; do [ "$a" = "--stop" ] && stopping=1; done
if [ "$stopping" = 0 ]; then
	if [ ! -x "$ATLAS" ]; then
		cargo build -q -p atlas-cli
		codesign -f -s - "$ATLAS" 2>/dev/null || true
	fi
	if [ ! -f "$APP" ] || [ "$APP" -nt "$STAMP" ]; then
		echo "building the debug app (first time only; rebuild with: $build)" >&2
		# macOS: app bundle only, a DMG makes tauri's bundle_dmg.sh mount the image and open Finder on the user's screen
		$build >&2
	fi
fi
exec "$ATLAS" desktop --agent "$@"
