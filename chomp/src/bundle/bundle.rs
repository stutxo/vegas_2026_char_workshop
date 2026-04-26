use super::codec::{CodecError, decode_borsh_bundle, encode_borsh_bundle};
use super::payload::BorshPayload;
use borsh::{BorshDeserialize, BorshSerialize};

/// Version tag carried by a [`BorshBundle`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize, Default)]
pub enum BorshBundleVersion {
    #[default]
    V1,
}

/// Collection of validated payload items encoded as one bundle value.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BorshBundle<T> {
    version: BorshBundleVersion,
    items: Vec<T>,
}

impl<T> BorshBundle<T>
where
    T: BorshPayload,
{
    /// Build a validated bundle from typed items.
    pub fn new(items: Vec<T>) -> Result<Self, CodecError> {
        let bundle = Self {
            version: BorshBundleVersion::V1,
            items,
        };
        bundle.validate()?;
        Ok(bundle)
    }

    /// Return the bundle version.
    pub fn version(&self) -> BorshBundleVersion {
        self.version
    }

    /// Borrow the items in the bundle.
    pub fn items(&self) -> &[T] {
        &self.items
    }

    /// Return the number of items in the bundle.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Return `true` when the bundle contains no items.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Consume the bundle and return the contained items.
    pub fn into_items(self) -> Vec<T> {
        self.items
    }

    /// Encode the bundle to validated Borsh bytes.
    pub fn encode(&self) -> Result<Vec<u8>, CodecError> {
        encode_borsh_bundle(self)
    }

    /// Decode and validate a bundle from bytes.
    pub fn decode(bytes: impl AsRef<[u8]>) -> Result<Self, CodecError> {
        decode_borsh_bundle(bytes)
    }
}

impl<T> BorshPayload for BorshBundle<T>
where
    T: BorshPayload,
{
    fn validate(&self) -> Result<(), CodecError> {
        if self.items.is_empty() {
            return Err(CodecError::EmptyBundle);
        }

        for item in &self.items {
            item.validate()?;
        }

        Ok(())
    }
}
