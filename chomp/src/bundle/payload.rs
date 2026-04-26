use super::codec::CodecError;
use borsh::{BorshDeserialize, BorshSerialize};

/// Trait implemented by typed payloads that can be encoded and stored by CHOMP.
pub trait BorshPayload: BorshSerialize + BorshDeserialize + Send + Sync + 'static {
    /// Validate the payload before encoding and after decoding.
    fn validate(&self) -> Result<(), CodecError> {
        Ok(())
    }
}

/// Raw byte payload wrapper for callers that do not want a custom Borsh type.
#[derive(Debug, Clone, PartialEq, Eq, Default, BorshSerialize, BorshDeserialize)]
pub struct ChompPayload(Vec<u8>);

impl ChompPayload {
    /// Construct a payload from owned bytes.
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// Borrow the underlying bytes.
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    /// Return the payload length in bytes.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Return `true` when the payload is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Consume the payload and return the underlying bytes.
    pub fn into_vec(self) -> Vec<u8> {
        self.0
    }
}

impl AsRef<[u8]> for ChompPayload {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl From<Vec<u8>> for ChompPayload {
    fn from(bytes: Vec<u8>) -> Self {
        Self::new(bytes)
    }
}

impl From<&[u8]> for ChompPayload {
    fn from(bytes: &[u8]) -> Self {
        Self::new(bytes.to_vec())
    }
}

impl From<ChompPayload> for Vec<u8> {
    fn from(value: ChompPayload) -> Self {
        value.into_vec()
    }
}

impl BorshPayload for ChompPayload {}
