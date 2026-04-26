use super::bundle::BorshBundle;
use super::payload::BorshPayload;
use crate::{DaError, SemanticError, UsageError};
use thiserror::Error;

/// Encoding or decoding error returned by the bundle codec helpers.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CodecError {
    #[error("borsh serialization failed: {0}")]
    Serialize(String),

    #[error("borsh deserialization failed: {0}")]
    Deserialize(String),

    #[error("bundle must contain at least one item")]
    EmptyBundle,
}

impl From<CodecError> for DaError {
    fn from(value: CodecError) -> Self {
        match value {
            CodecError::Serialize(message) => UsageError::InvalidRequest(message).into(),
            CodecError::Deserialize(message) => SemanticError::DecodeFailure(message).into(),
            CodecError::EmptyBundle => {
                SemanticError::DecodeFailure("bundle must contain at least one item".to_string())
                    .into()
            }
        }
    }
}

/// Encode a validated Borsh payload value.
pub fn encode_borsh<T>(value: &T) -> Result<Vec<u8>, CodecError>
where
    T: BorshPayload,
{
    value.validate()?;
    borsh::to_vec(value).map_err(|err| CodecError::Serialize(err.to_string()))
}

/// Decode and validate a Borsh payload value.
pub fn decode_borsh<T>(bytes: impl AsRef<[u8]>) -> Result<T, CodecError>
where
    T: BorshPayload,
{
    let value = borsh::from_slice::<T>(bytes.as_ref())
        .map_err(|err| CodecError::Deserialize(err.to_string()))?;
    value.validate()?;
    Ok(value)
}

/// Encode a validated [`BorshBundle`].
pub fn encode_borsh_bundle<T>(bundle: &BorshBundle<T>) -> Result<Vec<u8>, CodecError>
where
    T: BorshPayload,
{
    encode_borsh(bundle)
}

/// Decode and validate a [`BorshBundle`].
pub fn decode_borsh_bundle<T>(bytes: impl AsRef<[u8]>) -> Result<BorshBundle<T>, CodecError>
where
    T: BorshPayload,
{
    decode_borsh(bytes)
}
