#!/usr/bin/env python3
# floor.py <label> <html-file> [wk.py args...] : run wk.py (plain GTK3 + WebKitGTK 4.1, no Tauri) in the private display,
# settle 10 s, measure 30 s: CPU % per process role (from /proc stat deltas), RSS, PSS. One JSON line to out/floor.jsonl
import subprocess, sys, time, os, json, re
label, html, *extra = sys.argv[1:]
env = dict(os.environ)
p = subprocess.Popen(["/home/matheus/.cache/webkit-study/inenv.sh", *[e for e in extra if "=" in e and not e.startswith("--")], "python3", "-W", "ignore", "/home/matheus/.cache/webkit-study/wk.py", "--html", html, *[e for e in extra if not ("=" in e and not e.startswith("--"))]], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
def tree(root):
    kids = {}
    for d in os.listdir("/proc"):
        if d.isdigit():
            try:
                s = open(f"/proc/{d}/stat").read(); pp = int(s[s.rindex(")")+2:].split()[1]); kids.setdefault(pp, []).append(int(d))
            except Exception: pass
    out = [root]; i = 0
    while i < len(out): out += kids.get(out[i], []); i += 1
    return out
def snap(pids):
    r = {}
    for pid in pids:
        try:
            s = open(f"/proc/{pid}/stat").read(); c = s[s.index("(")+1:s.rindex(")")]; f = s[s.rindex(")")+2:].split()
            m = open(f"/proc/{pid}/smaps_rollup").read()
            kb = lambda k: int(re.search(rf"^{k}:\s+(\d+) kB", m, re.M).group(1))
            role = {"WebKitWebProces": "webprocess", "WebKitNetworkPr": "network", "WebKitGPUProces": "gpuprocess"}.get(c[:15], "ui" if "python" in c else c)
            r[pid] = (role, int(f[11]) + int(f[12]), kb("Rss"), kb("Pss"), kb("SwapPss"))
        except Exception: pass
    return r
time.sleep(10 + 8)
pids = tree(p.pid); a = snap(pids); t0 = time.time(); time.sleep(30); b = snap(tree(p.pid)); dt = time.time() - t0
cpu = {}; rss = pss = swap = 0; procs = []
for pid, (role, t, r, ps, sw) in b.items():
    c = (t - a.get(pid, (0, t))[1]) / 100 / dt * 100
    cpu[role] = round(cpu.get(role, 0) + c, 1); rss += r; pss += ps; swap += sw
    procs.append({"role": role, "pid": pid, "cpuPct": round(c, 1), "pssMb": round(ps/1024, 1), "rssMb": round(r/1024, 1)})
res = {"label": label, "totalCpuPct": round(sum(cpu.values()), 1), "cpuPct": cpu, "pssMb": round(pss/1024, 1), "rssMb": round(rss/1024, 1), "swapMb": round(swap/1024, 1), "load": os.getloadavg()[0], "processes": procs}
print(json.dumps(res)); open(os.path.expanduser("~/.cache/webkit-study/out/floor.jsonl"), "a").write(json.dumps(res) + "\n")
os.killpg(p.pid, 15)
