// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! Integration: utils + framing + semantics (domain, vote encode/decode). No transport.

use char_sdk::{
    decode_referendum_vote_hex, domain_hash, domain_hash_to_hex, encode_referendum_vote_hex,
    hex_to_bytes, DomainId, REFERENDUM_VOTE_LEAF_TYPE,
};

#[test]
fn domain_hash_utils_and_semantics_domain_id_match() {
    let preimage = b"char.network/hello";
    let hash = domain_hash(preimage);
    let id = DomainId::from_preimage(preimage);
    let hex = domain_hash_to_hex(&hash);
    assert_eq!(id.to_string(), hex);
}

#[test]
fn domain_id_from_hex_roundtrip() {
    let preimage_hex = "636861722e6e6574776f726b"; // "char.network" in hex
    let id = DomainId::from_preimage_hex(preimage_hex).unwrap();
    let hex_out = id.to_string();
    assert_eq!(hex_out.len(), 64);
    let id2 = DomainId::from_preimage_hex(preimage_hex).unwrap();
    assert_eq!(id, id2);
}

#[test]
fn vote_encode_decode_framing_roundtrip() {
    let payload = b"hello";
    let hex = encode_referendum_vote_hex(payload);
    let p = decode_referendum_vote_hex(&hex).unwrap();
    assert_eq!(p, payload);
}

#[test]
fn vote_hex_uses_utils_encoding() {
    use char_sdk::encode_referendum_vote;
    let bytes = encode_referendum_vote(b"x");
    // One byte payload: CompactSize(1) = 0x01, then 'x'.
    assert_eq!(bytes, vec![1, b'x']);
    let hex = encode_referendum_vote_hex(b"x");
    let decoded = hex_to_bytes(&hex).unwrap();
    assert_eq!(decoded, bytes);
    assert_eq!(REFERENDUM_VOTE_LEAF_TYPE, 0);
}
