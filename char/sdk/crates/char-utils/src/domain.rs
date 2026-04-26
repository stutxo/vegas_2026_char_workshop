// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! Domain hash: canonical 32-byte identifier from preimage.
//!
//! Matches Char node `DomainToDomainIdentifier` in `src/rpc/char_rpc.cpp`:
//! `HashWriter h; h << DomainToBytes(domain);` - in Bitcoin Core, serializing
//! a `vector<unsigned char>` is **CompactSize(len) then raw bytes**.

use bitcoin::consensus::Encodable;
use bitcoin::hashes::sha256::Hash as Sha256Hash;
use bitcoin::hashes::Hash;
use crate::error::UtilsError;
use crate::hex;
use sha2::{Digest, Sha256};

/// 32-byte domain identifier digest (SHA256 of CompactSize-encoded preimage).
pub type DomainHash = Sha256Hash;

/// Compute the domain hash from preimage bytes.
/// Uses bitcoin crate's Encodable for Vec<u8> (CompactSize(len) || bytes), then SHA256.
pub fn domain_hash(preimage: &[u8]) -> DomainHash {
    let mut preimage_encoded = Vec::new();
    preimage
        .to_vec()
        .consensus_encode(&mut preimage_encoded)
        .expect("Vec::write cannot fail");
    let mut hasher = Sha256::new();
    hasher.update(&preimage_encoded);
    let bytes: [u8; 32] = hasher.finalize().into();
    DomainHash::from_byte_array(bytes)
}

/// Compute domain hash from a hex-encoded preimage (e.g. RPC `domain_preimage_hex`).
/// Fails if hex is empty or invalid (matches node error semantics).
/// A leading `0x` or `0X` is stripped before decoding.
pub fn domain_hash_from_hex(preimage_hex: &str) -> Result<DomainHash, UtilsError> {
    let trimmed = preimage_hex.trim();
    if trimmed.is_empty() {
        return Err(UtilsError::EmptyPreimage);
    }
    let hex_str = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")).unwrap_or(trimmed);
    let bytes = hex::hex_to_bytes(hex_str)?;
    if bytes.is_empty() {
        return Err(UtilsError::PreimageBytesEmpty);
    }
    Ok(domain_hash(&bytes))
}

/// Format a domain hash as lowercase hex string (e.g. for logging or RPC comparison).
pub fn domain_hash_to_hex(hash: &DomainHash) -> String {
    hex::bytes_to_hex(hash.as_byte_array())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_preimage_hex_err() {
        assert!(matches!(
            domain_hash_from_hex(""),
            Err(UtilsError::EmptyPreimage)
        ));
        assert!(matches!(
            domain_hash_from_hex("   "),
            Err(UtilsError::EmptyPreimage)
        ));
    }

    #[test]
    fn invalid_hex_err() {
        assert!(matches!(
            domain_hash_from_hex("zz"),
            Err(UtilsError::HexParse(_))
        ));
    }

    #[test]
    fn domain_hash_deterministic() {
        let preimage = b"char.network/hello";
        let h1 = domain_hash(preimage);
        let h2 = domain_hash(preimage);
        assert_eq!(h1, h2);
    }

    #[test]
    fn domain_hash_uses_compact_size_prefix() {
        // Raw SHA256("hello") would differ from SHA256(CompactSize(5)||"hello").
        // hello-char and node both use CompactSize||preimage.
        let preimage = b"hello";
        let h = domain_hash(preimage);
        // Sanity: different from plain SHA256(preimage)
        let mut plain = Sha256::new();
        plain.update(preimage);
        let plain_hash: [u8; 32] = plain.finalize().into();
        assert_ne!(
            *h.as_byte_array(),
            plain_hash,
            "domain_hash must use CompactSize prefix"
        );
    }

    #[test]
    fn domain_hash_from_hex_matches_hello_char_style() {
        // hello-char-demo uses consensus_encode(Vec) = CompactSize(len)||bytes.
        let preimage_hex = "636861722e6e6574776f726b2f68656c6c6f"; // "char.network/hello"
        let hash = domain_hash_from_hex(preimage_hex).unwrap();
        let bytes = hex::hex_to_bytes(preimage_hex).unwrap();
        assert_eq!(domain_hash(&bytes), hash);
    }

    #[test]
    fn domain_hash_from_hex_strips_0x() {
        let without = domain_hash_from_hex("deadbeef").unwrap();
        let with = domain_hash_from_hex("0xdeadbeef").unwrap();
        assert_eq!(without, with);
    }

    #[test]
    fn domain_hash_to_hex_roundtrip() {
        let original = DomainHash::from_byte_array([1u8; 32]);
        let hex = domain_hash_to_hex(&original);
        let decoded = hex::hex_to_bytes(&hex).unwrap();
        assert_eq!(decoded.as_slice(), original.as_byte_array().as_slice());
    }
}
