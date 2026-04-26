// This module defines the type-facing Bitcoin backend surface.
// runtime.rs owns DataAvailability orchestration.
mod fees;
mod read;
mod rpc;
mod runtime;
#[cfg(test)]
mod tests;
mod tx;
mod types;
mod write;

use self::fees::validate_single_fee_policy;
use crate::da::backends::common::validate_oversize_policy;
use crate::da::{
    BitcoinBlobLocator, DaError, FeePolicy, Locator, NamespaceId, OversizePolicy, UsageError,
};
use ::bitcoin::{Network, Txid};
use bitcoind_async_client::Client as BitcoinClient;

const BITCOIN_INSCRIPTION_CHUNK_BYTES: usize = 520;
const BITCOIN_MAX_SINGLE_INSCRIPTION_PAYLOAD_BYTES: usize = 396_000;
const BITCOIN_MAX_UNCONFIRMED_CLUSTER_VBYTES: u64 = 100_000;

/// Configuration for constructing a [`BitcoinDa`] backend.
pub struct BitcoinDaConfig {
    /// Expected Bitcoin network reported by the connected node.
    pub network: Network,
    /// Wallet-scoped Bitcoin RPC client used for read and write operations.
    pub client: BitcoinClient,
    /// Namespace identifier used to derive the reveal-tx txid target prefix.
    pub namespace_id: NamespaceId,
    /// Fee selection policy for commit and reveal transactions.
    pub fee_policy: FeePolicy,
    /// Policy to apply when a payload is too large for one standard inscription.
    pub oversize_policy: OversizePolicy,
}

impl BitcoinDaConfig {
    /// Default fee policy used by the Bitcoin backend examples.
    pub fn default_fee_policy() -> FeePolicy {
        FeePolicy::next_block()
    }
}

/// Bitcoin data-availability backend built on a wallet-scoped RPC endpoint.
pub struct BitcoinDa {
    network: Network,
    client: BitcoinClient,
    namespace_id: NamespaceId,
    fee_policy: FeePolicy,
    oversize_policy: OversizePolicy,
}

impl BitcoinDa {
    /// Construct a validated Bitcoin backend from configuration.
    pub fn new(config: BitcoinDaConfig) -> Result<Self, DaError> {
        validate_single_fee_policy(&config.fee_policy)?;
        validate_oversize_policy(&config.oversize_policy)?;

        Ok(Self {
            network: config.network,
            client: config.client,
            namespace_id: config.namespace_id,
            fee_policy: config.fee_policy,
            oversize_policy: config.oversize_policy,
        })
    }

    /// Build a stable locator from one reveal transaction id.
    pub fn locator_from_txid(txid: Txid) -> Result<Locator, DaError> {
        Locator::from_key(&BitcoinBlobLocator::from_txid(txid))
    }

    /// Build a stable locator from one reveal transaction id.
    pub fn provider_locator_from_txid(txid: Txid) -> Result<Locator, DaError> {
        Self::locator_from_txid(txid)
    }

    /// Build a stable locator from ordered chunk transaction ids.
    pub fn locator_from_chunks(chunk_txids: Vec<Txid>) -> Result<Locator, DaError> {
        Locator::from_key(&BitcoinBlobLocator::from_chunked(chunk_txids)?)
    }

    /// Build a stable locator from ordered chunk transaction ids.
    pub fn provider_locator_from_chunks(chunk_txids: Vec<Txid>) -> Result<Locator, DaError> {
        Self::locator_from_chunks(chunk_txids)
    }

    /// Extract a single reveal transaction id from a Bitcoin locator.
    pub fn txid_from_locator(locator: &Locator) -> Result<Txid, DaError> {
        let key = serde_json::from_slice::<BitcoinBlobLocator>(locator.key_bytes())
            .map_err(|err| UsageError::BadLocator(err.to_string()))?;
        key.into_txid()
    }

    /// Borrow the underlying Bitcoin RPC client.
    pub fn client(&self) -> &BitcoinClient {
        &self.client
    }
}
