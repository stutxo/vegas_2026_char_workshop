// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit.

//! Referendum vote wire for the **`ReferendumVote` body** Char serializes on the stream.
//!
//! **Char node (`src/char/primitives/referendum_vote.h`):** `SERIALIZE_METHODS` only
//! `READWRITE(obj.data)`. That is Bitcoin **`std::vector<uint8_t>`** serialization: **CompactSize(len)
//! + raw payload bytes**. Ballot is **not** on this byte stream; the node supplies it from context
//! (`FromBamboo(..., BallotNumber)`, pending ballot for RPC).
//!
//! **RPC `addreferendumvote` / `getreferendumdecisionroll` `decision_roll.data`:** hex of **payload
//! bytes only** (no CompactSize in the hex string). Use `char_utils::bytes_to_hex` / `hex_to_bytes`
//! for those strings. [`encode_referendum_vote`] is for the **binary** vote body when you need the
//! same layout as the node’s `data` serialization (e.g. tests, adapters), not the RPC hex field.
//!
//! **Full Bamboo leaf** (`WithLeafType` + tag + value) is tag **CompactSize(`LeafType`)** then this
//! body; the SDK does not assemble that wrapper here—only the inner `data` vector encoding.

use crate::compact_size::{read_compact_size_from_slice, write_compact_size};
use crate::error::FramingError;
use char_utils::{bytes_to_hex, hex_to_bytes, strip_0x_prefix, ShortDiag};
use std::io;

/// `Char::LeafType::REFERENDUM_VOTE` discriminant (`src/char/primitives/leaf_types.h`).
/// Not necessarily the first byte of [`encode_referendum_vote`] output (that is CompactSize(payload
/// length), which is `0` only when the payload is empty).
pub const REFERENDUM_VOTE_LEAF_TYPE: u8 = 0;

/// Encode the referendum vote **`data` field** only: CompactSize(payload_len) || payload.
pub fn encode_referendum_vote(payload: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(9 + payload.len());
    write_compact_size(&mut buf, payload.len() as u64);
    buf.extend_from_slice(payload);
    buf
}

/// Hex of [`encode_referendum_vote`]. Lowercase, no `0x` prefix.
pub fn encode_referendum_vote_hex(payload: &[u8]) -> String {
    bytes_to_hex(&encode_referendum_vote(payload))
}

/// Decode bytes produced by [`encode_referendum_vote`] (CompactSize length + payload, exact length).
pub fn decode_referendum_vote(bytes: &[u8]) -> Result<Vec<u8>, FramingError> {
    let Some((len, consumed)) = read_compact_size_from_slice(bytes) else {
        return Err(FramingError::ConsensusDecode);
    };
    let len: usize = len
        .try_into()
        .map_err(|_| FramingError::PayloadLengthOverflow)?;
    let end = consumed
        .checked_add(len)
        .ok_or(FramingError::PayloadLengthOverflow)?;
    if end > bytes.len() {
        return Err(FramingError::Io(io::ErrorKind::UnexpectedEof));
    }
    if end != bytes.len() {
        let remainder = &bytes[end..];
        let hex = bytes_to_hex(remainder);
        let summary = if hex.len() > 48 {
            format!("{}…+{}hex", &hex[..48], hex.len().saturating_sub(48))
        } else {
            hex
        };
        return Err(FramingError::TrailingBytes {
            consumed: end,
            total_len: bytes.len(),
            remainder_len: remainder.len(),
            remainder_hex: ShortDiag::truncate(&summary),
        });
    }
    Ok(bytes[consumed..end].to_vec())
}

/// Decode from hex (strips optional `0x` prefix).
pub fn decode_referendum_vote_hex(hex: &str) -> Result<Vec<u8>, FramingError> {
    let hex_str = strip_0x_prefix(hex);
    let bytes = hex_to_bytes(hex_str).map_err(|e| match e {
        char_utils::UtilsError::HexParse(h) => FramingError::HexParse(h),
        char_utils::UtilsError::EmptyPreimage | char_utils::UtilsError::PreimageBytesEmpty => {
            FramingError::HexParse(char_utils::HexParseError::Empty)
        }
    })?;
    decode_referendum_vote(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let payload = b"hello world";
        let bytes = encode_referendum_vote(payload);
        let p = decode_referendum_vote(&bytes).unwrap();
        assert_eq!(p, payload);
    }

    #[test]
    fn roundtrip_hex() {
        let payload = &[0xab];
        let hex = encode_referendum_vote_hex(payload);
        let p = decode_referendum_vote_hex(&hex).unwrap();
        assert_eq!(p, payload);
    }

    #[test]
    fn hex_strips_0x() {
        let hex = encode_referendum_vote_hex(b"x");
        let prefixed = format!("0x{hex}");
        let p = decode_referendum_vote_hex(&prefixed).unwrap();
        assert_eq!(p, b"x");
    }

    #[test]
    fn empty_payload() {
        let bytes = encode_referendum_vote(&[]);
        assert_eq!(bytes, vec![0u8]);
        let p = decode_referendum_vote(&bytes).unwrap();
        assert!(p.is_empty());
    }

    #[test]
    fn rejects_truncated_compact_size() {
        let bytes = [255u8];
        let err = decode_referendum_vote(&bytes).unwrap_err();
        assert!(matches!(err, FramingError::ConsensusDecode));
    }

    #[test]
    fn rejects_trailing_bytes() {
        let mut bytes = encode_referendum_vote(b"a");
        bytes.push(0xff);
        let err = decode_referendum_vote(&bytes).unwrap_err();
        assert!(matches!(err, FramingError::TrailingBytes { .. }));
    }

    #[test]
    fn compact_size_payload_len_boundary_253() {
        let payload: Vec<u8> = vec![b'z'; 253];
        let bytes = encode_referendum_vote(&payload);
        assert_eq!(&bytes[0..3], &[0xfd, 0xfd, 0x00]);
        let out = decode_referendum_vote(&bytes).unwrap();
        assert_eq!(out.len(), 253);
        assert!(out.iter().all(|&b| b == b'z'));
    }
}
