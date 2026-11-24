#!/bin/sh
# mode.sh <gpu|xterm> [scale] [WxH]: fresh private sway + app, a project, ONE bash terminal tab with a short prompt, checklist dismissed
M=$1; SC=${2:-1}; SZ=${3:-1440x900}
G=1; [ "$M" = xterm ] && G=0
cd ~/.cache/GpuTermPhase3b
./cfg.sh gles2 $M $SZ $SC ATLAS_GPU_TERMINAL=$G >/dev/null 2>&1
. ./env
./dbg.sh eval 'const a = window.__atlas; await window.__TAURI_INTERNALS__.invoke("projects_add", { root: "/tmp/g3b-proj" }); await a.store.refresh(); const id = a.store.projects.find((p) => p.root === "/tmp/g3b-proj").id; a.shell.select(a.shell.landing(id)); await new Promise((r) => setTimeout(r, 2000)); return id' >/dev/null
./dbg.sh eval 'document.querySelectorAll("[aria-label=\"Dismiss checklist\"],[aria-label=\"Dismiss\"]").forEach(b=>b.click()); return 1' >/dev/null
./dbg.sh eval 'const t = await window.__atlas.ws.newTerminal(window.__atlas.store.projects[0].id, { argv: ["env","PS1=$ ","bash","--norc","--noprofile","-i"], label: "k2p" }); await new Promise((r) => setTimeout(r, 3000)); return t.id' >/dev/null
echo "$M up"
