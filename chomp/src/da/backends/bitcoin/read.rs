use super::{BitcoinDa, types::BitcoinTransactionBlob};
use crate::da::backends::common::{
    ChunkPayload, collect_instructions, extract_blob_from_inscription_instructions,
};
use crate::da::{ChunkedBlobLocator, DaError, SemanticError};
use ::bitcoin::{
    Script, Transaction, Txid,
    hashes::Hash,
    opcodes::all::{OP_ENDIF, OP_IF},
    script::Instruction,
};

fn is_empty_push(instruction: &Instruction<'_>) -> bool {
    matches!(instruction, Instruction::PushBytes(bytes) if bytes.is_empty())
}

fn pushed_bytes(instruction: &Instruction<'_>) -> Option<Vec<u8>> {
    match instruction {
        Instruction::PushBytes(bytes) => Some(bytes.as_bytes().to_vec()),
        Instruction::Op(_) => None,
    }
}

pub(super) fn extract_blob_from_inscription_tapscript(
    tapscript: &Script,
) -> Result<Option<BitcoinTransactionBlob>, DaError> {
    let instructions = collect_instructions(
        tapscript.instructions(),
        "failed to parse inscription tapscript",
    )?;

    let Some(payload) = extract_blob_from_inscription_instructions(
        &instructions,
        is_empty_push,
        |instruction| matches!(instruction, Instruction::Op(op) if *op == OP_IF),
        |instruction| matches!(instruction, Instruction::Op(op) if *op == OP_ENDIF),
        pushed_bytes,
    ) else {
        return Ok(None);
    };

    if let Ok(chunk) = ChunkPayload::decode(payload.as_slice()) {
        return Ok(Some(BitcoinTransactionBlob::Chunk(chunk)));
    }

    Ok(Some(BitcoinTransactionBlob::Raw(payload)))
}

pub(super) fn extract_blob_from_inscription(
    tx: &Transaction,
) -> Result<BitcoinTransactionBlob, DaError> {
    for input in &tx.input {
        let Some(tapscript_bytes) = input.witness.second_to_last() else {
            continue;
        };
        let tapscript = Script::from_bytes(tapscript_bytes);

        if let Some(blob) = extract_blob_from_inscription_tapscript(tapscript)? {
            return Ok(blob);
        }
    }

    Err(SemanticError::NotFound.into())
}

pub(super) fn extract_blob_from_transaction(
    tx: &Transaction,
) -> Result<BitcoinTransactionBlob, DaError> {
    extract_blob_from_inscription(tx)
}

impl BitcoinDa {
    pub(super) async fn read_blob_by_txid(&self, txid: Txid) -> Result<Vec<u8>, DaError> {
        match extract_blob_from_transaction(&self.fetch_transaction(&txid).await?)? {
            BitcoinTransactionBlob::Raw(blob) => Ok(blob),
            BitcoinTransactionBlob::Chunk(chunk) => Ok(chunk.payload().to_vec()),
        }
    }

    pub(super) async fn read_blob_by_chunks(
        &self,
        locator: &ChunkedBlobLocator,
    ) -> Result<Vec<u8>, DaError> {
        locator.validate()?;

        let mut payload = Vec::new();
        for chunk_txid_bytes in locator.chunks().iter() {
            let chunk_txid = Txid::from_byte_array(*chunk_txid_bytes);
            match extract_blob_from_transaction(&self.fetch_transaction(&chunk_txid).await?)? {
                BitcoinTransactionBlob::Raw(chunk) => payload.extend_from_slice(&chunk),
                BitcoinTransactionBlob::Chunk(chunk) => payload.extend_from_slice(chunk.payload()),
            }
        }

        Ok(payload)
    }
}
