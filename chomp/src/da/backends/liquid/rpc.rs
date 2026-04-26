use super::{
    LiquidDa,
    runtime::{map_liquid_client_error, map_liquid_wallet_client_error},
    types::{
        FinalizePsbtResult, FundRawTransactionResult, LiquidChainParams, SidechainInfo,
        TestMempoolAcceptResult, WalletAddressInfoResult, WalletPsbtResult,
    },
};
use crate::da::{DaError, RuntimeError, TxidPrefix};
use anyhow::Context;
use bitcoin::Amount;
use bitcoind_async_client::Client as BitcoinClient;
use elements::{
    AssetId as LiquidAssetId, BlockHash as LiquidBlockHash, Transaction as LiquidTransaction,
    Txid as LiquidTxid, encode, hex::FromHex, secp256k1_zkp::PublicKey as LiquidBlindingPublicKey,
};
use std::str::FromStr;

// Liquid RPC calls stay raw on purpose. The typed client wrappers are modeled around Bitcoin Core
// `bitcoin::*` types, while this backend works with Elements addresses, txids, transactions, and
// Liquid-specific wallet/RPC responses.

pub(super) async fn fetch_liquid_chain_params(
    client: &BitcoinClient,
) -> Result<LiquidChainParams, DaError> {
    let sidechain_info: SidechainInfo = client
        .call_raw("getsidechaininfo", &[])
        .await
        .map_err(map_liquid_client_error)?;
    let pegged_asset = LiquidAssetId::from_str(&sidechain_info.pegged_asset).map_err(|err| {
        RuntimeError::Internal(format!(
            "Invalid pegged asset id returned by Liquid RPC: {err}"
        ))
    })?;
    let genesis_hash_hex: String = client
        .call_raw("getblockhash", &[serde_json::json!(0)])
        .await
        .map_err(map_liquid_client_error)?;
    let genesis_hash = LiquidBlockHash::from_str(&genesis_hash_hex).map_err(|err| {
        RuntimeError::Internal(format!(
            "Invalid genesis hash returned by Liquid RPC: {err}"
        ))
    })?;

    Ok(LiquidChainParams {
        genesis_hash,
        pegged_asset,
    })
}

pub(super) async fn probe_liquid_mempool_acceptance(
    client: &BitcoinClient,
    tx: &LiquidTransaction,
) -> Result<TestMempoolAcceptResult, DaError> {
    let params = vec![serde_json::json!([encode::serialize_hex(tx)])];
    let result: Vec<TestMempoolAcceptResult> = client
        .call_raw("testmempoolaccept", &params)
        .await
        .map_err(map_liquid_client_error)?;
    let Some(entry) = result.first() else {
        return Err(
            RuntimeError::Internal("testmempoolaccept returned no results".to_string()).into(),
        );
    };

    Ok(TestMempoolAcceptResult {
        allowed: entry.allowed,
        reject_reason: entry.reject_reason.clone(),
        reject_details: entry.reject_details.clone(),
    })
}

pub(super) fn liquid_mempool_rejection_summary(result: &TestMempoolAcceptResult) -> String {
    format!(
        "testmempoolaccept rejected the transaction: {:?} {:?}",
        result.reject_reason, result.reject_details
    )
}

pub(super) fn is_liquid_fee_rejection(result: &TestMempoolAcceptResult) -> bool {
    let reason = result.reject_reason.as_deref().unwrap_or_default();
    let details = result.reject_details.as_deref().unwrap_or_default();

    reason == "insufficient fee"
        || reason == "bad-txns-fee-outofrange"
        || reason.contains("fee")
        || details.contains("fee")
}

pub(super) async fn send_liquid_transaction(
    client: &BitcoinClient,
    tx: &LiquidTransaction,
) -> Result<LiquidTxid, DaError> {
    let txid_hex: String = client
        .call_raw(
            "sendrawtransaction",
            &[serde_json::json!(encode::serialize_hex(tx))],
        )
        .await
        .map_err(map_liquid_client_error)?;
    LiquidTxid::from_str(&txid_hex).map_err(|err| {
        RuntimeError::Internal(format!(
            "Liquid sendrawtransaction returned invalid txid '{}': {err}",
            txid_hex
        ))
        .into()
    })
}

async fn fetch_liquid_transaction_by_txid(
    client: &BitcoinClient,
    txid: &LiquidTxid,
) -> Result<LiquidTransaction, DaError> {
    let tx_hex: String = client
        .call_raw(
            "getrawtransaction",
            &[
                serde_json::json!(txid.to_string()),
                serde_json::json!(false),
            ],
        )
        .await
        .map_err(map_liquid_client_error)?;
    let tx_bytes =
        Vec::<u8>::from_hex(&tx_hex).map_err(|err| RuntimeError::Internal(err.to_string()))?;
    encode::deserialize(&tx_bytes).map_err(|err| RuntimeError::Internal(err.to_string()).into())
}

impl LiquidDa {
    pub(super) async fn fetch_transaction(
        &self,
        txid: &LiquidTxid,
    ) -> Result<LiquidTransaction, DaError> {
        fetch_liquid_transaction_by_txid(&self.client, txid).await
    }

    async fn fetch_wallet_new_address(
        &self,
        method: &str,
        address_type: &str,
    ) -> Result<elements::Address, DaError> {
        let address: String = self
            .client
            .call_raw(
                method,
                &[serde_json::json!(""), serde_json::json!(address_type)],
            )
            .await
            .map_err(|err| map_liquid_wallet_client_error(method, err))?;
        address.parse::<elements::Address>().map_err(|err| {
            RuntimeError::Internal(format!("failed to parse Liquid wallet address: {err}")).into()
        })
    }

    pub(super) async fn fetch_wallet_return_address(&self) -> Result<elements::Address, DaError> {
        match self
            .fetch_wallet_new_address("getrawchangeaddress", "bech32")
            .await
        {
            Ok(address) => Ok(address),
            Err(_) => {
                self.fetch_wallet_new_address("getnewaddress", "bech32")
                    .await
            }
        }
    }

    pub(super) async fn fetch_wallet_internal_key(
        &self,
    ) -> Result<elements::schnorr::XOnlyPublicKey, DaError> {
        let address = self
            .fetch_wallet_new_address("getnewaddress", "bech32")
            .await?;
        let info: WalletAddressInfoResult = self
            .client
            .call_raw("getaddressinfo", &[serde_json::json!(address.to_string())])
            .await
            .map_err(|err| map_liquid_wallet_client_error("getaddressinfo", err))?;
        if !info.ismine {
            return Err(RuntimeError::Misconfigured(format!(
                "Liquid wallet address {} is not owned by the connected wallet",
                address
            ))
            .into());
        }
        let pubkey = info.pubkey.ok_or_else(|| {
            RuntimeError::Misconfigured(format!(
                "Liquid wallet did not return a pubkey for {}",
                address
            ))
        })?;
        let pubkey = LiquidBlindingPublicKey::from_str(&pubkey).map_err(|err| {
            RuntimeError::Misconfigured(format!(
                "Liquid wallet returned an invalid pubkey for {}: {err}",
                address
            ))
        })?;

        Ok(pubkey.x_only_public_key().0)
    }

    pub(super) async fn fund_wallet_transaction(
        &self,
        tx: &LiquidTransaction,
        sat_per_vb: f64,
    ) -> Result<(LiquidTransaction, u64), DaError> {
        let result: FundRawTransactionResult = self
            .client
            .call_raw(
                "fundrawtransaction",
                &[
                    serde_json::json!(encode::serialize_hex(tx)),
                    serde_json::json!({
                        "add_inputs": true,
                        "fee_rate": sat_per_vb,
                        "minconf": 1,
                        "replaceable": true,
                    }),
                ],
            )
            .await
            .map_err(|err| map_liquid_wallet_client_error("fundrawtransaction", err))?;
        let tx_bytes = Vec::<u8>::from_hex(&result.hex).map_err(|err| {
            RuntimeError::Internal(format!("invalid funded Liquid tx hex: {err}"))
        })?;
        let funded_tx: LiquidTransaction = encode::deserialize(&tx_bytes).map_err(|err| {
            RuntimeError::Internal(format!("failed to decode funded Liquid tx: {err}"))
        })?;
        let fee_sats = Amount::from_btc(result.fee)
            .context("invalid Liquid fee returned by fundrawtransaction")
            .map_err(|err| RuntimeError::Internal(err.to_string()))?
            .to_sat();

        Ok((funded_tx, fee_sats))
    }

    fn apply_pset_txid_prefix(
        &self,
        pset_base64: &str,
        target_prefix: &TxidPrefix,
    ) -> Result<String, DaError> {
        let bytes = base64::decode(pset_base64).map_err(|err| {
            RuntimeError::Internal(format!("failed to decode Liquid PSET: {err}"))
        })?;
        let mut pset: elements::pset::PartiallySignedTransaction = encode::deserialize(&bytes)
            .map_err(|err| {
                RuntimeError::Internal(format!("failed to decode Liquid PSET bytes: {err}"))
            })?;
        let mut tx = pset.extract_tx().map_err(|err| {
            RuntimeError::Internal(format!("failed to extract Liquid tx from PSET: {err}"))
        })?;
        super::tx::apply_liquid_txid_prefix(&mut tx, target_prefix)?;
        pset.global.tx_data.fallback_locktime = Some(tx.lock_time);

        Ok(base64::encode(encode::serialize(&pset)))
    }

    pub(super) async fn sign_wallet_transaction(
        &self,
        tx: &LiquidTransaction,
        target_prefix: Option<&TxidPrefix>,
    ) -> Result<LiquidTransaction, DaError> {
        let pset: String = self
            .client
            .call_raw(
                "converttopsbt",
                &[
                    serde_json::json!(encode::serialize_hex(tx)),
                    serde_json::json!(false),
                ],
            )
            .await
            .map_err(|err| map_liquid_wallet_client_error("converttopsbt", err))?;
        let filled: WalletPsbtResult = self
            .client
            .call_raw("walletfillpsbtdata", &[serde_json::json!(pset)])
            .await
            .map_err(|err| map_liquid_wallet_client_error("walletfillpsbtdata", err))?;
        let blinded: String = self
            .client
            .call_raw("blindpsbt", &[serde_json::json!(filled.psbt)])
            .await
            .map_err(|err| map_liquid_wallet_client_error("blindpsbt", err))?;
        let blinded = if let Some(target_prefix) = target_prefix {
            self.apply_pset_txid_prefix(&blinded, target_prefix)?
        } else {
            blinded
        };
        let signed: WalletPsbtResult = self
            .client
            .call_raw("walletsignpsbt", &[serde_json::json!(blinded)])
            .await
            .map_err(|err| map_liquid_wallet_client_error("walletsignpsbt", err))?;
        let finalized: FinalizePsbtResult = self
            .client
            .call_raw("finalizepsbt", &[serde_json::json!(signed.psbt)])
            .await
            .map_err(|err| map_liquid_wallet_client_error("finalizepsbt", err))?;
        if matches!(signed.complete, Some(false)) || !finalized.complete {
            return Err(RuntimeError::Misconfigured(
                "Liquid wallet did not fully sign the funded PSET".to_string(),
            )
            .into());
        }
        let hex = finalized.hex.ok_or_else(|| {
            RuntimeError::Internal(
                "Liquid finalizepsbt returned no extracted transaction".to_string(),
            )
        })?;
        let tx_bytes = Vec::<u8>::from_hex(&hex).map_err(|err| {
            RuntimeError::Internal(format!("invalid finalized Liquid tx hex: {err}"))
        })?;
        encode::deserialize(&tx_bytes).map_err(|err| {
            RuntimeError::Internal(format!("failed to decode finalized Liquid tx: {err}")).into()
        })
    }
}
