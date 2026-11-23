#!/bin/sh
# app.sh [VAR=val ...] : start the app as an agent instance in /tmp/aws (private display), pid in /tmp/aws/app.pid
W=/home/matheus/Projects/atlas-wt/perf/webkit-linux-study
mkdir -p /tmp/aws/home/.atlas/desktop /tmp/aws/hostd
. ~/.cache/webkit-study/env
setsid sh -c '"$@"; echo "exit=$?" > /tmp/aws/app.exit' x env -i PATH=$PATH SHELL=/bin/bash LANG=C.UTF-8 TERM=xterm-256color HOME=/tmp/aws/home ATLAS_HOSTD_DIR=/tmp/aws/hostd ATLAS_DEBUG_SOCK=/tmp/aws/debug.sock ATLAS_AGENT=1 ATLAS_BIN=$W/target/debug/atlas XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR DBUS_SESSION_BUS_ADDRESS=$DBUS_SESSION_BUS_ADDRESS SWAYSOCK=$SWAYSOCK WAYLAND_DISPLAY=$WAYLAND_DISPLAY XDG_SESSION_TYPE=wayland GDK_BACKEND=wayland NO_AT_BRIDGE=1 GTK_A11Y=none "$@" $W/target/debug/atlas-desktop > /tmp/aws/app.out 2>&1 < /dev/null &
echo $! > /tmp/aws/app.pid
