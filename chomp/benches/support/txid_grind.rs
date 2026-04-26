use bitcoin::Transaction;
use bitcoin::hashes::{Hash, sha256d};
use chomp::{MIN_NAMESPACE_ID_LEN, NamespaceId, NamespaceIdError};
use elements::Transaction as LiquidTransaction;
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;

pub(crate) const PREFIX_LEN: usize = MIN_NAMESPACE_ID_LEN;
pub(crate) type TxidPrefix = [u8; PREFIX_LEN];
const CHUNK_SIZE: u32 = 65_536;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub(crate) enum GrindError {
    #[error("transaction has no inputs")]
    NoInputs,
    #[error("unable to find a matching txid prefix before exhausting the search space")]
    Exhausted,
}

pub(crate) fn derive_prefix(namespace_id: &[u8]) -> Result<TxidPrefix, NamespaceIdError> {
    let namespace_id = NamespaceId::new(namespace_id)?;
    namespace_id.as_bytes()[..PREFIX_LEN]
        .try_into()
        .map_err(|_| NamespaceIdError::NamespaceIdTooShort {
            len: namespace_id.as_bytes().len(),
        })
}

pub(crate) fn benchmark_bitcoin_attempts(
    tx: &Transaction,
    target_prefix: &TxidPrefix,
    attempts: u32,
) -> u64 {
    benchmark_serialized_attempts(serialize_non_witness(tx), target_prefix, attempts)
}

pub(crate) fn benchmark_liquid_attempts(
    tx: &LiquidTransaction,
    target_prefix: &TxidPrefix,
    attempts: u32,
) -> u64 {
    benchmark_serialized_attempts(serialize_liquid_non_witness(tx), target_prefix, attempts)
}

pub(crate) fn grind_txid_prefix(
    tx: &Transaction,
    target_prefix: &TxidPrefix,
) -> Result<u32, GrindError> {
    grind_serialized_txid_prefix(tx.input.len(), serialize_non_witness(tx), target_prefix)
}

pub(crate) fn grind_liquid_txid_prefix(
    tx: &LiquidTransaction,
    target_prefix: &TxidPrefix,
) -> Result<u32, GrindError> {
    grind_serialized_txid_prefix(
        tx.input.len(),
        serialize_liquid_non_witness(tx),
        target_prefix,
    )
}

fn serialize_non_witness(tx: &Transaction) -> Vec<u8> {
    let mut out = Vec::new();
    bitcoin::consensus::Encodable::consensus_encode(&tx.version, &mut out).unwrap();
    bitcoin::consensus::Encodable::consensus_encode(&tx.input, &mut out).unwrap();
    bitcoin::consensus::Encodable::consensus_encode(&tx.output, &mut out).unwrap();
    bitcoin::consensus::Encodable::consensus_encode(&tx.lock_time, &mut out).unwrap();
    out
}

fn serialize_liquid_non_witness(tx: &LiquidTransaction) -> Vec<u8> {
    let mut out = Vec::new();
    elements::encode::Encodable::consensus_encode(&tx.version, &mut out).unwrap();
    elements::encode::Encodable::consensus_encode(&0u8, &mut out).unwrap();
    elements::encode::Encodable::consensus_encode(&tx.input, &mut out).unwrap();
    elements::encode::Encodable::consensus_encode(&tx.output, &mut out).unwrap();
    elements::encode::Encodable::consensus_encode(&tx.lock_time, &mut out).unwrap();
    out
}

fn displayed_txid_matches_prefix(
    raw: &[u8; sha256d::Hash::LEN],
    target_prefix: &TxidPrefix,
) -> bool {
    raw[31] == target_prefix[0] && raw[30] == target_prefix[1] && raw[29] == target_prefix[2]
}

fn benchmark_serialized_attempts(base: Vec<u8>, target_prefix: &TxidPrefix, attempts: u32) -> u64 {
    let locktime_offset = base.len() - 4;
    let num_chunks = attempts.div_ceil(CHUNK_SIZE);

    (0..num_chunks)
        .into_par_iter()
        .map(|chunk_idx| {
            let mut local_buf = base.clone();
            let start = chunk_idx.saturating_mul(CHUNK_SIZE);
            let end = start.saturating_add(CHUNK_SIZE).min(attempts);
            let mut matches = 0u64;

            for nonce in start..end {
                local_buf[locktime_offset..].copy_from_slice(&nonce.to_le_bytes());
                let hash = sha256d::Hash::hash(&local_buf);

                if displayed_txid_matches_prefix(hash.as_byte_array(), target_prefix) {
                    matches += 1;
                }
            }

            matches
        })
        .sum()
}

fn grind_serialized_txid_prefix(
    input_count: usize,
    base: Vec<u8>,
    target_prefix: &TxidPrefix,
) -> Result<u32, GrindError> {
    if input_count == 0 {
        return Err(GrindError::NoInputs);
    }

    let locktime_offset = base.len() - 4;
    let found = AtomicBool::new(false);
    let num_chunks = (u32::MAX / CHUNK_SIZE) + 1;

    let winner = (0..num_chunks).into_par_iter().find_map_any(|chunk_idx| {
        if found.load(Ordering::Relaxed) {
            return None;
        }

        let mut local_buf = base.clone();
        let start = chunk_idx.saturating_mul(CHUNK_SIZE);
        let end = start.saturating_add(CHUNK_SIZE);

        for nonce in start..end {
            local_buf[locktime_offset..].copy_from_slice(&nonce.to_le_bytes());
            let hash = sha256d::Hash::hash(&local_buf);

            if displayed_txid_matches_prefix(hash.as_byte_array(), target_prefix) {
                found.store(true, Ordering::Relaxed);
                return Some(nonce);
            }
        }

        None
    });

    winner.ok_or(GrindError::Exhausted)
}
