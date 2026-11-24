#!/usr/bin/env python3
# prof.py <seconds> [out.jsonl label]: per-thread CPU ticks + timeslices (schedstat col3 = wakeups run) over a window for the instance's processes
import os,sys,time,json,glob
D='/home/matheus/.cache/gti-run'
secs=float(sys.argv[1]); out=sys.argv[2] if len(sys.argv)>2 else None; label=sys.argv[3] if len(sys.argv)>3 else ''
def pids():
    r={}
    for e in glob.glob('/proc/[0-9]*/environ'):
        try:
            if f'ATLAS_HOSTD_DIR={D}/hostd'.encode() in open(e,'rb').read().split(b'\0'):
                p=int(e.split('/')[2]); r[p]=1
        except Exception: pass
    # include children by ppid walk
    ch={}
    for d in glob.glob('/proc/[0-9]*/stat'):
        try:
            s=open(d).read(); pp=int(s[s.rindex(')')+2:].split()[1]); ch.setdefault(pp,[]).append(int(d.split('/')[2]))
        except Exception: pass
    st=list(r)
    while st:
        p=st.pop()
        for c in ch.get(p,[]):
            if c not in r: r[c]=1; st.append(c)
    return list(r)
def snap():
    s={}
    for p in pids():
        try: comm=open(f'/proc/{p}/comm').read().strip()
        except Exception: continue
        for t in glob.glob(f'/proc/{p}/task/*'):
            tid=int(t.split('/')[-1])
            try:
                name=open(t+'/comm').read().strip()
                ss=open(t+'/schedstat').read().split()
                st=open(t+'/stat').read(); f=st[st.rindex(')')+2:].split()
                ticks=int(f[11])+int(f[12])
                s[(p,tid)]=(comm,name,int(ss[0]),int(ss[2]),ticks)
            except Exception: pass
    return s
a=snap(); l0=open('/proc/loadavg').read().split()[0]; t0=time.time(); time.sleep(secs); b=snap(); dt=time.time()-t0; l1=open('/proc/loadavg').read().split()[0]
rows=[]
for k,v in b.items():
    o=a.get(k,(v[0],v[1],0,0,0))
    rows.append(dict(pid=k[0],tid=k[1],proc=v[0],thread=v[1],cpu_pct=(v[2]-o[2])/1e9/dt*100,wake_s=(v[3]-o[3])/dt,ticks=v[4]-o[4]))
tot=sum(r['cpu_pct'] for r in rows); wk=sum(r['wake_s'] for r in rows)
byp={}
for r in rows:
    d=byp.setdefault(r['proc'],[0,0]); d[0]+=r['cpu_pct']; d[1]+=r['wake_s']
rows.sort(key=lambda r:-r['wake_s'])
if out:
    open(out,'a').write(json.dumps(dict(label=label,seconds=dt,load_before=l0,load_after=l1,cpu_pct=round(tot,2),wake_s=round(wk,1),procs={k:[round(x,2) for x in v] for k,v in byp.items()},top=[{k:(round(v,2) if isinstance(v,float) else v) for k,v in r.items()} for r in rows if r['wake_s']>=0.05 or r['cpu_pct']>=0.02 or 'term' in r['thread'] or r['thread'].startswith('pty-')]))+'\n')
print(f'load {l0}->{l1} window {dt:.1f}s total cpu {tot:.2f}% wakeups/s {wk:.0f}')
for k,v in sorted(byp.items(),key=lambda x:-x[1][0]): print(f'  {k:18s} cpu {v[0]:.2f}%  wake/s {v[1]:.0f}')
print('  top threads by wakeups/s:')
for r in rows[:22]: print(f"    {r['proc'][:14]:14s} {r['thread'][:16]:16s} tid {r['tid']:8d} wake/s {r['wake_s']:6.1f} cpu {r['cpu_pct']:.3f}%")
