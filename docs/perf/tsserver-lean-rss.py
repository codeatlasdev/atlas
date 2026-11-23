#!/usr/bin/env python3
"""lsp_exp.py ROOT FILE [--opts JSON] [--settle S] [--args ...]: start typescript-language-server like atlas-lsp does,
open FILE, wait, print the process-tree footprint (PSS+SwapPss, anon) per process, then shut down."""
import json, os, subprocess, sys, time, threading, glob

root, file = sys.argv[1], sys.argv[2]
opts = {"hostInfo": "atlas"}
settle = 25
extra = []
i = 3
while i < len(sys.argv):
    if sys.argv[i] == "--opts":
        opts.update(json.loads(sys.argv[i + 1])); i += 2
    elif sys.argv[i] == "--settle":
        settle = int(sys.argv[i + 1]); i += 2
    elif sys.argv[i] == "--args":
        extra = sys.argv[i + 1:]; break
    else:
        i += 1

cli = glob.glob(os.path.expanduser("~/.cache/lsp-tmp/atlas-lsp-smoke/lsp/typescript-language-server/*/node_modules/typescript-language-server/lib/cli.mjs"))[0]
node = glob.glob(os.path.expanduser("~/.local/share/mise/installs/node/latest/bin/node"))[0]
env = dict(os.environ)
p = subprocess.Popen([node, cli, "--stdio", *extra], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, cwd=root, env=env)
seq = [0]
diag = []

def send(m):
    b = json.dumps(m).encode()
    p.stdin.write(b"Content-Length: %d\r\n\r\n" % len(b) + b)
    p.stdin.flush()

def reader():
    f = p.stdout
    while True:
        h = f.readline()
        if not h:
            return
        if h.lower().startswith(b"content-length:"):
            n = int(h.split(b":")[1])
            f.readline()
            body = json.loads(f.read(n))
            if body.get("method") == "textDocument/publishDiagnostics":
                diag.append(len(body["params"]["diagnostics"]))
            if "id" in body and "method" in body:  # server request: answer null
                send({"jsonrpc": "2.0", "id": body["id"], "result": None})

threading.Thread(target=reader, daemon=True).start()
uri = "file://" + os.path.join(root, file)
send({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"processId": os.getpid(), "rootUri": "file://" + root, "capabilities": {"textDocument": {"publishDiagnostics": {}}}, "initializationOptions": opts}})
time.sleep(1)
send({"jsonrpc": "2.0", "method": "initialized", "params": {}})
send({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {"textDocument": {"uri": uri, "languageId": "typescript", "version": 1, "text": open(os.path.join(root, file)).read()}}})
time.sleep(settle)


def tree(pid):
    out = [pid]
    for c in glob.glob("/proc/[0-9]*/stat"):
        try:
            s = open(c).read()
            pp = int(s[s.rfind(")") + 2:].split()[1])
            if pp in out:
                cp = int(c.split("/")[2])
                if cp not in out:
                    out.append(cp)
        except Exception:
            pass
    return out


def roll(pid):
    d = {}
    for l in open(f"/proc/{pid}/smaps_rollup"):
        k, _, v = l.partition(":")
        if k in ("Pss", "SwapPss", "Anonymous", "Rss"):
            d[k] = int(v.split()[0]) / 1024
    return d


tot = 0
anon = 0
rows = []
for q in sorted(set(tree(p.pid))):
    try:
        r = roll(q)
        cmd = open(f"/proc/{q}/cmdline").read().replace("\0", " ")
        short = "syntax(partialSemantic)" if "partialSemantic" in cmd else "typingsInstaller" if "typingsInstaller" in cmd else "tsserver" if "tsserver.js" in cmd else "lang-server"
        rows.append((short, round(r["Pss"] + r["SwapPss"]), round(r["Anonymous"] + r["SwapPss"])))
        tot += r["Pss"] + r["SwapPss"]
        anon += r["Anonymous"] + r["SwapPss"]
    except Exception:
        pass
print(json.dumps({"opts": opts, "args": extra, "diagMsgs": len(diag), "procs": rows, "totalMB(pss+swap)": round(tot), "anonMB(anon+swap)": round(anon)}))
send({"jsonrpc": "2.0", "id": 99, "method": "shutdown", "params": None})
time.sleep(0.5)
send({"jsonrpc": "2.0", "method": "exit", "params": None})
time.sleep(1)
p.kill()
