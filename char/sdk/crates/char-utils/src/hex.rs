// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! Hex encode/decode. Used by domain_hash_from_hex & by framing/semantics.

use crate::error::{HexParseError, UtilsError};
use std::fmt::Write;

/// Decode a hex string into bytes (lower- or uppercase).
/// Does not strip `0x`; use `strip_0x_prefix` first if needed (e.g. RPC hex strings).
/// Fails on odd length or non-hex characters.
pub fn hex_to_bytes(hex: &str) -> Result<Vec<u8>, UtilsError> {
    let hex = hex.trim();
    if hex.is_empty() {
        return Err(UtilsError::HexParse(HexParseError::Empty));
    }
    if hex.len() % 2 != 0 {
        return Err(UtilsError::HexParse(HexParseError::OddLength));
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    for chunk in hex.as_bytes().chunks(2) {
        let hi = nibble(chunk[0])?;
        let lo = nibble(chunk[1])?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn nibble(b: u8) -> Result<u8, UtilsError> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(UtilsError::HexParse(HexParseError::InvalidNibble { byte: b })),
    }
}

/// Strip optional `0x` / `0X` prefix. Returns the rest of the string.
/// Use before `hex_to_bytes` when decoding RPC or ZMQ hex that may be prefixed.
pub fn strip_0x_prefix(hex: &str) -> &str {
    let t = hex.trim_start();
    if t.starts_with("0x") || t.starts_with("0X") {
        &t[2..]
    } else {
        t
    }
}

/// Encode bytes as lowercase hex.
pub fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        let _ = write!(s, "{:02x}", b);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::HexParseError;

    #[test]
    fn hex_roundtrip() {
        let original = b"hello";
        let h = bytes_to_hex(original);
        assert_eq!(hex_to_bytes(&h).unwrap(), original);
    }

    #[test]
    fn hex_uppercase_accepted() {
        assert_eq!(hex_to_bytes("FF").unwrap(), vec![0xff]);
        assert_eq!(hex_to_bytes("ff").unwrap(), vec![0xff]);
    }

    #[test]
    fn empty_hex_err() {
        assert!(matches!(
            hex_to_bytes(""),
            Err(UtilsError::HexParse(HexParseError::Empty))
        ));
    }

    #[test]
    fn odd_length_err() {
        assert!(matches!(
            hex_to_bytes("a"),
            Err(UtilsError::HexParse(HexParseError::OddLength))
        ));
    }

    #[test]
    fn strip_0x_prefix_works() {
        assert_eq!(super::strip_0x_prefix("0xab"), "ab");
        assert_eq!(super::strip_0x_prefix("0Xab"), "ab");
        assert_eq!(super::strip_0x_prefix("ab"), "ab");
        assert_eq!(super::strip_0x_prefix("  0xab"), "ab");
    }
}
