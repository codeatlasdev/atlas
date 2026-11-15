#! /usr/bin/env bash
# Atlas's AppImage environment, sourced by AppRun before the app starts. Replaces the hook
# linuxdeploy's GTK plugin generates (scripts/appimage-patch.sh copies it over), so every
# decision is here and reviewed (#266):
# - GDK_BACKEND: ATLAS_GDK_BACKEND wins, then the GDK_BACKEND the session or user set (Omarchy
#   sets wayland,x11,*), except that a Wayland request on NVIDIA's proprietary driver falls back
#   to X11 (tauri-apps/tauri#8541). Nothing set: X11 (XWayland on Wayland), until native Wayland
#   passes #266's hardware matrix. Only ATLAS_GDK_BACKEND forces Wayland on NVIDIA.
# - GTK_THEME is not forced to Adwaita: GTK follows the desktop. APPIMAGE_GTK_THEME still sets one.
# - The bundle's own GTK/GIO/pixbuf/schema paths, as linuxdeploy set them.

export APPDIR="${APPDIR:-"$(dirname "$(realpath "$0")")"}"

atlas_backend() {
    if [ -n "${ATLAS_GDK_BACKEND:-}" ]; then
        printf '%s' "$ATLAS_GDK_BACKEND"
    elif [ -n "${GDK_BACKEND:-}" ]; then
        case "$GDK_BACKEND" in
            wayland*) [ -e "${ATLAS_NVIDIA_PROC:-/proc/driver/nvidia/version}" ] && { printf 'x11'; return; } ;;
        esac
        printf '%s' "$GDK_BACKEND"
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
