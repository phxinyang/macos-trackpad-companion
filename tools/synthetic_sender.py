#!/usr/bin/env python3
"""Synthetic touch-frame sender for the companion's network transport.

Emits the wire format documented in docs/wire-protocol.md (and decoded
by crates/touchpad-proto) to `--host:--port` over UDP, driving the
gesture engine without a real phone — the Mac-side equivalent of a
robot finger. Used by M2/M3 verification.

Examples:
  synthetic_sender.py --host host.orb.internal --port 4242 --mode circle
  synthetic_sender.py --host 192.168.1.20 --mode pinch_out
  synthetic_sender.py ... --mode clear     # one all-lifted frame (reset)
"""

import argparse
import math
import random
import socket
import struct
import time

MAGIC = b"ATP1"
AUTH_MAGIC = b"ATK1"
VERSION = 1
FLAG_BUTTON = 0x01
CONTACT_TIP = 0x01
CONTACT_CONFIDENCE = 0x02


def contact(cid: int, x: float, y: float, tip: bool = True, conf: bool = True) -> bytes:
    flags = (CONTACT_TIP if tip else 0) | (CONTACT_CONFIDENCE if conf else 0)
    return bytes([cid, flags]) + struct.pack("<ff", x, y)


def encode(seq: int, scan_time: int, button: bool, contacts: list[bytes]) -> bytes:
    return (
        MAGIC
        + bytes([VERSION])
        + bytes([FLAG_BUTTON if button else 0])
        + bytes([len(contacts)])
        + struct.pack("<I", seq)
        + struct.pack("<I", scan_time & 0xFFFFFFFF)
        + b"".join(contacts)
    )


class Sender:
    """Frames at a fixed rate, mirroring what phone clients do."""

    def __init__(self, sock: socket.socket, addr, rate_hz: float, token: str | None = None):
        self.sock = sock
        self.addr = addr
        self.period = 1.0 / rate_hz
        # Random start: short-lived sender processes must not collide
        # with the receiver's recent-seq replay window (docs/wire-protocol.md).
        self.seq = random.randrange(1 << 32)
        self.t0 = time.monotonic_ns()
        self.token = token.encode("utf-8") if token else None
        if self.token is not None and not 1 <= len(self.token) <= 256:
            raise ValueError("token must be 1..256 UTF-8 bytes")

    def now_ticks(self) -> int:
        # Sender monotonic clock in 100 µs ticks; receiver uses low 16 bits.
        return (time.monotonic_ns() - self.t0) // 100_000

    def send(self, contacts: list[bytes], button: bool = False, lift_extra: bool = False):
        packet = encode(self.seq, self.now_ticks(), button, contacts)
        if self.token is not None:
            packet = AUTH_MAGIC + struct.pack("<H", len(self.token)) + self.token + packet
        self.sock.sendto(packet, self.addr)
        # The final "all lifted" frame is the only stateful transition in
        # the protocol; losing it stalls taps/clicks until the next touch.
        # Mirrors what the phone clients do on gesture end.
        copies = 3 if lift_extra else 1
        for _ in range(copies - 1):
            self.seq += 1
            self.sock.sendto(packet, self.addr)
        self.seq += 1

    def land(self, fingers: list[bytes], gap_s: float = 0.022):
        """Land fingers one per frame like real hardware does — chips
        never report N contacts appearing in a single instant."""
        down = []
        for f in fingers:
            down.append(f)
            self.send(down)
            time.sleep(gap_s)

    def animate(
        self,
        duration_s: float,
        render,  # fn(t_norm: float) -> list[bytes]
        lift_extra: bool = True,
    ):
        steps = max(2, int(duration_s / self.period))
        start = time.monotonic()
        for i in range(steps):
            delay = start + i * self.period - time.monotonic()
            if delay > 0:
                time.sleep(delay)
            self.send(render(i / (steps - 1)))
        self.send([], lift_extra=lift_extra)


# --- Modes -----------------------------------------------------------------
# Coordinates are millimeters on an implied virtual surface (~96×60 mm, like
# the reference PTP firmware). All motion stays well inside it.


def mode_clear(s: Sender, args):
    s.send([], lift_extra=True)


def mode_tap(s: Sender, args):
    s.send([contact(1, 50, 30)])
    time.sleep(0.055)
    s.send([], lift_extra=True)


def mode_doubletap(s: Sender, args):
    for _ in range(2):
        mode_tap(s, args)
        time.sleep(max(0.06, args.gap_ms / 1000))


def mode_right_tap(s: Sender, args):
    s.land([contact(1, 40, 30), contact(2, 62, 32)])
    time.sleep(0.06)
    s.send([], lift_extra=True)


def mode_move(s: Sender, args):
    def render(t):
        return [contact(1, 20 + 55 * t, 30)]

    s.animate(0.8, render)


def mode_circle(s: Sender, args):
    cx, cy, r = 48, 38, 22

    def render(t):
        a = 2 * math.pi * t
        return [contact(1, cx + r * math.cos(a), cy + r * math.sin(a))]

    s.animate(1.2, render)


def mode_scroll(s: Sender, args):
    # Both fingers translate together — pan wins the engine's 2F lock ⇒ scroll.
    # animate() ends with the lift frame straight after the fastest motion
    # sample, so inertia starts from real terminal velocity.
    scale = args.dist
    duration = args.dur if args.dur > 0 else 0.6

    def render(t):
        y = 20 + 35 * t * scale
        return [contact(1, 45, y), contact(2, 45, y + 18)]

    s.animate(duration, render, lift_extra=True)


def make_pinch(direction: int):
    # direction +1 = fingers converge (pinch in / zoom out);
    #                -1 = fingers diverge (pinch out / zoom in).
    def wrapped(s: Sender, args):
        cx, cy, spread0, spread1 = 48, 38, 26, 10

        def pts(t):
            half = abs(spread0 + (spread1 - spread0) * direction * t)
            return [contact(1, cx - half, cy), contact(2, cx + half, cy)]

        s.land(pts(0.0))
        s.animate(0.7, lambda t: pts(max(0.02, t)))

    return wrapped


def mode_rotate(s: Sender, args):
    cx, cy, r = 48, 38, 16

    def pts(t):
        a = math.radians(-30 + 75 * t)
        return [
            contact(1, cx + r * math.cos(a), cy + r * math.sin(a)),
            contact(2, cx - r * math.cos(a), cy - r * math.sin(a)),
        ]

    s.land(pts(0.0))
    s.animate(0.9, lambda t: pts(max(0.02, t)))


def make_swipe(n_fingers: int, dx: float, dy: float, dur: float = 0.25):
    def wrapped(s: Sender, args):
        scale = args.dist
        duration = args.dur if args.dur > 0 else dur

        def render(t):
            out = []
            offsets = -(n_fingers - 1) * 7
            for f in range(n_fingers):
                y0 = 34 + offsets + f * 14
                out.append(contact(f + 1, 48 + dx * t * scale, y0 + dy * t * scale))
            return out

        s.land(render(0.0))
        s.animate(duration, lambda t: render(max(0.02, t)))

    return wrapped


def make_drag(n_fingers: int, dx: float, dy: float, dur: float = 1.0):
    def wrapped(s: Sender, args):
        scale = args.dist
        duration = args.dur if args.dur > 0 else dur

        def render(t):
            out = []
            offsets = -(n_fingers - 1) * 7
            for f in range(n_fingers):
                y0 = 34 + offsets + f * 14
                out.append(contact(f + 1, 30 + dx * t * scale, y0 + dy * t * scale))
            return out

        s.land(render(0.0))
        s.animate(duration, lambda t: render(max(0.02, t)), lift_extra=True)

    return wrapped


MODES = {
    "clear": mode_clear,
    "tap": mode_tap,
    "doubletap": mode_doubletap,
    "right_tap": mode_right_tap,
    "move": mode_move,
    "circle": mode_circle,
    "scroll": mode_scroll,
    "pinch_in": make_pinch(+1),
    "pinch_out": make_pinch(-1),
    "rotate": mode_rotate,
    "drag3_right": make_drag(3, +30, 0, 1.0),
    "drag3_left": make_drag(3, -30, 0, 1.0),
    "drag3_up": make_drag(3, 0, -25, 1.0),
    "drag3_down": make_drag(3, 0, +25, 1.0),
    "swipe3_left": make_swipe(3, -36, 0),
    "swipe3_right": make_swipe(3, +36, 0),
    "swipe3_up": make_swipe(3, 0, -28),
    "swipe3_down": make_swipe(3, 0, +28),
    "swipe4_left": make_swipe(4, -36, 0),
    "swipe4_right": make_swipe(4, +36, 0),
    "swipe4_up": make_swipe(4, 0, -28),
    "swipe4_down": make_swipe(4, 0, +28),
}


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--host", required=True, help="receiver host (Mac LAN IP or host.orb.internal)")
    ap.add_argument("--port", type=int, default=4242)
    ap.add_argument("--mode", default="circle", choices=sorted(MODES))
    ap.add_argument("--rate", type=float, default=120.0, help="frame rate for animated modes")
    ap.add_argument("--gap-ms", type=int, default=120, help="gap between double-tap halves")
    ap.add_argument("--repeat", type=int, default=1, help="times to run the mode")
    ap.add_argument("--dist", type=float, default=1.0, help="travel scale for animated modes")
    ap.add_argument("--dur", type=float, default=0, help="override duration (s); 0 = mode default")
    ap.add_argument("--token", help="optional [net].token; wraps packets in the ATK1 envelope")
    args = ap.parse_args()

    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sender = Sender(sock, (args.host, args.port), args.rate, args.token)

    # Always start from a known lifted state.
    sender.send([], lift_extra=True)
    time.sleep(0.15)

    fn = MODES[args.mode]
    for i in range(args.repeat):
        if i > 0:
            time.sleep(0.4)
        fn(sender, args)
    print(f"sent {args.mode} ×{args.repeat} → {args.host}:{args.port}")


if __name__ == "__main__":
    main()
