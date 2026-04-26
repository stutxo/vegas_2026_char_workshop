//! Data-availability domain types and backend traits.
//!
//! Most applications can import the main SDK items from the crate root. This
//! module is the advanced entrypoint for locator, policy, and backend-authoring
//! types.
pub mod backends;
mod locator;
mod multi;
mod namespace_id;
mod policy;
mod result;
mod traits;
mod txid_grind;

pub use crate::core::{DaError, MemberId, RuntimeError, SemanticError, UsageError};
pub use backends::{BitcoinDa, BitcoinDaConfig, CouncilDa, LiquidDa, LiquidDaConfig};
pub use locator::{
    BitcoinBlobLocator, ChunkedBlobLocator, CouncilBlobLocator, DaKey, DynKey, LiquidBlobLocator,
    Locator, PolicyKey, PolicyLeafKey,
};
pub use multi::MultiDa;
pub(crate) use namespace_id::TxidPrefix;
pub use namespace_id::{MIN_NAMESPACE_ID_LEN, NamespaceId, NamespaceIdError};
pub use policy::{FeePolicy, OversizePolicy, Policy, PolicySpec};
pub use result::{BlobWriteReceipt, DaVerifyReport};
pub use traits::{DaMember, DataAvailability, DataAvailabilityExt};
pub(crate) use txid_grind::{grind_liquid_txid_prefix, grind_txid_prefix};
