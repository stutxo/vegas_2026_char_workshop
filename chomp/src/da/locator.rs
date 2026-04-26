use crate::core::{DaError, MemberId, UsageError};
use bitcoin::Txid;
use bitcoin::hashes::Hash;
use elements::Txid as LiquidTxid;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::fmt::Debug;
use std::sync::Arc;

pub type DynKey = Arc<dyn DaKey>;

/// Object-safe provider key returned by leaf backends.
pub trait DaKey: Send + Sync + Debug {
    fn as_any(&self) -> &dyn Any;
    fn provider_kind(&self) -> &'static str;
    fn encode(&self) -> Result<Vec<u8>, DaError>;
}

/// Ordered list of transaction ids used when a blob is chunked across writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct ChunkedBlobLocator {
    chunks: Vec<[u8; 32]>,
}

impl ChunkedBlobLocator {
    /// Build a validated chunk locator from ordered txids.
    pub fn new(chunks: Vec<[u8; 32]>) -> Result<Self, DaError> {
        let locator = Self { chunks };
        locator.validate()?;
        Ok(locator)
    }

    /// Validate the locator invariants.
    pub fn validate(&self) -> Result<(), DaError> {
        if self.chunks.is_empty() {
            return Err(UsageError::BadLocator(
                "chunked locator must contain at least one txid".to_string(),
            )
            .into());
        }
        Ok(())
    }

    /// Borrow the ordered txid bytes.
    pub fn chunks(&self) -> &[[u8; 32]] {
        &self.chunks
    }

    /// Consume the locator and return the ordered txid bytes.
    pub fn into_parts(self) -> Vec<[u8; 32]> {
        self.chunks
    }
}

/// Bitcoin provider locator, either a single reveal txid or a chunk list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(untagged)]
pub enum BitcoinBlobLocator {
    Txid([u8; 32]),
    Chunked(ChunkedBlobLocator),
}

impl BitcoinBlobLocator {
    /// Build a single-txid Bitcoin locator from raw bytes.
    pub fn new(txid_bytes: [u8; 32]) -> Self {
        Self::Txid(txid_bytes)
    }

    /// Build a single-txid Bitcoin locator from a typed txid.
    pub fn from_txid(txid: Txid) -> Self {
        Self::Txid(txid.to_byte_array())
    }

    /// Build a chunked Bitcoin locator from ordered chunk txids.
    pub fn from_chunked(chunks: Vec<Txid>) -> Result<Self, DaError> {
        ChunkedBlobLocator::new(
            chunks
                .into_iter()
                .map(|txid| txid.to_byte_array())
                .collect(),
        )
        .map(Self::Chunked)
    }

    /// Return the txid when this locator addresses one reveal transaction.
    pub fn as_txid(&self) -> Option<Txid> {
        match self {
            Self::Txid(txid) => Some(Txid::from_byte_array(*txid)),
            Self::Chunked(_) => None,
        }
    }

    /// Consume the locator into one txid or fail for chunked locators.
    pub fn into_txid(self) -> Result<Txid, DaError> {
        match self {
            Self::Txid(txid) => Ok(Txid::from_byte_array(txid)),
            Self::Chunked(_) => Err(UsageError::BadLocator(
                "chunked bitcoin locator does not identify a single txid".to_string(),
            )
            .into()),
        }
    }

    /// Borrow the chunk locator when this locator is chunked.
    pub fn as_chunked(&self) -> Option<&ChunkedBlobLocator> {
        match self {
            Self::Chunked(locator) => Some(locator),
            Self::Txid(_) => None,
        }
    }
}

impl DaKey for BitcoinBlobLocator {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn provider_kind(&self) -> &'static str {
        "bitcoin"
    }

    fn encode(&self) -> Result<Vec<u8>, DaError> {
        serde_json::to_vec(self).map_err(|err| UsageError::BadLocator(err.to_string()).into())
    }
}

/// Liquid provider locator, either a single reveal txid or a chunk list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(untagged)]
pub enum LiquidBlobLocator {
    Txid([u8; 32]),
    Chunked(ChunkedBlobLocator),
}

impl LiquidBlobLocator {
    /// Build a single-txid Liquid locator from raw bytes.
    pub fn new(txid_bytes: [u8; 32]) -> Self {
        Self::Txid(txid_bytes)
    }

    /// Build a single-txid Liquid locator from a typed txid.
    pub fn from_txid(txid: LiquidTxid) -> Self {
        Self::Txid(txid.to_byte_array())
    }

    /// Build a chunked Liquid locator from ordered chunk txids.
    pub fn from_chunked(chunks: Vec<LiquidTxid>) -> Result<Self, DaError> {
        ChunkedBlobLocator::new(
            chunks
                .into_iter()
                .map(|txid| txid.to_byte_array())
                .collect(),
        )
        .map(Self::Chunked)
    }

    /// Return the txid when this locator addresses one reveal transaction.
    pub fn as_txid(&self) -> Option<LiquidTxid> {
        match self {
            Self::Txid(txid) => Some(LiquidTxid::from_byte_array(*txid)),
            Self::Chunked(_) => None,
        }
    }

    /// Consume the locator into one txid or fail for chunked locators.
    pub fn into_txid(self) -> Result<LiquidTxid, DaError> {
        match self {
            Self::Txid(txid) => Ok(LiquidTxid::from_byte_array(txid)),
            Self::Chunked(_) => Err(UsageError::BadLocator(
                "chunked liquid locator does not identify a single txid".to_string(),
            )
            .into()),
        }
    }

    /// Borrow the chunk locator when this locator is chunked.
    pub fn as_chunked(&self) -> Option<&ChunkedBlobLocator> {
        match self {
            Self::Chunked(locator) => Some(locator),
            Self::Txid(_) => None,
        }
    }
}

impl DaKey for LiquidBlobLocator {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn provider_kind(&self) -> &'static str {
        "liquid"
    }

    fn encode(&self) -> Result<Vec<u8>, DaError> {
        serde_json::to_vec(self).map_err(|err| UsageError::BadLocator(err.to_string()).into())
    }
}

/// Council provider locator addressed by a content hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(transparent)]
pub struct CouncilBlobLocator([u8; 32]);

impl CouncilBlobLocator {
    /// Build a council locator from raw hash bytes.
    pub fn new(hash_bytes: [u8; 32]) -> Self {
        Self(hash_bytes)
    }

    /// Borrow the raw hash bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Consume the locator and return the raw hash bytes.
    pub fn into_array(self) -> [u8; 32] {
        self.0
    }
}

impl DaKey for CouncilBlobLocator {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn provider_kind(&self) -> &'static str {
        "council"
    }

    fn encode(&self) -> Result<Vec<u8>, DaError> {
        serde_json::to_vec(self).map_err(|err| UsageError::BadLocator(err.to_string()).into())
    }
}

/// Stable serialized locator for a provider-native key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct Locator {
    provider_kind: String,
    key_bytes: Vec<u8>,
}

impl Locator {
    pub fn new(provider_kind: impl Into<String>, key_bytes: Vec<u8>) -> Result<Self, DaError> {
        let locator = Self {
            provider_kind: provider_kind.into(),
            key_bytes,
        };
        locator.validate()?;
        Ok(locator)
    }

    pub fn from_key(key: &dyn DaKey) -> Result<Self, DaError> {
        Self::new(key.provider_kind(), key.encode()?)
    }

    pub fn validate(&self) -> Result<(), DaError> {
        if self.provider_kind.trim().is_empty() {
            return Err(UsageError::BadLocator("provider kind cannot be empty".to_string()).into());
        }
        if self.key_bytes.is_empty() {
            return Err(UsageError::BadLocator("locator bytes cannot be empty".to_string()).into());
        }
        Ok(())
    }

    pub fn provider_kind(&self) -> &str {
        &self.provider_kind
    }

    pub fn key_bytes(&self) -> &[u8] {
        &self.key_bytes
    }
}

/// Persisted key tree returned by policy-aware writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyLeafKey {
    member_id: MemberId,
    locator: Locator,
}

impl PolicyLeafKey {
    pub fn new(member_id: MemberId, locator: Locator) -> Self {
        Self { member_id, locator }
    }

    pub fn member_id(&self) -> &MemberId {
        &self.member_id
    }

    pub fn locator(&self) -> &Locator {
        &self.locator
    }

    pub fn into_parts(self) -> (MemberId, Locator) {
        (self.member_id, self.locator)
    }

    pub fn validate(&self) -> Result<(), DaError> {
        self.locator.validate()?;

        if !self
            .member_id
            .matches_provider_kind(self.locator.provider_kind())
        {
            return Err(UsageError::MemberLocatorMismatch {
                member_id: self.member_id.clone(),
                locator_kind: self.locator.provider_kind().to_string(),
            }
            .into());
        }

        Ok(())
    }
}

/// Persisted key tree returned by policy-aware writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyKey {
    Leaf(PolicyLeafKey),
    And(Vec<PolicyKey>),
    Or(Vec<PolicyKey>),
}

impl PolicyKey {
    pub fn leaf(member_id: MemberId, locator: Locator) -> Self {
        Self::Leaf(PolicyLeafKey::new(member_id, locator))
    }

    pub fn as_leaf(&self) -> Result<&PolicyLeafKey, DaError> {
        match self {
            Self::Leaf(leaf) => Ok(leaf),
            _ => Err(UsageError::BadPolicyKey("expected a leaf policy key".to_string()).into()),
        }
    }

    pub fn as_leaf_for_member(&self, member_id: &MemberId) -> Result<&Locator, DaError> {
        let leaf = self.as_leaf()?;
        leaf.validate()?;

        if leaf.member_id() != member_id {
            return Err(UsageError::BadPolicyKey(format!(
                "expected member '{}' but key targets '{}'",
                member_id,
                leaf.member_id()
            ))
            .into());
        }

        Ok(leaf.locator())
    }
}
