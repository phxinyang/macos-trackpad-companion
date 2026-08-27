# Touchpad wire protocol (v1)

The binary format shared by every network transport feeding the
companion's gesture engine:

- `companion-net` UDP listener ← native Android app (M5)
- `companion-net` WebSocket (binary messages) ← browser touchpad page (M4)
- `tools/synthetic_sender.py` ← test harness / robot finger

One packet or one WebSocket message = one complete touch frame. There is
no session state; every frame stands alone, exactly like a PTP input
report. Loss only matters for the *final* all-lifted frame of a gesture,
so senders retransmit that one a few times.

## Layout

All integers little-endian.

```text
offset  size  field
0       4     magic "ATP1"
4       1     version (= 1)
5       1     flags    bit0 = integrated button; bits 1..7 reserved (must be 0)
6       1     contact count n   (0..=10)
7       4     seq      u32 — increments per frame from the sender
11      4     scan_time u32 — sender's monotonic clock in 100 µs ticks
                        (mod 2^32). The receiver feeds the low 16 bits to
                        ScanTimeClock, so wrapping behaves like HID chips.
15      n·10  contact records:
                +0   id     u8    stable pointer id (fingers keep their id
                                   for the whole touch)
                +1   flags  bit0 tip · bit1 confidence · rest reserved (0)
                +2   x      f32   millimeters, left → right
                +6   y      f32   millimeters, top → bottom
```

Total size = 15 + 10n bytes ≤ 115 B.

## Rules

- **seq starts at a random 32-bit value per sender session.** The
  receiver deduplicates replays only within a short (~600 ms) time
  window — an identical seq arriving later is treated as a new session,
  not a drop. Senders that restart at 0 (scripts, phone reboots) must
  not be swallowed by the receiver's recent-history.
- Coordinates are **millimeters on the sender's physical surface**.
  Gesture thresholds and cursor sensitivity live downstream in mm, so a
  sender that skips the pixel→mm conversion feels wrong by orders of
  magnitude. Convert using one isotropic pixel-pitch scalar. Native Android
  uses the average of the display's valid `xdpi`/`ydpi` values (falling back
  to `densityDpi`); the browser uses the CSS 96-DPI reference. Apply a
  user-visible overall calibration factor after that conversion. Do not map
  the two axes independently to an arbitrary virtual rectangle: on a
  portrait phone that makes identical finger motion feel directionally
  different.
- Semantics mirror PTP:
  - a contact present in a frame has `tip=1`; lifting sets it absent
    from the next frame (the engine infers lift from disappearance),
  - `confidence` follows the platform palm rejection when available;
    browsers report `confidence=1`,
  - the integrated button maps to a physical press; phone clients leave
    it 0.
- Decoding is strict (see `touchpad-proto::decode`): wrong magic/
  version, truncation, trailing bytes, reserved flag bits set, contact
  count > 10, or non-finite coordinates reject the whole frame.

## Canonical vector

The fixed example frame used in unit tests across implementations —

- button = true, seq = 42, scan_time = 987654 (0x000F1206)
- contact id 5 at (-13.5, 77.25) mm, tip + confidence
- contact id 9 at (4.0, -0.5) mm, tip only

```text
41 54 50 31 01 01 02 2a 00 00 00 06 12 0f 00
05 03 00 00 58 c1 00 80 9a 42
09 01 00 00 80 40 00 00 00 bf
```

Locked-in by `crates/touchpad-proto`'s `canonical_vector_decodes_and_reencodes`
test; the Python sender produces byte-identical output.

## Relationship to the HID path

`hid.rs` decodes a PTP report into `report::Frame` and maps the chip's
scan time onto the host clock via `ScanTimeClock` before calling the
gesture engine. The net path reuses both halves unchanged: decode an
ATP1 packet, truncate scan_time to its low 16 bits, hand the same
clock-mapped timestamp to `gesture::State::on_frame_at`. From there on
it *is* the same pipeline.

## Reporting cadence and the silence watchdog

Clients must send a frame for every contact state, not only for changes.
A resting finger has to keep producing frames at roughly display rate —
the Android client resends the last contact set every 16 ms, the browser
client sends one per animation frame — each with a fresh sequence number
and a current timestamp.

This is not redundancy for its own sake. Both platforms only deliver
touch callbacks when a pointer *moves* (`ACTION_MOVE`, `pointermove`), so
a finger held still stops the stream entirely, and a stopped stream is
indistinguishable from a client that walked away. Physical trackpads
report at a fixed rate regardless of motion; clients of this protocol
must do the same.

The receiver treats `IDLE_LIFT_AFTER` (250 ms) of silence with contacts
down as a **link fault, not a lift**: the gesture is canceled via
`gesture::State::cancel_touch`, which closes out held buttons and phased
event streams but suppresses every tap path. Ending such a gesture as if
the user had lifted is what previously manufactured `dur=0ms` taps, which
macOS then coalesced with a preceding real tap into a double-click that
was never performed.

The all-lifted frame remains the one transition worth retransmitting
(×3 at 0/30/90 ms), since losing it strands the receiver mid-gesture
until the watchdog fires.
