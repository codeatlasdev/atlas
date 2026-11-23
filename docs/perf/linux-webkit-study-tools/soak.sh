#!/bin/sh
# soak.sh <minutes> <out>: scripted use of the running instance (select worktrees, open+close a terminal printing 2 MB, chat stream bursts, dock toggles);
# samples PSS/RSS (smaps_rollup) of every process of the instance each 30 s -> out (tsv: sec role pid pssMb rssMb)
MIN=${1:-10}; OUT=${2:-out/soak.tsv}
cd ~/.cache/webkit-study; : > $OUT
D=./dbg.sh
sample(){ T=$1; for p in $(cat /tmp/aws/hostd/hostd.pid) $(pgrep -P $(./dbg.sh info | jq .pid) ) $(./dbg.sh info | jq .pid); do c=$(cat /proc/$p/comm 2>/dev/null) || continue; m=/proc/$p/smaps_rollup; [ -r $m ] || continue; printf "%s\t%s\t%s\t%s\t%s\t%s\n" $T $c $p $(awk '/^Pss:/{p=$2} /^Rss:/{r=$2} /^SwapPss:/{s=$2} END{printf "%.1f\t%.1f\t%.1f", p/1024, r/1024, s/1024}' $m) >> $OUT; done; }
ID=$($D eval "const a=window.__atlas; return a.store.projects.find(p=>p.root.endsWith('idle-project')).id" | tr -d '"')
$D eval "window.__atlas.shell.select(window.__atlas.shell.landing('$ID')); return 1" >/dev/null
T0=$(date +%s); S=0; END=$((MIN*60)); i=0
sample 0
while [ $S -lt $END ]; do
  i=$((i+1))
  $D eval "const a=window.__atlas; const id='$ID'; const t=await a.ws.newTerminal(id,{argv:['sh','-c','yes \"2026-11-22 12:00:00 INFO request handled path=/api/v1/items status=200\" | head -c 2M; sleep 3'],label:'soak'}); await new Promise(r=>setTimeout(r,5000)); a.ws.closeTab(t.id); return 1" >/dev/null 2>&1
  $D eval "const a=window.__atlas; const id='$ID'; const t=await a.ws.newTerminal(id,{argv:['python3','/home/matheus/Projects/atlas-wt/perf/webkit-linux-study/scripts/fixtures/linux-idle/fake-omp.py','--mode','rpc-ui','--stream-file','/tmp/aws/soak-stream'],chat:true,agent:'omp',label:'chat'}); await new Promise(r=>setTimeout(r,1000)); return t.id" >/dev/null 2>&1
  touch /tmp/aws/soak-stream; sleep 8; rm -f /tmp/aws/soak-stream; sleep 2
  $D eval "const a=window.__atlas; for (const t of a.ws.tabsOf('$ID')) if (t.kind==='terminal') a.ws.closeTab(t.id); return 1" >/dev/null 2>&1
  # sample every ~30 s
  sleep 5; S=$(( $(date +%s) - T0 )); sample $S
done
echo SOAK-DONE
