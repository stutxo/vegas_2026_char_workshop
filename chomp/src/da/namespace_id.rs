use thiserror::Error;

pub(crate) const PREFIX_LEN: usize = 3;
pub(crate) type TxidPrefix = [u8; PREFIX_LEN];
/// Minimum number of bytes required in a valid [`NamespaceId`].
pub const MIN_NAMESPACE_ID_LEN: usize = PREFIX_LEN;

/// Validation error returned while constructing a [`NamespaceId`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum NamespaceIdError {
    #[error("namespace_id must contain at least {PREFIX_LEN} bytes; got {len}")]
    NamespaceIdTooShort { len: usize },
}

/// Validated namespace identifier used to derive txid prefixes for Bitcoin and Liquid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamespaceId(Vec<u8>);

impl NamespaceId {
    /// Build a validated namespace identifier from owned or borrowed bytes.
    pub fn new<T>(namespace_id: T) -> Result<Self, NamespaceIdError>
    where
        T: Into<Vec<u8>>,
    {
        Self::try_from(namespace_id.into())
    }

    /// Borrow the validated namespace-id bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Consume the identifier and return the underlying bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }

    pub(crate) fn prefix(&self) -> TxidPrefix {
        derive_prefix(self.as_ref()).expect("NamespaceId instances are validated on construction")
    }
}

impl AsRef<[u8]> for NamespaceId {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl From<NamespaceId> for Vec<u8> {
    fn from(namespace_id: NamespaceId) -> Self {
        namespace_id.into_bytes()
    }
}

impl TryFrom<Vec<u8>> for NamespaceId {
    type Error = NamespaceIdError;

    fn try_from(namespace_id: Vec<u8>) -> Result<Self, Self::Error> {
        validate_namespace_id(&namespace_id)?;
        Ok(Self(namespace_id))
    }
}

impl TryFrom<&[u8]> for NamespaceId {
    type Error = NamespaceIdError;

    fn try_from(namespace_id: &[u8]) -> Result<Self, Self::Error> {
        Self::try_from(namespace_id.to_vec())
    }
}

/// Uses the first 3 bytes of `namespace_id` as the target txid prefix.
///
/// The returned bytes are in displayed-txid byte order: prefix[0] is the
/// first hex byte you see in a block explorer.
pub(crate) fn derive_prefix(namespace_id: &[u8]) -> Result<TxidPrefix, NamespaceIdError> {
    validate_namespace_id(namespace_id)?;
    namespace_id
        .get(..PREFIX_LEN)
        .ok_or(NamespaceIdError::NamespaceIdTooShort {
            len: namespace_id.len(),
        })
        .map(|prefix| prefix.try_into().expect("slice length already validated"))
}

fn validate_namespace_id(namespace_id: &[u8]) -> Result<(), NamespaceIdError> {
    if namespace_id.len() < MIN_NAMESPACE_ID_LEN {
        return Err(NamespaceIdError::NamespaceIdTooShort {
            len: namespace_id.len(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{NamespaceId, NamespaceIdError, derive_prefix};

    #[test]
    fn derive_prefix_uses_namespace_id_bytes_directly() {
        let prefix = derive_prefix(b"abc123").expect("namespace id should yield a prefix");
        assert_eq!(prefix, *b"abc");
    }

    #[test]
    fn derive_prefix_rejects_short_namespace_ids() {
        assert_eq!(
            derive_prefix(b"ab"),
            Err(NamespaceIdError::NamespaceIdTooShort { len: 2 })
        );
    }

    #[test]
    fn namespace_id_new_rejects_short_values() {
        assert_eq!(
            NamespaceId::new(b"ab"),
            Err(NamespaceIdError::NamespaceIdTooShort { len: 2 })
        );
    }

    #[test]
    fn namespace_id_new_accepts_exact_minimum_length() {
        let namespace_id = NamespaceId::new(b"abc").expect("3-byte namespace id should validate");
        assert_eq!(namespace_id.as_bytes(), b"abc");
        assert_eq!(namespace_id.prefix(), *b"abc");
    }
}
