#!/usr/bin/env python3
# pty_probe.py <corpus.bin> [--bufsize 65536] [--opost] -> one JSON line: how a pty master hands out the bytes of a `cat` of the corpus.
# `cat file` writes into the slave; the master side reads with os.read(fd, bufsize). Reports reads, mean size and the share of
# reads at exactly 4095 bytes (a Linux tty buffer). --opost leaves output processing ON (the shell default: \n -> \r\n), which is
# what a real session has and what makes `seq`-shaped floods arrive in tiny pieces (docs/terminal-linux-design.md, 5.4).
import argparse, json, os, subprocess, sys, termios, time
ap = argparse.ArgumentParser(); ap.add_argument('corpus'); ap.add_argument('--bufsize', type=int, default=65536); ap.add_argument('--opost', action='store_true')
a = ap.parse_args()
m, s = os.openpty()
attr = termios.tcgetattr(s)
if not a.opost: attr[1] &= ~termios.OPOST
attr[3] &= ~(termios.ECHO | termios.ICANON); termios.tcsetattr(s, termios.TCSANOW, attr)
p = subprocess.Popen(['cat', a.corpus], stdout=s); os.close(s)
sizes = []; t0 = time.time(); cpu0 = time.process_time()
while True:
    try: b = os.read(m, a.bufsize)
    except OSError: break
    if not b: break
    sizes.append(len(b))
dt = time.time() - t0; total = sum(sizes)
print(json.dumps(dict(corpus=os.path.basename(a.corpus), opost=a.opost, bufsize=a.bufsize, bytes=total, reads=len(sizes), mean=round(total / len(sizes)),
    share_4095=round(sum(1 for x in sizes if x == 4095) / len(sizes), 3), mbps=round(total / 1e6 / dt, 1), load1=os.getloadavg()[0])))
