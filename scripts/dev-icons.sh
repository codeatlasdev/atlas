#!/bin/sh
# bun run icons:dev — regenerate crates/atlas-desktop/icons-dev/ (the orange "Atlas Dev" app icon) from the
# release icon, so a dev build is never mistaken for the installed Atlas in the Dock, taskbar or launcher.
# Rerun whenever crates/atlas-desktop/icons/icon.png changes. Needs ImageMagick (magick) and the Tauri CLI.
set -eu
cd "$(dirname "$0")/../crates/atlas-desktop"
tmp=$(mktemp -d)
magick icons/icon.png -modulate 100,140,185 "$tmp/src.png" # hue → orange, a bit more saturated
cargo tauri icon "$tmp/src.png" -o "$tmp/out" >/dev/null 2>&1
mkdir -p icons-dev
for f in 32x32.png 128x128.png 128x128@2x.png icon.png icon.icns icon.ico; do cp "$tmp/out/$f" icons-dev/; done
rm -rf "$tmp"
echo "icons-dev/ regenerated"
