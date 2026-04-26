//! Encoding-facing types and helpers.
//!
//! Most applications can import the main bundle items from the crate root. This
//! module is useful when you want the encoding surface grouped together.
mod bundle;
mod codec;
mod payload;
#[cfg(test)]
mod tests;

pub use bundle::{BorshBundle, BorshBundleVersion};
pub use codec::{CodecError, decode_borsh, decode_borsh_bundle, encode_borsh, encode_borsh_bundle};
pub use payload::{BorshPayload, ChompPayload};
