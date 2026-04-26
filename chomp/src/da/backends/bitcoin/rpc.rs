use super::{
    BitcoinDa,
    runtime::{map_bitcoin_client_error, map_bitcoin_wallet_client_error},
    types::{BitcoinWalletKeyInfo, FinalizePsbtResult, WalletCreateFundedPsbtResult},
};
use crate::da::{DaError, RuntimeError};
use ::bitcoin::{Address, Amount, Psbt, Transaction, Txid};
use anyhow::Context;
use bitcoind_async_client::{
    Client as BitcoinClient,
    traits::{Broadcaster, Reader, Signer, Wallet},
};

pub(super) async fn probe_bitcoin_mempool_acceptance(
    client: &BitcoinClient,
    tx: &Transaction,
) -> Result<(bool, Option<String>, Option<String>), DaError> {
    let result = client
        .test_mempool_accept(tx)
        .await
        .map_err(map_bitcoin_client_error)?;
    let Some(entry) = result.results.first() else {
        return Err(
            RuntimeError::Internal("testmempoolaccept returned no results".to_string()).into(),
        );
    };

    Ok((
        entry.allowed,
        entry.reject_reason.clone(),
        entry.reject_details.clone(),
    ))
}

pub(super) fn bitcoin_mempool_rejection_summary(
    reject_reason: &Option<String>,
    reject_details: &Option<String>,
) -> String {
    format!(
        "testmempoolaccept rejected the transaction: {:?} {:?}",
        reject_reason, reject_details
    )
}

pub(super) fn is_bitcoin_fee_rejection(
    reject_reason: &Option<String>,
    reject_details: &Option<String>,
) -> bool {
    let reject_reason = reject_reason.as_deref().unwrap_or_default();
    let reject_details = reject_details.as_deref().unwrap_or_default();
    reject_reason.contains("fee")
        || reject_details.contains("fee")
        || reject_reason.contains("mempool min fee not met")
}

pub(super) async fn send_bitcoin_transaction(
    client: &BitcoinClient,
    tx: &Transaction,
) -> Result<Txid, DaError> {
    client
        .send_raw_transaction(tx)
        .await
        .map_err(map_bitcoin_client_error)
}

impl BitcoinDa {
    pub(super) async fn fetch_transaction(&self, txid: &Txid) -> Result<Transaction, DaError> {
        self.client
            .get_raw_transaction_verbosity_zero(txid)
            .await
            .map_err(map_bitcoin_client_error)
            .map(|transaction| transaction.0)
    }

    async fn fetch_wallet_new_address(
        &self,
        method: &str,
        address_type: &str,
    ) -> Result<Address, DaError> {
        // Keep this raw: the typed wallet helper does not let us request a specific address type.
        let address: String = self
            .client
            .call_raw(
                method,
                &[serde_json::json!(""), serde_json::json!(address_type)],
            )
            .await
            .map_err(|err| map_bitcoin_wallet_client_error(method, err))?;
        let address = address
            .parse::<Address<_>>()
            .context("failed to parse Bitcoin wallet address")
            .map_err(|err| RuntimeError::Internal(err.to_string()))?;
        address
            .require_network(self.network)
            .map_err(|err| RuntimeError::Misconfigured(err.to_string()).into())
    }

    pub(super) async fn fetch_wallet_return_address(&self) -> Result<Address, DaError> {
        match self
            .fetch_wallet_new_address("getrawchangeaddress", "bech32m")
            .await
        {
            Ok(address) => Ok(address),
            Err(_) => {
                self.fetch_wallet_new_address("getnewaddress", "bech32m")
                    .await
            }
        }
    }

    pub(super) async fn fetch_wallet_internal_key_info(
        &self,
    ) -> Result<BitcoinWalletKeyInfo, DaError> {
        let address = self
            .fetch_wallet_new_address("getnewaddress", "bech32")
            .await?;
        let info = self
            .client
            .get_address_info(&address)
            .await
            .map_err(|err| map_bitcoin_wallet_client_error("getaddressinfo", err))?;
        if !info.is_mine {
            return Err(RuntimeError::Misconfigured(format!(
                "Bitcoin wallet address {} is not owned by the connected wallet",
                address
            ))
            .into());
        }
        let pubkey = info.pubkey.ok_or_else(|| {
            RuntimeError::Misconfigured(format!(
                "Bitcoin wallet did not return a pubkey for {}",
                address
            ))
        })?;
        let internal_key = pubkey.inner.x_only_public_key().0;

        Ok(BitcoinWalletKeyInfo { internal_key })
    }

    pub(super) async fn fund_wallet_transaction(
        &self,
        outputs: Vec<serde_json::Value>,
        sat_per_vb: f64,
    ) -> Result<(Transaction, u64), DaError> {
        // Keep this raw: the typed wallet wrapper does not expose `minconf`, and we require
        // confirmed-only funding to avoid selecting unconfirmed wallet change.
        let result: WalletCreateFundedPsbtResult = self
            .client
            .call_raw(
                "walletcreatefundedpsbt",
                &[
                    serde_json::json!([]),
                    serde_json::json!(outputs),
                    serde_json::json!(0),
                    serde_json::json!({
                        "fee_rate": sat_per_vb,
                        "minconf": 1,
                        "replaceable": true,
                    }),
                    serde_json::json!(false),
                ],
            )
            .await
            .map_err(|err| map_bitcoin_wallet_client_error("walletcreatefundedpsbt", err))?;
        let psbt_bytes = base64::decode(&result.psbt).map_err(|err| {
            RuntimeError::Internal(format!("failed to decode funded Bitcoin PSBT: {err}"))
        })?;
        let funded_tx = Psbt::deserialize(&psbt_bytes)
            .map_err(|err| {
                RuntimeError::Internal(format!("failed to parse funded Bitcoin PSBT: {err}"))
            })?
            .unsigned_tx;
        let fee_sats = Amount::from_btc(result.fee)
            .context("invalid Bitcoin fee returned by walletcreatefundedpsbt")
            .map_err(|err| RuntimeError::Internal(err.to_string()))?
            .to_sat();

        Ok((funded_tx, fee_sats))
    }

    pub(super) async fn sign_wallet_transaction(
        &self,
        tx: &Transaction,
    ) -> Result<Transaction, DaError> {
        // Keep this raw: there is no typed `converttopsbt` wrapper in the client crate.
        let psbt: String = self
            .client
            .call_raw(
                "converttopsbt",
                &[
                    serde_json::json!(::bitcoin::consensus::encode::serialize_hex(tx)),
                    serde_json::json!(false),
                ],
            )
            .await
            .map_err(|err| map_bitcoin_wallet_client_error("converttopsbt", err))?;
        let processed = self
            .client
            .wallet_process_psbt(&psbt, None, None, None)
            .await
            .map_err(|err| map_bitcoin_wallet_client_error("walletprocesspsbt", err))?;
        // Keep this raw: there is no typed `finalizepsbt` wrapper in the client crate.
        let finalized: FinalizePsbtResult = self
            .client
            .call_raw(
                "finalizepsbt",
                &[serde_json::json!(processed.psbt.to_string())],
            )
            .await
            .map_err(|err| map_bitcoin_wallet_client_error("finalizepsbt", err))?;
        if !processed.complete || !finalized.complete {
            return Err(RuntimeError::Misconfigured(
                "Bitcoin wallet did not fully sign the funded PSBT".to_string(),
            )
            .into());
        }
        let hex = finalized.hex.ok_or_else(|| {
            RuntimeError::Internal(
                "Bitcoin finalizepsbt returned no extracted transaction".to_string(),
            )
        })?;

        ::bitcoin::consensus::encode::deserialize_hex::<Transaction>(&hex).map_err(|err| {
            RuntimeError::Internal(format!("failed to decode finalized Bitcoin tx: {err}")).into()
        })
    }
}
