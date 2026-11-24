import json,sys,statistics as st
d=sys.argv[1]
def rows(f):
    try: return [json.loads(l) for l in open(f"{d}/{f}") if l.strip().startswith("{")]
    except FileNotFoundError: return []
cpu,mem,k=rows("cpu.jsonl"),rows("mem.jsonl"),rows("k2p.jsonl")
print("## CPU (valid windows only)")
for sc in ("terminal-idle","terminal-output"):
    for m in ("gpu","xterm"):
        r=[x for x in cpu if x["mode"]==m and x["label"].endswith(sc) and x["valid"]]
        bad=len([x for x in cpu if x["mode"]==m and x["label"].endswith(sc) and not x["valid"]])
        if not r: print(sc,m,"no valid window"); continue
        if sc=="terminal-output":
            # CPU per MB: passes from probe not stored; show total CPU and throughput if present
            print(sc,m,"n=%d"%len(r),"total%%=%s"%[x["totalCpuPct"] for x in r],"loads=%s"%[x["loadBefore"] for x in r],"discarded=%d"%bad, "passes=%s"%[x["live"]["after"]-x["live"]["before"] for x in r],"ms_per_MB=%s"%[round(x["totalCpuPct"]/100*x["seconds"]*1000/((x["live"]["after"]-x["live"]["before"])*20),1) for x in r])
        else:
            print(sc,m,"n=%d"%len(r),"total%%=%s"%[x["totalCpuPct"] for x in r],"app=%s"%[x["cpuPct"].get("app") for x in r],"web=%s"%[x["cpuPct"].get("webprocess") for x in r],"hostd=%s"%[x["cpuPct"].get("hostd") for x in r],"loads=%s"%[x["loadBefore"] for x in r],"discarded=%d"%bad)
print("## key to photon")
for m in ("gpu","xterm"):
    r=[x for x in k if x["mode"]==m and x["valid"]]
    print(m,"n=%d"%len(r),"p50=%s"%[x["p50"] for x in r],"p95=%s"%[x["p95"] for x in r],"loads=%s"%[x["loadBefore"] for x in r],"discarded=%d"%len([x for x in k if x["mode"]==m and not x["valid"]]))
print("## VRAM MiB (valid: load1<2)")
for m in ("gpu","xterm"):
    for n in (0,1,4):
        r=[x for x in mem if x["mode"]==m and x["terminals"]==n and x["load1"]<2]
        print(m,n,"vram=%s"%[round(x["mem"]["vram_kib"]/1024,1) for x in r],"gtt=%s"%[round(x["mem"]["gtt_kib"]/1024,1) for x in r])
