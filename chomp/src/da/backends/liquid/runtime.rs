use super::{
    LiquidDa,
    read::extract_blob_from_transaction,
    rpc::fetch_liquid_chain_params,
    types::{LiquidRuntime, LiquidTransactionBlob},
};
use crate::da::{
    BlobWriteReceipt, DaError, DaVerifyReport, DataAvailability, DynKey, LiquidBlobLocator,
    Locator, MemberId, PolicyKey, RuntimeError, UsageError,
};
use ::bitcoin::hashes::Hash;
use async_trait::async_trait;
use bitcoind_async_client::error::ClientError;
use elements::{Txid as LiquidTxid, secp256k1_zkp::Secp256k1 as LiquidSecp256k1};

pub(super) fn map_liquid_client_error(err: ClientError) -> DaError {
    crate::da::backends::common::map_rpc_client_error("liquid", err)
}

pub(super) fn map_liquid_wallet_client_error(method: &str, err: ClientError) -> DaError {
    RuntimeError::Misconfigured(format!(
        "Liquid wallet RPC '{}' failed; use a wallet-scoped Liquid RPC endpoint and make sure the wallet is loaded: {}",
        method, err
    ))
    .into()
}

#[async_trait]
impl DataAvailability for LiquidDa {
    fn provider_kind(&self) -> &'static str {
        "liquid"
    }

    fn member_id(&self) -> MemberId {
        MemberId::Liquid
    }

    async fn write_blob(&self, data: &[u8]) -> Result<BlobWriteReceipt, DaError> {
        let secp = LiquidSecp256k1::new();
        let target_prefix = self.namespace_id.prefix();
        let chain_params = fetch_liquid_chain_params(&self.client).await?;
        let runtime = LiquidRuntime {
            secp: &secp,
            target_prefix: &target_prefix,
            chain_params: &chain_params,
        };
        let locator = self.write_payload(runtime, data).await?;

        Ok(BlobWriteReceipt::new(
            PolicyKey::leaf(self.member_id(), Locator::from_key(&locator)?),
            data.len(),
        ))
    }

    async fn read_blob(&self, key: &dyn crate::da::DaKey) -> Result<Vec<u8>, DaError> {
        let Some(liquid_locator) = key.as_any().downcast_ref::<LiquidBlobLocator>() else {
            return Err(UsageError::WrongKeyType {
                expected: "LiquidBlobLocator",
            }
            .into());
        };

        match liquid_locator {
            LiquidBlobLocator::Txid(txid) => {
                self.read_blob_by_txid(LiquidTxid::from_byte_array(*txid))
                    .await
            }
            LiquidBlobLocator::Chunked(chunked) => self.read_blob_by_chunks(chunked).await,
        }
    }

    async fn verify_key(&self, key: &dyn crate::da::DaKey) -> Result<DaVerifyReport, DaError> {
        let Some(liquid_locator) = key.as_any().downcast_ref::<LiquidBlobLocator>() else {
            return Err(UsageError::WrongKeyType {
                expected: "LiquidBlobLocator",
            }
            .into());
        };

        match liquid_locator {
            LiquidBlobLocator::Txid(txid) => {
                let txid = LiquidTxid::from_byte_array(*txid);
                let description =
                    match extract_blob_from_transaction(&self.fetch_transaction(&txid).await?)? {
                        LiquidTransactionBlob::Raw(_) => {
                            "liquid transaction exists and contains a readable inscription blob"
                        }
                        LiquidTransactionBlob::Chunk(_) => {
                            "liquid transaction exists and contains a readable chunk payload"
                        }
                    };
                let _ = self.read_blob_by_txid(txid).await?;
                Ok(DaVerifyReport::new(true, Some(description.to_string())))
            }
            LiquidBlobLocator::Chunked(chunked) => {
                let _ = self.read_blob_by_chunks(chunked).await?;
                Ok(DaVerifyReport::new(
                    true,
                    Some(
                        "liquid chunked locator resolves to a readable multi-transaction blob"
                            .to_string(),
                    ),
                ))
            }
        }
    }

    fn decode_key(&self, locator: &Locator) -> Result<DynKey, DaError> {
        if locator.provider_kind() != "liquid" {
            return Err(UsageError::WrongProvider { expected: "liquid" }.into());
        }

        let key = serde_json::from_slice::<LiquidBlobLocator>(locator.key_bytes())
            .map_err(|err| UsageError::BadLocator(err.to_string()))?;
        Ok(std::sync::Arc::new(key))
    }
}
