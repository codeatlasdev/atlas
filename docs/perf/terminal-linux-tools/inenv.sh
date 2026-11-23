#!/bin/sh
# inenv.sh [VAR=val ...] -- cmd : run in the private display with a clean env (no user DISPLAY, no XDG_CONFIG_HOME).
. ~/.cache/GpuTermLinux/env
exec env -i PATH=$PATH HOME=/tmp/gtl/home LANG=C.UTF-8 XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR DBUS_SESSION_BUS_ADDRESS=$DBUS_SESSION_BUS_ADDRESS SWAYSOCK=$SWAYSOCK WAYLAND_DISPLAY=$WAYLAND_DISPLAY XDG_SESSION_TYPE=wayland GDK_BACKEND=wayland NO_AT_BRIDGE=1 GTK_A11Y=none "$@"
