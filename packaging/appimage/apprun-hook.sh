#! /usr/bin/env bash
# Atlas's AppImage environment, sourced by AppRun before the app starts. Replaces the hook
# linuxdeploy's GTK plugin generates (scripts/appimage-patch.sh copies it over), so every
# decision is here and reviewed (#266):
# - GDK_BACKEND: the user's value wins. Otherwise native Wayland on a Wayland session (GTK
#   falls back to X11), except on NVIDIA's proprietary driver, where WebKitGTK on Wayland is
#   where tauri-apps/tauri#8541 crashes cluster: X11 (XWayland) there. ATLAS_GDK_BACKEND
#   overrides both.
# - GTK_THEME is not forced to Adwaita: GTK follows the desktop. APPIMAGE_GTK_THEME still sets one.
# - The bundle's own GTK/GIO/pixbuf/schema paths, as linuxdeploy set them.

export APPDIR="${APPDIR:-"$(dirname "$(realpath "$0")")"}"

atlas_backend() {
    if [ -n "${ATLAS_GDK_BACKEND:-}" ]; then
        printf '%s' "$ATLAS_GDK_BACKEND"
    elif [ -n "${GDK_BACKEND:-}" ]; then
        printf '%s' "$GDK_BACKEND"
    elif [ -n "${WAYLAND_DISPLAY:-}" ] || [ "${XDG_SESSION_TYPE:-}" = wayland ]; then
        if [ -e "${ATLAS_NVIDIA_PROC:-/proc/driver/nvidia/version}" ]; then
            printf 'x11'
        else
            printf 'wayland,x11'
        fi
    else
        printf 'x11'
    fi
}
GDK_BACKEND="$(atlas_backend)"
export GDK_BACKEND
unset -f atlas_backend

if [ -n "${APPIMAGE_GTK_THEME:-}" ]; then
    export GTK_THEME="$APPIMAGE_GTK_THEME"
fi

export GTK_DATA_PREFIX="$APPDIR"
export XDG_DATA_DIRS="$APPDIR/usr/share:/usr/share:${XDG_DATA_DIRS:-/usr/local/share}"
export GSETTINGS_SCHEMA_DIR="$APPDIR/usr/share/glib-2.0/schemas"
export GI_TYPELIB_PATH="$APPDIR/usr/lib/girepository-1.0"
export GTK_EXE_PREFIX="$APPDIR/usr"
export GTK_PATH="$APPDIR/usr/lib/gtk-3.0"
export GTK_IM_MODULE_FILE="$APPDIR/usr/lib/gtk-3.0/3.0.0/immodules.cache"
export GDK_PIXBUF_MODULE_FILE="$APPDIR/usr/lib/gdk-pixbuf-2.0/2.10.0/loaders.cache"
export GIO_EXTRA_MODULES="$APPDIR/usr/lib/x86_64-linux-gnu/gio/modules"
