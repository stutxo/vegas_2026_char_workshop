use super::{
    LIQUID_INSCRIPTION_CHUNK_BYTES, LIQUID_TX_VERSION,
    types::{
        BlindedOutputPlan, LiquidInscriptionArtifacts, LiquidRevealBuildRequest, LiquidRuntime,
    },
};
use crate::da::backends::common::CHOMP_PROTOCOL_ID;
use crate::da::{DaError, RuntimeError, SemanticError, TxidPrefix, grind_liquid_txid_prefix};
use ::bitcoin::{Network, hashes::Hash, secp256k1::rand::rngs::OsRng};
use elements::{
    Address as LiquidAddress, AddressParams as LiquidAddressParams, AssetId as LiquidAssetId,
    BlockHash as LiquidBlockHash, LockTime as LiquidLockTime, OutPoint as LiquidOutPoint,
    SchnorrSig as LiquidSchnorrSig, SchnorrSighashType as LiquidSchnorrSighashType,
    Script as LiquidScript, Sequence as LiquidSequence, Transaction as LiquidTransaction,
    TxIn as LiquidTxIn, TxOut as LiquidTxOut, TxOutSecrets as LiquidTxOutSecrets,
    TxOutWitness as LiquidTxOutWitness, Txid as LiquidTxid,
    confidential::{
        Asset as LiquidAsset, AssetBlindingFactor as LiquidAssetBlindingFactor,
        Nonce as LiquidNonce, Value as LiquidValue,
        ValueBlindingFactor as LiquidValueBlindingFactor,
    },
    opcodes::{
        self,
        all::{OP_CHECKSIG, OP_ENDIF, OP_IF},
    },
    schnorr::Keypair as LiquidKeypair,
    script::Builder as LiquidScriptBuilder,
    secp256k1_zkp::{
        All as LiquidSecp256k1All, Message as LiquidMessage, PublicKey as LiquidBlindingPublicKey,
        Secp256k1 as LiquidSecp256k1, SecretKey as LiquidSecretKey,
    },
    sighash::{Prevouts as LiquidPrevouts, SighashCache as LiquidSighashCache},
    taproot::{
        LeafVersion as LiquidLeafVersion, TapLeafHash as LiquidTapLeafHash,
        TaprootBuilder as LiquidTaprootBuilder, TaprootSpendInfo as LiquidTaprootSpendInfo,
    },
};

pub(super) fn liquid_address_params(network: Network) -> &'static LiquidAddressParams {
    match network {
        Network::Bitcoin => &LiquidAddressParams::LIQUID,
        Network::Regtest => &LiquidAddressParams::ELEMENTS,
        Network::Testnet | Network::Testnet4 | Network::Signet => {
            &LiquidAddressParams::LIQUID_TESTNET
        }
    }
}

pub(super) fn build_liquid_inscription_script(
    x_only_pubkey: elements::schnorr::XOnlyPublicKey,
    payload: &[u8],
) -> LiquidScript {
    let mut builder = LiquidScriptBuilder::new()
        .push_slice(&x_only_pubkey.serialize())
        .push_opcode(OP_CHECKSIG)
        .push_opcode(opcodes::OP_FALSE)
        .push_opcode(OP_IF)
        .push_slice(CHOMP_PROTOCOL_ID);

    for chunk in payload.chunks(LIQUID_INSCRIPTION_CHUNK_BYTES) {
        builder = builder.push_slice(chunk);
    }

    builder.push_opcode(OP_ENDIF).into_script()
}

pub(super) fn build_liquid_inscription_artifacts(
    secp: &LiquidSecp256k1<LiquidSecp256k1All>,
    internal_key: elements::schnorr::XOnlyPublicKey,
    blinding_pubkey: Option<LiquidBlindingPublicKey>,
    address_params: &'static LiquidAddressParams,
    reveal_keypair: &LiquidKeypair,
    payload: &[u8],
) -> Result<LiquidInscriptionArtifacts, DaError> {
    let reveal_key = reveal_keypair.x_only_public_key().0;
    let tapscript = build_liquid_inscription_script(reveal_key, payload);
    let spend_info = LiquidTaprootBuilder::new()
        .add_leaf(0, tapscript.clone())
        .map_err(|err| {
            RuntimeError::Internal(format!("Failed to add Liquid inscription leaf: {err}"))
        })?
        .finalize(secp, internal_key)
        .map_err(|err| {
            RuntimeError::Internal(format!(
                "Failed to finalize Liquid taproot inscription tree: {err}"
            ))
        })?;
    let commit_address = LiquidAddress::p2tr(
        secp,
        internal_key,
        spend_info.merkle_root(),
        blinding_pubkey,
        address_params,
    );
    let commit_script_pubkey = commit_address.script_pubkey();

    Ok(LiquidInscriptionArtifacts {
        tapscript,
        spend_info,
        commit_address,
        commit_script_pubkey,
        reveal_keypair: *reveal_keypair,
    })
}

pub(super) fn liquid_explicit_output(
    asset_id: LiquidAssetId,
    value_sats: u64,
    script_pubkey: LiquidScript,
) -> LiquidTxOut {
    LiquidTxOut {
        asset: LiquidAsset::Explicit(asset_id),
        value: LiquidValue::Explicit(value_sats),
        nonce: LiquidNonce::Null,
        script_pubkey,
        witness: LiquidTxOutWitness::default(),
    }
}

pub(super) fn liquid_explicit_txout_secrets(
    asset_id: LiquidAssetId,
    value_sats: u64,
) -> LiquidTxOutSecrets {
    LiquidTxOutSecrets::new(
        asset_id,
        LiquidAssetBlindingFactor::zero(),
        value_sats,
        LiquidValueBlindingFactor::zero(),
    )
}

fn liquid_blinding_pubkey(address: &LiquidAddress) -> Result<LiquidBlindingPublicKey, DaError> {
    address.blinding_pubkey.ok_or_else(|| {
        RuntimeError::Internal("Liquid destination address must be confidential".to_string()).into()
    })
}

fn address_output_plan(
    address: &LiquidAddress,
    value_sats: u64,
) -> Result<BlindedOutputPlan, DaError> {
    Ok(BlindedOutputPlan {
        script_pubkey: address.script_pubkey(),
        blinding_pubkey: liquid_blinding_pubkey(address)?,
        value_sats,
    })
}

fn build_blinded_outputs(
    secp: &LiquidSecp256k1<LiquidSecp256k1All>,
    spent_output_secrets: &[LiquidTxOutSecrets],
    asset_id: LiquidAssetId,
    output_plans: &[BlindedOutputPlan],
) -> Result<Vec<LiquidTxOut>, DaError> {
    let mut rng = OsRng;
    let mut built_outputs = Vec::with_capacity(output_plans.len());
    let mut prior_output_secrets = Vec::with_capacity(output_plans.len().saturating_sub(1));

    for plan in output_plans
        .iter()
        .take(output_plans.len().saturating_sub(1))
    {
        let asset_bf = LiquidAssetBlindingFactor::new(&mut rng);
        let value_bf = LiquidValueBlindingFactor::new(&mut rng);
        let ephemeral_sk = LiquidSecretKey::new(&mut rng);
        let secrets = LiquidTxOutSecrets::new(asset_id, asset_bf, plan.value_sats, value_bf);
        let txout = LiquidTxOut::with_txout_secrets(
            &mut rng,
            secp,
            plan.script_pubkey.clone(),
            plan.blinding_pubkey,
            ephemeral_sk,
            secrets,
            spent_output_secrets,
        )
        .map_err(|err| RuntimeError::Internal(format!("failed to blind Liquid output: {err}")))?;
        prior_output_secrets.push(secrets);
        built_outputs.push(txout);
    }

    let Some(last_plan) = output_plans.last() else {
        return Ok(built_outputs);
    };
    let last_output_secrets = prior_output_secrets.iter().collect::<Vec<_>>();
    let asset_bf = LiquidAssetBlindingFactor::new(&mut rng);
    let ephemeral_sk = LiquidSecretKey::new(&mut rng);
    let (txout, value_bf) = LiquidTxOut::with_secrets_last(
        &mut rng,
        secp,
        last_plan.value_sats,
        last_plan.script_pubkey.clone(),
        last_plan.blinding_pubkey,
        asset_id,
        ephemeral_sk,
        asset_bf,
        spent_output_secrets,
        &last_output_secrets,
    )
    .map_err(|err| RuntimeError::Internal(format!("failed to blind Liquid output: {err}")))?;
    let _last_output_secrets =
        LiquidTxOutSecrets::new(asset_id, asset_bf, last_plan.value_sats, value_bf);
    built_outputs.push(txout);

    Ok(built_outputs)
}

pub(super) fn build_liquid_commit_intent_tx(
    commit_address: &LiquidAddress,
    asset_id: LiquidAssetId,
    commit_value_sats: u64,
) -> LiquidTransaction {
    LiquidTransaction {
        version: LIQUID_TX_VERSION,
        lock_time: LiquidLockTime::ZERO,
        input: vec![],
        output: vec![LiquidTxOut {
            asset: LiquidAsset::Explicit(asset_id),
            value: LiquidValue::Explicit(commit_value_sats),
            nonce: commit_address
                .blinding_pubkey
                .map(LiquidNonce::from)
                .unwrap_or(LiquidNonce::Null),
            script_pubkey: commit_address.script_pubkey(),
            witness: LiquidTxOutWitness::default(),
        }],
    }
}

pub(super) fn build_liquid_reveal_tx(
    secp: &LiquidSecp256k1<LiquidSecp256k1All>,
    commit_txid: LiquidTxid,
    commit_output_vout: u32,
    commit_output_secrets: &LiquidTxOutSecrets,
    destination: &LiquidAddress,
    fee_sats: u64,
) -> Result<LiquidTransaction, DaError> {
    let commit_value_sats = commit_output_secrets.value;
    let reveal_value_sats = commit_value_sats.checked_sub(fee_sats).ok_or_else(|| {
        SemanticError::PreconditionFailed(format!(
            "Commit output value {} sats is too small to cover the {} sat reveal fee",
            commit_value_sats, fee_sats
        ))
    })?;

    if reveal_value_sats == 0 {
        return Err(SemanticError::PreconditionFailed(
            "Reveal transaction would leave a zero-valued destination output".to_string(),
        )
        .into());
    }

    let output = build_blinded_outputs(
        secp,
        std::slice::from_ref(commit_output_secrets),
        commit_output_secrets.asset,
        &[address_output_plan(destination, reveal_value_sats)?],
    )?;

    Ok(LiquidTransaction {
        version: LIQUID_TX_VERSION,
        lock_time: LiquidLockTime::ZERO,
        input: vec![LiquidTxIn {
            previous_output: LiquidOutPoint::new(commit_txid, commit_output_vout),
            sequence: LiquidSequence::MAX,
            ..Default::default()
        }],
        output: vec![
            output
                .first()
                .expect("single reveal output should exist")
                .clone(),
            LiquidTxOut::new_fee(fee_sats, commit_output_secrets.asset),
        ],
    })
}

pub(super) fn find_liquid_output(
    tx: &LiquidTransaction,
    expected_script_pubkey: &LiquidScript,
    expected_asset: LiquidAssetId,
    expected_value_sats: u64,
) -> Result<(u32, LiquidTxOut), DaError> {
    let mut matches = tx.output.iter().enumerate().filter(|(_, output)| {
        output.script_pubkey == *expected_script_pubkey
            && output.asset.explicit() == Some(expected_asset)
            && output.value.explicit() == Some(expected_value_sats)
    });

    let (index, output) = matches.next().ok_or_else(|| {
        RuntimeError::Internal(
            "funded Liquid commit tx did not contain the expected inscription output".to_string(),
        )
    })?;
    if matches.next().is_some() {
        return Err(RuntimeError::Internal(
            "funded Liquid commit tx contained multiple matching inscription outputs".to_string(),
        )
        .into());
    }

    Ok((index as u32, output.clone()))
}

pub(super) fn sign_liquid_scriptspend_tx(
    secp: &LiquidSecp256k1<LiquidSecp256k1All>,
    mut tx: LiquidTransaction,
    prevout: LiquidTxOut,
    tapscript: &LiquidScript,
    spend_info: &LiquidTaprootSpendInfo,
    keypair: &LiquidKeypair,
    genesis_hash: LiquidBlockHash,
) -> Result<LiquidTransaction, DaError> {
    let input_index = 0usize;
    let leaf_version = LiquidLeafVersion::default();
    let leaf_hash = LiquidTapLeafHash::from_script(tapscript, leaf_version);
    let control_block = spend_info
        .control_block(&(tapscript.clone(), leaf_version))
        .ok_or_else(|| {
            RuntimeError::Internal("Failed to derive Liquid taproot control block".to_string())
        })?;

    let mut sighasher = LiquidSighashCache::new(&mut tx);
    let sighash = sighasher
        .taproot_script_spend_signature_hash(
            input_index,
            &LiquidPrevouts::All(&[prevout]),
            leaf_hash,
            LiquidSchnorrSighashType::Default,
            genesis_hash,
        )
        .map_err(|err| {
            RuntimeError::Internal(format!(
                "Failed to construct Liquid Taproot script-spend sighash: {err}"
            ))
        })?;

    let msg = LiquidMessage::from_digest(sighash.to_byte_array());
    let signature = secp.sign_schnorr_no_aux_rand(&msg, keypair);
    let signature = LiquidSchnorrSig {
        sig: signature,
        hash_ty: LiquidSchnorrSighashType::Default,
    };

    let witness = sighasher.witness_mut(input_index).ok_or_else(|| {
        RuntimeError::Internal("Missing witness slot for Liquid input 0".to_string())
    })?;
    witness.push(signature.to_vec());
    witness.push(tapscript.to_bytes());
    witness.push(control_block.serialize());
    Ok(tx)
}

pub(super) fn apply_liquid_txid_prefix(
    tx: &mut LiquidTransaction,
    target_prefix: &TxidPrefix,
) -> Result<(), DaError> {
    let nonce = grind_liquid_txid_prefix(tx, target_prefix).map_err(|err| {
        SemanticError::PreconditionFailed(format!(
            "Failed to grind Liquid txid prefix {}: {err}",
            hex::encode(target_prefix)
        ))
    })?;
    tx.lock_time = LiquidLockTime::from_consensus(nonce);
    Ok(())
}

pub(super) fn build_signed_liquid_reveal_tx(
    runtime: LiquidRuntime<'_>,
    request: &LiquidRevealBuildRequest<'_>,
) -> Result<LiquidTransaction, DaError> {
    let mut unsigned_reveal_tx = build_liquid_reveal_tx(
        runtime.secp,
        request.commit_txid,
        request.commit_output_vout,
        request.commit_output_secrets,
        request.destination,
        request.fee_sats,
    )?;
    if let Some(target_prefix) = request.target_prefix {
        apply_liquid_txid_prefix(&mut unsigned_reveal_tx, target_prefix)?;
    }
    sign_liquid_scriptspend_tx(
        runtime.secp,
        unsigned_reveal_tx,
        request.commit_output.clone(),
        &request.artifacts.tapscript,
        &request.artifacts.spend_info,
        &request.artifacts.reveal_keypair,
        runtime.chain_params.genesis_hash,
    )
}
