#!/bin/bash
cd ~/.cache/LinuxImModule; . ./env
: > proj/note.txt
./dbg.sh eval 'const a=window.__atlas; const id=a.ws.selected.id; a.ws.openTab(id,{kind:"editor",label:"note.txt",path:"note.txt"}); await a.agents.launch(id,"omp",{chat:true}); await new Promise(r=>setTimeout(r,5000)); document.querySelectorAll("[aria-label=\"Dismiss\"]").forEach(b=>b.click()); return 1' >/dev/null
