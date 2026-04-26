// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! Small, fixed-size UTF-8 buffers for **optional human text** on errors.
//!
//! Use this when the wire or peer can supply an arbitrary string (e.g. JSON-RPC `message`) and you
//! still want to show something in logs or `Display` without allocating unbounded `String` payloads
//! inside your public error types. Classification stays in enum variants; `ShortDiag` is only a
//! clipped snippet for presentation.

use std::fmt;

/// At most [`ShortDiag::CAP`] UTF-8 bytes, truncated on char boundaries. For logging / `Display` only.
#[derive(Clone, PartialEq, Eq)]
pub struct ShortDiag {
    bytes: [u8; Self::CAP],
    len: u8,
}

impl ShortDiag {
    pub const CAP: usize = 64;

    /// Truncate `s` to at most [`CAP`] UTF-8 bytes without splitting a codepoint.
    pub fn truncate(s: &str) -> Self {
        let mut bytes = [0u8; Self::CAP];
        let mut len = 0usize;
        for ch in s.chars() {
            let mut tmp = [0u8; 4];
            let enc = ch.encode_utf8(&mut tmp);
            let b = enc.as_bytes();
            if len + b.len() > Self::CAP {
                break;
            }
            bytes[len..len + b.len()].copy_from_slice(b);
            len += b.len();
        }
        Self {
            bytes,
            len: len as u8,
        }
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }
}

impl fmt::Debug for ShortDiag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ShortDiag").field(&self.as_str()).finish()
    }
}

impl fmt::Display for ShortDiag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
