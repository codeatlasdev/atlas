#!/bin/sh
# gpumem.sh : GPU memory (KiB) of the running instance, from /proc/<pid>/fdinfo (amdgpu drm-total-vram/gtt), one row per DRM client
# (a client is counted once however many fds share it), for the app, its WebKit processes and hostd. Prints JSON.
APP=$(cat /home/matheus/.cache/g3b-run/app.pid); APP=$(pgrep -f '^/home/matheus/.cache/atlas-agents-target-gpu/release/atlas-desktop' | head -1)
PIDS="$APP $(pgrep -P $APP) $(for p in $(grep -lz '^ATLAS_HOSTD_DIR=/home/matheus/.cache/g3b-run/hostd$' /proc/[0-9]*/environ 2>/dev/null | cut -d/ -f3); do echo $p; done)"
for p in $PIDS; do
  for f in /proc/$p/fdinfo/*; do grep -q '^drm-driver' $f 2>/dev/null && awk -v pid=$p -v comm="$(cat /proc/$p/comm)" '
    /^drm-client-id/ {id=$2} /^drm-total-vram/ {v=$2} /^drm-total-gtt/ {g=$2} /^drm-shared-vram/ {sv=$2}
    END {print pid, comm, id, v, g, sv}' $f; done
done | sort -u -k1,1n -k3,3n | awk '
  {key=$1" "$3; if (!(key in seen)) {seen[key]=1; vram+=$4; gtt+=$5; rows=rows (rows?",":"") sprintf("{\"pid\":%s,\"comm\":\"%s\",\"client\":%s,\"vram_kib\":%s,\"gtt_kib\":%s}",$1,$2,$3,$4,$5)}}
  END {printf "{\"vram_kib\":%d,\"gtt_kib\":%d,\"clients\":[%s]}\n", vram, gtt, rows}'
