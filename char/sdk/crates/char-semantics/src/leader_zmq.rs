// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! Leader ZMQ wire format: body is [ballot][32-byte domain hash].
//!
//! Node uses **CompactSize** for ballot (see upstream "[char] Use CompactSize in char ZMQ notifications").
//! Older nodes may have used varint; for ballot < 253 the encoding is the same (single byte).

use crate::domain::DomainId;
use char_framing::read_compact_size_from_slice;

/// Decode Char leader ZMQ message body. Returns ballot and domain id if body is valid.
///
/// **Wire format:** The node sends `[CompactSize(ballot)][32-byte domain hash]`.
/// - **CompactSize(ballot):** Bitcoin-style encoding: 1 byte if ballot < 253; else 0xfd + 2-byte LE,
///   0xfe + 4-byte LE, or 0xff + 8-byte LE. We read it with `read_compact_size_from_slice`.
/// - **Domain hash:** Exactly 32 bytes (SHA256 of domain identifier). Parsed as `DomainId`.
///
/// **How we decode:** Read the first CompactSize from the start of `body`; that gives the ballot
/// and the number of bytes consumed. The body must be **exactly** `consumed + 32` bytes: the next
/// 32 bytes are the domain hash. Extra trailing bytes yield `None` (strict parse).
#[inline]
pub fn decode_leader_zmq_body(body: &[u8]) -> Option<(u64, DomainId)> {
    let (ballot, consumed) = read_compact_size_from_slice(body)?;
    let end = consumed.checked_add(32)?;
    if body.len() != end {
        return None;
    }
    let domain_bytes: [u8; 32] = body[consumed..end].try_into().ok()?;
    Some((ballot, DomainId(domain_bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_exact_length() {
        let mut body = vec![1u8];
        body.extend_from_slice(&[0x55; 32]);
        assert_eq!(body.len(), 33);
        let (b, d) = decode_leader_zmq_body(&body).unwrap();
        assert_eq!(b, 1);
        assert_eq!(d.0, [0x55; 32]);
    }

    #[test]
    fn rejects_trailing_byte() {
        let mut body = vec![1u8];
        body.extend_from_slice(&[0x55; 32]);
        body.push(0);
        assert!(decode_leader_zmq_body(&body).is_none());
    }
}
