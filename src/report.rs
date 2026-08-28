//! Decode a PTP touch input report (ID 0x01) using a [`Layout`] from
//! [`crate::descriptor::parse`]. Coordinates are converted from chip
//! pixels to millimeters using the descriptor's per-axis density
//! ([`Layout::mm_per_logical_px_x`] / `_y`) so downstream gesture code
//! works in physical units and is firmware-agnostic.

use crate::descriptor::{BitField, Layout};

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub struct Contact {
    pub id: u8,
    /// X position in millimeters (left → right).
    pub x: f64,
    /// Y position in millimeters (top → bottom; PTP origin is top-left).
    pub y: f64,
    pub tip: bool,
    pub confidence: bool,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct Frame {
    pub contacts: Vec<Contact>,
    pub scan_time_100us: u16,
    pub button: bool,
}

pub fn decode(layout: &Layout, report: &[u8]) -> Option<Frame> {
    if report.len() < layout.total_payload_bytes {
        return None;
    }
    if report[0] != layout.report_id {
        return None;
    }

    let contact_count = layout
        .contact_count_field
        .map(|field| read_bits(report, field).unwrap_or(0) as usize)
        .unwrap_or_else(|| report[layout.contact_count_offset] as usize);
    let n = contact_count.min(layout.contact_slots);

    let mm_per_px_x = layout.mm_per_logical_px_x();
    let mm_per_px_y = layout.mm_per_logical_px_y();

    let mut contacts = Vec::with_capacity(n);
    for i in 0..n {
        let fields = layout.contact_fields;
        let (id, x, y, confidence, tip) = if let Some(fields) = fields {
            let base = layout.fingers_bit_offset + i * fields.stride_bits;
            let read = |field: BitField| read_bits_at(report, base + field.bit_offset, field.bit_width);
            let id = read(fields.id)? as u8;
            let x = read(fields.x)? as i32;
            let y = read(fields.y)? as i32;
            let confidence = fields.confidence.map(|field| read(field).unwrap_or(0) != 0).unwrap_or(true);
            let tip = read(fields.tip)? != 0;
            (id, x, y, confidence, tip)
        } else {
            let off = layout.fingers_offset + i * layout.bytes_per_contact;
            if off + layout.bytes_per_contact > report.len() { break; }
            let flags = report[off];
            let id = report[off + 1];
            let x = u16::from_le_bytes([report[off + 2], report[off + 3]]) as i32;
            let y = u16::from_le_bytes([report[off + 4], report[off + 5]]) as i32;
            (id, x, y, (flags & 0x01) != 0, (flags & 0x02) != 0)
        };

        contacts.push(Contact {
            id,
            x: (x as f64) * mm_per_px_x,
            y: (y as f64) * mm_per_px_y,
            tip,
            confidence,
        });
    }

    let scan_time = layout
        .scan_time_field
        .and_then(|field| read_bits(report, field))
        .unwrap_or_else(|| u16::from_le_bytes([
            report[layout.scan_time_offset],
            report[layout.scan_time_offset + 1],
        ]) as u64) as u16;
    let button = layout
        .button_field
        .and_then(|field| read_bits(report, field))
        .map(|value| value != 0)
        .unwrap_or_else(|| (report[layout.button_offset] & (1 << layout.button_bit)) != 0);

    Some(Frame {
        contacts,
        scan_time_100us: scan_time,
        button,
    })
}

fn read_bits(report: &[u8], field: BitField) -> Option<u64> {
    read_bits_at(report, field.bit_offset, field.bit_width)
}

fn read_bits_at(report: &[u8], bit_offset: usize, bit_width: u8) -> Option<u64> {
    if bit_width == 0 || bit_width > 64 {
        return None;
    }
    let end = bit_offset.checked_add(bit_width as usize)?;
    if end > report.len().checked_mul(8)? {
        return None;
    }
    let mut value = 0u64;
    for i in 0..bit_width as usize {
        let bit = bit_offset + i;
        let byte = report[bit / 8];
        value |= u64::from((byte >> (bit % 8)) & 1) << i;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::descriptor::{BitField, ContactBitFields};

    fn fake_layout() -> Layout {
        Layout {
            report_id: 0x01,
            input_mode_report_id: None,
            contact_slots: 5,
            bytes_per_contact: 6,
            fingers_offset: 1,
            fingers_bit_offset: 8,
            scan_time_offset: 31,
            contact_count_offset: 33,
            button_offset: 34,
            button_bit: 0,
            logical_x_max: 3936,
            logical_y_max: 2424,
            physical_x_max_mm: 65.0,
            physical_y_max_mm: 40.0,
            total_payload_bytes: 35,
            contact_fields: None,
            scan_time_field: None,
            contact_count_field: None,
            button_field: None,
        }
    }

    #[test]
    fn decodes_two_contacts() {
        let layout = fake_layout();
        let mut buf = vec![0u8; 35];
        buf[0] = 0x01;
        // Contact 0: tip=1, conf=1, id=7, x=1968, y=1212 (pad midpoint)
        buf[1] = 0x03;
        buf[2] = 7;
        buf[3..5].copy_from_slice(&1968u16.to_le_bytes());
        buf[5..7].copy_from_slice(&1212u16.to_le_bytes());
        // Contact 1: tip=1, conf=1, id=8, x=2952, y=606
        buf[7] = 0x03;
        buf[8] = 8;
        buf[9..11].copy_from_slice(&2952u16.to_le_bytes());
        buf[11..13].copy_from_slice(&606u16.to_le_bytes());
        // scan_time = 0x1234, count=2, button=1
        buf[31..33].copy_from_slice(&0x1234u16.to_le_bytes());
        buf[33] = 2;
        buf[34] = 0x01;

        let frame = decode(&layout, &buf).expect("decode");
        assert_eq!(frame.contacts.len(), 2);
        assert_eq!(frame.contacts[0].id, 7);
        // Midpoint chip pixel → midpoint mm.
        assert!((frame.contacts[0].x - 32.5).abs() < 0.05, "{}", frame.contacts[0].x);
        assert!((frame.contacts[0].y - 20.0).abs() < 0.05, "{}", frame.contacts[0].y);
        assert_eq!(frame.scan_time_100us, 0x1234);
        assert!(frame.button);
    }

    #[test]
    fn decodes_bit_packed_contact_fields_from_descriptor() {
        let layout = Layout {
            report_id: 1,
            input_mode_report_id: None,
            contact_slots: 2,
            bytes_per_contact: 4,
            fingers_offset: 1,
            fingers_bit_offset: 8,
            scan_time_offset: 9,
            contact_count_offset: 11,
            button_offset: 12,
            button_bit: 0,
            logical_x_max: 4095,
            logical_y_max: 4095,
            physical_x_max_mm: 40.0,
            physical_y_max_mm: 30.0,
            total_payload_bytes: 13,
            contact_fields: Some(ContactBitFields {
                stride_bits: 30,
                tip: BitField { bit_offset: 0, bit_width: 1 },
                confidence: Some(BitField { bit_offset: 1, bit_width: 1 }),
                id: BitField { bit_offset: 2, bit_width: 4 },
                x: BitField { bit_offset: 6, bit_width: 12 },
                y: BitField { bit_offset: 18, bit_width: 12 },
            }),
            scan_time_field: Some(BitField { bit_offset: 68, bit_width: 16 }),
            contact_count_field: Some(BitField { bit_offset: 84, bit_width: 4 }),
            button_field: Some(BitField { bit_offset: 88, bit_width: 1 }),
        };
        let mut buf = vec![0u8; 13];
        buf[0] = 1;
        fn put(buf: &mut [u8], offset: usize, width: u8, value: u64) {
            for i in 0..width as usize {
                if (value >> i) & 1 != 0 {
                    buf[(offset + i) / 8] |= 1 << ((offset + i) % 8);
                }
            }
        }
        put(&mut buf, 8, 1, 1);
        put(&mut buf, 9, 1, 1);
        put(&mut buf, 10, 4, 5);
        put(&mut buf, 14, 12, 2048);
        put(&mut buf, 26, 12, 1024);
        put(&mut buf, 38, 1, 1);
        put(&mut buf, 39, 1, 0);
        put(&mut buf, 40, 4, 9);
        put(&mut buf, 44, 12, 3072);
        put(&mut buf, 56, 12, 2048);
        put(&mut buf, 68, 16, 0x1234);
        put(&mut buf, 84, 4, 2);
        put(&mut buf, 88, 1, 1);

        let frame = decode(&layout, &buf).expect("decode bit-packed report");
        assert_eq!(frame.contacts.len(), 2);
        assert_eq!(frame.contacts[0].id, 5);
        assert!(frame.contacts[0].tip && frame.contacts[0].confidence);
        assert_eq!(frame.contacts[1].id, 9);
        assert!(frame.contacts[1].tip && !frame.contacts[1].confidence);
        assert_eq!(frame.scan_time_100us, 0x1234);
        assert!(frame.button);
    }
}
