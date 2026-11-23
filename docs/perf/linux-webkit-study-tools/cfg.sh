#!/bin/sh
# cfg.sh <renderer: pixman|gles2> <label> [VAR=val ...]: fresh private display + fresh HOME + app (env knobs), ready for scenarios
R=$1; L=$2; shift 2
cd ~/.cache/webkit-study

./stopapp.sh
./down.sh
./up.sh $R > /dev/null
./app.sh "$@"
for i in $(seq 60); do ./dbg.sh info >/dev/null 2>&1 && break; sleep 1; done
# a virtual keyboard on the PRIVATE seat only, so the window can take keyboard focus (a headless seat has none: document.hasFocus() is false forever)
pkill -f 'wtype -P Shift_R' 2>/dev/null
. ~/.cache/webkit-study/env
setsid env -i PATH=$PATH XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR WAYLAND_DISPLAY=$WAYLAND_DISPLAY wtype -P Shift_R -s 7200000 >/dev/null 2>&1 &
sleep 2
env -i PATH=$PATH SWAYSOCK=$SWAYSOCK swaymsg '[app_id=".*"] focus' >/dev/null
sleep 6
echo "$L up: $(./dbg.sh info | tr -d '\n ')"
