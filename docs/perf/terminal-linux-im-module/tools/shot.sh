#!/bin/sh
# shot.sh out.png [grim args]
. ~/.cache/LinuxImModule/env
o=$1; shift
env -i PATH=$PATH XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR WAYLAND_DISPLAY=$WAYLAND_DISPLAY grim "$@" "$o"
