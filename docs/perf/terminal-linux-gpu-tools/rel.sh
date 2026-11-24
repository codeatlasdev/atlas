#!/bin/bash
# rel.sh <binary> <N> <out.jsonl> <label>: fresh instance (native terminal on), N terminal tabs with the LAST in front,
# GPU memory right after they are open (hidden ones still hold their renderers), again 45 s later (past the 30 s
# release delay), then the FIRST tab is shown: rebuild time from stderr and memory after. Every sample waits for 1-min load < 2.
export APP_BIN=$1; N=$2; OUT=$3; L=$4
. ~/.cache/GpuTermPhase4/lib.sh
cd $D
./stopapp.sh; ./down.sh
./cfg.sh gles2 $L 1440x900 1 ATLAS_GPU_TERMINAL=1 >/dev/null 2>&1
P=$D/proj
./dbg.sh eval 'const a = window.__atlas; await window.__TAURI_INTERNALS__.invoke("projects_add", { root: "'$P'" }); await a.store.refresh(); const id = a.store.projects.find((p) => p.root === "'$P'").id; a.shell.select(a.shell.landing(id)); await new Promise((r) => setTimeout(r, 2000)); return id' >/dev/null
./dbg.sh eval 'document.querySelectorAll("[aria-label=\"Dismiss checklist\"],[aria-label=\"Dismiss\"]").forEach(b=>b.click()); return 1' >/dev/null
sleep 5
sample() { # phase
  wait_load 900; l=$(load1)
  printf '{"label":"%s","n":%s,"phase":"%s","load1":%s,"mem":%s,"gpu_bytes_note":"fdinfo"}\n' $L $N $1 $l "$(./gpumem.sh)" >> $OUT
}
sample base
for i in $(seq $N); do ./dbg.sh eval 'const t = await window.__atlas.ws.newTerminal(window.__atlas.store.projects[0].id, { argv: ["env","PS1=$ ","bash","--norc","--noprofile","-i"], label: "t'$i'" }); await new Promise((r) => setTimeout(r, 1500)); return t.id' >/dev/null; done
sleep 4
sample open
sleep 45
sample hidden45s
FIRST=$(./dbg.sh eval 'const a=window.__atlas; const ts=a.ws.doc.tabs.filter(t=>t.kind==="terminal"); return ts[0].id' | tr -d '"')
S0=$(grep -c "renderer rebuilt on show" ~/.cache/p4-run/app.out)
./dbg.sh eval 'window.__atlas.ws.activate("'$FIRST'"); await new Promise(r=>setTimeout(r,2500)); return 1' >/dev/null
grep "renderer rebuilt on show" ~/.cache/p4-run/app.out | tail -n +$((S0+1)) | head -3 | sed "s/^/{\"label\":\"$L\",\"n\":$N,\"rebuild\":\"/; s/$/\"}/" >> $OUT
./shot.sh $D/rel-$L-$N.png
sample shown
./stopapp.sh; ./down.sh
