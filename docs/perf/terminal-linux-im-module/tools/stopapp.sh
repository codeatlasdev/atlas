#!/bin/sh
# kill only this instance: processes whose environment has ATLAS_HOSTD_DIR=/home/matheus/.cache/lim-ime/hostd
for p in $(grep -lz '^ATLAS_HOSTD_DIR=/home/matheus/.cache/lim-ime/hostd$' /proc/[0-9]*/environ 2>/dev/null | cut -d/ -f3); do [ "$p" = "$$" ] || kill $p 2>/dev/null; done
sleep 2
for p in $(grep -lz '^ATLAS_HOSTD_DIR=/home/matheus/.cache/lim-ime/hostd$' /proc/[0-9]*/environ 2>/dev/null | cut -d/ -f3); do [ "$p" = "$$" ] || kill -9 $p 2>/dev/null; done
rm -f /home/matheus/.cache/lim-ime/debug.sock
