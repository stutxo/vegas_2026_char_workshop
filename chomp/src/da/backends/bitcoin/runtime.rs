use super::{
    BitcoinDa,
    read::extract_blob_from_transaction,
    types::{BitcoinRuntime, BitcoinTransactionBlob},
};
use crate::da::{
    BitcoinBlobLocator, BlobWriteReceipt, DaError, DaVerifyReport, DataAvailability, DynKey,
    Locator, MemberId, PolicyKey, RuntimeError, UsageError,
};
use ::bitcoin::{Txid, hashes::Hash, key::Secp256k1};
use async_trait::async_trait;
use bitcoind_async_client::error::ClientError;
use bitcoind_async_client::traits::Reader;

pub(super) fn map_bitcoin_client_error(err: ClientError) -> DaError {
    crate::da::backends::common::map_rpc_client_error("bitcoin", err)
}

pub(super) fn map_bitcoin_wallet_client_error(method: &str, err: ClientError) -> DaError {
    RuntimeError::Misconfigured(format!(
        "Bitcoin wallet RPC '{}' failed; use a wallet-scoped Bitcoin RPC endpoint and make sure the wallet is loaded: {}",
        method, err
    ))
    .into()
}

#[async_trait]
impl DataAvailability for BitcoinDa {
    fn provider_kind(&self) -> &'static str {
        "bitcoin"
    }

    fn member_id(&self) -> MemberId {
        MemberId::Bitcoin
    }

    async fn write_blob(&self, data: &[u8]) -> Result<BlobWriteReceipt, DaError> {
        let secp = Secp256k1::new();
        let target_prefix = self.namespace_id.prefix();
        let node_network = self
            .client
            .network()
            .await
            .map_err(map_bitcoin_client_error)?;
        if node_network != self.network {
            return Err(RuntimeError::Misconfigured(format!(
                "BitcoinDa network mismatch: configured {:?}, node reports {:?}",
                self.network, node_network
            ))
            .into());
        }

        let runtime = BitcoinRuntime {
            secp: &secp,
            target_prefix: &target_prefix,
            network: self.network,
        };
        let locator = self.write_payload(runtime, data).await?;

        Ok(BlobWriteReceipt::new(
            PolicyKey::leaf(self.member_id(), Locator::from_key(&locator)?),
            data.len(),
        ))
    }

    async fn read_blob(&self, key: &dyn crate::da::DaKey) -> Result<Vec<u8>, DaError> {
        let Some(bitcoin_locator) = key.as_any().downcast_ref::<BitcoinBlobLocator>() else {
            return Err(UsageError::WrongProvider {
                expected: "bitcoin",
            }
            .into());
        };

        match bitcoin_locator {
            BitcoinBlobLocator::Txid(txid) => {
                self.read_blob_by_txid(Txid::from_byte_array(*txid)).await
            }
            BitcoinBlobLocator::Chunked(chunked) => self.read_blob_by_chunks(chunked).await,
        }
    }

    async fn verify_key(&self, key: &dyn crate::da::DaKey) -> Result<DaVerifyReport, DaError> {
        let Some(bitcoin_locator) = key.as_any().downcast_ref::<BitcoinBlobLocator>() else {
            return Err(UsageError::WrongKeyType {
                expected: "BitcoinBlobLocator",
            }
            .into());
        };

        match bitcoin_locator {
            BitcoinBlobLocator::Txid(txid) => {
                let txid = Txid::from_byte_array(*txid);
                let description =
                    match extract_blob_from_transaction(&self.fetch_transaction(&txid).await?)? {
                        BitcoinTransactionBlob::Raw(_) => {
                            "bitcoin transaction exists and contains a readable inscription blob"
                        }
                        BitcoinTransactionBlob::Chunk(_) => {
                            "bitcoin transaction exists and contains a readable chunk payload"
                        }
                    };
                let _ = self.read_blob_by_txid(txid).await?;
                Ok(DaVerifyReport::new(true, Some(description.to_string())))
            }
            BitcoinBlobLocator::Chunked(chunked) => {
                let _ = self.read_blob_by_chunks(chunked).await?;
                Ok(DaVerifyReport::new(
                    true,
                    Some(
                        "bitcoin chunked locator resolves to a readable multi-transaction blob"
                            .to_string(),
                    ),
                ))
            }
        }
    }

    fn decode_key(&self, locator: &Locator) -> Result<DynKey, DaError> {
        if locator.provider_kind() != "bitcoin" {
            return Err(UsageError::WrongProvider {
                expected: "bitcoin",
            }
            .into());
        }

        let key = serde_json::from_slice::<BitcoinBlobLocator>(locator.key_bytes())
            .map_err(|err| UsageError::BadLocator(err.to_string()))?;
        Ok(std::sync::Arc::new(key))
    }
}
