use crate::bundle::{BorshPayload, decode_borsh, encode_borsh};
use crate::core::{DaError, MemberId, SemanticError, UsageError};
use crate::da::{BlobWriteReceipt, DaKey, DaVerifyReport, DynKey, Locator, PolicyKey};
use async_trait::async_trait;
use std::sync::Arc;

/// Object-safe blob DA interface implemented by leaf backends.
#[async_trait]
pub trait DataAvailability: Send + Sync {
    fn provider_kind(&self) -> &'static str;

    fn member_id(&self) -> MemberId;

    async fn write_blob(&self, bytes: &[u8]) -> Result<BlobWriteReceipt, DaError>;

    async fn read_blob(&self, key: &dyn DaKey) -> Result<Vec<u8>, DaError>;

    async fn verify_key(&self, key: &dyn DaKey) -> Result<DaVerifyReport, DaError> {
        let _ = self.read_blob(key).await?;
        Ok(DaVerifyReport::new(true, None))
    }

    fn decode_key(&self, locator: &Locator) -> Result<DynKey, DaError>;
}

/// Typed payload convenience methods layered above the blob interface.
#[async_trait]
pub trait DataAvailabilityExt: DataAvailability {
    async fn write<T>(&self, value: &T) -> Result<BlobWriteReceipt, DaError>
    where
        T: BorshPayload + Sync,
    {
        let bytes = encode_borsh(value).map_err(map_codec_error)?;
        self.write_blob(&bytes).await
    }

    async fn read<T>(&self, key: &PolicyKey) -> Result<T, DaError>
    where
        T: BorshPayload + Send,
    {
        let locator = key.as_leaf_for_member(&self.member_id())?;
        let decoded = self.decode_key(locator)?;
        let bytes = self.read_blob(decoded.as_ref()).await?;
        decode_borsh(bytes.as_slice()).map_err(map_codec_error)
    }

    async fn verify(&self, key: &PolicyKey) -> Result<DaVerifyReport, DaError> {
        let locator = key.as_leaf_for_member(&self.member_id())?;
        let decoded = self.decode_key(locator)?;
        self.verify_key(decoded.as_ref()).await
    }
}

impl<TBackend> DataAvailabilityExt for TBackend where TBackend: DataAvailability + ?Sized {}

/// Member entry used by [`crate::MultiDa`] to associate an id with a backend.
#[derive(Clone)]
pub struct DaMember {
    id: MemberId,
    backend: Arc<dyn DataAvailability>,
}

impl DaMember {
    /// Build a member from an explicit [`MemberId`] and backend implementation.
    pub fn new<B>(id: MemberId, backend: B) -> Self
    where
        B: DataAvailability + 'static,
    {
        Self {
            id,
            backend: Arc::new(backend),
        }
    }

    /// Borrow the member identifier.
    pub fn id(&self) -> &MemberId {
        &self.id
    }

    pub(crate) fn backend(&self) -> &(dyn DataAvailability + '_) {
        self.backend.as_ref()
    }

    pub(crate) fn backend_arc(&self) -> Arc<dyn DataAvailability> {
        Arc::clone(&self.backend)
    }

    /// Build a Bitcoin member entry.
    pub fn bitcoin<B>(backend: B) -> Self
    where
        B: DataAvailability + 'static,
    {
        Self::new(MemberId::Bitcoin, backend)
    }

    /// Build a Liquid member entry.
    pub fn liquid<B>(backend: B) -> Self
    where
        B: DataAvailability + 'static,
    {
        Self::new(MemberId::Liquid, backend)
    }
}

impl<TBackend> From<TBackend> for DaMember
where
    TBackend: DataAvailability + 'static,
{
    fn from(backend: TBackend) -> Self {
        let member_id = backend.member_id();
        Self::new(member_id, backend)
    }
}

fn map_codec_error(err: crate::bundle::CodecError) -> DaError {
    match err {
        crate::bundle::CodecError::Serialize(message) => UsageError::InvalidRequest(message).into(),
        crate::bundle::CodecError::Deserialize(message) => {
            SemanticError::DecodeFailure(message).into()
        }
        crate::bundle::CodecError::EmptyBundle => {
            SemanticError::DecodeFailure("bundle must contain at least one item".to_string()).into()
        }
    }
}
