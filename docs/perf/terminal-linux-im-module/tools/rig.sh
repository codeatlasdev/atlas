#!/bin/bash
# rig.sh <gpu 1|0> [extra VAR=val ...]: private sway + private fcitx5 + app WITHOUT GTK_IM_MODULE in its launch env.
cd ~/.cache/LinuxImModule
GPU=${1:-1}; shift
export APP_BIN=$HOME/.cache/atlas-linuximmodule-desktop
./stopapp.sh; ./fcitx.sh stop; ./down.sh
[ -f wtype.pid ] && kill $(cat wtype.pid) 2>/dev/null
./up.sh gles2 1440x900 1 >/dev/null
. ./env
W="env -i PATH=$PATH XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR WAYLAND_DISPLAY=$WAYLAND_DISPLAY"
./fcitx.sh start || exit 1
if [ "${HOLD_LAYOUT:-}" = abnt2 ]; then setsid $W $HOME/.cache/LinuxImModule/vk/vk -L br -M abnt2 -d 7200000 Shift_R >/dev/null 2>&1 &
elif [ "${HOLD_LAYOUT:-}" = intl ]; then setsid $W $HOME/.cache/LinuxImModule/vk/vk -L us -V intl -d 7200000 Shift_R >/dev/null 2>&1 &
else setsid $W wtype -k Shift_R -s 7200000 >/dev/null 2>&1 &
fi
echo $! > wtype.pid
sleep 1
./app.sh ATLAS_GPU_TERMINAL=$GPU XMODIFIERS=@im=fcitx LANG=en_US.UTF-8 "$@"
for i in $(seq 90); do ./dbg.sh info >/dev/null 2>&1 && break; sleep 1; done
sleep 3; env -i PATH=$PATH SWAYSOCK=$SWAYSOCK swaymsg '[app_id=".*"] focus' >/dev/null
P=$HOME/.cache/LinuxImModule/proj
./dbg.sh eval 'const a = window.__atlas; await window.__TAURI_INTERNALS__.invoke("projects_add", { root: "'$P'" }); await a.store.refresh(); const id = a.store.projects.find((p) => p.root === "'$P'").id; a.shell.select(a.shell.landing(id)); await new Promise((r) => setTimeout(r, 2000)); return id' >/dev/null
./dbg.sh eval 'document.querySelectorAll("[aria-label=\"Dismiss checklist\"],[aria-label=\"Dismiss\"]").forEach(b=>b.click()); return 1' >/dev/null
