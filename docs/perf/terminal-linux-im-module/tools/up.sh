#!/usr/bin/env bash
# up.sh <pixman|gles2> [WxH] [scale] -> private dbus (no servicedirs) + headless sway in /home/matheus/.cache/lim-ime; writes ./env
set -eu
R=${1:-gles2}; SIZE=${2:-1440x900}; SCALE=${3:-1}
D=/home/matheus/.cache/lim-ime; rm -rf $D; mkdir -p $D/run $D/home; chmod 700 $D/run
cat > $D/bus.conf <<C
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN" "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig><type>session</type><listen>unix:path=$D/run/bus</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*" eavesdrop="true"/><allow eavesdrop="true"/><allow own="*"/></policy></busconfig>
C
printf 'output HEADLESS-1 resolution %s scale %s\ndefault_border none\ndefault_floating_border none\nxwayland disable\nfor_window [app_id=".*"] floating enable\nexec_always true\n' "$SIZE" "$SCALE" > $D/sway.conf
CLEAN="env -i PATH=$PATH HOME=$D/home XDG_RUNTIME_DIR=$D/run"
setsid $CLEAN dbus-daemon --nofork --config-file $D/bus.conf > $D/dbus.log 2>&1 &
echo $! > $D/dbus.pid
for i in $(seq 100); do [ -S $D/run/bus ] && break; sleep 0.05; done
EXTRA=""
[ "$R" != pixman ] && EXTRA="WLR_RENDER_DRM_DEVICE=/dev/dri/renderD128"
setsid $CLEAN DBUS_SESSION_BUS_ADDRESS=unix:path=$D/run/bus XDG_SESSION_TYPE=wayland WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=$R $EXTRA sway -c $D/sway.conf > $D/sway.log 2>&1 &
echo $! > $D/sway.pid
for i in $(seq 100); do ls $D/run/sway-ipc.*.sock >/dev/null 2>&1 && break; sleep 0.05; done
SS=$(ls $D/run/sway-ipc.*.sock | head -1); WD=$(cd $D/run && ls | grep -m1 '^wayland-[0-9]*$')
cat > $HOME/.cache/LinuxImModule/env <<C
export XDG_RUNTIME_DIR=$D/run DBUS_SESSION_BUS_ADDRESS=unix:path=$D/run/bus SWAYSOCK=$SS WAYLAND_DISPLAY=$WD XDG_SESSION_TYPE=wayland GDK_BACKEND=wayland NO_AT_BRIDGE=1 GTK_A11Y=none
C
echo "sway $(cat $D/sway.pid) renderer=$R $SIZE scale=$SCALE"
