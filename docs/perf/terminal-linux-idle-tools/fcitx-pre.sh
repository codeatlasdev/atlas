#!/bin/bash
# fcitx-pre.sh <instance dir>: a private fcitx5 (pinyin) on the instance's private bus/sway (never the user's daemon or config)
D=$1; H=$D/fcitx-home; . $D/env
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
setsid env -i PATH=$PATH HOME=$H XDG_CONFIG_HOME=$H/.config XDG_DATA_HOME=$H/.local/share XDG_CACHE_HOME=$H/.cache XDG_RUNTIME_DIR=$D/run WAYLAND_DISPLAY=$WAYLAND_DISPLAY DBUS_SESSION_BUS_ADDRESS=unix:path=$D/run/bus LANG=en_US.UTF-8 fcitx5 --disable=notificationitem,kimpanel > $D/fcitx.log 2>&1 < /dev/null &
for i in $(seq 60); do env -i DBUS_SESSION_BUS_ADDRESS=unix:path=$D/run/bus busctl --user list 2>/dev/null | grep -q org.fcitx.Fcitx5 && exit 0; sleep 0.5; done
exit 1
