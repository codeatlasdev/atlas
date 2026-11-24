#!/bin/sh
# cfg.sh <pixman|gles2> <label> [WxH] [scale] [VAR=val ...]: fresh private display + fresh HOME + app, ready for scenarios
R=$1; L=$2; SZ=${3:-1440x900}; SC=${4:-1}; shift 4 2>/dev/null || shift $#
cd ~/.cache/GpuTermPhase3b
./stopapp.sh; ./down.sh
./up.sh $R $SZ $SC > /dev/null
./app.sh "$@"
for i in $(seq 90); do ./dbg.sh info >/dev/null 2>&1 && break; sleep 1; done
. ~/.cache/GpuTermPhase3b/env
pkill -f 'wtype -P Shift_R' 2>/dev/null
setsid env -i PATH=$PATH XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR WAYLAND_DISPLAY=$WAYLAND_DISPLAY wtype -P Shift_R -s 7200000 >/dev/null 2>&1 &
sleep 2
env -i PATH=$PATH SWAYSOCK=$SWAYSOCK swaymsg '[app_id=".*"] focus' >/dev/null
sleep 5
echo "$L up: $(./dbg.sh info | tr -d '\n ')"
