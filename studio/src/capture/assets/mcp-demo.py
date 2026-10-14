#!/usr/bin/env python3
"""Real `atlas mcp` over stdio; the agent side is a scripted stand-in that prints each call and the live reply.

env: ATLAS_BIN      the atlas binary to run as the MCP server (required)
     MCP_DEMO_DIR   the project directory the server serves (required)
     MCP_DEMO_HOLD  seconds to keep the final screen (default 8)
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
import time

R = "\033[0m"; D = "\033[2m"; B = "\033[1m"; V = "\033[38;5;141m"
G = "\033[32m"; Y = "\033[33m"; RD = "\033[31m"


def out(s: str = "", d: float = 0.0) -> None:
    sys.stdout.write(s + "\n")
    sys.stdout.flush()
    time.sleep(d)


p = subprocess.Popen(
    [os.environ["ATLAS_BIN"], "mcp"],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.DEVNULL,
    text=True,
    cwd=os.environ["MCP_DEMO_DIR"],
)


def send(msg: dict) -> None:
    p.stdin.write(json.dumps({"jsonrpc": "2.0", **msg}) + "\n")
    p.stdin.flush()


def rpc(i: int, method: str, params: dict | None = None) -> dict:
    send({"id": i, "method": method, "params": params or {}})
    return json.loads(p.stdout.readline())


rpc(1, "initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "omp", "version": "1"}})
send({"method": "notifications/initialized"})


def call(i: int, name: str, args: dict) -> str:
    out(f"{V}π{R} {D}tool call{R} {B}{name}{R}{D}({json.dumps(args) if args else ''}){R}", 0.5)
    return rpc(i, "tools/call", {"name": name, "arguments": args})["result"]["content"][0]["text"]


out(f"{V}π{R} {B}the checkout page is slow, find out why{R}\n", 0.9)
status = json.loads(call(2, "atlas_status", {}))
for s in status["services"]:
    c = {"running": G, "failed": RD, "stopped": D}.get(s["state"], Y)
    out(f"   {c}●{R} {s['name']:<9}{D}{s['state']:<9}{R}" + (f"{D}:{s['port']}{R}" if s["port"] else ""), 0.12)
out("", 0.7)
matches = call(3, "atlas_logs_search", {"query": "/stripe|timeout/", "limit": 3})
for line in matches.strip().splitlines()[-3:]:
    out(f"   {RD}{line[:92]}{R}", 0.18)
out("", 0.6)
out(f"{V}π{R} {D}stripe is timing out under load, retrying the api service{R}", 0.7)
call(4, "atlas_restart_service", {"service": "api"})
out(f"   {G}✓{R} restart accepted", 0.5)
call(5, "atlas_wait_ready", {"service": "api", "timeout_s": 20})
out(f"   {G}✓{R} api running and healthy", 0.6)
out(f"\n{V}π{R} {B}Fixed.{R} {D}Atlas saw it, I restarted it, no terminal juggling.{R}", 0.5)
p.terminate()
time.sleep(float(os.environ.get("MCP_DEMO_HOLD", "8")))
