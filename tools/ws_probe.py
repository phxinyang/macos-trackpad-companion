#!/usr/bin/env python3
"""Minimal raw-WebSocket client that speaks the touchpad protocol.

Handshakes against companion-net's /ws endpoint and streams the same
frames the browser touchpad would send. Exists so the WS path can be
validated from a terminal without a phone: run it while watching the
companion-net stats line (`ws=`) and the screen.

    ws_probe.py --host host.orb.internal --port 4242 --mode circle
"""

import argparse
import base64
import math
import os
import socket
import struct
import time

MAGIC = b"ATP1"
VERSION = 1


def contact(cid, x, y):
    return bytes([cid, 0b11]) + struct.pack("<ff", x, y)


def frame_payload(seq, scan_ticks, contacts):
    return (
        MAGIC
        + bytes([VERSION])
        + bytes([0])
        + bytes([len(contacts)])
        + struct.pack("<I", seq)
        + struct.pack("<I", scan_ticks & 0xFFFFFFFF)
        + b"".join(contacts)
    )


def ws_connect(host, port):
    s = socket.create_connection((host, port), timeout=5)
    key = base64.b64encode(os.urandom(16)).decode()
    req = (
        f"GET /ws HTTP/1.1\r\nHost: {host}:{port}\r\n"
        "Upgrade: websocket\r\nConnection: Upgrade\r\n"
        f"Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
    )
    s.sendall(req.encode())
    resp = b""
    while b"\r\n\r\n" not in resp:
        chunk = s.recv(4096)
        if not chunk:
            raise ConnectionError("closed during handshake")
        resp += chunk
    head = resp.split(b"\r\n\r\n")[0].decode()
    if "101" not in head.splitlines()[0]:
        raise ConnectionError(f"handshake refused: {head.splitlines()[0]}")
    return s


def ws_send(s, payload, opcode=0x2):
    mask = os.urandom(4)
    header = bytes([0x80 | opcode])
    n = len(payload)
    if n < 126:
        header += bytes([0x80 | n])
    elif n < 65536:
        header += bytes([0x80 | 126]) + struct.pack(">H", n)
    else:
        header += bytes([0x80 | 127]) + struct.pack(">Q", n)
    masked = bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
    s.sendall(header + mask + masked)


def animate(s, seq0, dur_s, rate_hz, render):
    steps = max(2, int(dur_s * rate_hz))
    period = 1 / rate_hz
    start = time.monotonic()
    for i in range(steps):
        t = i / (steps - 1)
        delay = start + i * period - time.monotonic()
        if delay > 0:
            time.sleep(delay)
        seq = (seq0 + i) & 0xFFFFFFFF
        ws_send(s, frame_payload(seq, int(time.monotonic() * 10000), render(t)))
    return seq0 + steps


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--port", type=int, default=4242)
    ap.add_argument("--mode", default="circle",
                    choices=["circle", "scroll", "swipe3_up"])
    args = ap.parse_args()

    sock = ws_connect(args.host, args.port)
    print(f"WS handshake ok → ws://{args.host}:{args.port}/ws")
    seq = struct.unpack("<I", os.urandom(4))[0]

    if args.mode == "circle":
        cx, cy, r = 48, 38, 22
        seq = animate(sock, seq, 1.2, 90,
                      lambda t: [contact(1, cx + r * math.cos(2 * math.pi * t),
                                         cy + r * math.sin(2 * math.pi * t))])
    elif args.mode == "scroll":
        seq = animate(sock, seq, 0.6, 90,
                      lambda t: [contact(1, 45, 18 + 34 * t), contact(2, 45, 36 + 34 * t)])
    else:
        seq = animate(sock, seq, 0.3, 90,
                      lambda t: [contact(f, 48, 22 - 26 * t) for f in range(1, 4)])

    # Lift ×3
    for _ in range(3):
        seq += 1
        ws_send(sock, frame_payload(seq, int(time.monotonic() * 10000), []))
        time.sleep(0.02)
    print("frames sent; watch companion-net `stats: ... ws=` increase")
    sock.close()


if __name__ == "__main__":
    main()
