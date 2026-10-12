#!/usr/bin/env bash
# Regenerate the Atlas Desktop icons from their SVG sources.
#   assets/app-icon.svg       -> assets/app-icon-1024.png -> crates/atlas-desktop/icons/* (cargo tauri icon)
#   assets/tray-template.svg  -> crates/atlas-desktop/icons/tray.png (44px = 22pt @2x, black + alpha only)
# The app icon follows Apple's Big Sur+ template (1024 canvas, 824 squircle, baked shadow).
# The tray icon MUST stay monochrome: main.rs sets icon_as_template(true) and macOS tints it by alpha.
set -euo pipefail
cd "$(dirname "$0")/.."

command -v rsvg-convert >/dev/null || { echo "rsvg-convert missing (brew install librsvg)" >&2; exit 1; }

icons=crates/atlas-desktop/icons

rsvg-convert -w 1024 -h 1024 assets/app-icon.svg -o assets/app-icon-1024.png
(cd crates/atlas-desktop && cargo tauri icon ../../assets/app-icon-1024.png)

# `cargo tauri icon` also writes mobile / Windows-store / extra sizes we do not bundle
# (tauri.conf.json bundle.icon: 32x32, 128x128, 128x128@2x, icon.icns).
rm -rf "$icons"/android "$icons"/ios "$icons"/Square*.png "$icons"/StoreLogo.png "$icons"/64x64.png

rsvg-convert -w 44 -h 44 assets/tray-template.svg -o "$icons/tray.png"
echo "icons updated in $icons"
