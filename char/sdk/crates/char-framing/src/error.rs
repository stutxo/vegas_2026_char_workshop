// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! Typed errors for char-framing.

use char_utils::{HexParseError, ShortDiag};
use std::fmt;
use std::io;

/// Errors from vote/precommit encode or decode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FramingError {
    /// Hex input invalid (see [`HexParseError`]).
    HexParse(HexParseError),
    /// Unexpected leaf type (expected 0 for ReferendumVote).
    UnexpectedLeafType(u8),
    /// Declared payload length does not fit in `usize`.
    PayloadLengthOverflow,
    /// Read past end of buffer or other I/O while decoding.
    Io(io::ErrorKind),
    /// Bitcoin `consensus_decode` / varint decode failed on the vote blob.
    ConsensusDecode,
    /// Bytes remain after the full vote blob (strict decode). `remainder_hex` is clipped for logs.
    TrailingBytes {
        consumed: usize,
        total_len: usize,
        remainder_len: usize,
        remainder_hex: ShortDiag,
    },
}

impl fmt::Display for FramingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FramingError::HexParse(e) => write!(f, "hex: {e}"),
            FramingError::UnexpectedLeafType(t) => write!(f, "unexpected leaf type: {t}"),
            FramingError::PayloadLengthOverflow => f.write_str("payload length overflow"),
            FramingError::Io(k) => write!(f, "io: {k}"),
            FramingError::ConsensusDecode => f.write_str("consensus decode failed"),
            FramingError::TrailingBytes {
                consumed,
                total_len,
                remainder_len,
                remainder_hex,
            } => write!(
                f,
                "trailing bytes after referendum vote: consumed {consumed}/{total_len} bytes, \
                 {remainder_len} byte(s) left, hex {}",
                remainder_hex.as_str()
            ),
        }
    }
}

impl std::error::Error for FramingError {}
