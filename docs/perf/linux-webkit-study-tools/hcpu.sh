#!/bin/sh
# hcpu.sh <seconds> : CPU% of this instance's hostd, web process, app over N seconds
N=${1:-20}
pick(){ for p in $(grep -lz '^ATLAS_HOSTD_DIR=/tmp/aws/hostd$' /proc/[0-9]*/environ 2>/dev/null | cut -d/ -f3); do c=$(cat /proc/$p/comm 2>/dev/null); [ "$c" = "$1" ] && echo $p && return; done; }
H=$(pick atlas); W=$(pick WebKitWebProces); A=$(pick atlas-desktop)
t(){ awk '{print $14+$15}' /proc/$1/stat; }
h1=$(t $H); w1=$(t $W); a1=$(t $A); sleep $N; h2=$(t $H); w2=$(t $W); a2=$(t $A)
echo "hostd $(( (h2-h1)*100/N ))%x100  web $(( (w2-w1)*100/N ))%x100  app $(( (a2-a1)*100/N ))%x100  (units: 1/100 of a core... shown as hundredths of %)"
