use crate::da::TxidPrefix;
use crate::da::backends::common::ChunkPayload;
use elements::{
    Address as LiquidAddress, AssetId as LiquidAssetId, BlockHash as LiquidBlockHash,
    Script as LiquidScript, Transaction as LiquidTransaction, TxOut as LiquidTxOut,
    TxOutSecrets as LiquidTxOutSecrets, Txid as LiquidTxid,
    schnorr::Keypair as LiquidKeypair,
    secp256k1_zkp::{
        All as LiquidSecp256k1All, PublicKey as LiquidBlindingPublicKey,
        Secp256k1 as LiquidSecp256k1,
    },
    taproot::TaprootSpendInfo as LiquidTaprootSpendInfo,
};
use serde::Deserialize;

#[derive(Debug, Clone, Copy)]
pub(super) struct LiquidChainParams {
    pub(super) genesis_hash: LiquidBlockHash,
    pub(super) pegged_asset: LiquidAssetId,
}

#[derive(Clone)]
pub(super) struct LiquidInscriptionArtifacts {
    pub(super) tapscript: LiquidScript,
    pub(super) spend_info: LiquidTaprootSpendInfo,
    pub(super) commit_address: LiquidAddress,
    pub(super) commit_script_pubkey: LiquidScript,
    pub(super) reveal_keypair: LiquidKeypair,
}

#[derive(Debug, Clone)]
pub(super) struct BlindedOutputPlan {
    pub(super) script_pubkey: LiquidScript,
    pub(super) blinding_pubkey: LiquidBlindingPublicKey,
    pub(super) value_sats: u64,
}

pub(super) struct LiquidInscriptionCandidate {
    pub(super) commit_tx: LiquidTransaction,
    pub(super) commit_output_vout: u32,
    pub(super) reveal_tx: LiquidTransaction,
    pub(super) commit_output_secrets: LiquidTxOutSecrets,
    pub(super) artifacts: LiquidInscriptionArtifacts,
    pub(super) reveal_destination: LiquidAddress,
}

pub(super) struct LiquidInscriptionCandidateRequest {
    pub(super) artifacts: LiquidInscriptionArtifacts,
    pub(super) reveal_destination: LiquidAddress,
    pub(super) reserved_reveal_fee_sats: u64,
    pub(super) commit_sat_per_vb: f64,
    pub(super) reveal_sat_per_vb: f64,
    pub(super) grind_reveal_prefix: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum LiquidTransactionBlob {
    Raw(Vec<u8>),
    Chunk(ChunkPayload),
}

#[derive(Debug, Deserialize)]
pub(super) struct SidechainInfo {
    pub(super) pegged_asset: String,
}

#[derive(Debug, Deserialize, Clone)]
pub(super) struct TestMempoolAcceptResult {
    pub(super) allowed: bool,
    #[serde(rename = "reject-reason")]
    pub(super) reject_reason: Option<String>,
    #[serde(rename = "reject-details")]
    pub(super) reject_details: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct MempoolInfo {
    pub(super) mempoolminfee: f64,
    pub(super) minrelaytxfee: f64,
}

#[derive(Debug, Deserialize)]
pub(super) struct WalletAddressInfoResult {
    pub(super) ismine: bool,
    pub(super) pubkey: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct FundRawTransactionResult {
    pub(super) hex: String,
    pub(super) fee: f64,
}

#[derive(Debug, Deserialize)]
pub(super) struct FinalizePsbtResult {
    pub(super) hex: Option<String>,
    pub(super) complete: bool,
}

#[derive(Debug, Deserialize)]
pub(super) struct WalletPsbtResult {
    pub(super) psbt: String,
    pub(super) complete: Option<bool>,
}

#[derive(Clone, Copy)]
pub(super) struct LiquidRuntime<'a> {
    pub(super) secp: &'a LiquidSecp256k1<LiquidSecp256k1All>,
    pub(super) target_prefix: &'a TxidPrefix,
    pub(super) chain_params: &'a LiquidChainParams,
}

pub(super) struct LiquidRevealBuildRequest<'a> {
    pub(super) commit_txid: LiquidTxid,
    pub(super) commit_output_vout: u32,
    pub(super) commit_output: &'a LiquidTxOut,
    pub(super) commit_output_secrets: &'a LiquidTxOutSecrets,
    pub(super) destination: &'a LiquidAddress,
    pub(super) artifacts: &'a LiquidInscriptionArtifacts,
    pub(super) fee_sats: u64,
    pub(super) target_prefix: Option<&'a TxidPrefix>,
}
