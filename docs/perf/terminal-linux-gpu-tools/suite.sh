#!/bin/bash
# suite.sh <gpu|xterm> <run#> <binary> <outdir> [tag]: one alternated run: idle 30 s, flood 30 s, GPU memory with 0/1/4 terminals, key-to-photon 100 keys.
# Every measured window starts at 1-minute load < 2 (waited for, never footnoted); a window that ends over 2 is discarded and retried (<= 3 tries).
M=$1; R=$2; export APP_BIN=$3; O=$4; TAG=${5:-$M}
. ~/.cache/GpuTermPhase4/lib.sh
cd $D; mkdir -p $O
G=1; [ $M = xterm ] && G=0
T=/home/matheus/Projects/atlas-wt/scratch/p4-bin
bench() { # scenario secs
  (cd $T && mise exec bun@latest -- bun scripts/linux-idle-bench.ts --sock $D/../p4-run/debug.sock --atlas $ATLAS --label $TAG --scenario $1 --settle 10 --seconds 30 --probe 0 --max-load 0 --out /dev/stdout 2>/dev/null | tail -1)
}
win() { # scenario: retries until load before (and, for idle, after) < 2
  for try in 1 2 3; do
    wait_load || { echo "no quiet window for $1" >&2; return 1; }
    b=$(load1); line=$(bench $1); a=$(load1)
    ok=1; lt2 $b || ok=0; [ $1 = terminal-idle ] && { lt2 $a || ok=0; }
    echo "$line" | jq -c --arg m $M --argjson r $R --arg b $b --arg a $a --argjson ok $ok '. + {mode:$m,run:$r,loadBefore:($b|tonumber),loadAfter:($a|tonumber),valid:($ok==1)}' >> $O/cpu.jsonl
    [ $ok = 1 ] && return 0
  done; return 1
}
rm -f ~/.cache/p4-run/debug.sock
./cfg.sh gles2 $M 1440x900 1 ATLAS_GPU_TERMINAL=$G >/dev/null 2>&1
win terminal-idle; win terminal-output
# memory + key to photon on a fresh instance
./cfg.sh gles2 $M 1440x900 1 ATLAS_GPU_TERMINAL=$G >/dev/null 2>&1
. ./env
E="env -i PATH=$PATH XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR WAYLAND_DISPLAY=$WAYLAND_DISPLAY"
P=$D/proj; mkdir -p $P; (cd $P && git init -q 2>/dev/null; true)
./dbg.sh eval 'const a = window.__atlas; await window.__TAURI_INTERNALS__.invoke("projects_add", { root: "'$P'" }); await a.store.refresh(); const id = a.store.projects.find((p) => p.root === "'$P'").id; a.shell.select(a.shell.landing(id)); await new Promise((r) => setTimeout(r, 2000)); return id' >/dev/null
./dbg.sh eval 'document.querySelectorAll("[aria-label=\"Dismiss checklist\"],[aria-label=\"Dismiss\"]").forEach(b=>b.click()); return 1' >/dev/null
sleep 8
mem() { wait_load 600; printf '{"mode":"%s","run":%s,"terminals":%s,"load1":%s,"mem":%s}\n' $M $R $1 $(load1) "$(./gpumem.sh)" >> $O/mem.jsonl; }
newt() { ./dbg.sh eval 'const t = await window.__atlas.ws.newTerminal(window.__atlas.store.projects[0].id, { argv: ["env","PS1=$ ","bash","--norc","--noprofile","-i"], label: "t'$1'" }); await new Promise((r) => setTimeout(r, 2500)); return t.id' >/dev/null; }
mem 0; newt 1; sleep 3; mem 1
focus() { ./dbg.sh eval 'document.querySelectorAll("[aria-label=\"Dismiss\"]").forEach(b=>b.click()); const t=document.querySelector(".xterm-helper-textarea, textarea.sink"); t&&t.focus(); return document.activeElement===t' >/dev/null; }
for try in 1 2 3; do
  wait_load || break
  focus
  b=$(load1); raw=$O/k2p-$M$R.raw; $E $K2P --region 345,122,300,30 --control > $raw 2>&1; $E $K2P --region 345,122,300,30 --n 100 >> $raw 2>&1; l2=$(load1)
  ok=1; lt2 $b || ok=0; lt2 $l2 || ok=0
  jq -c -s --arg m $M --argjson r $R --arg b $b --arg a $l2 --argjson ok $ok '[.[]|select(type=="object" and has("ms"))|.ms] as $v | ([.[]|select(type=="object" and .missed==true)]|length) as $miss | ($v|sort) as $s | {mode:$m,run:$r,n:($v|length),missed:$miss,p50:$s[(($s|length)*0.5|floor)],p95:$s[(($s|length)*0.95|floor)],max:$s[-1],min:$s[0],loadBefore:($b|tonumber),loadAfter:($a|tonumber),valid:($ok==1 and $miss==0)}' <(grep '^{' $raw) >> $O/k2p.jsonl
  jq -e 'select(.valid)' <(tail -1 $O/k2p.jsonl) >/dev/null && break
done
for i in 2 3 4; do newt $i; done; sleep 3; mem 4
./stopapp.sh; ./down.sh
echo "$M run $R done"
