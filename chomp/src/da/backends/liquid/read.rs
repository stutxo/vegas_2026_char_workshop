use super::{LiquidDa, types::LiquidTransactionBlob};
use crate::da::backends::common::{
    ChunkPayload, collect_instructions, extract_blob_from_inscription_instructions,
};
use crate::da::{ChunkedBlobLocator, DaError, SemanticError};
use ::bitcoin::hashes::Hash;
use elements::{
    Script as LiquidScript, Transaction as LiquidTransaction, Txid as LiquidTxid,
    opcodes::all::{OP_ENDIF, OP_IF},
    script::Instruction as LiquidInstruction,
};

fn is_empty_push(instruction: &LiquidInstruction<'_>) -> bool {
    matches!(instruction, LiquidInstruction::PushBytes(bytes) if bytes.is_empty())
}

fn pushed_bytes(instruction: &LiquidInstruction<'_>) -> Option<Vec<u8>> {
    match instruction {
        LiquidInstruction::PushBytes(bytes) => Some(bytes.to_vec()),
        LiquidInstruction::Op(_) => None,
    }
}

pub(super) fn extract_blob_from_inscription_tapscript(
    tapscript: &LiquidScript,
) -> Result<Option<LiquidTransactionBlob>, DaError> {
    let instructions = collect_instructions(
        tapscript.instructions(),
        "failed to parse Liquid inscription tapscript",
    )?;

    let Some(payload) = extract_blob_from_inscription_instructions(
        &instructions,
        is_empty_push,
        |instruction| matches!(instruction, LiquidInstruction::Op(op) if *op == OP_IF),
        |instruction| matches!(instruction, LiquidInstruction::Op(op) if *op == OP_ENDIF),
        pushed_bytes,
    ) else {
        return Ok(None);
    };

    if let Ok(chunk) = ChunkPayload::decode(payload.as_slice()) {
        return Ok(Some(LiquidTransactionBlob::Chunk(chunk)));
    }

    Ok(Some(LiquidTransactionBlob::Raw(payload)))
}

pub(super) fn extract_blob_from_inscription(
    tx: &LiquidTransaction,
) -> Result<LiquidTransactionBlob, DaError> {
    for input in &tx.input {
        if input.witness.script_witness.len() < 2 {
            continue;
        }
        let tapscript_bytes = &input.witness.script_witness[input.witness.script_witness.len() - 2];
        let tapscript = LiquidScript::from(tapscript_bytes.clone());

        if let Some(blob) = extract_blob_from_inscription_tapscript(&tapscript)? {
            return Ok(blob);
        }
    }

    Err(SemanticError::NotFound.into())
}

pub(super) fn extract_blob_from_transaction(
    tx: &LiquidTransaction,
) -> Result<LiquidTransactionBlob, DaError> {
    extract_blob_from_inscription(tx)
}

impl LiquidDa {
    pub(super) async fn read_blob_by_txid(&self, txid: LiquidTxid) -> Result<Vec<u8>, DaError> {
        match extract_blob_from_transaction(&self.fetch_transaction(&txid).await?)? {
            LiquidTransactionBlob::Raw(blob) => Ok(blob),
            LiquidTransactionBlob::Chunk(chunk) => Ok(chunk.payload().to_vec()),
        }
    }

    pub(super) async fn read_blob_by_chunks(
        &self,
        locator: &ChunkedBlobLocator,
    ) -> Result<Vec<u8>, DaError> {
        locator.validate()?;

        let mut payload = Vec::new();
        for chunk_txid_bytes in locator.chunks().iter() {
            let chunk_txid = LiquidTxid::from_byte_array(*chunk_txid_bytes);
            match extract_blob_from_transaction(&self.fetch_transaction(&chunk_txid).await?)? {
                LiquidTransactionBlob::Raw(chunk) => payload.extend_from_slice(&chunk),
                LiquidTransactionBlob::Chunk(chunk) => payload.extend_from_slice(chunk.payload()),
            }
        }

        Ok(payload)
    }
}
