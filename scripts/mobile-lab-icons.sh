#!/bin/sh
# bun run mobile:lab:icons — regenerate the orange "Atlas Lab" phone icons from the release ones, the way
# `bun run icons:dev` makes the orange Atlas Dev desktop icon (same hue shift), so the Lab app is never
# mistaken for the real one on the home screen. Rerun when apps/mobile's release icons change.
# iOS: ios/Runner/Assets.xcassets/AppIcon-Lab.appiconset   Android: android/app/src/lab/res/mipmap-*
# Needs ImageMagick (magick).
set -eu
cd "$(dirname "$0")/../apps/mobile"
tint() { magick "$1" -modulate 100,140,185 "$2"; }

ios=ios/Runner/Assets.xcassets
rm -rf "$ios/AppIcon-Lab.appiconset"
mkdir -p "$ios/AppIcon-Lab.appiconset"
for f in "$ios"/AppIcon.appiconset/*.png; do
	tint "$f" "$ios/AppIcon-Lab.appiconset/$(basename "$f")"
done
cp "$ios/AppIcon.appiconset/Contents.json" "$ios/AppIcon-Lab.appiconset/Contents.json"

res=android/app/src/main/res
rm -rf android/app/src/lab/res
for d in "$res"/mipmap-*dpi; do
	out=android/app/src/lab/res/$(basename "$d")
	mkdir -p "$out"
	for f in "$d"/*.png; do tint "$f" "$out/$(basename "$f")"; done
done
echo "Atlas Lab icons regenerated"
