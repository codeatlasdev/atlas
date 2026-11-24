#!/bin/bash
# term.sh : project + one bash terminal, checklist dismissed (as GpuTermPhase3b/mode.sh)
I=/home/matheus/.cache/GpuTermIdle/inst.sh
mkdir -p /tmp/gti-proj
$I dbg eval 'const a = window.__atlas; await window.__TAURI_INTERNALS__.invoke("projects_add", { root: "/tmp/gti-proj" }); await a.store.refresh(); const id = a.store.projects.find((p) => p.root === "/tmp/gti-proj").id; a.shell.select(a.shell.landing(id)); await new Promise((r) => setTimeout(r, 2000)); return id' >/dev/null
$I dbg eval 'document.querySelectorAll("[aria-label=\"Dismiss checklist\"],[aria-label=\"Dismiss\"]").forEach(b=>b.click()); return 1' >/dev/null
$I dbg eval 'const t = await window.__atlas.ws.newTerminal(window.__atlas.store.projects[0].id, { argv: ["env","PS1=$ ","bash","--norc","--noprofile","-i"], label: "idle" }); await new Promise((r) => setTimeout(r, 3000)); return t.id' >/dev/null
