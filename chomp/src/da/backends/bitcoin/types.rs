use crate::da::TxidPrefix;
use crate::da::backends::common::ChunkPayload;
use ::bitcoin::{
    Address, Network, ScriptBuf, Transaction, TxOut, Txid, XOnlyPublicKey,
    key::{Keypair, Secp256k1},
    secp256k1::All,
    taproot::TaprootSpendInfo,
};

#[derive(Debug, serde::Deserialize)]
pub(super) struct EstimateSmartFeeResult {
    pub(super) feerate: Option<f64>,
}

#[derive(Debug, serde::Deserialize)]
pub(super) struct WalletCreateFundedPsbtResult {
    pub(super) psbt: String,
    pub(super) fee: f64,
}

#[derive(Debug, serde::Deserialize)]
pub(super) struct FinalizePsbtResult {
    pub(super) hex: Option<String>,
    pub(super) complete: bool,
}

#[derive(Debug, Clone)]
pub(super) struct BitcoinWalletKeyInfo {
    pub(super) internal_key: XOnlyPublicKey,
}

#[derive(Clone)]
pub(super) struct BitcoinInscriptionArtifacts {
    pub(super) tapscript: ScriptBuf,
    pub(super) spend_info: TaprootSpendInfo,
    pub(super) commit_address: Address,
    pub(super) commit_script_pubkey: ScriptBuf,
    pub(super) reveal_keypair: Keypair,
}

pub(super) struct BitcoinInscriptionCandidate {
    pub(super) commit_tx: Transaction,
    pub(super) commit_output_vout: u32,
    pub(super) reveal_tx: Transaction,
    pub(super) artifacts: BitcoinInscriptionArtifacts,
    pub(super) reveal_destination: Address,
}

pub(super) struct BitcoinInscriptionCandidateRequest {
    pub(super) artifacts: BitcoinInscriptionArtifacts,
    pub(super) reveal_destination: Address,
    pub(super) reserved_reveal_fee_sats: u64,
    pub(super) commit_sat_per_vb: f64,
    pub(super) reveal_sat_per_vb: f64,
    pub(super) grind_reveal_prefix: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum BitcoinTransactionBlob {
    Raw(Vec<u8>),
    Chunk(ChunkPayload),
}

#[derive(Clone, Copy)]
pub(super) struct BitcoinRuntime<'a> {
    pub(super) secp: &'a Secp256k1<All>,
    pub(super) target_prefix: &'a TxidPrefix,
    pub(super) network: Network,
}

pub(super) struct BitcoinRevealBuildRequest<'a> {
    pub(super) commit_txid: Txid,
    pub(super) commit_output_vout: u32,
    pub(super) commit_output: &'a TxOut,
    pub(super) destination: &'a Address,
    pub(super) artifacts: &'a BitcoinInscriptionArtifacts,
    pub(super) fee_sats: u64,
    pub(super) target_prefix: Option<&'a TxidPrefix>,
}
