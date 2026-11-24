#!/usr/bin/env bash
D=/home/matheus/.cache/g3b-run
for f in sway dbus; do [ -f $D/$f.pid ] && kill $(cat $D/$f.pid) 2>/dev/null; done
sleep 0.5; rm -rf $D
