use super::{
    BITCOIN_INSCRIPTION_CHUNK_BYTES, BITCOIN_MAX_UNCONFIRMED_CLUSTER_VBYTES,
    types::{BitcoinInscriptionArtifacts, BitcoinRevealBuildRequest},
};
use crate::da::backends::common::CHOMP_PROTOCOL_ID;
use crate::da::{DaError, RuntimeError, SemanticError, TxidPrefix, grind_txid_prefix};
use ::bitcoin::{
    Address, Amount, Network, OutPoint, Script, ScriptBuf, Sequence, TapSighashType, Transaction,
    TxIn, TxOut, Txid, Witness, XOnlyPublicKey, absolute,
    key::{Keypair, Secp256k1},
    opcodes::{
        self,
        all::{OP_CHECKSIG, OP_ENDIF, OP_IF},
    },
    script::{Builder, PushBytesBuf},
    secp256k1::{All, Message},
    sighash::{Prevouts, SighashCache},
    taproot::{
        LeafVersion, Signature as TaprootSignature, TapLeafHash, TaprootBuilder, TaprootSpendInfo,
    },
    transaction,
};
use anyhow::Context;
use std::time::Instant;
use tracing::info;

fn push_bytes(data: &[u8]) -> Result<PushBytesBuf, DaError> {
    let mut push_bytes = PushBytesBuf::new();
    push_bytes
        .extend_from_slice(data)
        .map_err(|err| SemanticError::ExceedsLimit(err.to_string()))?;
    Ok(push_bytes)
}

pub(super) fn build_bitcoin_inscription_script(
    x_only_pubkey: XOnlyPublicKey,
    payload: &[u8],
) -> Result<ScriptBuf, DaError> {
    let mut builder = Builder::new()
        .push_x_only_key(&x_only_pubkey)
        .push_opcode(OP_CHECKSIG)
        .push_opcode(opcodes::OP_FALSE)
        .push_opcode(OP_IF);

    builder = builder.push_slice(push_bytes(CHOMP_PROTOCOL_ID)?);

    for chunk in payload.chunks(BITCOIN_INSCRIPTION_CHUNK_BYTES) {
        builder = builder.push_slice(push_bytes(chunk)?);
    }

    Ok(builder.push_opcode(OP_ENDIF).into_script())
}

pub(super) fn build_bitcoin_inscription_artifacts(
    secp: &Secp256k1<All>,
    internal_key: XOnlyPublicKey,
    reveal_keypair: &Keypair,
    network: Network,
    payload: &[u8],
) -> Result<BitcoinInscriptionArtifacts, DaError> {
    let reveal_key = reveal_keypair.x_only_public_key().0;
    let tapscript = build_bitcoin_inscription_script(reveal_key, payload)?;
    let spend_info = TaprootBuilder::new()
        .add_leaf(0, tapscript.clone())
        .map_err(|err| {
            RuntimeError::Internal(format!("Failed to add taproot inscription leaf: {err}"))
        })?
        .finalize(secp, internal_key)
        .map_err(|_| {
            RuntimeError::Internal("Failed to finalize taproot inscription tree".to_string())
        })
        .map_err(DaError::from)?;
    let commit_address = Address::p2tr_tweaked(spend_info.output_key(), network);
    let commit_script_pubkey = commit_address.script_pubkey();

    Ok(BitcoinInscriptionArtifacts {
        tapscript,
        spend_info,
        commit_address,
        commit_script_pubkey,
        reveal_keypair: *reveal_keypair,
    })
}

pub(super) fn build_bitcoin_reveal_tx(
    commit_txid: Txid,
    commit_output_vout: u32,
    commit_output: &TxOut,
    destination: &Address,
    fee_sats: u64,
) -> Result<Transaction, DaError> {
    let commit_value_sats = commit_output.value.to_sat();
    let reveal_value_sats = commit_value_sats.checked_sub(fee_sats).ok_or_else(|| {
        SemanticError::PreconditionFailed(format!(
            "Commit output value {} sats is too small to cover the {} sat inscription package fee",
            commit_value_sats, fee_sats
        ))
    })?;

    let dust_limit = destination.script_pubkey().minimal_non_dust().to_sat();
    if reveal_value_sats < dust_limit {
        return Err(SemanticError::PreconditionFailed(format!(
            "Commit output value {} sats leaves only {} sats after fees, below the destination dust limit of {} sats",
            commit_value_sats, reveal_value_sats, dust_limit
        ))
        .into());
    }

    Ok(Transaction {
        version: transaction::Version::TWO,
        lock_time: absolute::LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint::new(commit_txid, commit_output_vout),
            sequence: Sequence::MAX,
            ..Default::default()
        }],
        output: vec![TxOut {
            value: Amount::from_sat(reveal_value_sats),
            script_pubkey: destination.script_pubkey(),
        }],
    })
}

pub(super) fn find_bitcoin_output(
    tx: &Transaction,
    expected_script_pubkey: &Script,
    expected_value_sats: u64,
) -> Result<(u32, TxOut), DaError> {
    let mut matches = tx.output.iter().enumerate().filter(|(_, output)| {
        output.script_pubkey.as_script() == expected_script_pubkey
            && output.value.to_sat() == expected_value_sats
    });

    let (index, output) = matches.next().ok_or_else(|| {
        RuntimeError::Internal(
            "funded Bitcoin commit tx did not contain the expected inscription output".to_string(),
        )
    })?;
    if matches.next().is_some() {
        return Err(RuntimeError::Internal(
            "funded Bitcoin commit tx contained multiple matching inscription outputs".to_string(),
        )
        .into());
    }

    Ok((index as u32, output.clone()))
}

pub(super) fn sign_bitcoin_scriptspend_tx(
    secp: &Secp256k1<All>,
    tx: Transaction,
    prevout: TxOut,
    tapscript: &ScriptBuf,
    spend_info: &TaprootSpendInfo,
    keypair: &Keypair,
) -> Result<Transaction, DaError> {
    let input_index = 0usize;
    let leaf_hash = TapLeafHash::from_script(tapscript.as_script(), LeafVersion::TapScript);
    let control_block = spend_info
        .control_block(&(tapscript.clone(), LeafVersion::TapScript))
        .ok_or_else(|| RuntimeError::Internal("Failed to derive taproot control block".to_string()))
        .map_err(DaError::from)?;

    let mut sighasher = SighashCache::new(tx);
    let sighash = sighasher
        .taproot_script_spend_signature_hash(
            input_index,
            &Prevouts::All(&[prevout]),
            leaf_hash,
            TapSighashType::Default,
        )
        .context("Failed to construct Taproot script-spend sighash")
        .map_err(|err| RuntimeError::Internal(err.to_string()))?;

    let msg = Message::from(sighash);
    let signature = secp.sign_schnorr_no_aux_rand(&msg, keypair);
    let signature = TaprootSignature {
        signature,
        sighash_type: TapSighashType::Default,
    };
    let witness_items = [
        signature.to_vec(),
        tapscript.as_bytes().to_vec(),
        control_block.serialize(),
    ];

    *sighasher
        .witness_mut(input_index)
        .context("Missing witness slot for Bitcoin input 0")
        .map_err(|err| RuntimeError::Internal(err.to_string()))? =
        Witness::from_slice(&witness_items);

    Ok(sighasher.into_transaction())
}

pub(super) fn apply_bitcoin_txid_prefix(
    tx: &mut Transaction,
    target_prefix: &TxidPrefix,
) -> Result<(), DaError> {
    let start = Instant::now();
    let prefix_hex = hex::encode(target_prefix);
    info!(
        prefix = %prefix_hex,
        inputs = tx.input.len(),
        outputs = tx.output.len(),
        "Grinding Bitcoin txid prefix"
    );
    let nonce = grind_txid_prefix(tx, target_prefix).map_err(|err| {
        SemanticError::PreconditionFailed(format!(
            "Failed to grind Bitcoin txid prefix {}: {err}",
            hex::encode(target_prefix)
        ))
    })?;
    tx.lock_time = absolute::LockTime::from_consensus(nonce);
    info!(
        prefix = %prefix_hex,
        nonce,
        elapsed_ms = start.elapsed().as_millis(),
        "Matched Bitcoin txid prefix"
    );
    Ok(())
}

pub(super) fn build_signed_bitcoin_reveal_tx(
    secp: &Secp256k1<All>,
    request: &BitcoinRevealBuildRequest<'_>,
) -> Result<Transaction, DaError> {
    let mut unsigned_reveal_tx = build_bitcoin_reveal_tx(
        request.commit_txid,
        request.commit_output_vout,
        request.commit_output,
        request.destination,
        request.fee_sats,
    )?;
    let provisional_reveal_tx = sign_bitcoin_scriptspend_tx(
        secp,
        unsigned_reveal_tx.clone(),
        request.commit_output.clone(),
        &request.artifacts.tapscript,
        &request.artifacts.spend_info,
        &request.artifacts.reveal_keypair,
    )?;

    if provisional_reveal_tx.weight() > Transaction::MAX_STANDARD_WEIGHT {
        return Err(SemanticError::ExceedsLimit(format!(
            "Bitcoin inscription reveal exceeds the standard transaction weight limit: {} wu > {} wu",
            provisional_reveal_tx.weight().to_wu(),
            Transaction::MAX_STANDARD_WEIGHT.to_wu()
        ))
        .into());
    }

    if let Some(target_prefix) = request.target_prefix {
        apply_bitcoin_txid_prefix(&mut unsigned_reveal_tx, target_prefix)?;
        sign_bitcoin_scriptspend_tx(
            secp,
            unsigned_reveal_tx,
            request.commit_output.clone(),
            &request.artifacts.tapscript,
            &request.artifacts.spend_info,
            &request.artifacts.reveal_keypair,
        )
    } else {
        Ok(provisional_reveal_tx)
    }
}

pub(super) fn validate_bitcoin_inscription_cluster_size(
    commit_tx: &Transaction,
    reveal_tx: &Transaction,
) -> Result<(), DaError> {
    let cluster_vbytes = (commit_tx.vsize() as u64).saturating_add(reveal_tx.vsize() as u64);
    if cluster_vbytes > BITCOIN_MAX_UNCONFIRMED_CLUSTER_VBYTES {
        return Err(SemanticError::ExceedsLimit(format!(
            "Bitcoin inscription commit+reveal cluster exceeds the mempool cluster size limit: {} vB > {} vB",
            cluster_vbytes,
            BITCOIN_MAX_UNCONFIRMED_CLUSTER_VBYTES
        ))
        .into());
    }
    Ok(())
}
