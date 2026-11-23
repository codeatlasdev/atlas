import json,glob,os
base={}
rows=[]
def L(f):
    d={}
    for l in open(f):
        r=json.loads(l); d[r['label'].split('/',1)[1]]=r
    return d
files=sorted(glob.glob('out/knob-*.jsonl'))
scs=['workspace','stack-running','chat-stream','terminal-idle','terminal-output','chat-idle']
print("| config | "+" | ".join(f"{s} CPU% (web/app/hostd)" for s in scs[:4])+" | terminal-output CPU% | web PSS MB after terminal-output | chat-idle web PSS MB |")
print("|"+"---|"*8)
def cell(r):
    if not r: return "–"
    c=r['cpuPct']; return f"**{r['totalCpuPct']}** ({c.get('webprocess',0)}/{c.get('app',0)}/{c.get('hostd',0)})"
def web(r):
    if not r: return "–"
    p=[x for x in r['processes'] if x['role']=='webprocess']; return f"{p[0]['pssMb']}" if p else "–"
for f in files:
    n=os.path.basename(f)[5:-6]; d=L(f)
    print(f"| {n} | "+" | ".join(cell(d.get(s)) for s in scs[:4])+f" | {cell(d.get('terminal-output'))} | {web(d.get('terminal-output'))} | {web(d.get('chat-idle'))} |")
