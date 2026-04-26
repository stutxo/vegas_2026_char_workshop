// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! Char SDK - vote and precommit wire formats.
//! See SDKSPEC.md Section 2.2 and Section 4.7.

mod compact_size;
mod error;
mod vote;

pub use compact_size::{read_compact_size_from_slice, write_compact_size};
pub use error::FramingError;
pub use vote::{
    decode_referendum_vote, decode_referendum_vote_hex, encode_referendum_vote,
    encode_referendum_vote_hex, REFERENDUM_VOTE_LEAF_TYPE,
};
