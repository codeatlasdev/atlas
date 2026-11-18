#!/usr/bin/env bash
# Patch an extracted AppImage dir (AppDir) after `cargo tauri build` (#266):
#   1. drop host-ABI libraries the bundle must never carry,
#   2. replace linuxdeploy's GTK hook with packaging/appimage/apprun-hook.sh,
#   3. fail if a forbidden library is still there,
#   4. drop the bundled GStreamer plugins whose libraries are neither in the bundle nor on the host list (libgstjack.so
#      needs libjack.so.0, which linuxdeploy never bundles: gst-plugin-scanner warned on every start of a host without
#      JACK), then fail if any is left,
#   5. fail if the bundled GStreamer has no plugins or no hook pointing at them (#265).
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
# GStreamer plugins: linuxdeploy copies every plugin of the build machine but only the libraries it does not exclude
# (libjack, libasound's JACK bridge, ...), so a plugin can need a library nobody ships, and gst-plugin-scanner prints
# `Failed to load plugin ... cannot open shared object file` on each start. A plugin is kept when every library it
# NEEDs is in the bundle or is a host library: glibc and the C++ runtime, zlib, libX11 (GTK needs it on any
# X11/XWayland session) and the host-ABI libraries stripped above (Mesa, Wayland, libdrm). Any other missing library
# drops the plugin; it is listed in the build log. Those plugins are not needed to play audio or video in WebKitGTK
# (the .deb/.rpm/Arch packages declare gst-plugins-base and -good only).
host_libs=(libc.so.* libm.so.* libdl.so.* libpthread.so.* librt.so.* libutil.so.* libresolv.so.* ld-linux*.so.*
  libgcc_s.so.* libstdc++.so.* libz.so.* libX11.so.* "${excluded[@]}")
readelf_bin="${ATLAS_READELF:-readelf}"
# The file names in the bundle, one per line (plugins are not libraries anyone NEEDs, so dropping one changes nothing here).
bundled="$(find "$appdir" \( -type f -o -type l \) | sed 's|.*/||')"

needed_libs() {
  local out
  out="$("$readelf_bin" -d "$1" 2>&1)" || { echo "cannot read the libraries of $1: $out" >&2; return 1; }
  printf '%s\n' "$out" | sed -n 's/.*(NEEDED).*\[\(.*\)\].*/\1/p'
}

# Prints the libraries plugin $1 needs that are nowhere.
missing_libs() {
  local lib pat ok libs
  libs="$(needed_libs "$1")" || return 1
  while IFS= read -r lib; do
    [ -n "$lib" ] || continue
    ok=0
    for pat in "${host_libs[@]}"; do
      # shellcheck disable=SC2254
      case "$lib" in $pat) ok=1; break ;; esac
    done
    [ "$ok" = 1 ] || grep -qxF -- "$lib" <<<"$bundled" || echo "$lib"
  done <<<"$libs"
}

while IFS= read -r plugin; do
  gone="$(missing_libs "$plugin")" || exit 1
  if [ -n "$gone" ]; then
    echo "dropping GStreamer plugin $(basename "$plugin"): needs ${gone//$'\n'/ } which is neither bundled nor a host library" >&2
    rm -f "$plugin"
  fi
done < <(find "$appdir" -path '*/gstreamer-1.0/*' -name '*.so' -type f | sort)
while IFS= read -r plugin; do
  gone="$(missing_libs "$plugin")" || exit 1
  [ -z "$gone" ] || { echo "GStreamer plugin $(basename "$plugin") needs ${gone//$'\n'/ }, which is not in the AppImage" >&2; exit 1; }
done < <(find "$appdir" -path '*/gstreamer-1.0/*' -name '*.so' -type f | sort)

# GStreamer: the AppImage's libgstreamer looks for plugins at the build distro's path, which other distros do not
# have, so `bundleMediaFramework` ships the plugins and a hook with GST_PLUGIN_SYSTEM_PATH_1_0. Without them every media
# element in the page logs `GStreamer element appsrc not found` (appsrc is in libgstapp.so) and playback fails.
if ! find "$appdir" -path '*/gstreamer-1.0/libgstapp.so' | grep -q .; then
  echo "no GStreamer plugins in the AppImage (libgstapp.so, which holds appsrc, is missing): media would fail" >&2
  exit 1
fi
[ -f "$hooks/linuxdeploy-plugin-gstreamer.sh" ] || { echo "linuxdeploy-plugin-gstreamer.sh is missing: the bundled plugins would not be found" >&2; exit 1; }
grep -q 'export GDK_BACKEND=x11' "$hooks"/*.sh && { echo "a hook still forces GDK_BACKEND=x11" >&2; exit 1; }
echo "appimage patched: $appdir"
