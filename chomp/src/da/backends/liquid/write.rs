use super::{
    LIQUID_MAX_SINGLE_INSCRIPTION_PAYLOAD_BYTES, LIQUID_MAX_STANDARD_WEIGHT, LiquidDa,
    fees::{
        estimate_liquid_reveal_fee_sats, estimate_reserved_liquid_reveal_fee_sats,
        fetch_liquid_fee_rate_sat_per_vb, resolve_liquid_estimated_fee_rate,
    },
    rpc::{
        is_liquid_fee_rejection, liquid_mempool_rejection_summary, probe_liquid_mempool_acceptance,
        send_liquid_transaction,
    },
    tx::{
        build_liquid_commit_intent_tx, build_liquid_inscription_artifacts,
        build_signed_liquid_reveal_tx, find_liquid_output, liquid_address_params,
        liquid_explicit_txout_secrets,
    },
    types::{
        LiquidInscriptionArtifacts, LiquidInscriptionCandidate, LiquidInscriptionCandidateRequest,
        LiquidRevealBuildRequest, LiquidRuntime,
    },
};
use crate::da::backends::common::{chunk_size_bounds, encode_chunk_payloads, fee_policy_rates};
use crate::da::{DaError, LiquidBlobLocator, RuntimeError, SemanticError};
use bitcoin::secp256k1::rand::rngs::OsRng;
use elements::{
    Transaction as LiquidTransaction, Txid as LiquidTxid,
    schnorr::Keypair as LiquidKeypair,
    secp256k1_zkp::{
        All as LiquidSecp256k1All, Secp256k1 as LiquidSecp256k1, SecretKey as LiquidSecretKey,
    },
};

impl LiquidDa {
    pub(super) async fn inscription_fee_rates(&self) -> Result<Vec<f64>, DaError> {
        let relay_floor = fetch_liquid_fee_rate_sat_per_vb(&self.client).await?;
        let estimated_rate =
            resolve_liquid_estimated_fee_rate(&self.client, &self.fee_policy).await?;
        fee_policy_rates(&self.fee_policy, estimated_rate, relay_floor)
    }

    fn random_reveal_keypair(&self, secp: &LiquidSecp256k1<LiquidSecp256k1All>) -> LiquidKeypair {
        let secret_key = LiquidSecretKey::new(&mut OsRng);
        LiquidKeypair::from_secret_key(secp, &secret_key)
    }

    async fn build_inscription_artifacts(
        &self,
        runtime: LiquidRuntime<'_>,
        payload: &[u8],
    ) -> Result<LiquidInscriptionArtifacts, DaError> {
        let internal_key = self.fetch_wallet_internal_key().await?;
        let reveal_keypair = self.random_reveal_keypair(runtime.secp);
        build_liquid_inscription_artifacts(
            runtime.secp,
            internal_key,
            None,
            liquid_address_params(self.network),
            &reveal_keypair,
            payload,
        )
    }

    async fn build_inscription_candidate(
        &self,
        runtime: LiquidRuntime<'_>,
        request: LiquidInscriptionCandidateRequest,
    ) -> Result<LiquidInscriptionCandidate, DaError> {
        let LiquidInscriptionCandidateRequest {
            artifacts,
            reveal_destination,
            reserved_reveal_fee_sats,
            commit_sat_per_vb,
            reveal_sat_per_vb,
            grind_reveal_prefix,
        } = request;
        let commit_output_value_sats = reserved_reveal_fee_sats.saturating_add(1).max(1);
        let (unsigned_commit_tx, _commit_fee_sats) = self
            .fund_wallet_transaction(
                &build_liquid_commit_intent_tx(
                    &artifacts.commit_address,
                    runtime.chain_params.pegged_asset,
                    commit_output_value_sats,
                ),
                commit_sat_per_vb,
            )
            .await?;
        let commit_tx = self
            .sign_wallet_transaction(&unsigned_commit_tx, None)
            .await?;
        let (commit_output_vout, commit_output) = find_liquid_output(
            &commit_tx,
            &artifacts.commit_script_pubkey,
            runtime.chain_params.pegged_asset,
            commit_output_value_sats,
        )?;
        let commit_output_secrets = liquid_explicit_txout_secrets(
            runtime.chain_params.pegged_asset,
            commit_output_value_sats,
        );
        let reveal_fee_sats = estimate_liquid_reveal_fee_sats(
            runtime,
            &artifacts,
            &commit_output,
            &commit_output_secrets,
            &reveal_destination,
            reveal_sat_per_vb,
        )?;
        let provisional_reveal_tx = build_signed_liquid_reveal_tx(
            runtime,
            &LiquidRevealBuildRequest {
                commit_txid: commit_tx.txid(),
                commit_output_vout,
                commit_output: &commit_output,
                commit_output_secrets: &commit_output_secrets,
                destination: &reveal_destination,
                artifacts: &artifacts,
                fee_sats: reveal_fee_sats,
                target_prefix: None,
            },
        )?;
        if provisional_reveal_tx.weight() > LIQUID_MAX_STANDARD_WEIGHT {
            return Err(SemanticError::ExceedsLimit(format!(
                "Liquid inscription reveal exceeds the standard transaction weight limit: {} wu > {} wu",
                provisional_reveal_tx.weight(),
                LIQUID_MAX_STANDARD_WEIGHT
            ))
            .into());
        }
        let reveal_tx = if grind_reveal_prefix {
            build_signed_liquid_reveal_tx(
                runtime,
                &LiquidRevealBuildRequest {
                    commit_txid: commit_tx.txid(),
                    commit_output_vout,
                    commit_output: &commit_output,
                    commit_output_secrets: &commit_output_secrets,
                    destination: &reveal_destination,
                    artifacts: &artifacts,
                    fee_sats: reveal_fee_sats,
                    target_prefix: Some(runtime.target_prefix),
                },
            )?
        } else {
            provisional_reveal_tx
        };

        Ok(LiquidInscriptionCandidate {
            commit_tx,
            commit_output_vout,
            reveal_tx,
            commit_output_secrets,
            artifacts,
            reveal_destination,
        })
    }

    fn build_reveal_tx_at_rate(
        &self,
        runtime: LiquidRuntime<'_>,
        candidate: &LiquidInscriptionCandidate,
        commit_txid: LiquidTxid,
        sat_per_vb: f64,
    ) -> Result<LiquidTransaction, DaError> {
        let commit_output = &candidate.commit_tx.output[candidate.commit_output_vout as usize];
        let reveal_fee_sats = estimate_liquid_reveal_fee_sats(
            runtime,
            &candidate.artifacts,
            commit_output,
            &candidate.commit_output_secrets,
            &candidate.reveal_destination,
            sat_per_vb,
        )?;
        build_signed_liquid_reveal_tx(
            runtime,
            &LiquidRevealBuildRequest {
                commit_txid,
                commit_output_vout: candidate.commit_output_vout,
                commit_output,
                commit_output_secrets: &candidate.commit_output_secrets,
                destination: &candidate.reveal_destination,
                artifacts: &candidate.artifacts,
                fee_sats: reveal_fee_sats,
                target_prefix: Some(runtime.target_prefix),
            },
        )
    }

    async fn write_inscription_only_payload(
        &self,
        runtime: LiquidRuntime<'_>,
        payload: &[u8],
    ) -> Result<LiquidTxid, DaError> {
        let fee_rates = self.inscription_fee_rates().await?;
        let max_reveal_rate = fee_rates.last().copied().ok_or_else(|| {
            RuntimeError::Internal("Liquid inscription fee policy returned no rates".to_string())
        })?;
        let reveal_destination = self.fetch_wallet_return_address().await?;
        let artifacts = self.build_inscription_artifacts(runtime, payload).await?;
        let reserved_reveal_fee_sats = estimate_reserved_liquid_reveal_fee_sats(
            runtime,
            &artifacts,
            &reveal_destination,
            max_reveal_rate,
        )?;

        for (index, fee_rate) in fee_rates.iter().copied().enumerate() {
            let candidate = self
                .build_inscription_candidate(
                    runtime,
                    LiquidInscriptionCandidateRequest {
                        artifacts: artifacts.clone(),
                        reveal_destination: reveal_destination.clone(),
                        reserved_reveal_fee_sats,
                        commit_sat_per_vb: fee_rate,
                        reveal_sat_per_vb: fee_rate,
                        grind_reveal_prefix: true,
                    },
                )
                .await?;
            let commit_result =
                probe_liquid_mempool_acceptance(&self.client, &candidate.commit_tx).await?;
            if !commit_result.allowed {
                if is_liquid_fee_rejection(&commit_result) {
                    continue;
                }
                return Err(
                    SemanticError::PreconditionFailed(liquid_mempool_rejection_summary(
                        &commit_result,
                    ))
                    .into(),
                );
            }

            let commit_txid = send_liquid_transaction(&self.client, &candidate.commit_tx).await?;
            for reveal_rate in fee_rates[index..].iter().copied() {
                let reveal_tx = if reveal_rate == fee_rate {
                    candidate.reveal_tx.clone()
                } else {
                    self.build_reveal_tx_at_rate(runtime, &candidate, commit_txid, reveal_rate)?
                };
                let reveal_result =
                    probe_liquid_mempool_acceptance(&self.client, &reveal_tx).await?;
                if !reveal_result.allowed {
                    if is_liquid_fee_rejection(&reveal_result) {
                        continue;
                    }
                    return Err(SemanticError::PreconditionFailed(
                        liquid_mempool_rejection_summary(&reveal_result),
                    )
                    .into());
                }

                return send_liquid_transaction(&self.client, &reveal_tx).await;
            }

            return Err(RuntimeError::Internal(format!(
                "Liquid commit {} broadcast, but reveal transaction was rejected at every configured fee level",
                commit_txid
            ))
            .into());
        }

        Err(SemanticError::PreconditionFailed(
            "No acceptable Liquid inscription fee rate found".to_string(),
        )
        .into())
    }

    fn fixed_chunk_size(&self) -> Result<usize, DaError> {
        let Some((initial_chunk_size, min_chunk_size)) = chunk_size_bounds(&self.oversize_policy)
        else {
            return Err(SemanticError::ExceedsLimit(
                "Liquid inscription exceeds the single-transaction standardness limit and chunking is disabled".to_string(),
            )
            .into());
        };
        let chunk_size = initial_chunk_size.min(LIQUID_MAX_SINGLE_INSCRIPTION_PAYLOAD_BYTES);
        if chunk_size < min_chunk_size {
            return Err(SemanticError::ExceedsLimit(format!(
                "Liquid chunked inscription minimum size {} exceeds the fixed {}-byte chunk cap",
                min_chunk_size, LIQUID_MAX_SINGLE_INSCRIPTION_PAYLOAD_BYTES
            ))
            .into());
        }

        Ok(chunk_size)
    }

    async fn write_chunked_payload(
        &self,
        runtime: LiquidRuntime<'_>,
        payload: &[u8],
    ) -> Result<LiquidBlobLocator, DaError> {
        let chunk_size = self.fixed_chunk_size()?;
        let encoded_chunks = encode_chunk_payloads(payload, chunk_size)?;
        let mut chunk_txids = Vec::with_capacity(encoded_chunks.len());
        for chunk in encoded_chunks {
            chunk_txids.push(
                self.write_inscription_only_payload(runtime, chunk.as_slice())
                    .await?,
            );
        }

        LiquidBlobLocator::from_chunked(chunk_txids)
    }

    pub(super) async fn write_payload(
        &self,
        runtime: LiquidRuntime<'_>,
        payload: &[u8],
    ) -> Result<LiquidBlobLocator, DaError> {
        if payload.len() > LIQUID_MAX_SINGLE_INSCRIPTION_PAYLOAD_BYTES {
            return self.write_chunked_payload(runtime, payload).await;
        }

        match self.write_inscription_only_payload(runtime, payload).await {
            Ok(txid) => Ok(LiquidBlobLocator::from_txid(txid)),
            Err(DaError::Semantic(SemanticError::ExceedsLimit(_))) => {
                self.write_chunked_payload(runtime, payload).await
            }
            Err(err) => Err(err),
        }
    }
}
