#!/bin/bash
# run2.sh <imodule: fcitx|wayland>: private sway + private fcitx5 + app; a held virtual keyboard so the seat has wl_keyboard; logs composition events of the sink
cd ~/.cache/GpuTermPhase4/ime
IMM=${1:-fcitx}
export APP_BIN=$HOME/.cache/GpuTermPhase4/atlas-desktop.ime
./stopapp.sh; ./fcitx.sh stop; ./down.sh
[ -f wtype.pid ] && kill $(cat wtype.pid) 2>/dev/null
./up.sh gles2 1440x900 1 >/dev/null
. ./env
W="env -i PATH=$PATH XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR WAYLAND_DISPLAY=$WAYLAND_DISPLAY"
./fcitx.sh start || exit 1
setsid $W wtype -k Shift_R -s 7200000 >/dev/null 2>&1 &
echo $! > wtype.pid
sleep 1
./app.sh ATLAS_GPU_TERMINAL=1 GTK_IM_MODULE=$IMM XMODIFIERS=@im=fcitx LANG=en_US.UTF-8
for i in $(seq 90); do ./dbg.sh info >/dev/null 2>&1 && break; sleep 1; done
sleep 3; env -i PATH=$PATH SWAYSOCK=$SWAYSOCK swaymsg '[app_id=".*"] focus' >/dev/null
P=$HOME/.cache/GpuTermPhase4/proj
./dbg.sh eval 'const a = window.__atlas; await window.__TAURI_INTERNALS__.invoke("projects_add", { root: "'$P'" }); await a.store.refresh(); const id = a.store.projects.find((p) => p.root === "'$P'").id; a.shell.select(a.shell.landing(id)); await new Promise((r) => setTimeout(r, 2000)); return id' >/dev/null
./dbg.sh eval 'document.querySelectorAll("[aria-label=\"Dismiss checklist\"],[aria-label=\"Dismiss\"]").forEach(b=>b.click()); return 1' >/dev/null
./dbg.sh eval 'const t = await window.__atlas.ws.newTerminal(window.__atlas.store.projects[0].id, { argv: ["cat","-v"], label: "ime" }); await new Promise((r) => setTimeout(r, 3000)); const s=document.querySelector("textarea.sink"); window.__imelog=[]; if(s){["compositionstart","compositionupdate","compositionend","keydown","input","focus","blur"].forEach(n=>s.addEventListener(n,e=>window.__imelog.push(n+":"+(e.data??e.key??""))));} s&&s.focus(); return document.activeElement===s'
env -i PATH=$PATH SWAYSOCK=$SWAYSOCK swaymsg -t get_inputs | grep -E '"(name|type)"' | head
