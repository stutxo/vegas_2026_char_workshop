// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! Char SDK - shared utilities (domain hash, hex, CompactSize, helpers).
//! See SDKSPEC.md Section 2.2 and Section 2.3.
//!
//! **Domain hash** uses the bitcoin crate's Encodable for Vec<u8> (CompactSize(len) || bytes)
//! that is then hashed with SHA256.
//!
//! **Hex**: `hex_to_bytes` / `bytes_to_hex`; `strip_0x_prefix` for caller-controlled normalization.
//! [`domain_hash_from_hex`] accepts an optional `0x` / `0X` prefix on preimage hex.

mod bounded;
pub mod constants;
mod domain;
mod error;
mod hex;

pub use bounded::ShortDiag;
pub use constants::{
    GET_REFERENDUM_DECISION_ROLL_MAX_INCLUSIVE_SPAN, GET_REFERENDUM_DECISION_ROLL_MAX_RANGE,
    MAX_CHAR_BAMBOO_SIZE,
};
pub use domain::{domain_hash, domain_hash_from_hex, domain_hash_to_hex, DomainHash};
pub use error::{HexParseError, UtilsError};
pub use hex::{bytes_to_hex, hex_to_bytes, strip_0x_prefix};
