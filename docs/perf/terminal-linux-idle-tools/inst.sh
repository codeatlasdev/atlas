#!/bin/bash
# inst.sh up <bin> [ENV=val...] | down | dbg ... : private headless sway (gles2, renderD128) + private dbus + agent instance. Nothing in caller's env.
D=/home/matheus/.cache/gti-run; B=/home/matheus/.cache/GpuTermIdle/bin
cmd=$1; shift
case $cmd in
up)
  BIN=$1; shift
  "$0" down
  mkdir -p $D/run $D/home/.atlas/desktop $D/hostd; chmod 700 $D/run
  cat > $D/bus.conf <<C
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN" "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig><type>session</type><listen>unix:path=$D/run/bus</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*" eavesdrop="true"/><allow eavesdrop="true"/><allow own="*"/></policy></busconfig>
C
  printf 'output HEADLESS-1 resolution 1440x900 scale 1\ndefault_border none\ndefault_floating_border none\nxwayland disable\nfor_window [app_id=".*"] floating enable\nexec_always true\n' > $D/sway.conf
  [ -n "$GTI_SETTINGS" ] && { mkdir -p $D/home/.config/gtk-3.0; cp $GTI_SETTINGS $D/home/.config/gtk-3.0/settings.ini; }
  CLEAN="env -i PATH=$PATH HOME=$D/home XDG_RUNTIME_DIR=$D/run"
  setsid $CLEAN dbus-daemon --nofork --config-file $D/bus.conf > $D/dbus.log 2>&1 &
  echo $! > $D/dbus.pid
  for i in $(seq 100); do [ -S $D/run/bus ] && break; sleep 0.05; done
  setsid $CLEAN DBUS_SESSION_BUS_ADDRESS=unix:path=$D/run/bus XDG_SESSION_TYPE=wayland WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=gles2 WLR_RENDER_DRM_DEVICE=/dev/dri/renderD128 sway -c $D/sway.conf > $D/sway.log 2>&1 &
  echo $! > $D/sway.pid
  for i in $(seq 100); do ls $D/run/sway-ipc.*.sock >/dev/null 2>&1 && break; sleep 0.05; done
  SS=$(ls $D/run/sway-ipc.*.sock | head -1); WD=$(cd $D/run && ls | grep -m1 '^wayland-[0-9]*$')
  echo "SWAYSOCK=$SS WAYLAND_DISPLAY=$WD" > $D/env
  [ -n "$GTI_PRE" ] && "$GTI_PRE" $D
  setsid env -i PATH=$PATH SHELL=/bin/bash LANG=C.UTF-8 TERM=xterm-256color HOME=$D/home ATLAS_HOSTD_DIR=$D/hostd ATLAS_DEBUG_SOCK=$D/debug.sock ATLAS_AGENT=1 ATLAS_BIN=${GTI_ATLAS:-$B/atlas} XDG_RUNTIME_DIR=$D/run DBUS_SESSION_BUS_ADDRESS=unix:path=$D/run/bus SWAYSOCK=$SS WAYLAND_DISPLAY=$WD XDG_SESSION_TYPE=wayland GDK_BACKEND=wayland NO_AT_BRIDGE=1 GTK_A11Y=none "$@" $BIN > $D/app.out 2>&1 < /dev/null &
  for i in $(seq 90); do "$0" dbg info >/dev/null 2>&1 && break; sleep 1; done
  setsid env -i PATH=$PATH XDG_RUNTIME_DIR=$D/run WAYLAND_DISPLAY=$WD wtype -P Shift_R -s 7200000 >/dev/null 2>&1 &
  sleep 2
  env -i PATH=$PATH SWAYSOCK=$SS swaymsg '[app_id=".*"] focus' >/dev/null
  sleep 3
  "$0" dbg info | tr -d '\n '; echo ;;
dbg) ATLAS_DEBUG_SOCK=$D/debug.sock exec $B/atlas desktop debug "$@" ;;
down)
  for p in $(grep -lz "^ATLAS_HOSTD_DIR=$D/hostd\$" /proc/[0-9]*/environ 2>/dev/null | cut -d/ -f3); do [ "$p" = "$$" ] || kill $p 2>/dev/null; done
  pkill -f "wtype -P Shift_R -s 7200000" 2>/dev/null
  sleep 1.5
  for p in $(grep -lz "^ATLAS_HOSTD_DIR=$D/hostd\$" /proc/[0-9]*/environ 2>/dev/null | cut -d/ -f3); do [ "$p" = "$$" ] || kill -9 $p 2>/dev/null; done
  for p in $(grep -lz "^XDG_RUNTIME_DIR=$D/run\$" /proc/[0-9]*/environ 2>/dev/null | cut -d/ -f3); do [ "$p" = "$$" ] || kill $p 2>/dev/null; done
  for f in sway dbus; do [ -f $D/$f.pid ] && kill $(cat $D/$f.pid) 2>/dev/null; done
  sleep 0.5; rm -rf $D ;;
esac
