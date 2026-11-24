#!/bin/sh
# fallback.sh <pixman|gles2> <label> [VAR=val ...]: app with the setting ON; opens a terminal; reports capability, whether xterm.js drew the text, the toast, and a screenshot
R=$1; L=$2; shift 2
cd ~/.cache/GpuTermPhase3b
./cfg.sh $R $L 1440x900 1 ATLAS_GPU_TERMINAL=1 "$@" >/dev/null 2>&1
./dbg.sh eval 'const a = window.__atlas; await window.__TAURI_INTERNALS__.invoke("projects_add", { root: "/tmp/g3b-proj" }); await a.store.refresh(); const id = a.store.projects.find((p) => p.root === "/tmp/g3b-proj").id; a.shell.select(a.shell.landing(id)); await new Promise((r) => setTimeout(r, 2000)); return id' >/dev/null
./dbg.sh eval 'await window.__atlas.ws.newTerminal(window.__atlas.store.projects[0].id, { argv: ["sh","-c","echo hello-from-'$L'; exec sleep 100000"], label: "fb" }); await new Promise((r) => setTimeout(r, 4000)); return 1' >/dev/null
echo "== $R $L $*"
./dbg.sh eval 'const c = await window.__TAURI_INTERNALS__.invoke("term_capability"); return {capability: c, xtermDrawn: !!document.querySelector(".xterm-screen"), rows: document.querySelector(".xterm-rows")?.innerText?.trim().slice(0,60), toast: [...document.querySelectorAll("[role=status],[role=alert],[data-sonner-toast],[class*=toast]")].map(e=>e.innerText).filter(Boolean).slice(0,2)}' | jq -c .
./shot.sh /tmp/g3b-fb-$L.png
grep -h "native terminal" /home/matheus/.cache/g3b-run/home/.atlas/desktop/app.log 2>/dev/null
