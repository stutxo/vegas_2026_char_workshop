use super::{
    BITCOIN_MAX_SINGLE_INSCRIPTION_PAYLOAD_BYTES, BitcoinDa,
    fees::{
        estimate_bitcoin_reveal_fee_sats, estimate_reserved_bitcoin_reveal_fee_sats,
        fetch_bitcoin_relay_floor_sat_per_vb, resolve_bitcoin_estimated_fee_rate,
    },
    rpc::{
        bitcoin_mempool_rejection_summary, is_bitcoin_fee_rejection,
        probe_bitcoin_mempool_acceptance, send_bitcoin_transaction,
    },
    tx::{
        build_bitcoin_inscription_artifacts, build_signed_bitcoin_reveal_tx,
        build_signed_bitcoin_reveal_tx_async, find_bitcoin_output,
        validate_bitcoin_inscription_cluster_size,
    },
    types::{
        BitcoinInscriptionArtifacts, BitcoinInscriptionCandidate,
        BitcoinInscriptionCandidateRequest, BitcoinRevealBuildRequest, BitcoinRuntime,
    },
};
use crate::da::backends::common::{chunk_size_bounds, encode_chunk_payloads, fee_policy_rates};
use crate::da::{BitcoinBlobLocator, DaError, RuntimeError, SemanticError};
use ::bitcoin::{
    Address, Amount, Transaction, Txid,
    key::{Keypair, Secp256k1},
    secp256k1::{All, SecretKey},
};
use serde_json::{Map as JsonMap, Value as JsonValue};

impl BitcoinDa {
    pub(super) async fn inscription_fee_rates(&self) -> Result<Vec<f64>, DaError> {
        let relay_floor = fetch_bitcoin_relay_floor_sat_per_vb(&self.client).await?;
        let estimated_rate =
            resolve_bitcoin_estimated_fee_rate(&self.client, &self.fee_policy).await?;
        fee_policy_rates(&self.fee_policy, estimated_rate, relay_floor)
    }

    fn random_reveal_keypair(&self, secp: &Secp256k1<All>) -> Keypair {
        let secret_key = SecretKey::new(&mut ::bitcoin::secp256k1::rand::rngs::OsRng);
        Keypair::from_secret_key(secp, &secret_key)
    }

    fn wallet_commit_outputs(
        &self,
        commit_address: &Address,
        commit_value_sats: u64,
    ) -> Vec<JsonValue> {
        let mut output = JsonMap::new();
        output.insert(
            commit_address.to_string(),
            serde_json::json!(Amount::from_sat(commit_value_sats).to_btc()),
        );
        vec![JsonValue::Object(output)]
    }

    async fn build_inscription_artifacts(
        &self,
        runtime: BitcoinRuntime<'_>,
        payload: &[u8],
    ) -> Result<BitcoinInscriptionArtifacts, DaError> {
        let wallet_key = self.fetch_wallet_internal_key_info().await?;
        let reveal_keypair = self.random_reveal_keypair(runtime.secp);
        build_bitcoin_inscription_artifacts(
            runtime.secp,
            wallet_key.internal_key,
            &reveal_keypair,
            runtime.network,
            payload,
        )
    }

    async fn build_inscription_candidate(
        &self,
        runtime: BitcoinRuntime<'_>,
        request: BitcoinInscriptionCandidateRequest,
    ) -> Result<BitcoinInscriptionCandidate, DaError> {
        let BitcoinInscriptionCandidateRequest {
            artifacts,
            reveal_destination,
            reserved_reveal_fee_sats,
            commit_sat_per_vb,
            reveal_sat_per_vb,
            grind_reveal_prefix,
        } = request;
        let commit_dust_limit = artifacts.commit_script_pubkey.minimal_non_dust().to_sat();
        let destination_dust_limit = reveal_destination
            .script_pubkey()
            .minimal_non_dust()
            .to_sat();
        let commit_output_value_sats =
            commit_dust_limit.max(destination_dust_limit.saturating_add(reserved_reveal_fee_sats));
        let (unsigned_commit_tx, _commit_fee_sats) = self
            .fund_wallet_transaction(
                self.wallet_commit_outputs(&artifacts.commit_address, commit_output_value_sats),
                commit_sat_per_vb,
            )
            .await?;
        let commit_tx = self.sign_wallet_transaction(&unsigned_commit_tx).await?;
        let (commit_output_vout, commit_output) = find_bitcoin_output(
            &commit_tx,
            &artifacts.commit_script_pubkey,
            commit_output_value_sats,
        )?;
        let reveal_fee_sats = estimate_bitcoin_reveal_fee_sats(
            runtime.secp,
            &artifacts,
            &commit_output,
            &reveal_destination,
            reveal_sat_per_vb,
        )?;
        let provisional_reveal_tx = build_signed_bitcoin_reveal_tx(
            runtime.secp,
            &BitcoinRevealBuildRequest {
                commit_txid: commit_tx.compute_txid(),
                commit_output_vout,
                commit_output: &commit_output,
                destination: &reveal_destination,
                artifacts: &artifacts,
                fee_sats: reveal_fee_sats,
                target_prefix: None,
            },
        )?;
        validate_bitcoin_inscription_cluster_size(&commit_tx, &provisional_reveal_tx)?;
        let reveal_tx = if grind_reveal_prefix {
            build_signed_bitcoin_reveal_tx_async(
                runtime.secp,
                &BitcoinRevealBuildRequest {
                    commit_txid: commit_tx.compute_txid(),
                    commit_output_vout,
                    commit_output: &commit_output,
                    destination: &reveal_destination,
                    artifacts: &artifacts,
                    fee_sats: reveal_fee_sats,
                    target_prefix: Some(runtime.target_prefix),
                },
            )
            .await?
        } else {
            provisional_reveal_tx
        };

        Ok(BitcoinInscriptionCandidate {
            commit_tx,
            commit_output_vout,
            reveal_tx,
            artifacts,
            reveal_destination,
        })
    }

    async fn build_reveal_tx_at_rate(
        &self,
        runtime: BitcoinRuntime<'_>,
        candidate: &BitcoinInscriptionCandidate,
        commit_txid: Txid,
        sat_per_vb: f64,
    ) -> Result<Transaction, DaError> {
        let commit_output = &candidate.commit_tx.output[candidate.commit_output_vout as usize];
        let reveal_fee_sats = estimate_bitcoin_reveal_fee_sats(
            runtime.secp,
            &candidate.artifacts,
            commit_output,
            &candidate.reveal_destination,
            sat_per_vb,
        )?;
        build_signed_bitcoin_reveal_tx_async(
            runtime.secp,
            &BitcoinRevealBuildRequest {
                commit_txid,
                commit_output_vout: candidate.commit_output_vout,
                commit_output,
                destination: &candidate.reveal_destination,
                artifacts: &candidate.artifacts,
                fee_sats: reveal_fee_sats,
                target_prefix: Some(runtime.target_prefix),
            },
        )
        .await
    }

    async fn write_inscription_only_payload(
        &self,
        runtime: BitcoinRuntime<'_>,
        payload: &[u8],
    ) -> Result<Txid, DaError> {
        let fee_rates = self.inscription_fee_rates().await?;
        let max_reveal_rate = fee_rates.last().copied().ok_or_else(|| {
            RuntimeError::Internal("Bitcoin inscription fee policy returned no rates".to_string())
        })?;
        let reveal_destination = self.fetch_wallet_return_address().await?;
        let artifacts = self.build_inscription_artifacts(runtime, payload).await?;
        let reserved_reveal_fee_sats = estimate_reserved_bitcoin_reveal_fee_sats(
            runtime.secp,
            &artifacts,
            &reveal_destination,
            max_reveal_rate,
        )?;

        for (index, fee_rate) in fee_rates.iter().copied().enumerate() {
            let candidate = self
                .build_inscription_candidate(
                    runtime,
                    BitcoinInscriptionCandidateRequest {
                        artifacts: artifacts.clone(),
                        reveal_destination: reveal_destination.clone(),
                        reserved_reveal_fee_sats,
                        commit_sat_per_vb: fee_rate,
                        reveal_sat_per_vb: fee_rate,
                        grind_reveal_prefix: true,
                    },
                )
                .await?;
            let (commit_allowed, commit_reason, commit_details) =
                probe_bitcoin_mempool_acceptance(&self.client, &candidate.commit_tx).await?;
            if !commit_allowed {
                if is_bitcoin_fee_rejection(&commit_reason, &commit_details) {
                    continue;
                }
                return Err(
                    SemanticError::PreconditionFailed(bitcoin_mempool_rejection_summary(
                        &commit_reason,
                        &commit_details,
                    ))
                    .into(),
                );
            }

            let commit_txid = send_bitcoin_transaction(&self.client, &candidate.commit_tx).await?;
            for reveal_rate in fee_rates[index..].iter().copied() {
                let reveal_tx = if reveal_rate == fee_rate {
                    candidate.reveal_tx.clone()
                } else {
                    self.build_reveal_tx_at_rate(runtime, &candidate, commit_txid, reveal_rate)
                        .await?
                };
                let (reveal_allowed, reveal_reason, reveal_details) =
                    probe_bitcoin_mempool_acceptance(&self.client, &reveal_tx).await?;
                if !reveal_allowed {
                    if is_bitcoin_fee_rejection(&reveal_reason, &reveal_details) {
                        continue;
                    }
                    return Err(SemanticError::PreconditionFailed(
                        bitcoin_mempool_rejection_summary(&reveal_reason, &reveal_details),
                    )
                    .into());
                }

                return send_bitcoin_transaction(&self.client, &reveal_tx).await;
            }

            return Err(RuntimeError::Internal(format!(
                "Bitcoin commit {} broadcast, but reveal transaction was rejected at every configured fee level",
                commit_txid
            ))
            .into());
        }

        Err(SemanticError::PreconditionFailed(
            "No acceptable Bitcoin inscription fee rate found".to_string(),
        )
        .into())
    }

    fn fixed_chunk_size(&self) -> Result<usize, DaError> {
        let Some((initial_chunk_size, min_chunk_size)) = chunk_size_bounds(&self.oversize_policy)
        else {
            return Err(SemanticError::ExceedsLimit(
                "Bitcoin inscription exceeds the single-transaction standardness limit and chunking is disabled".to_string(),
            )
            .into());
        };
        let chunk_size = initial_chunk_size.min(BITCOIN_MAX_SINGLE_INSCRIPTION_PAYLOAD_BYTES);
        if chunk_size < min_chunk_size {
            return Err(SemanticError::ExceedsLimit(format!(
                "Bitcoin chunked inscription minimum size {} exceeds the fixed {}-byte chunk cap",
                min_chunk_size, BITCOIN_MAX_SINGLE_INSCRIPTION_PAYLOAD_BYTES
            ))
            .into());
        }

        Ok(chunk_size)
    }

    async fn write_chunked_payload(
        &self,
        runtime: BitcoinRuntime<'_>,
        payload: &[u8],
    ) -> Result<BitcoinBlobLocator, DaError> {
        let chunk_size = self.fixed_chunk_size()?;
        let encoded_chunks = encode_chunk_payloads(payload, chunk_size)?;
        let mut chunk_txids = Vec::with_capacity(encoded_chunks.len());
        for chunk in encoded_chunks {
            chunk_txids.push(
                self.write_inscription_only_payload(runtime, chunk.as_slice())
                    .await?,
            );
        }

        BitcoinBlobLocator::from_chunked(chunk_txids)
    }

    pub(super) async fn write_payload(
        &self,
        runtime: BitcoinRuntime<'_>,
        payload: &[u8],
    ) -> Result<BitcoinBlobLocator, DaError> {
        if payload.len() > BITCOIN_MAX_SINGLE_INSCRIPTION_PAYLOAD_BYTES {
            return self.write_chunked_payload(runtime, payload).await;
        }

        match self.write_inscription_only_payload(runtime, payload).await {
            Ok(txid) => Ok(BitcoinBlobLocator::from_txid(txid)),
            Err(DaError::Semantic(SemanticError::ExceedsLimit(_))) => {
                self.write_chunked_payload(runtime, payload).await
            }
            Err(err) => Err(err),
        }
    }
}
