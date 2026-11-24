#!/bin/sh
# app.sh [VAR=val ...] : start the release app as an agent instance in /home/matheus/.cache/g3b-run (private display), pid in /home/matheus/.cache/g3b-run/app.pid
T=/home/matheus/.cache/atlas-agents-target-gpu/release
mkdir -p /home/matheus/.cache/g3b-run/home/.atlas/desktop /home/matheus/.cache/g3b-run/hostd
. ~/.cache/GpuTermPhase3b/env
setsid sh -c '"$@"; echo "exit=$?" > /home/matheus/.cache/g3b-run/app.exit' x env -i PATH=$PATH SHELL=/bin/bash LANG=C.UTF-8 TERM=xterm-256color HOME=/home/matheus/.cache/g3b-run/home ATLAS_HOSTD_DIR=/home/matheus/.cache/g3b-run/hostd ATLAS_DEBUG_SOCK=/home/matheus/.cache/g3b-run/debug.sock ATLAS_AGENT=1 ATLAS_BIN=$T/atlas XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR DBUS_SESSION_BUS_ADDRESS=$DBUS_SESSION_BUS_ADDRESS SWAYSOCK=$SWAYSOCK WAYLAND_DISPLAY=$WAYLAND_DISPLAY XDG_SESSION_TYPE=wayland GDK_BACKEND=wayland NO_AT_BRIDGE=1 GTK_A11Y=none "$@" $T/atlas-desktop > /home/matheus/.cache/g3b-run/app.out 2>&1 < /dev/null &
echo $! > /home/matheus/.cache/g3b-run/app.pid
