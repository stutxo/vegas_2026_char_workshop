use super::{
    BitcoinDa, BitcoinDaConfig,
    fees::validate_single_fee_policy,
    read::{extract_blob_from_inscription, extract_blob_from_transaction},
    tx::{
        build_bitcoin_inscription_artifacts, build_signed_bitcoin_reveal_tx, find_bitcoin_output,
    },
    types::{BitcoinRevealBuildRequest, BitcoinTransactionBlob},
};
use crate::da::backends::common::{collect_instructions, fee_policy_rates};
use crate::da::{BitcoinBlobLocator, FeePolicy};
use ::bitcoin::{
    Address, Amount, Network, ScriptBuf, TapSighashType, Transaction, TxOut, Txid, absolute,
    hashes::Hash,
    key::{Keypair, Secp256k1},
    script::Instruction,
    secp256k1::{All, Message, SecretKey},
    sighash::{Prevouts, SighashCache},
    taproot::{LeafVersion, Signature as TaprootSignature, TapLeafHash, TaprootSpendInfo},
    transaction,
};
use std::str::FromStr;

fn regtest_keypair(hex: &str) -> (Secp256k1<All>, SecretKey, Keypair) {
    let secp = Secp256k1::new();
    let secret_key = SecretKey::from_str(hex).expect("valid secret key");
    let keypair = Keypair::from_secret_key(&secp, &secret_key);
    (secp, secret_key, keypair)
}

fn regtest_address() -> Address {
    let (secp, _, keypair) =
        regtest_keypair("0f0e0d0c0b0a090807060504030201100f0e0d0c0b0a09080706050403020110");
    let (internal_key, _) = keypair.x_only_public_key();
    let spend_info = TaprootSpendInfo::new_key_spend(&secp, internal_key, None);
    Address::p2tr_tweaked(spend_info.output_key(), Network::Regtest)
}

fn displayed_txid_prefix(txid: Txid) -> [u8; 3] {
    let txid = txid.to_string();
    let prefix = &txid[..6];
    let bytes = hex::decode(prefix).expect("txid prefix should decode");
    bytes
        .try_into()
        .expect("decoded txid prefix should be 3 bytes")
}

fn verify_reveal_signature(
    secp: &Secp256k1<All>,
    keypair: &Keypair,
    tx: &Transaction,
    prevout: &TxOut,
) {
    let signature = TaprootSignature::from_slice(
        tx.input[0]
            .witness
            .nth(0)
            .expect("reveal witness should contain a signature"),
    )
    .expect("reveal signature should decode");
    let tapscript = ScriptBuf::from_bytes(
        tx.input[0]
            .witness
            .second_to_last()
            .expect("reveal witness should contain tapscript")
            .to_vec(),
    );
    let leaf_hash = TapLeafHash::from_script(tapscript.as_script(), LeafVersion::TapScript);
    let sighash = SighashCache::new(tx)
        .taproot_script_spend_signature_hash(
            0,
            &Prevouts::All(std::slice::from_ref(prevout)),
            leaf_hash,
            TapSighashType::Default,
        )
        .expect("reveal sighash should build");
    let msg = Message::from(sighash);
    let x_only_pubkey = keypair.x_only_public_key().0;

    secp.verify_schnorr(&signature.signature, &msg, &x_only_pubkey)
        .expect("reveal signature should verify");
}

#[test]
fn bitcoin_default_fee_policy_uses_next_block_target() {
    let policy = BitcoinDaConfig::default_fee_policy();
    validate_single_fee_policy(&policy).expect("default bitcoin fee policy should validate");
    assert_eq!(policy, FeePolicy::next_block());
    let rates = fee_policy_rates(&policy, 1.5, 1.0).expect("target fee policy should build rates");
    assert_eq!(
        rates,
        vec![1.5, 1.875, 2.34375, 2.9296875, 3.662109375, 4.57763671875]
    );
}

#[test]
fn bitcoin_provider_locator_round_trips_through_txid() {
    let txid = Txid::from_str("00000000000000000000000000000000000000000000000000000000000000aa")
        .expect("valid txid");

    let locator = BitcoinDa::provider_locator_from_txid(txid).expect("locator should build");
    let decoded = BitcoinDa::txid_from_locator(&locator).expect("locator should decode");

    assert_eq!(decoded, txid);
}

#[test]
fn bitcoin_chunked_locator_carries_chunk_txids() {
    let chunk_txids = vec![
        Txid::from_str("00000000000000000000000000000000000000000000000000000000000000aa")
            .expect("valid txid"),
        Txid::from_str("00000000000000000000000000000000000000000000000000000000000000bb")
            .expect("valid txid"),
    ];
    let locator = BitcoinDa::provider_locator_from_chunks(chunk_txids.clone())
        .expect("chunked locator should build");

    assert!(BitcoinDa::txid_from_locator(&locator).is_err());
    let provider = serde_json::from_slice::<BitcoinBlobLocator>(locator.key_bytes())
        .expect("bitcoin locator should decode");
    let chunked = provider
        .as_chunked()
        .expect("chunked bitcoin locator expected");
    assert_eq!(
        chunked.chunks(),
        &chunk_txids
            .iter()
            .map(|txid| txid.to_byte_array())
            .collect::<Vec<_>>()
    );
}

#[test]
fn find_bitcoin_output_returns_matching_vout() {
    let target_script = ScriptBuf::from_bytes(vec![0x51, 0x20, 0xaa, 0xbb, 0xcc]);
    let other_script = ScriptBuf::from_bytes(vec![0x51, 0x20, 0x11, 0x22, 0x33]);
    let tx = Transaction {
        version: transaction::Version::TWO,
        lock_time: absolute::LockTime::ZERO,
        input: vec![],
        output: vec![
            TxOut {
                value: Amount::from_sat(10_000),
                script_pubkey: other_script,
            },
            TxOut {
                value: Amount::from_sat(20_000),
                script_pubkey: target_script.clone(),
            },
        ],
    };

    let (vout, output) =
        find_bitcoin_output(&tx, target_script.as_script(), 20_000).expect("output should match");

    assert_eq!(vout, 1);
    assert_eq!(output.value.to_sat(), 20_000);
    assert_eq!(output.script_pubkey, target_script);
}

#[test]
fn bitcoin_inscription_artifacts_use_distinct_internal_and_reveal_keys() {
    let (secp, _, internal_keypair) =
        regtest_keypair("1111111111111111111111111111111111111111111111111111111111111111");
    let (_, _, reveal_keypair) =
        regtest_keypair("2222222222222222222222222222222222222222222222222222222222222222");
    let artifacts = build_bitcoin_inscription_artifacts(
        &secp,
        internal_keypair.x_only_public_key().0,
        &reveal_keypair,
        Network::Regtest,
        b"decision-roll",
    )
    .expect("artifacts should build");

    let instructions = collect_instructions(
        artifacts.tapscript.instructions(),
        "failed to parse inscription tapscript",
    )
    .expect("tapscript should parse");
    let reveal_key_push = match instructions.first() {
        Some(Instruction::PushBytes(bytes)) => bytes.as_bytes().to_vec(),
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
    let payload = vec![0xab; 601];
    let artifacts = build_bitcoin_inscription_artifacts(
        &secp,
        internal_keypair.x_only_public_key().0,
        &reveal_keypair,
        Network::Regtest,
        &payload,
    )
    .expect("artifacts should build");
    let commit_output = TxOut {
        value: Amount::from_sat(20_000),
        script_pubkey: artifacts.commit_script_pubkey.clone(),
    };
    let reveal_tx = build_signed_bitcoin_reveal_tx(
        &secp,
        &BitcoinRevealBuildRequest {
            commit_txid: Txid::from_byte_array([9u8; 32]),
            commit_output_vout: 3,
            commit_output: &commit_output,
            destination: &destination,
            artifacts: &artifacts,
            fee_sats: 500,
            target_prefix: None,
        },
    )
    .expect("reveal tx should build");

    let blob = extract_blob_from_inscription(&reveal_tx).expect("blob should decode");
    assert_eq!(blob, BitcoinTransactionBlob::Raw(payload.clone().into()));
    assert_eq!(
        extract_blob_from_transaction(&reveal_tx).expect("transaction should decode"),
        BitcoinTransactionBlob::Raw(payload.clone().into())
    );
    assert_eq!(reveal_tx.input[0].previous_output.vout, 3);
    verify_reveal_signature(&secp, &reveal_keypair, &reveal_tx, &commit_output);
}

#[test]
fn bitcoin_inscription_reveal_txid_matches_namespace_prefix() {
    let (secp, _, internal_keypair) =
        regtest_keypair("5555555555555555555555555555555555555555555555555555555555555555");
    let (_, _, reveal_keypair) =
        regtest_keypair("6666666666666666666666666666666666666666666666666666666666666666");
    let destination = regtest_address();
    let artifacts = build_bitcoin_inscription_artifacts(
        &secp,
        internal_keypair.x_only_public_key().0,
        &reveal_keypair,
        Network::Regtest,
        b"decision-roll",
    )
    .expect("artifacts should build");
    let commit_output = TxOut {
        value: Amount::from_sat(20_000),
        script_pubkey: artifacts.commit_script_pubkey.clone(),
    };
    let provisional = build_signed_bitcoin_reveal_tx(
        &secp,
        &BitcoinRevealBuildRequest {
            commit_txid: Txid::from_byte_array([9u8; 32]),
            commit_output_vout: 3,
            commit_output: &commit_output,
            destination: &destination,
            artifacts: &artifacts,
            fee_sats: 500,
            target_prefix: None,
        },
    )
    .expect("reveal tx should build");
    let target_prefix = displayed_txid_prefix(provisional.compute_txid());
    let reveal_tx = build_signed_bitcoin_reveal_tx(
        &secp,
        &BitcoinRevealBuildRequest {
            commit_txid: Txid::from_byte_array([9u8; 32]),
            commit_output_vout: 3,
            commit_output: &commit_output,
            destination: &destination,
            artifacts: &artifacts,
            fee_sats: 500,
            target_prefix: Some(&target_prefix),
        },
    )
    .expect("reveal tx should build");

    assert!(
        reveal_tx
            .compute_txid()
            .to_string()
            .starts_with(&hex::encode(target_prefix))
    );
}
