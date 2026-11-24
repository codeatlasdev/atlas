#!/bin/sh
# ab.sh <gpu|xterm> <outdir> [n keys]: fresh instance; GPU memory with 0, 1 and 4 terminals; key-to-photon n keys (k2p);
# raw lines to <outdir>/<mode>-mem.jsonl and <outdir>/<mode>-k2p.jsonl
M=$1; O=$2; N=${3:-100}
G=1; [ "$M" = xterm ] && G=0
D=~/.cache/GpuTermPhase3b; cd $D
K=/home/matheus/.cache/atlas-agents-target-gpu/k2p-target/release/k2p
mkdir -p $O
./cfg.sh gles2 $M 1440x900 1 ATLAS_GPU_TERMINAL=$G >/dev/null 2>&1
. ./env
E="env -i PATH=$PATH XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR WAYLAND_DISPLAY=$WAYLAND_DISPLAY"
./dbg.sh eval 'const a = window.__atlas; await window.__TAURI_INTERNALS__.invoke("projects_add", { root: "/tmp/g3b-proj" }); await a.store.refresh(); const id = a.store.projects.find((p) => p.root === "/tmp/g3b-proj").id; a.shell.select(a.shell.landing(id)); await new Promise((r) => setTimeout(r, 2000)); return id' >/dev/null
./dbg.sh eval 'document.querySelectorAll("[aria-label=\"Dismiss checklist\"],[aria-label=\"Dismiss\"]").forEach(b=>b.click()); return 1' >/dev/null
sleep 8
mem() { printf '{"mode":"%s","terminals":%s,"load1":%s,"mem":%s}\n' $M $1 $(cut -d' ' -f1 /proc/loadavg) "$(./gpumem.sh)" >> $O/$M-mem.jsonl; }
newt() { ./dbg.sh eval 'const t = await window.__atlas.ws.newTerminal(window.__atlas.store.projects[0].id, { argv: ["env","PS1=$ ","bash","--norc","--noprofile","-i"], label: "t'$1'" }); await new Promise((r) => setTimeout(r, 2500)); return t.id' >/dev/null; }
rm -f $O/$M-mem.jsonl $O/$M-k2p.jsonl
mem 0
newt 1; sleep 3; mem 1
[ $G = 0 ] && ./dbg.sh eval 'const t=document.querySelector(".xterm-helper-textarea"); t&&t.focus(); return 1' >/dev/null
$E $K --region 345,122,300,30 --control >> $O/$M-k2p.jsonl 2>&1
printf '{"mode":"%s","load1":%s,"when":"%s"}\n' $M $(cut -d' ' -f1 /proc/loadavg) "$(date -Is)" >> $O/$M-k2p.jsonl
$E $K --region 345,122,300,30 --n $N >> $O/$M-k2p.jsonl 2>&1
for i in 2 3 4; do newt $i; done; sleep 3; mem 4
echo "$M done: $(tail -1 $O/$M-k2p.jsonl)"
