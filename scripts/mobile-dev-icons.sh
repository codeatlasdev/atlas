#!/bin/sh
# bun run mobile:dev:icons — regenerate the orange "Atlas Dev" phone icons from the store ones, the way
# `bun run icons:dev` makes the orange Atlas Dev desktop icon (same hue shift), so the dev app is never
# mistaken for the store app on the home screen. Rerun when apps/mobile's store icons change.
# iOS: ios/Runner/Assets.xcassets/AppIcon-Dev.appiconset   Android: android/app/src/dev/res/mipmap-*
# Needs ImageMagick (magick).
set -eu
cd "$(dirname "$0")/../apps/mobile"
tint() { magick "$1" -modulate 100,140,185 "$2"; }

ios=ios/Runner/Assets.xcassets
rm -rf "$ios/AppIcon-Dev.appiconset"
mkdir -p "$ios/AppIcon-Dev.appiconset"
for f in "$ios"/AppIcon.appiconset/*.png; do
	tint "$f" "$ios/AppIcon-Dev.appiconset/$(basename "$f")"
done
cp "$ios/AppIcon.appiconset/Contents.json" "$ios/AppIcon-Dev.appiconset/Contents.json"

res=android/app/src/main/res
rm -rf android/app/src/dev/res/mipmap-* # keeps res/values (the app name)
for d in "$res"/mipmap-*dpi; do
	out=android/app/src/dev/res/$(basename "$d")
	mkdir -p "$out"
	for f in "$d"/*.png; do tint "$f" "$out/$(basename "$f")"; done
done
echo "Atlas Dev icons regenerated"
