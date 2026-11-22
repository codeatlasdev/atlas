#!/usr/bin/env python3
import json, sys, glob
def load(f):
    d={}
    for l in open(f):
        r=json.loads(l); d[r['label'].split('/',1)[1]]=r
    return d
def row(n,r):
    c=r['cpuPct']; pg=r.get('page',{}); st=r.get('pageState',{})
    procs={p['role']:p for p in r['processes']}
    web=procs.get('webprocess',{}).get('pssMb',0); app=procs.get('app',{}).get('pssMb',0); host=procs.get('hostd',{}).get('pssMb',0); net=procs.get('network',{}).get('pssMb',0)
    return f"| {n} | **{r['totalCpuPct']}** | {c.get('app',0)} | {c.get('webprocess',0)} | {c.get('hostd',0)} | {c.get('network',0)} | {r['pssMb']} ({web}/{app}/{host}/{net}) | {r['rssMb']} | {pg.get('fps','')} | {len(pg.get('long_tasks',[]))} | {st.get('animations','')} | {r['swapMb']} | {r.get('loadAvg1','')} |"
H="| scenario | CPU % total | app | web | hostd | network | PSS MB (web/app/hostd/net) | RSS MB | page rAF/s | long tasks | running CSS anims | swapped MB | load1 |\n|---|---|---|---|---|---|---|---|---|---|---|---|---|"
for f in sys.argv[1:]:
    print("\n###",f); print(H)
    for n,r in load(f).items(): print(row(n,r))
