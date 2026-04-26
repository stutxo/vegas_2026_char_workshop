// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! Bitcoin-style CompactSize encoding (used in Char ZMQ notifications after varint->CompactSize change).
//!
//! Format: value < 253 -> 1 byte; <= 0xFFFF -> 0xfd + 2-byte LE; <= 0xFFFFFFFF -> 0xfe + 4-byte LE; else 0xff + 8-byte LE.
//! See `src/serialize.h` WriteCompactSize / ReadCompactSize.

/// Read a CompactSize-encoded u64 from the start of `buf`.
/// Returns `Some((value, bytes_consumed))` or `None` if buffer too short or invalid.
#[inline]
pub fn read_compact_size_from_slice(buf: &[u8]) -> Option<(u64, usize)> {
    if buf.is_empty() {
        return None;
    }
    let b0 = buf[0];
    if b0 < 253 {
        return Some((b0 as u64, 1));
    }
    if b0 == 253 {
        if buf.len() < 3 {
            return None;
        }
        let n = u16::from_le_bytes([buf[1], buf[2]]) as u64;
        if n < 253 {
            return None; // non-canonical
        }
        return Some((n, 3));
    }
    if b0 == 254 {
        if buf.len() < 5 {
            return None;
        }
        let n = u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]) as u64;
        if n < 0x1_0000 {
            return None; // non-canonical
        }
        return Some((n, 5));
    }
    // 255
    if buf.len() < 9 {
        return None;
    }
    let n = u64::from_le_bytes([
        buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7], buf[8],
    ]);
    if n < 0x1_0000_0000 {
        return None; // non-canonical
    }
    Some((n, 9))
}

/// Append a CompactSize-encoded u64 to `out`.
#[inline]
pub fn write_compact_size(out: &mut Vec<u8>, value: u64) {
    if value < 253 {
        out.push(value as u8);
        return;
    }
    if value <= 0xFFFF {
        out.push(253);
        out.extend_from_slice(&(value as u16).to_le_bytes());
        return;
    }
    if value <= 0xFFFF_FFFF {
        out.push(254);
        out.extend_from_slice(&(value as u32).to_le_bytes());
        return;
    }
    out.push(255);
    out.extend_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_size_one_byte() {
        assert_eq!(read_compact_size_from_slice(&[0]), Some((0, 1)));
        assert_eq!(read_compact_size_from_slice(&[1]), Some((1, 1)));
        assert_eq!(read_compact_size_from_slice(&[252]), Some((252, 1)));
    }

    #[test]
    fn compact_size_three_bytes() {
        // 253 in LE: fd 00 fd (253 as u16 LE)
        assert_eq!(read_compact_size_from_slice(&[253, 253, 0]), Some((253, 3)));
        assert_eq!(read_compact_size_from_slice(&[253, 0xff, 0xff]), Some((0xffff, 3)));
        assert_eq!(read_compact_size_from_slice(&[253, 1, 2]), Some((0x0201, 3)));
    }

    #[test]
    fn compact_size_five_bytes() {
        assert_eq!(
            read_compact_size_from_slice(&[254, 0, 0, 1, 0]),
            Some((0x1_0000, 5))
        );
    }

    #[test]
    fn compact_size_nine_bytes() {
        assert_eq!(
            read_compact_size_from_slice(&[255, 0, 0, 0, 0, 1, 0, 0, 0]),
            Some((0x1_0000_0000, 9))
        );
    }

    #[test]
    fn compact_size_too_short() {
        assert_eq!(read_compact_size_from_slice(&[]), None);
        assert_eq!(read_compact_size_from_slice(&[253]), None);
        assert_eq!(read_compact_size_from_slice(&[253, 0]), None);
        assert_eq!(read_compact_size_from_slice(&[254, 0, 0, 0]), None);
        assert_eq!(read_compact_size_from_slice(&[255, 0, 0, 0, 0, 0, 0, 0]), None);
    }

    #[test]
    fn compact_size_non_canonical() {
        // 253 encoded as 1 byte would be canonical; 253 as 3-byte must be >= 253
        assert_eq!(read_compact_size_from_slice(&[253, 0, 0]), None); // 0 < 253
        assert_eq!(read_compact_size_from_slice(&[253, 252, 0]), None); // 252 < 253
    }

    #[test]
    fn compact_size_write_roundtrip() {
        let values = [0u64, 1, 252, 253, 0xFFFF, 0x1_0000, 0x1_0000_0000];
        for v in values {
            let mut out = Vec::new();
            write_compact_size(&mut out, v);
            assert_eq!(read_compact_size_from_slice(&out), Some((v, out.len())));
        }
    }
}
