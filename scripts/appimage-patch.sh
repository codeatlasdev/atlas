#!/usr/bin/env bash
# Patch an extracted AppImage dir (AppDir) after `cargo tauri build` (#266):
#   1. drop host-ABI libraries the bundle must never carry,
#   2. replace linuxdeploy's GTK hook with packaging/appimage/apprun-hook.sh,
#   3. fail if a forbidden library is still there.
# usage: scripts/appimage-patch.sh <AppDir>
# Repacking is the caller's job (release.yml runs appimagetool).
set -euo pipefail

appdir="${1:?usage: appimage-patch.sh <AppDir>}"
here="$(cd "$(dirname "$0")/.." && pwd)"

# Host-ABI libraries: they must match the host's Mesa/compositor. The build distro's copies made
# every WebKitWebProcess abort with "Could not create default EGL display: EGL_BAD_PARAMETER" on
# Arch (Mesa 26), a blank window (#265).
excluded=(libwayland-client.so* libwayland-cursor.so* libwayland-egl.so* libwayland-server.so*
  libEGL.so* libGL.so* libGLX.so* libGLdispatch.so* libgbm.so* libdrm.so*)

for pat in "${excluded[@]}"; do
  find "$appdir" -name "$pat" \( -type f -o -type l \) -delete
done

hooks="$appdir/apprun-hooks"
[ -d "$hooks" ] || { echo "no apprun-hooks in $appdir" >&2; exit 1; }
rm -f "$hooks"/linuxdeploy-plugin-gtk.sh
cp "$here/packaging/appimage/apprun-hook.sh" "$hooks/linuxdeploy-plugin-gtk.sh"

left=""
for pat in "${excluded[@]}"; do
  left+="$(find "$appdir" -name "$pat")"
done
if [ -n "$left" ]; then
  echo "forbidden host libraries still in the AppImage:" >&2
  echo "$left" >&2
  exit 1
fi
grep -q 'export GDK_BACKEND=x11' "$hooks"/*.sh && { echo "a hook still forces GDK_BACKEND=x11" >&2; exit 1; }
echo "appimage patched: $appdir"
