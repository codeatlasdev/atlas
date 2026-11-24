import json,collections
rows=[json.loads(l) for l in open('idle-final.jsonl')]
g=collections.defaultdict(list)
for r in rows:
    name,focus=r['label'].split('|'); cell=name.rsplit('-',1)[0]
    lb=max(float(r['load_before']),float(r['load_after']))
    valid = 'valid' if float(r['load_before'])<2 and float(r['load_after'])<2 else ('discarded' if lb>=4 else 'marked')
    g[cell].append((name,r['cpu_pct'],r['wake_s'],r['load_before'],r['load_after'],valid,{k:v for k,v in r['procs'].items() if k!='bash'}))
for c,v in g.items():
    print(c)
    for x in v: print('  ',x[0],'cpu',x[1],'wake/s',x[2],'load',x[3],'->',x[4],x[5],{k:tuple(a) for k,a in x[6].items() if a[0]>0.01 or a[1]>0.5})
