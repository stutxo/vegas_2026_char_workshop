use super::{
    runtime::map_liquid_client_error,
    tx::{build_signed_liquid_reveal_tx, liquid_explicit_output, liquid_explicit_txout_secrets},
    types::{LiquidInscriptionArtifacts, LiquidRevealBuildRequest, LiquidRuntime, MempoolInfo},
};
use crate::da::backends::common::validate_fee_policy;
use crate::da::{DaError, FeePolicy, RuntimeError};
use ::bitcoin::hashes::Hash;
use bitcoind_async_client::Client as BitcoinClient;
use bitcoind_async_client::traits::Reader;
use elements::{
    Address as LiquidAddress, TxOut as LiquidTxOut, TxOutSecrets as LiquidTxOutSecrets,
    Txid as LiquidTxid,
};
use tracing::info;

pub(super) fn validate_single_fee_policy(policy: &FeePolicy) -> Result<(), DaError> {
    validate_fee_policy(policy)
}

pub(super) fn liquid_fee_rate_btc_per_kvb_to_sat_per_vb(
    fee_rate_btc_per_kvb: f64,
) -> Result<f64, DaError> {
    if !fee_rate_btc_per_kvb.is_finite() || fee_rate_btc_per_kvb < 0.0 {
        return Err(RuntimeError::Internal(format!(
            "Liquid node returned an invalid fee rate: {fee_rate_btc_per_kvb}"
        ))
        .into());
    }
    Ok(fee_rate_btc_per_kvb * 100_000.0)
}

pub(super) async fn fetch_liquid_fee_rate_sat_per_vb(
    client: &BitcoinClient,
) -> Result<f64, DaError> {
    let mempool_info: MempoolInfo = client
        .call_raw("getmempoolinfo", &[])
        .await
        .map_err(map_liquid_client_error)?;
    let fee_rate = mempool_info.mempoolminfee.max(mempool_info.minrelaytxfee);
    liquid_fee_rate_btc_per_kvb_to_sat_per_vb(fee_rate).map_err(|_| {
        RuntimeError::Internal(format!(
            "Liquid getmempoolinfo returned an invalid fee rate: mempoolminfee={} minrelaytxfee={}",
            mempool_info.mempoolminfee, mempool_info.minrelaytxfee
        ))
        .into()
    })
}

pub(super) async fn resolve_liquid_estimated_fee_rate(
    client: &BitcoinClient,
    policy: &FeePolicy,
) -> Result<f64, DaError> {
    match policy {
        FeePolicy::Manual { sat_per_vb } => Ok(*sat_per_vb),
        FeePolicy::Target {
            confirmation_blocks,
            ..
        } => match client.estimate_smart_fee(*confirmation_blocks).await {
            Ok(rate) if rate > 0 => Ok(rate as f64),
            Ok(_) | Err(_) => {
                let relay_floor = fetch_liquid_fee_rate_sat_per_vb(client).await?;
                info!(
                    confirmation_blocks,
                    relay_floor_sat_per_vb = relay_floor,
                    "Liquid smart-fee estimate unavailable; using relay floor"
                );
                Ok(relay_floor)
            }
        },
    }
}

pub(super) fn liquid_fee_sats(vbytes: u64, sat_per_vb: f64) -> Result<u64, DaError> {
    if !sat_per_vb.is_finite() || sat_per_vb < 0.0 {
        return Err(RuntimeError::Misconfigured(format!(
            "liquid fee rate must be finite and non-negative, got {sat_per_vb}"
        ))
        .into());
    }
    if sat_per_vb == 0.0 {
        return Ok(1);
    }

    let fee_sats = (vbytes as f64 * sat_per_vb).ceil();
    if !fee_sats.is_finite() || fee_sats < 0.0 || fee_sats > u64::MAX as f64 {
        return Err(RuntimeError::Internal(format!(
            "Liquid fee calculation overflowed for {vbytes} vbytes at {sat_per_vb} sat/vB"
        ))
        .into());
    }

    Ok((fee_sats as u64).max(1))
}

pub(super) fn estimate_liquid_reveal_fee_sats(
    runtime: LiquidRuntime<'_>,
    artifacts: &LiquidInscriptionArtifacts,
    commit_output: &LiquidTxOut,
    commit_output_secrets: &LiquidTxOutSecrets,
    destination: &LiquidAddress,
    sat_per_vb: f64,
) -> Result<u64, DaError> {
    let tx = build_signed_liquid_reveal_tx(
        runtime,
        &LiquidRevealBuildRequest {
            commit_txid: LiquidTxid::from_byte_array([0u8; 32]),
            commit_output_vout: 0,
            commit_output,
            commit_output_secrets,
            destination,
            artifacts,
            fee_sats: 0,
            target_prefix: None,
        },
    )?;
    liquid_fee_sats(tx.vsize() as u64, sat_per_vb)
}

pub(super) fn estimate_reserved_liquid_reveal_fee_sats(
    runtime: LiquidRuntime<'_>,
    artifacts: &LiquidInscriptionArtifacts,
    destination: &LiquidAddress,
    sat_per_vb: f64,
) -> Result<u64, DaError> {
    let provisional_commit_output = liquid_explicit_output(
        runtime.chain_params.pegged_asset,
        2,
        artifacts.commit_script_pubkey.clone(),
    );
    let provisional_commit_output_secrets =
        liquid_explicit_txout_secrets(runtime.chain_params.pegged_asset, 2);
    estimate_liquid_reveal_fee_sats(
        runtime,
        artifacts,
        &provisional_commit_output,
        &provisional_commit_output_secrets,
        destination,
        sat_per_vb,
    )
}
