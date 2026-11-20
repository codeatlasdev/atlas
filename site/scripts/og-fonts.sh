#!/bin/sh
# Rebuilds the static TTFs the OG image renderer (satori) reads from the site's variable Archivo. satori cannot select axes of a
# variable font, and does not read woff2, so each instance the template uses is pinned once and committed (src/assets/fonts/).
# Needs fontTools with brotli: `pip install fonttools brotli`. Archivo is SIL OFL 1.1 (public/fonts/LICENSES.txt).
set -eu
cd "$(dirname "$0")/.."
src=public/fonts/archivo-var.woff2
out=src/assets/fonts
mkdir -p "$out"
# Titles: the same instance as `.display` on the site (weight 800, width 68%).
python3 -m fontTools varLib.instancer "$src" wght=800 wdth=68 -o "$out/archivo-display.ttf"
# Subtitle and small print: regular width.
python3 -m fontTools varLib.instancer "$src" wght=500 wdth=100 -o "$out/archivo-text.ttf"
