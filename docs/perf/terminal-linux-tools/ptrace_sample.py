#!/usr/bin/env python3
# sample.py N cmd... : run cmd as a child, sample its RIP N times via ptrace attach/detach, symbolize with llvm-symbolizer (inline chains).
import ctypes, os, signal, subprocess, sys, time, collections, re
libc = ctypes.CDLL(None, use_errno=True)
PTRACE_ATTACH, PTRACE_DETACH, PTRACE_GETREGS = 16, 17, 12
class Regs(ctypes.Structure):
    _fields_ = [(n, ctypes.c_ulonglong) for n in "r15 r14 r13 r12 rbp rbx r11 r10 r9 r8 rax rcx rdx rsi rdi orig_rax rip cs eflags rsp ss fs_base gs_base ds es fs gs".split()]
n = int(sys.argv[1]); cmd = sys.argv[2:]
p = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env={**os.environ, "PASSES": os.environ.get("PASSES", "60")})
time.sleep(float(os.environ.get("WARM", "1.0")))
exe = os.path.realpath(cmd[0])
base = None
for l in open(f"/proc/{p.pid}/maps"):
    if exe in l and l.split()[2] == "00000000": base = int(l.split("-")[0], 16); break
addrs = collections.Counter()
for i in range(n):
    if p.poll() is not None: break
    if libc.ptrace(PTRACE_ATTACH, p.pid, 0, 0) != 0: break
    os.waitpid(p.pid, 0)
    r = Regs(); libc.ptrace(PTRACE_GETREGS, p.pid, 0, ctypes.byref(r))
    addrs[r.rip] += 1
    libc.ptrace(PTRACE_DETACH, p.pid, 0, 0)
    time.sleep(0.0007)
p.kill()
tot = sum(addrs.values())
sym = subprocess.run(["llvm-symbolizer", "-e", exe, "--inlining", "-f"] , input="\n".join(hex(a - base) for a in addrs), capture_output=True, text=True).stdout.split("\n\n")
fn = collections.Counter(); line = collections.Counter(); outer = collections.Counter()
for (a, c), s in zip(addrs.items(), sym):
    frames = [x for x in s.strip().split("\n")]
    pairs = list(zip(frames[0::2], frames[1::2]))
    if not pairs: continue
    fn[pairs[0][0]] += c; line[pairs[0][1].split("/")[-1]] += c
    outer[pairs[-1][0]] += c
print(f"samples {tot}")
print("-- innermost function"); [print(f"{c:5d} {100*c/tot:5.1f}%  {f[:110]}") for f, c in fn.most_common(14)]
print("-- innermost line"); [print(f"{c:5d} {100*c/tot:5.1f}%  {f[:110]}") for f, c in line.most_common(14)]
print("-- outermost (non-inlined) function"); [print(f"{c:5d} {100*c/tot:5.1f}%  {f[:110]}") for f, c in outer.most_common(10)]
