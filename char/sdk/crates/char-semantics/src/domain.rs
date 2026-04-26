// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! Domain abstraction: DomainId from preimage, canonical hash per Char node.

use bitcoin::hashes::Hash as _;
use char_utils::{
    DomainHash, HexParseError, domain_hash, domain_hash_from_hex, domain_hash_to_hex,
};
use std::fmt;
use thiserror::Error;

/// Canonical 32-byte domain identifier. SHA256(CompactSize(len) || preimage) per Char node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DomainId(pub [u8; 32]);

impl DomainId {
    /// From raw preimage bytes.
    pub fn from_preimage(preimage: &[u8]) -> Self {
        DomainId(domain_hash(preimage).to_byte_array())
    }

    /// From hex-encoded preimage (e.g. RPC domain_preimage_hex). Strips 0x.
    pub fn from_preimage_hex(hex: &str) -> Result<Self, DomainError> {
        let hash = domain_hash_from_hex(hex).map_err(DomainError::from)?;
        Ok(DomainId(hash.to_byte_array()))
    }
}

/// Lowercase hex, 64 characters (same as [`char_utils::domain_hash_to_hex`] on the id bytes).
impl fmt::Display for DomainId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&domain_hash_to_hex(&DomainHash::from_byte_array(self.0)))
    }
}

/// Domain preimage / domain id errors.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum DomainError {
    #[error("domain preimage empty")]
    EmptyPreimage,

    #[error("domain preimage hex: {0}")]
    InvalidHex(#[from] HexParseError),

    #[error("domain preimage decoded to empty bytes")]
    PreimageBytesEmpty,
}

impl From<char_utils::UtilsError> for DomainError {
    fn from(e: char_utils::UtilsError) -> Self {
        match e {
            char_utils::UtilsError::EmptyPreimage => DomainError::EmptyPreimage,
            char_utils::UtilsError::HexParse(h) => DomainError::InvalidHex(h),
            char_utils::UtilsError::PreimageBytesEmpty => DomainError::PreimageBytesEmpty,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_preimage_hex_empty_err() {
        assert!(matches!(
            DomainId::from_preimage_hex(""),
            Err(DomainError::EmptyPreimage)
        ));
        assert!(matches!(
            DomainId::from_preimage_hex("   "),
            Err(DomainError::EmptyPreimage)
        ));
    }

    #[test]
    fn from_preimage_hex_invalid_err() {
        assert!(matches!(
            DomainId::from_preimage_hex("zz"),
            Err(DomainError::InvalidHex(_))
        ));
    }

    #[test]
    fn from_preimage_hex_roundtrip() {
        let preimage_hex = "636861722e6e6574776f726b2f68656c6c6f";
        let id = DomainId::from_preimage_hex(preimage_hex).unwrap();
        assert_eq!(id.0.len(), 32);
    }

    #[test]
    fn from_preimage_hex_strips_0x() {
        let without = DomainId::from_preimage_hex("deadbeef").unwrap();
        let with = DomainId::from_preimage_hex("0xdeadbeef").unwrap();
        assert_eq!(without.0, with.0);
    }

    #[test]
    fn from_preimage_deterministic() {
        let preimage = b"char.network/hello";
        let a = DomainId::from_preimage(preimage);
        let b = DomainId::from_preimage(preimage);
        assert_eq!(a, b);
    }
}
