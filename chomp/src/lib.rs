//! CHOMP is a crate-root-first SDK for publishing Borsh-encoded payloads to
//! Bitcoin, Liquid, and council-style backends through one consistent API.
//!
//! Most application code should start from the crate root:
//!
//! - [`DataAvailability`] for blob read/write operations
//! - [`DataAvailabilityExt`] for typed Borsh payload helpers
//! - [`ChompPayload`] for raw bytes
//! - [`Locator`], [`Policy`], [`PolicyLeafKey`], and [`PolicyKey`] for persisted references
//! - [`BitcoinDa`], [`LiquidDa`], [`CouncilDa`], and [`MultiDa`] for backend selection
//!
//! Backend authors and lower-level integrations can use
//! [`crate::da::DataAvailability`] directly.
//!
//! Typical flow:
//!
//! 1. Build a backend instance.
//! 2. Call [`DataAvailabilityExt::write`] with a typed payload or [`ChompPayload`].
//! 3. Keep the returned [`BlobWriteReceipt`] and [`PolicyKey`].
//! 4. Later, call [`DataAvailabilityExt::read`] or [`DataAvailabilityExt::verify`].
//!
//! ```rust,no_run
//! use borsh::{BorshDeserialize, BorshSerialize};
//! use chomp::{BorshPayload, CouncilDa, DaError, DataAvailabilityExt};
//!
//! #[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
//! struct Vote {
//!     choice: String,
//! }
//!
//! impl BorshPayload for Vote {}
//!
//! #[tokio::main]
//! async fn main() -> Result<(), DaError> {
//!     let da = CouncilDa::new("default", "http://127.0.0.1:8080")?;
//!     let receipt = da
//!         .write(&Vote {
//!             choice: "accept".to_string(),
//!         })
//!         .await?;
//!     let _: Vote = da.read(receipt.key()).await?;
//!     let _ = da.verify(receipt.key()).await?;
//!     Ok(())
//! }
//! ```
/// Encoding-facing types and codec helpers. Most users can import these items
/// from the crate root and treat this module as an advanced entrypoint.
pub mod bundle;
/// Shared error and member-identity types. Most users can import these items
/// from the crate root and treat this module as an advanced entrypoint.
pub mod core;
/// Data-availability domain types and backend traits. Most users can import the
/// primary SDK items from the crate root and treat this module as an advanced
/// entrypoint.
pub mod da;
mod macros;

/// Bundle format for transporting multiple validated Borsh payloads together.
pub use bundle::BorshBundle;
/// Version tag carried by [`BorshBundle`].
pub use bundle::BorshBundleVersion;
/// Marker trait for typed payloads that CHOMP can encode, validate, and store.
pub use bundle::BorshPayload;
/// Raw byte payload type for callers that do not want a custom Borsh struct.
pub use bundle::ChompPayload;
/// Encoding or decoding error returned by the bundle codec helpers.
pub use bundle::CodecError;
/// Decode a validated Borsh payload value from bytes.
pub use bundle::decode_borsh;
/// Decode a validated [`BorshBundle`] from bytes.
pub use bundle::decode_borsh_bundle;
/// Encode a validated Borsh payload value into bytes.
pub use bundle::encode_borsh;
/// Encode a validated [`BorshBundle`] into bytes.
pub use bundle::encode_borsh_bundle;

/// Aggregated branch failure detail for policy evaluation.
pub use core::BranchFailure;
/// Branch failure classification used in policy aggregates.
pub use core::BranchFailureClass;
/// Preferred crate-wide error type for CHOMP operations.
pub use core::DaError;
/// Stable identifier for one member in a replicated [`MultiDa`] composition.
pub use core::MemberId;
/// Summary of retryable vs hard failures after policy exhaustion.
pub use core::PolicyFailureSummary;
/// Runtime/provider failure reported by a backend implementation.
pub use core::RuntimeError;
/// Data-level failure such as not found, integrity failure, or provider limits.
pub use core::SemanticError;
/// Caller or configuration error such as malformed locators or invalid member ids.
pub use core::UsageError;
/// Partial-write detail returned when a composite write succeeds only partially.
pub use core::WriteIncomplete;

/// Bitcoin-specific provider locator.
pub use da::BitcoinBlobLocator;
/// Bitcoin backend implementation.
pub use da::BitcoinDa;
/// Configuration for constructing a [`BitcoinDa`] instance.
pub use da::BitcoinDaConfig;
/// Result of a successful write operation.
pub use da::BlobWriteReceipt;
/// Ordered chunk locator used when a blob is split across multiple transactions.
pub use da::ChunkedBlobLocator;
/// Council-specific provider locator.
pub use da::CouncilBlobLocator;
/// Council backend implementation.
pub use da::CouncilDa;
/// Object-safe provider key trait.
pub use da::DaKey;
/// Member wrapper used by [`MultiDa`].
pub use da::DaMember;
/// Verification result returned by backend or policy verification.
pub use da::DaVerifyReport;
/// Primary blob-based SDK trait for leaf backends.
pub use da::DataAvailability;
/// Typed payload helpers layered on top of [`DataAvailability`].
pub use da::DataAvailabilityExt;
/// Shared type alias for one decoded provider key.
pub use da::DynKey;
/// Fee policy used by Bitcoin and Liquid backends.
pub use da::FeePolicy;
/// Liquid-specific provider locator.
pub use da::LiquidBlobLocator;
/// Liquid backend implementation.
pub use da::LiquidDa;
/// Configuration for constructing a [`LiquidDa`] instance.
pub use da::LiquidDaConfig;
/// Stable serialized provider locator.
pub use da::Locator;
/// Minimum number of bytes required in a [`NamespaceId`].
pub use da::MIN_NAMESPACE_ID_LEN;
/// Policy-aware composite backend.
pub use da::MultiDa;
/// Validated namespace identifier used for txid prefix targeting.
pub use da::NamespaceId;
/// Validation error returned while constructing a [`NamespaceId`].
pub use da::NamespaceIdError;
/// Large-payload handling policy used by Bitcoin and Liquid backends.
pub use da::OversizePolicy;
/// Composition policy used by [`MultiDa`].
pub use da::Policy;
/// Persisted key tree returned by policy-aware writes.
pub use da::PolicyKey;
/// Persisted leaf key carrying both member identity and provider locator.
pub use da::PolicyLeafKey;
/// Build-time policy DSL that owns live backend members.
pub use da::PolicySpec;

#[cfg(test)]
mod tests;
