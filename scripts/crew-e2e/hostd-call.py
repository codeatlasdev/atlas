#!/usr/bin/env python3
"""One hostd command over the socket an `ssh -L` tunnel brings from the crew server: what Atlas Desktop does
(`atlas-desktop/src/remote.rs`): the token from the server's `hostd.token`, the `hello` line, then NDJSON.

    hostd-call.py SOCKET TOKEN COMMAND [JSON_ARGS]

Prints the reply's `data` as JSON. Exit 1 with hostd's error on stderr when it answers `ok: false`.
"""
import json
import socket
import sys

PROTO = 2  # atlas_host::protocol::PROTO


def main() -> int:
    sock_path, token, cmd = sys.argv[1:4]
    args = json.loads(sys.argv[4]) if len(sys.argv) > 4 else {}
    s = socket.socket(socket.AF_UNIX)
    s.settimeout(60)
    s.connect(sock_path)
    f = s.makefile("rw", newline="\n")

    def send(v):
        f.write(json.dumps(v) + "\n")
        f.flush()

    send({"hello": {"token": token, "client": "crew-e2e", "proto": PROTO}})
    first = json.loads(f.readline() or "null") or {}
    if first.get("ok") is not True:
        print(f"hostd refused the hello: {first}", file=sys.stderr)
        return 1
    send({"id": 1, "cmd": cmd, "args": args})
    while True:
        line = f.readline()
        if not line:
            print("hostd closed the connection before it answered", file=sys.stderr)
            return 1
        v = json.loads(line)
        if v.get("id") != 1:
            continue  # an event
        if v.get("ok") is not True:
            print(v.get("error", "error"), file=sys.stderr)
            return 1
        print(json.dumps(v.get("data")))
        return 0


if __name__ == "__main__":
    sys.exit(main())
