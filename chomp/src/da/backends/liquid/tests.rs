use super::{
    LIQUID_MAX_STANDARD_WEIGHT, LIQUID_TX_VERSION, LiquidDa, LiquidDaConfig,
    read::{extract_blob_from_inscription, extract_blob_from_transaction},
    tx::{
        build_liquid_inscription_artifacts, build_signed_liquid_reveal_tx, find_liquid_output,
        liquid_explicit_output, liquid_explicit_txout_secrets,
    },
    types::{LiquidChainParams, LiquidRevealBuildRequest, LiquidRuntime, LiquidTransactionBlob},
};
use crate::da::backends::common::collect_instructions;
use crate::da::{FeePolicy, LiquidBlobLocator, TxidPrefix};
use ::bitcoin::hashes::Hash;
use elements::{
    Address as LiquidAddress, AddressParams as LiquidAddressParams, AssetId as LiquidAssetId,
    BlockHash as LiquidBlockHash, LockTime as LiquidLockTime, SchnorrSig as LiquidSchnorrSig,
    SchnorrSighashType as LiquidSchnorrSighashType, Script as LiquidScript,
    Transaction as LiquidTransaction, TxOut as LiquidTxOut, Txid as LiquidTxid,
    schnorr::Keypair as LiquidKeypair,
    script::Instruction as LiquidInstruction,
    secp256k1_zkp::{
        All as LiquidSecp256k1All, Message as LiquidMessage, Secp256k1 as LiquidSecp256k1,
        SecretKey as LiquidSecretKey,
    },
    sighash::{Prevouts as LiquidPrevouts, SighashCache as LiquidSighashCache},
    taproot::{LeafVersion as LiquidLeafVersion, TapLeafHash as LiquidTapLeafHash},
};
use std::str::FromStr;

fn regtest_secret_key(hex: &str) -> LiquidSecretKey {
    LiquidSecretKey::from_str(hex).expect("valid secret key")
}

fn regtest_keypair(
    hex: &str,
) -> (
    LiquidSecp256k1<LiquidSecp256k1All>,
    LiquidSecretKey,
    LiquidKeypair,
) {
    let secp = LiquidSecp256k1::new();
    let secret_key = regtest_secret_key(hex);
    let keypair = LiquidKeypair::from_secret_key(&secp, &secret_key);
    (secp, secret_key, keypair)
}

fn regtest_blinding_key() -> LiquidSecretKey {
    LiquidSecretKey::from_str("0101010101010101010101010101010101010101010101010101010101010101")
        .expect("valid blinding secret key")
}

fn regtest_address() -> LiquidAddress {
    let (secp, _, keypair) =
        regtest_keypair("0f0e0d0c0b0a090807060504030201100f0e0d0c0b0a09080706050403020110");
    let blinding_keypair = LiquidKeypair::from_secret_key(&secp, &regtest_blinding_key());
    let (internal_key, _) = keypair.x_only_public_key();
    LiquidAddress::p2tr(
        &secp,
        internal_key,
        None,
        Some(blinding_keypair.public_key()),
        &LiquidAddressParams::ELEMENTS,
    )
}

fn test_asset_id() -> LiquidAssetId {
    LiquidAssetId::from_slice(&[1u8; 32]).expect("asset id should decode")
}

fn test_genesis_hash() -> LiquidBlockHash {
    LiquidBlockHash::all_zeros()
}

fn test_chain_params() -> LiquidChainParams {
    LiquidChainParams {
        genesis_hash: test_genesis_hash(),
        pegged_asset: test_asset_id(),
    }
}

fn test_runtime<'a>(
    secp: &'a LiquidSecp256k1<LiquidSecp256k1All>,
    chain_params: &'a LiquidChainParams,
    target_prefix: &'a TxidPrefix,
) -> LiquidRuntime<'a> {
    LiquidRuntime {
        secp,
        target_prefix,
        chain_params,
    }
}

fn displayed_txid_prefix(txid: LiquidTxid) -> [u8; 3] {
    let txid = txid.to_string();
    let prefix = &txid[..6];
    let bytes = hex::decode(prefix).expect("txid prefix should decode");
    bytes
        .try_into()
        .expect("decoded txid prefix should be 3 bytes")
}

fn verify_reveal_signature(
    secp: &LiquidSecp256k1<LiquidSecp256k1All>,
    keypair: &LiquidKeypair,
    tx: &LiquidTransaction,
    prevout: &LiquidTxOut,
    genesis_hash: LiquidBlockHash,
) {
    let signature = LiquidSchnorrSig::from_slice(
        tx.input[0]
            .witness
            .script_witness
            .first()
            .expect("reveal witness should contain a signature"),
    )
    .expect("reveal signature should decode");
    let tapscript = LiquidScript::from(
        tx.input[0].witness.script_witness[tx.input[0].witness.script_witness.len() - 2].clone(),
    );
    let leaf_hash = LiquidTapLeafHash::from_script(&tapscript, LiquidLeafVersion::default());
    let sighash = LiquidSighashCache::new(tx)
        .taproot_script_spend_signature_hash(
            0,
            &LiquidPrevouts::All(std::slice::from_ref(prevout)),
            leaf_hash,
            LiquidSchnorrSighashType::Default,
            genesis_hash,
        )
        .expect("reveal sighash should build");
    let msg = LiquidMessage::from_digest(sighash.to_byte_array());
    let x_only_pubkey = keypair.x_only_public_key().0;

    secp.verify_schnorr(&signature.sig, &msg, &x_only_pubkey)
        .expect("reveal signature should verify");
}

#[test]
fn liquid_default_fee_policy_uses_next_block_target() {
    assert_eq!(
        LiquidDaConfig::default_fee_policy(),
        FeePolicy::next_block()
    );
}

#[test]
fn liquid_provider_locator_round_trips_through_txid() {
    let txid =
        LiquidTxid::from_str("00000000000000000000000000000000000000000000000000000000000000aa")
            .expect("valid txid");

    let locator = LiquidDa::provider_locator_from_txid(txid).expect("locator should build");
    let decoded = LiquidDa::txid_from_locator(&locator).expect("locator should decode");

    assert_eq!(decoded, txid);
}

#[test]
fn liquid_chunked_locator_carries_chunk_txids() {
    let chunk_txids = vec![
        LiquidTxid::from_str("00000000000000000000000000000000000000000000000000000000000000aa")
            .expect("valid txid"),
        LiquidTxid::from_str("00000000000000000000000000000000000000000000000000000000000000bb")
            .expect("valid txid"),
    ];
    let locator = LiquidDa::provider_locator_from_chunks(chunk_txids.clone())
        .expect("chunked locator should build");

    assert!(LiquidDa::txid_from_locator(&locator).is_err());
    let provider = serde_json::from_slice::<LiquidBlobLocator>(locator.key_bytes())
        .expect("liquid locator should decode");
    let chunked = provider
        .as_chunked()
        .expect("chunked liquid locator expected");
    assert_eq!(
        chunked.chunks(),
        &chunk_txids
            .iter()
            .map(|txid| txid.to_byte_array())
            .collect::<Vec<_>>()
    );
}

#[test]
fn find_liquid_output_returns_matching_vout() {
    let asset_id = test_asset_id();
    let target_script = LiquidScript::from(vec![0x51, 0x20, 0xaa, 0xbb, 0xcc]);
    let other_script = LiquidScript::from(vec![0x51, 0x20, 0x11, 0x22, 0x33]);
    let tx = LiquidTransaction {
        version: LIQUID_TX_VERSION,
        lock_time: LiquidLockTime::ZERO,
        input: vec![],
        output: vec![
            liquid_explicit_output(asset_id, 10_000, other_script),
            liquid_explicit_output(asset_id, 20_000, target_script.clone()),
        ],
    };

    let (vout, output) =
        find_liquid_output(&tx, &target_script, asset_id, 20_000).expect("output should match");

    assert_eq!(vout, 1);
    assert_eq!(output.value.explicit(), Some(20_000));
    assert_eq!(output.asset.explicit(), Some(asset_id));
    assert_eq!(output.script_pubkey, target_script);
}

#[test]
fn liquid_inscription_artifacts_use_distinct_internal_and_reveal_keys() {
    let (secp, _, internal_keypair) =
        regtest_keypair("1111111111111111111111111111111111111111111111111111111111111111");
    let (_, _, reveal_keypair) =
        regtest_keypair("2222222222222222222222222222222222222222222222222222222222222222");
    let blinding_keypair = LiquidKeypair::from_secret_key(&secp, &regtest_blinding_key());
    let artifacts = build_liquid_inscription_artifacts(
        &secp,
        internal_keypair.x_only_public_key().0,
        Some(blinding_keypair.public_key()),
        &LiquidAddressParams::ELEMENTS,
        &reveal_keypair,
        b"decision-roll",
    )
    .expect("artifacts should build");

    let instructions = collect_instructions(
        artifacts.tapscript.instructions(),
        "failed to parse Liquid inscription tapscript",
    )
    .expect("tapscript should parse");
    let reveal_key_push = match instructions.first() {
        Some(LiquidInstruction::PushBytes(bytes)) => bytes.to_vec(),
        other => panic!("unexpected first tapscript instruction: {other:?}"),
    };
    assert_eq!(
        reveal_key_push,
        reveal_keypair.x_only_public_key().0.serialize()
    );
    assert_eq!(
        artifacts.commit_script_pubkey,
        artifacts.commit_address.script_pubkey()
    );
}

#[test]
fn extract_blob_from_inscription_returns_payload() {
    let (secp, _, internal_keypair) =
        regtest_keypair("3333333333333333333333333333333333333333333333333333333333333333");
    let (_, _, reveal_keypair) =
        regtest_keypair("4444444444444444444444444444444444444444444444444444444444444444");
    let destination = regtest_address();
    let chain_params = test_chain_params();
    let target_prefix = [0u8; 3];
    let runtime = test_runtime(&secp, &chain_params, &target_prefix);
    let payload = vec![0xab; 601];
    let artifacts = build_liquid_inscription_artifacts(
        &secp,
        internal_keypair.x_only_public_key().0,
        None,
        &LiquidAddressParams::ELEMENTS,
        &reveal_keypair,
        &payload,
    )
    .expect("artifacts should build");
    let commit_output = liquid_explicit_output(
        chain_params.pegged_asset,
        20_000,
        artifacts.commit_script_pubkey.clone(),
    );
    let commit_output_secrets = liquid_explicit_txout_secrets(chain_params.pegged_asset, 20_000);
    let reveal_tx = build_signed_liquid_reveal_tx(
        runtime,
        &LiquidRevealBuildRequest {
            commit_txid: LiquidTxid::from_byte_array([9u8; 32]),
            commit_output_vout: 3,
            commit_output: &commit_output,
            commit_output_secrets: &commit_output_secrets,
            destination: &destination,
            artifacts: &artifacts,
            fee_sats: 500,
            target_prefix: None,
        },
    )
    .expect("reveal tx should build");

    let blob = extract_blob_from_inscription(&reveal_tx).expect("blob should decode");
    assert_eq!(blob, LiquidTransactionBlob::Raw(payload.clone().into()));
    assert_eq!(
        extract_blob_from_transaction(&reveal_tx).expect("transaction should decode"),
        LiquidTransactionBlob::Raw(payload.clone().into())
    );
    assert_eq!(reveal_tx.input[0].previous_output.vout, 3);
    verify_reveal_signature(
        &secp,
        &reveal_keypair,
        &reveal_tx,
        &commit_output,
        chain_params.genesis_hash,
    );
}

#[test]
fn liquid_inscription_reveal_txid_matches_namespace_prefix() {
    let (secp, _, internal_keypair) =
        regtest_keypair("7777777777777777777777777777777777777777777777777777777777777777");
    let (_, _, reveal_keypair) =
        regtest_keypair("8888888888888888888888888888888888888888888888888888888888888888");
    let destination = regtest_address();
    let chain_params = test_chain_params();
    let zero_prefix = [0u8; 3];
    let runtime = test_runtime(&secp, &chain_params, &zero_prefix);
    let artifacts = build_liquid_inscription_artifacts(
        &secp,
        internal_keypair.x_only_public_key().0,
        None,
        &LiquidAddressParams::ELEMENTS,
        &reveal_keypair,
        b"decision-roll",
    )
    .expect("artifacts should build");
    let commit_output = liquid_explicit_output(
        chain_params.pegged_asset,
        20_000,
        artifacts.commit_script_pubkey.clone(),
    );
    let commit_output_secrets = liquid_explicit_txout_secrets(chain_params.pegged_asset, 20_000);
    let provisional = build_signed_liquid_reveal_tx(
        runtime,
        &LiquidRevealBuildRequest {
            commit_txid: LiquidTxid::from_byte_array([9u8; 32]),
            commit_output_vout: 3,
            commit_output: &commit_output,
            commit_output_secrets: &commit_output_secrets,
            destination: &destination,
            artifacts: &artifacts,
            fee_sats: 500,
            target_prefix: None,
        },
    )
    .expect("reveal tx should build");
    let target_prefix = displayed_txid_prefix(provisional.txid());
    let runtime = test_runtime(&secp, &chain_params, &target_prefix);
    let reveal_tx = build_signed_liquid_reveal_tx(
        runtime,
        &LiquidRevealBuildRequest {
            commit_txid: LiquidTxid::from_byte_array([9u8; 32]),
            commit_output_vout: 3,
            commit_output: &commit_output,
            commit_output_secrets: &commit_output_secrets,
            destination: &destination,
            artifacts: &artifacts,
            fee_sats: 500,
            target_prefix: Some(&target_prefix),
        },
    )
    .expect("reveal tx should build");

    assert!(
        reveal_tx
            .txid()
            .to_string()
            .starts_with(&hex::encode(target_prefix))
    );
}

#[test]
fn liquid_huge_single_inscription_exceeds_standard_weight() {
    let (secp, _, internal_keypair) =
        regtest_keypair("5555555555555555555555555555555555555555555555555555555555555555");
    let (_, _, reveal_keypair) =
        regtest_keypair("6666666666666666666666666666666666666666666666666666666666666666");
    let destination = regtest_address();
    let chain_params = test_chain_params();
    let target_prefix = [0u8; 3];
    let runtime = test_runtime(&secp, &chain_params, &target_prefix);
    let payload = vec![0xef; 2_048_009];
    let artifacts = build_liquid_inscription_artifacts(
        &secp,
        internal_keypair.x_only_public_key().0,
        None,
        &LiquidAddressParams::ELEMENTS,
        &reveal_keypair,
        &payload,
    )
    .expect("artifacts should build");
    let commit_output = liquid_explicit_output(
        chain_params.pegged_asset,
        20_000,
        artifacts.commit_script_pubkey.clone(),
    );
    let commit_output_secrets = liquid_explicit_txout_secrets(chain_params.pegged_asset, 20_000);
    let reveal_tx = build_signed_liquid_reveal_tx(
        runtime,
        &LiquidRevealBuildRequest {
            commit_txid: LiquidTxid::from_byte_array([0u8; 32]),
            commit_output_vout: 0,
            commit_output: &commit_output,
            commit_output_secrets: &commit_output_secrets,
            destination: &destination,
            artifacts: &artifacts,
            fee_sats: 1,
            target_prefix: None,
        },
    )
    .expect("reveal tx should build");

    assert!(
        reveal_tx.weight() > LIQUID_MAX_STANDARD_WEIGHT,
        "unexpected reveal weight: {} <= {}",
        reveal_tx.weight(),
        LIQUID_MAX_STANDARD_WEIGHT
    );
}
