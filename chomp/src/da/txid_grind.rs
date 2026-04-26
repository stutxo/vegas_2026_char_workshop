use super::namespace_id::TxidPrefix;
use bitcoin::Transaction;
use bitcoin::hashes::{Hash, sha256d};
use elements::Transaction as LiquidTransaction;
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;

const CHUNK_SIZE: u32 = 65_536;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GrindError {
    #[error("transaction has no inputs")]
    NoInputs,
    #[error("unable to find a matching txid prefix before exhausting the search space")]
    Exhausted,
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
            let raw = hash.as_byte_array();

            if displayed_txid_matches_prefix(raw, target_prefix) {
                found.store(true, Ordering::Relaxed);
                return Some(nonce);
            }
        }

        None
    });

    winner.ok_or(GrindError::Exhausted)
}

/// Grinds nLockTime in parallel to find a value that gives the Bitcoin
/// transaction a displayed txid starting with `target_prefix`.
pub(crate) fn grind_txid_prefix(
    tx: &Transaction,
    target_prefix: &TxidPrefix,
) -> Result<u32, GrindError> {
    grind_serialized_txid_prefix(tx.input.len(), serialize_non_witness(tx), target_prefix)
}

/// Grinds nLockTime in parallel to find a value that gives the Liquid
/// transaction a displayed txid starting with `target_prefix`.
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

#[cfg(test)]
mod tests {
    use super::{
        TxidPrefix, displayed_txid_matches_prefix, grind_liquid_txid_prefix, grind_txid_prefix,
        serialize_non_witness,
    };
    use crate::da::namespace_id::derive_prefix;
    use bitcoin::blockdata::locktime::absolute::LockTime;
    use bitcoin::blockdata::script::ScriptBuf;
    use bitcoin::blockdata::transaction::{OutPoint, TxIn, TxOut, Version};
    use bitcoin::hashes::Hash;
    use bitcoin::hashes::sha256d;
    use bitcoin::{Sequence, Transaction, Witness};
    use elements::{
        LockTime as LiquidLockTime, OutPoint as LiquidOutPoint, Sequence as LiquidSequence,
        Transaction as LiquidTransaction, TxIn as LiquidTxIn, TxOut as LiquidTxOut,
        TxOutWitness as LiquidTxOutWitness,
        confidential::{Asset, Nonce, Value},
    };

    fn test_tx() -> Transaction {
        Transaction {
            version: Version(2),
            lock_time: LockTime::ZERO,
            input: vec![TxIn {
                previous_output: OutPoint::null(),
                script_sig: ScriptBuf::new(),
                sequence: Sequence::MAX,
                witness: Witness::default(),
            }],
            output: vec![TxOut {
                value: bitcoin::Amount::from_sat(50_000),
                script_pubkey: ScriptBuf::new(),
            }],
        }
    }

    fn test_liquid_tx() -> LiquidTransaction {
        LiquidTransaction {
            version: 2,
            lock_time: LiquidLockTime::ZERO,
            input: vec![LiquidTxIn {
                previous_output: LiquidOutPoint::default(),
                sequence: LiquidSequence::MAX,
                ..Default::default()
            }],
            output: vec![LiquidTxOut {
                asset: Asset::Explicit(elements::AssetId::from_slice(&[7u8; 32]).unwrap()),
                value: Value::Explicit(50_000),
                nonce: Nonce::Null,
                script_pubkey: elements::Script::new(),
                witness: LiquidTxOutWitness::default(),
            }],
        }
    }

    fn displayed_txid_prefix(txid_hex: &str) -> TxidPrefix {
        let bytes = hex::decode(&txid_hex[..6]).expect("txid prefix should decode");
        bytes
            .try_into()
            .expect("decoded txid prefix should have the expected length")
    }

    fn verify(tx: &Transaction, nonce: u32, prefix: &TxidPrefix) {
        let mut tx = tx.clone();
        tx.lock_time = LockTime::from_consensus(nonce);
        let nw = serialize_non_witness(&tx);
        let hash = sha256d::Hash::hash(&nw);
        assert!(displayed_txid_matches_prefix(hash.as_byte_array(), prefix));
    }

    #[test]
    fn finds_existing_prefix_immediately() {
        let tx = test_tx();
        let prefix = displayed_txid_prefix(&tx.compute_txid().to_string());
        let nonce = grind_txid_prefix(&tx, &prefix).expect("existing prefix should be found");
        assert_eq!(nonce, 0);
        verify(&tx, nonce, &prefix);
    }

    #[test]
    fn rejects_no_inputs() {
        let tx = Transaction {
            version: Version(2),
            lock_time: LockTime::ZERO,
            input: vec![],
            output: vec![TxOut {
                value: bitcoin::Amount::from_sat(50_000),
                script_pubkey: ScriptBuf::new(),
            }],
        };
        let prefix = derive_prefix(b"test").expect("namespace id should derive a prefix");
        assert_eq!(
            grind_txid_prefix(&tx, &prefix),
            Err(super::GrindError::NoInputs)
        );
    }

    #[test]
    fn liquid_grinder_finds_existing_prefix_immediately() {
        let tx = test_liquid_tx();
        let prefix = displayed_txid_prefix(&tx.txid().to_string());
        let nonce =
            grind_liquid_txid_prefix(&tx, &prefix).expect("existing prefix should be found");
        assert_eq!(nonce, 0);

        let mut tx = tx;
        tx.lock_time = LiquidLockTime::from_consensus(nonce);
        assert!(tx.txid().to_string().starts_with(&hex::encode(prefix)));
    }
}
