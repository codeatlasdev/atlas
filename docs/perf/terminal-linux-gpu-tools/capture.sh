#!/bin/sh
# capture.sh <scale> <WxH output> <outprefix>: fresh private sway at that scale, app with the GPU terminal, a terminal tab, an HTML menu over it, then resize and slide
S=$1; SZ=$2; P=$3
cd ~/.cache/GpuTermPhase3b
./cfg.sh gles2 s$S $SZ $S ATLAS_GPU_TERMINAL=1 >/dev/null 2>&1
. ./env
SW="env -i PATH=$PATH SWAYSOCK=$SWAYSOCK swaymsg"
./dbg.sh eval 'const a = window.__atlas; await window.__TAURI_INTERNALS__.invoke("projects_add", { root: "/tmp/g3b-proj" }); await a.store.refresh(); const id = a.store.projects.find((p) => p.root === "/tmp/g3b-proj").id; a.shell.select(a.shell.landing(id)); await new Promise((r) => setTimeout(r, 2000)); return id' >/dev/null
./dbg.sh eval 'const t = await window.__atlas.ws.newTerminal(window.__atlas.store.projects[0].id, { argv: ["sh","-c","ls -la --color=always /usr/lib | head -24; for i in 1 2 3; do printf \"\\033[1;3${i}mAtlas GPU terminal line $i\\033[0m\\n\"; done; printf \"\\033[48;2;40;80;160m  truecolor bg  \\033[0m \\xe2\\x9c\\x93 ligatures => != ===\\n\"; exec sleep 100000"], label: "demo" }); await new Promise((r) => setTimeout(r, 3000)); return t.id' >/dev/null
for t in 1 2 3; do ./dbg.sh eval 'document.querySelectorAll("[aria-label=\"Dismiss checklist\"],[aria-label=\"Dismiss\"]").forEach(b=>b.click()); return 1' >/dev/null; done
$SW '[app_id=".*"] move position 0 0' >/dev/null; sleep 1
./shot.sh $P-plain.png
./dbg.sh click 'button[aria-label="Project menu"]' >/dev/null; sleep 1
./shot.sh $P-menu.png
./dbg.sh eval 'document.dispatchEvent(new KeyboardEvent("keydown",{key:"Escape",bubbles:true})); return 1' >/dev/null
sleep 0.5
$SW '[app_id=".*"] resize set 900 560' >/dev/null; sleep 1.5
./shot.sh $P-resized.png
./dbg.sh click 'button[aria-label="Toggle left dock"]' >/dev/null; sleep 1.5
./shot.sh $P-dockhidden.png
./dbg.sh click 'button[aria-label="Toggle left dock"]' >/dev/null; sleep 0.12
./shot.sh $P-slide-mid.png
sleep 1.5
./shot.sh $P-dockshown.png
echo "$P captured: $(./dbg.sh info | tr -d '\n ')"; tail -3 /home/matheus/.cache/g3b-run/app.out
