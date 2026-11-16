#!/usr/bin/env python3
"""One request to the crew spawn helper, over the socket an `ssh -L` tunnel brings from the server (the crew
user may connect to it, nobody else): a 4-byte big-endian length and that many bytes of JSON each way
(`atlas_host::crew::spawn::wire`). A reply may carry file descriptors; they are closed unread.

    helper-call.py SOCKET JSON_REQUEST

Prints the reply's JSON. Exit 1 when it cannot connect or read an answer.
"""
import json
import socket
import struct
import sys


def main() -> int:
    path, request = sys.argv[1:3]
    body = json.dumps(json.loads(request)).encode()
    s = socket.socket(socket.AF_UNIX)
    s.settimeout(30)
    s.connect(path)
    s.sendall(struct.pack(">I", len(body)) + body)
    head = b""
    while len(head) < 4:
        msg, fds, _, _ = socket.recv_fds(s, 4 - len(head), 8)
        if not msg:
            print("the helper closed the connection without answering", file=sys.stderr)
            return 1
        head += msg
    import os

    for fd in fds:
        os.close(fd)
    (n,) = struct.unpack(">I", head)
    data = b""
    while len(data) < n:
        chunk = s.recv(n - len(data))
        if not chunk:
            break
        data += chunk
    print(data.decode())
    return 0


if __name__ == "__main__":
    sys.exit(main())
