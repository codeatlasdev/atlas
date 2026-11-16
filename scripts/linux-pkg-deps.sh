#!/usr/bin/env bash
# Assert that a release package declares the runtime dependencies Atlas Desktop needs (#267, #265).
#
# Usage: scripts/linux-pkg-deps.sh <atlas_*.deb | atlas-*.rpm | codeatlas-atlas-bin-*.pkg.tar.zst>
#
# Reads what the package says with the format's own tool (dpkg-deb -f, rpm -qpR, pacman -Qip), splits it into
# package names (versions and alternatives dropped) and requires each one below by exact name: `gst-plugins-base`
# is not satisfied by `gst-plugins-base-libs`, which webkit2gtk already pulls in and which has no demuxers.
# Exit 0 when all are declared, 1 naming what is missing, 2 on a usage error. verify-linux runs it in each distro's
# container (where that distro's tool exists) before it installs the package.
set -euo pipefail

f=${1:-}
[[ -f "$f" ]] || { echo "usage: $0 <package file>" >&2; exit 2; }

case "$f" in
  *.deb)
    raw=$(dpkg-deb -f "$f" Depends)
    # name  regex of the package names that satisfy it (one per row)
    required=(
      "WebKitGTK 4.1|libwebkit2gtk-4\.1-0"
      "GStreamer base plugins|gstreamer1\.0-plugins-base"
      "GStreamer good plugins|gstreamer1\.0-plugins-good"
      "tray library (dlopen'd at start)|libayatana-appindicator3-1|libappindicator3-1"
      "ripgrep (project search)|ripgrep"
      "git|git"
    )
    ;;
  *.rpm)
    raw=$(rpm -qpR "$f")
    required=(
      "WebKitGTK 4.1|webkit2gtk4\.1"
      "GStreamer base plugins|gstreamer1-plugins-base"
      "GStreamer good plugins|gstreamer1-plugins-good"
      "tray library (dlopen'd at start)|libayatana-appindicator-gtk3|libappindicator-gtk3"
      "ripgrep (project search)|ripgrep"
      "git|git"
    )
    ;;
  *.pkg.tar.*)
    # Only the hard dependencies: `Optional Deps` is a different line.
    raw=$(pacman -Qip "$f" | grep "^Depends On" || true)
    required=(
      "WebKitGTK 4.1|webkit2gtk-4\.1"
      "GStreamer base plugins|gst-plugins-base"
      "GStreamer good plugins|gst-plugins-good"
      "tray library (dlopen'd at start)|libayatana-appindicator"
      "ripgrep (project search)|ripgrep"
      "git|git"
    )
    ;;
  *) echo "unknown package type: $f" >&2; exit 2 ;;
esac

# Package names only: no `(>= 1.0)` / `>= 1.0`, no `a | b` alternatives glued together, one per line.
names=$(sed -E 's/\([^)]*\)//g; s/[<>=]+[^ ,|]*//g' <<<"$raw" | tr ' ,|\t' '\n\n\n\n' | sed '/^$/d')

echo "$f declares:"
sed 's/^/  /' <<<"$names"

missing=0
for row in "${required[@]}"; do
  label=${row%%|*}
  pattern=${row#*|}
  if grep -qE "^($pattern)\$" <<<"$names"; then
    echo "ok       $label"
  else
    echo "MISSING  $label (one of: ${pattern//|/, })"
    missing=1
  fi
done
exit "$missing"
