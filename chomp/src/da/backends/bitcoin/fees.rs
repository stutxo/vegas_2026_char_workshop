use super::{
    runtime::map_bitcoin_client_error,
    tx::{build_bitcoin_reveal_tx, sign_bitcoin_scriptspend_tx},
    types::{BitcoinInscriptionArtifacts, EstimateSmartFeeResult},
};
use crate::da::backends::common::validate_fee_policy;
use crate::da::{DaError, FeePolicy, RuntimeError};
use ::bitcoin::{
    Address, Amount, FeeRate as BitcoinFeeRate, TxOut, Txid, hashes::Hash, key::Secp256k1,
    secp256k1::All,
};
use bitcoind_async_client::Client as BitcoinClient;
use bitcoind_async_client::traits::Reader;
use tracing::info;

const BITCOIN_MIN_RELAY_FLOOR_SAT_PER_VB: f64 = 0.1;

pub(super) fn validate_single_fee_policy(policy: &FeePolicy) -> Result<(), DaError> {
    validate_fee_policy(policy)
}

fn apply_bitcoin_min_relay_floor(node_floor_sat_per_vb: f64) -> Result<f64, DaError> {
    if node_floor_sat_per_vb.is_finite() && node_floor_sat_per_vb >= 0.0 {
        Ok(node_floor_sat_per_vb.max(BITCOIN_MIN_RELAY_FLOOR_SAT_PER_VB))
    } else {
        Err(RuntimeError::Internal(format!(
            "Bitcoin node returned an invalid relay floor fee rate: {node_floor_sat_per_vb}"
        ))
        .into())
    }
}

pub(super) fn bitcoin_fee_rate_btc_per_kvb_to_sat_per_vb(
    fee_rate_btc_per_kvb: f64,
) -> Result<f64, DaError> {
    if fee_rate_btc_per_kvb.is_finite() && fee_rate_btc_per_kvb >= 0.0 {
        Ok(fee_rate_btc_per_kvb * 100_000.0)
    } else {
        Err(RuntimeError::Internal(format!(
            "Bitcoin node returned an invalid fee rate: {fee_rate_btc_per_kvb}"
        ))
        .into())
    }
}

pub(super) fn extract_bitcoin_estimated_fee_rate(
    result: EstimateSmartFeeResult,
) -> Result<Option<f64>, DaError> {
    result
        .feerate
        .map(bitcoin_fee_rate_btc_per_kvb_to_sat_per_vb)
        .transpose()
}

pub(super) fn bitcoin_fee_rate_to_sat_per_vb(rate: BitcoinFeeRate) -> Result<f64, DaError> {
    let sat_per_vb = rate.to_sat_per_kwu() as f64 / 250.0;
    if sat_per_vb.is_finite() && sat_per_vb >= 0.0 {
        Ok(sat_per_vb)
    } else {
        Err(RuntimeError::Internal(format!(
            "Bitcoin node returned an invalid fee rate: {sat_per_vb}"
        ))
        .into())
    }
}

pub(super) async fn fetch_bitcoin_relay_floor_sat_per_vb(
    client: &BitcoinClient,
) -> Result<f64, DaError> {
    let mempool_info = client
        .get_mempool_info()
        .await
        .map_err(map_bitcoin_client_error)?;
    let mut rates = Vec::new();
    if let Some(rate) = mempool_info.mempool_min_fee {
        rates.push(bitcoin_fee_rate_to_sat_per_vb(rate)?);
    }
    if let Some(rate) = mempool_info.min_relay_tx_fee {
        rates.push(bitcoin_fee_rate_to_sat_per_vb(rate)?);
    }
    let node_floor_sat_per_vb = rates.into_iter().max_by(f64::total_cmp).ok_or_else(|| {
        DaError::from(RuntimeError::Internal(
            "Bitcoin getmempoolinfo did not return a relay or mempool minimum fee".to_string(),
        ))
    })?;
    apply_bitcoin_min_relay_floor(node_floor_sat_per_vb)
}

pub(super) async fn resolve_bitcoin_estimated_fee_rate(
    client: &BitcoinClient,
    policy: &FeePolicy,
) -> Result<f64, DaError> {
    match policy {
        FeePolicy::Manual { sat_per_vb } => Ok(*sat_per_vb),
        FeePolicy::Target {
            confirmation_blocks,
            ..
        } => {
            // Keep this raw: the typed wrapper bakes in its own fallback fee semantics, but this
            // backend intentionally distinguishes "missing estimate" from a real node value.
            let result: EstimateSmartFeeResult = client
                .call_raw(
                    "estimatesmartfee",
                    &[serde_json::json!(*confirmation_blocks)],
                )
                .await
                .map_err(map_bitcoin_client_error)?;
            match extract_bitcoin_estimated_fee_rate(result)? {
                Some(rate) if rate > 0.0 => Ok(rate),
                Some(_) | None => {
                    let relay_floor = fetch_bitcoin_relay_floor_sat_per_vb(client).await?;
                    info!(
                        confirmation_blocks,
                        relay_floor_sat_per_vb = relay_floor,
                        "Bitcoin smart-fee estimate unavailable; using relay floor"
                    );
                    Ok(relay_floor)
                }
            }
        }
    }
}

pub(super) fn bitcoin_fee_sats(vbytes: u64, sat_per_vb: f64) -> Result<u64, DaError> {
    if !sat_per_vb.is_finite() || sat_per_vb < 0.0 {
        return Err(RuntimeError::Misconfigured(format!(
            "bitcoin fee rate must be finite and non-negative, got {sat_per_vb}"
        ))
        .into());
    }

    let fee_sats = (vbytes as f64 * sat_per_vb).ceil();
    if !fee_sats.is_finite() || fee_sats < 0.0 || fee_sats > u64::MAX as f64 {
        return Err(RuntimeError::Internal(format!(
            "bitcoin fee calculation overflowed for {vbytes} vbytes at {sat_per_vb} sat/vB"
        ))
        .into());
    }

    Ok((fee_sats as u64).max(1))
}

pub(super) fn estimate_bitcoin_reveal_vbytes(
    secp: &Secp256k1<All>,
    artifacts: &BitcoinInscriptionArtifacts,
    commit_output: &TxOut,
    destination: &Address,
) -> Result<u64, DaError> {
    let provisional_reveal_tx = sign_bitcoin_scriptspend_tx(
        secp,
        build_bitcoin_reveal_tx(
            Txid::from_byte_array([0u8; 32]),
            0,
            commit_output,
            destination,
            0,
        )?,
        commit_output.clone(),
        &artifacts.tapscript,
        &artifacts.spend_info,
        &artifacts.reveal_keypair,
    )?;
    Ok(provisional_reveal_tx.vsize() as u64)
}

pub(super) fn estimate_bitcoin_reveal_fee_sats(
    secp: &Secp256k1<All>,
    artifacts: &BitcoinInscriptionArtifacts,
    commit_output: &TxOut,
    destination: &Address,
    sat_per_vb: f64,
) -> Result<u64, DaError> {
    let vbytes = estimate_bitcoin_reveal_vbytes(secp, artifacts, commit_output, destination)?;
    bitcoin_fee_sats(vbytes, sat_per_vb)
}

pub(super) fn estimate_reserved_bitcoin_reveal_fee_sats(
    secp: &Secp256k1<All>,
    artifacts: &BitcoinInscriptionArtifacts,
    destination: &Address,
    sat_per_vb: f64,
) -> Result<u64, DaError> {
    let commit_dust_limit = artifacts.commit_script_pubkey.minimal_non_dust().to_sat();
    let destination_dust_limit = destination.script_pubkey().minimal_non_dust().to_sat();
    let provisional_commit_output = TxOut {
        value: Amount::from_sat(commit_dust_limit.max(destination_dust_limit.saturating_add(1))),
        script_pubkey: artifacts.commit_script_pubkey.clone(),
    };
    estimate_bitcoin_reveal_fee_sats(
        secp,
        artifacts,
        &provisional_commit_output,
        destination,
        sat_per_vb,
    )
}

#[cfg(test)]
mod tests {
    use super::{bitcoin_fee_rate_btc_per_kvb_to_sat_per_vb, extract_bitcoin_estimated_fee_rate};
    use crate::da::backends::bitcoin::types::EstimateSmartFeeResult;

    #[test]
    fn bitcoin_raw_fee_rate_preserves_fractional_sat_per_vb() {
        let sat_per_vb = bitcoin_fee_rate_btc_per_kvb_to_sat_per_vb(0.000001)
            .expect("fractional bitcoin fee rate should convert");

        assert!((sat_per_vb - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn bitcoin_raw_fee_rate_none_stays_none() {
        let rate = extract_bitcoin_estimated_fee_rate(EstimateSmartFeeResult { feerate: None })
            .expect("missing feerate should be allowed");

        assert_eq!(rate, None);
    }
}
