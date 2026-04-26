// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! Typed errors for char-utils

use std::fmt;

/// Why hex decoding failed (finite classification; no arbitrary strings).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HexParseError {
    Empty,
    OddLength,
    InvalidNibble { byte: u8 },
}

impl fmt::Display for HexParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HexParseError::Empty => f.write_str("empty hex"),
            HexParseError::OddLength => f.write_str("odd-length hex"),
            HexParseError::InvalidNibble { byte } => {
                write!(f, "non-hex byte {byte:#04x}")
            }
        }
    }
}

impl std::error::Error for HexParseError {}

/// Errors from domain hash, hex, or other utility operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UtilsError {
    /// Domain preimage was empty (node: "Domain cannot be empty").
    EmptyPreimage,
    /// Hex string could not be parsed.
    HexParse(HexParseError),
    /// Hex decoded to zero bytes where non-empty preimage bytes are required.
    PreimageBytesEmpty,
}

impl fmt::Display for UtilsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UtilsError::EmptyPreimage => f.write_str("domain preimage empty"),
            UtilsError::HexParse(e) => write!(f, "hex: {e}"),
            UtilsError::PreimageBytesEmpty => {
                f.write_str("domain preimage decoded to empty bytes")
            }
        }
    }
}

impl std::error::Error for UtilsError {}
