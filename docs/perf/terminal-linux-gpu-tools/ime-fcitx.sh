#!/bin/bash
# fcitx.sh start|stop: a private fcitx5 (pinyin) on the private bus/sway of up.sh; never touches the user's daemon or ~/.config.
D=$HOME/.cache/p4-ime; . $HOME/.cache/GpuTermPhase4/ime/env
H=$D/home
case $1 in
start)
  mkdir -p $H/.config/fcitx5 $H/.local/share $H/.cache
  cat > $H/.config/fcitx5/profile <<P
[Groups/0]
Name=Default
Default Layout=us
DefaultIM=pinyin

[Groups/0/Items/0]
Name=pinyin
Layout=

[GroupOrder]
0=Default
P
  setsid env -i PATH=$PATH HOME=$H XDG_CONFIG_HOME=$H/.config XDG_DATA_HOME=$H/.local/share XDG_CACHE_HOME=$H/.cache XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR WAYLAND_DISPLAY=$WAYLAND_DISPLAY DBUS_SESSION_BUS_ADDRESS=$DBUS_SESSION_BUS_ADDRESS LANG=en_US.UTF-8 fcitx5 --disable=notificationitem,kimpanel > $D/fcitx.log 2>&1 < /dev/null &
  echo $! > $D/fcitx.pid
  for i in $(seq 60); do env -i DBUS_SESSION_BUS_ADDRESS=$DBUS_SESSION_BUS_ADDRESS busctl --user list 2>/dev/null | grep -q org.fcitx.Fcitx5 && { echo "fcitx5 up"; exit 0; }; sleep 0.5; done
  echo "fcitx5 did not appear"; tail -5 $D/fcitx.log; exit 1;;
stop) [ -f $D/fcitx.pid ] && kill $(cat $D/fcitx.pid) 2>/dev/null; true;;
esac
