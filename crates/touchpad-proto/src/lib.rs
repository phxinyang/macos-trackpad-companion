//! Wire format shared by every transport that feeds the companion's
//! gesture engine over the network: the Android web touchpad (WebSocket
//! binary messages) and the native APK (UDP datagrams).
//!
//! One packet = one touch frame. Layout (little-endian):
//!
//! ```text
//! offset  size  field
//! 0       4     magic "ATP1"
//! 4       1     version  (= 1)
//! 5       1     flags    bit0 = integrated button; bits 1..7 reserved, must be 0
//! 6       1     contact count n   (0..=MAX_CONTACTS)
//! 7       4     seq      u32 — monotonically increasing per sender
//! 11      4     scan_time u32 — sender monotonic clock in 100 µs ticks
//!                        (mod 2^32); receiver uses the low 16 bits so the
//!                       existing ScanTimeClock wrap logic applies as-is
//! 15      n*10  contact records:
//!                 +0  id    u8   stable pointer id
//!                 +1  flags bit0 tip, bit1 confidence; bits 2..7 reserved
//!                 +2  x     f32  millimeters
//!                 +6  y     f32  millimeters
//!
//! total = 15 + 10n bytes
//! ```
//!
//! Decoding is strict: bad magic/version, truncation, trailing bytes,
//! unknown flag bits, out-of-range contact counts, and non-finite
//! coordinates are all rejected. Gesture thresholds are expressed in
//! millimeters downstream, so senders must convert screen pixels using
//! the device's reported physical density before encoding.

/// Maximum contacts a packet may carry. Roomy on purpose: real PTP
/// hardware declares five, phones comfortably track ten.
pub const MAX_CONTACTS: usize = 10;

const MAGIC: [u8; 4] = *b"ATP1";
const VERSION: u8 = 1;
const FLAG_BUTTON: u8 = 0x01;
const CONTACT_FLAG_TIP: u8 = 0x01;
const CONTACT_FLAG_CONFIDENCE: u8 = 0x02;

const HEADER_LEN: usize = 15;
const CONTACT_LEN: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact {
    pub id: u8,
    /// Millimeters, left → right.
    pub x: f32,
    /// Millimeters, top → bottom (PTP origin convention).
    pub y: f32,
    pub tip: bool,
    pub confidence: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub seq: u32,
    /// Sender's monotonic clock in 100 µs ticks, mod 2^32.
    pub scan_time_100us: u32,
    pub button: bool,
    pub contacts: Vec<Contact>,
}

impl Frame {
    /// Low 16 bits of [`Frame::scan_time_100us`] — the value HID-style
    /// consumers (`ScanTimeClock::observe`) expect.
    pub fn scan_time_u16(&self) -> u16 {
        self.scan_time_100us as u16
    }

    pub fn encode(&self) -> Vec<u8> {
        assert!(
            self.contacts.len() <= MAX_CONTACTS,
            "frame exceeds MAX_CONTACTS"
        );
        let mut buf = Vec::with_capacity(HEADER_LEN + self.contacts.len() * CONTACT_LEN);
        buf.extend_from_slice(&MAGIC);
        buf.push(VERSION);
        buf.push(if self.button { FLAG_BUTTON } else { 0 });
        buf.push(self.contacts.len() as u8);
        buf.extend_from_slice(&self.seq.to_le_bytes());
        buf.extend_from_slice(&self.scan_time_100us.to_le_bytes());
        for c in &self.contacts {
            let mut flags = 0u8;
            if c.tip {
                flags |= CONTACT_FLAG_TIP;
            }
            if c.confidence {
                flags |= CONTACT_FLAG_CONFIDENCE;
            }
            buf.push(c.id);
            buf.push(flags);
            buf.extend_from_slice(&c.x.to_le_bytes());
            buf.extend_from_slice(&c.y.to_le_bytes());
        }
        buf
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum DecodeError {
    TooShort,
    BadMagic,
    BadVersion,
    UnknownFlag { at: usize },
    BadContactCount(usize),
    NonFiniteCoordinate { id: u8 },
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort => write!(f, "packet shorter than its declared frame"),
            Self::BadMagic => write!(f, "bad magic (expected \"ATP1\")"),
            Self::BadVersion => write!(f, "unsupported protocol version"),
            Self::UnknownFlag { at } => {
                write!(f, "unknown/reserved flag bits set at byte {at}")
            }
            Self::BadContactCount(n) => write!(f, "contact count {n} out of range"),
            Self::NonFiniteCoordinate { id } => {
                write!(f, "non-finite coordinate for contact id {id}")
            }
        }
    }
}

impl std::error::Error for DecodeError {}

pub fn decode(buf: &[u8]) -> Result<Frame, DecodeError> {
    if buf.len() < HEADER_LEN {
        return Err(DecodeError::TooShort);
    }
    if buf[0..4] != MAGIC {
        return Err(DecodeError::BadMagic);
    }
    if buf[4] != VERSION {
        return Err(DecodeError::BadVersion);
    }
    if buf[5] & !FLAG_BUTTON != 0 {
        return Err(DecodeError::UnknownFlag { at: 5 });
    }
    let n = buf[6] as usize;
    if n > MAX_CONTACTS {
        return Err(DecodeError::BadContactCount(n));
    }
    let expected = HEADER_LEN + n * CONTACT_LEN;
    if buf.len() < expected {
        return Err(DecodeError::TooShort);
    }
    if buf.len() > expected {
        // Strict framing keeps UDP resync behavior obvious: a stale or
        // concatenated buffer is garbage, not something to slice.
        return Err(DecodeError::UnknownFlag { at: expected });
    }

    let seq = u32::from_le_bytes(buf[7..11].try_into().unwrap());
    let scan_time_100us = u32::from_le_bytes(buf[11..15].try_into().unwrap());

    let mut contacts = Vec::with_capacity(n);
    for i in 0..n {
        let off = HEADER_LEN + i * CONTACT_LEN;
        let rec = &buf[off..off + CONTACT_LEN];
        if rec[1] & !(CONTACT_FLAG_TIP | CONTACT_FLAG_CONFIDENCE) != 0 {
            return Err(DecodeError::UnknownFlag { at: off + 1 });
        }
        let x = f32::from_le_bytes(rec[2..6].try_into().unwrap());
        let y = f32::from_le_bytes(rec[6..10].try_into().unwrap());
        if !x.is_finite() || !y.is_finite() {
            return Err(DecodeError::NonFiniteCoordinate { id: rec[0] });
        }
        contacts.push(Contact {
            id: rec[0],
            tip: rec[1] & CONTACT_FLAG_TIP != 0,
            confidence: rec[1] & CONTACT_FLAG_CONFIDENCE != 0,
            x,
            y,
        });
    }

    Ok(Frame {
        seq,
        scan_time_100us,
        button: buf[5] & FLAG_BUTTON != 0,
        contacts,
    })
}

/// Classifies incoming sequence numbers so the UDP path can log loss
/// (gap) and ignore retransmits of lift frames (duplicate). Handles
/// wraparound by treating sequence as a signed 32-bit distance.
#[derive(Default)]
pub struct SequenceTracker {
    last: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeqClass {
    /// Normal forward step.
    Forward,
    /// Forward progress with a hole right before it — packets were
    /// lost mid-stream. `lost` counts them.
    ForwardAfterGap { lost: u32 },
    /// At or behind the last seen seq — already processed.
    Duplicate,
}

impl SequenceTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn classify(&mut self, seq: u32) -> SeqClass {
        match self.last {
            None => {
                self.last = Some(seq);
                SeqClass::Forward
            }
            Some(last) => {
                let dist = seq.wrapping_sub(last) as i32;
                // Forward progress; or a jump so far behind the window
                // that no plausible late/lost packet explains it — a
                // restarted sender (new session at low seq after wrap).
                if !(-DUPLICATE_WINDOW..=0).contains(&dist) {
                    self.last = Some(seq);
                    if dist > 1 {
                        SeqClass::ForwardAfterGap {
                            lost: (dist - 1) as u32,
                        }
                    } else {
                        SeqClass::Forward
                    }
                } else {
                    SeqClass::Duplicate
                }
            }
        }
    }
}

/// A sequence this far behind the last seen value can't be a mere
/// retransmit (≈ 2.3 h of frames at a 120 Hz touch rate) — treat it as
/// a sender restart and resynchronize.
const DUPLICATE_WINDOW: i32 = 1_000_000;

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(seq: u32) -> Frame {
        Frame {
            seq,
            scan_time_100us: 123_456,
            button: false,
            contacts: vec![Contact {
                id: 3,
                x: 25.0,
                y: 1.0,
                tip: true,
                confidence: true,
            }],
        }
    }

    #[test]
    fn header_layout_matches_spec() {
        let bytes = frame(0xdead_beef).encode();
        assert_eq!(bytes.len(), HEADER_LEN + CONTACT_LEN);
        assert_eq!(&bytes[0..4], b"ATP1");
        assert_eq!(bytes[4], 1);
        assert_eq!(bytes[5], 0);
        assert_eq!(bytes[6], 1);
        assert_eq!(&bytes[7..11], &[0xef, 0xbe, 0xad, 0xde]);
        assert_eq!(&bytes[11..15], &[0x40, 0xe2, 0x01, 0x00]); // 123456 LE
        // Contact record: id, flags(tip|conf), x=25.0mm, y=1.0mm
        assert_eq!(bytes[15], 3);
        assert_eq!(bytes[16], 0b11);
        assert_eq!(&bytes[17..21], &[0x00, 0x00, 0xc8, 0x41]); // 25.0f32
        assert_eq!(&bytes[21..25], &[0x00, 0x00, 0x80, 0x3f]); // 1.0f32
    }

    #[test]
    fn round_trips_empty_and_multi_contact() {
        for n in [0usize, 1, MAX_CONTACTS] {
            let f = Frame {
                seq: 7,
                scan_time_100us: u16::MAX as u32 + 999, // exercises low-16 wrap
                button: true,
                contacts: (0..n)
                    .map(|i| Contact {
                        id: i as u8 * 3,
                        x: -12.75 + i as f32,
                        y: 300.25 - i as f32,
                        tip: i % 2 == 0,
                        confidence: i % 3 != 0,
                    })
                    .collect(),
            };
            assert_eq!(decode(&f.encode()).unwrap(), f);
        }
    }

    #[test]
    fn scan_time_exposes_low_sixteen_bits() {
        let mut f = frame(0);
        f.scan_time_100us = 0x0012_3456;
        assert_eq!(f.scan_time_u16(), 0x3456);
    }

    #[test]
    fn rejects_bad_magic_version_and_flags() {
        let mut b = frame(1).encode();
        b[0] = b'X';
        assert_eq!(decode(&b), Err(DecodeError::BadMagic));

        let mut b = frame(1).encode();
        b[4] = 2;
        assert_eq!(decode(&b), Err(DecodeError::BadVersion));

        let mut b = frame(1).encode();
        b[5] |= 0b1000_0000;
        assert_eq!(decode(&b), Err(DecodeError::UnknownFlag { at: 5 }));

        let mut b = frame(1).encode();
        b[16] |= 0b0100_0000;
        assert_eq!(decode(&b), Err(DecodeError::UnknownFlag { at: 16 }));
    }

    #[test]
    fn rejects_count_and_length_violations() {
        let mut b = frame(1).encode();
        b[6] = (MAX_CONTACTS + 1) as u8;
        assert_eq!(
            decode(&b),
            Err(DecodeError::BadContactCount(MAX_CONTACTS + 1))
        );

        let full = frame(1).encode();
        for cut in 0..full.len() {
            assert!(
                matches!(decode(&full[..cut]), Err(DecodeError::TooShort)),
                "prefix of len {cut} should be rejected"
            );
        }
        let mut padded = full.clone();
        padded.push(0);
        assert!(matches!(
            decode(&padded),
            Err(DecodeError::UnknownFlag { .. })
        ));
    }

    #[test]
    fn rejects_non_finite_coordinates() {
        let mut f = frame(1);
        f.contacts[0].x = f32::NAN;
        assert!(matches!(
            decode(&f.encode()),
            Err(DecodeError::NonFiniteCoordinate { id: 3 })
        ));
    }

    #[test]
    fn sequence_tracker_classifies_forward_dup_restart_and_gaps() {
        let mut t = SequenceTracker::new();
        assert_eq!(t.classify(100), SeqClass::Forward);
        assert_eq!(t.classify(101), SeqClass::Forward);
        assert_eq!(t.classify(101), SeqClass::Duplicate);
        assert_eq!(t.classify(90), SeqClass::Duplicate);
        // A skip of three reports the two lost frames in between.
        assert_eq!(t.classify(104), SeqClass::ForwardAfterGap { lost: 2 });
        // Jumping straight to MAX without touching the wrap point is a
        // ~4-billion step *backward* — implausible, so Duplicate.
        assert_eq!(t.classify(u32::MAX), SeqClass::Duplicate);
        // A genuine wrap walked through one frame at a time is forward,
        // and MAX → 0 (+1) stays forward across the boundary.
        let mut t = SequenceTracker::new();
        assert_eq!(t.classify(u32::MAX - 5), SeqClass::Forward);
        for s in [u32::MAX - 4, u32::MAX - 3, u32::MAX - 2, u32::MAX - 1] {
            assert_eq!(t.classify(s), SeqClass::Forward);
        }
        assert_eq!(t.classify(u32::MAX), SeqClass::Forward);
        assert_eq!(t.classify(0), SeqClass::Forward);
        assert_eq!(t.classify(1), SeqClass::Forward);
        // Restarted sender (new session from seq 0 of another wrap) resyncs.
        let mut t = SequenceTracker::new();
        assert_eq!(t.classify(900_000_000), SeqClass::Forward);
        assert_eq!(t.classify(42), SeqClass::Forward);
    }

    /// Canonical cross-language vector — byte-identical with the
    /// Python sender and documented in docs/wire-protocol.md.
    #[test]
    fn canonical_vector_decodes_and_reencodes() {
        let hex = |s: &str| {
            s.split_whitespace()
                .map(|x| u8::from_str_radix(x, 16).unwrap())
                .collect::<Vec<u8>>()
        };
        // button=true; seq=42; scan=987654;
        // c5 (-13.5, 77.25) tip+conf; c9 (4.0, -0.5) tip only
        let bytes = hex(concat!(
            "41 54 50 31 01 01 02 2a 00 00 00 06 12 0f 00",
            " 05 03 00 00 58 c1 00 80 9a 42",
            " 09 01 00 00 80 40 00 00 00 bf"
        ));
        let expect = Frame {
            seq: 42,
            scan_time_100us: 987_654,
            button: true,
            contacts: vec![
                Contact {
                    id: 5,
                    x: -13.5,
                    y: 77.25,
                    tip: true,
                    confidence: true,
                },
                Contact {
                    id: 9,
                    x: 4.0,
                    y: -0.5,
                    tip: true,
                    confidence: false,
                },
            ],
        };
        assert_eq!(decode(&bytes).unwrap(), expect);
        assert_eq!(expect.encode(), bytes);
    }

    /// Deterministic LCG fuzz: random frames always round-trip.
    #[test]
    fn round_trips_random_frames() {
        let mut seed = 0x1234_5678u64;
        let mut rng = move || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as u32
        };
        for _ in 0..500 {
            let f = Frame {
                seq: rng(),
                scan_time_100us: rng(),
                button: rng() & 1 == 0,
                contacts: (0..rng() as usize % (MAX_CONTACTS + 1))
                    .map(|_| Contact {
                        id: rng() as u8,
                        x: f32::from_bits((rng() >> 8) << 8) / 256.0,
                        y: f32::from_bits((rng() >> 8) << 8) / 256.0,
                        tip: rng() & 1 == 0,
                        confidence: rng() & 1 == 0,
                    })
                    .filter(|c| c.x.is_finite() && c.y.is_finite())
                    .collect(),
            };
            assert_eq!(decode(&f.encode()).unwrap(), f);
        }
    }
}
