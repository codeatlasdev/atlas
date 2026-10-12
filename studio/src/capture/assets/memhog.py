#!/usr/bin/env python3
"""Stand-in for a database/search service in the demo stack: holds a wavy amount of memory and burns a little CPU so
the panel's sparklines and resource charts have something to draw.

usage: memhog.py <MB> <cpu 0..1> <seed>
"""
import math
import random
import sys
import time

mb = float(sys.argv[1])
cpu = float(sys.argv[2])
rng = random.Random(int(sys.argv[3]))
buf: list[bytearray] = []
t0 = time.time()
while True:
    t = time.time() - t0
    target = int(mb * (1 + 0.25 * math.sin(t / 7 + rng.random() * 0.01)))
    while len(buf) < target:
        buf.append(bytearray(1024 * 1024))
    while len(buf) > target:
        buf.pop()
    for b in buf:
        b[0] = 1
    busy = time.time() + 0.1 * cpu * (1 + 0.6 * math.sin(t / 3))
    while time.time() < busy:
        pass
    time.sleep(0.1)
