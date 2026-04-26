use crate::da::{DaError, RuntimeError, SemanticError};
use borsh::{BorshDeserialize, BorshSerialize};
use sha2::{Digest, Sha256};

const CHUNK_PAYLOAD_MAGIC: [u8; 8] = *b"bda.chk1";
const CHUNK_PAYLOAD_VERSION: u8 = 1;

#[derive(Debug, Clone, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub(crate) struct ChunkPayload {
    magic: [u8; 8],
    version: u8,
    blob_hash: [u8; 32],
    chunk_index: u32,
    chunk_count: u32,
    payload: Vec<u8>,
}

impl ChunkPayload {
    pub fn blob_hash_from_payload(payload: &[u8]) -> [u8; 32] {
        Sha256::digest(payload).into()
    }

    pub fn new(
        blob_hash: [u8; 32],
        chunk_index: usize,
        chunk_count: usize,
        payload: &[u8],
    ) -> Result<Self, DaError> {
        let chunk_index = u32::try_from(chunk_index).map_err(|_| {
            RuntimeError::Internal(format!(
                "chunk index {} exceeds supported chunk header width",
                chunk_index
            ))
        })?;
        let chunk_count = u32::try_from(chunk_count).map_err(|_| {
            RuntimeError::Internal(format!(
                "chunk count {} exceeds supported chunk header width",
                chunk_count
            ))
        })?;
        let payload = Self {
            magic: CHUNK_PAYLOAD_MAGIC,
            version: CHUNK_PAYLOAD_VERSION,
            blob_hash,
            chunk_index,
            chunk_count,
            payload: payload.to_vec(),
        };
        payload.validate()?;
        Ok(payload)
    }

    pub fn encode(&self) -> Result<Vec<u8>, DaError> {
        borsh::to_vec(self).map_err(|err| {
            RuntimeError::Internal(format!("chunk payload serialization failed: {err}")).into()
        })
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, DaError> {
        let chunk: Self = borsh::from_slice(bytes).map_err(|_| SemanticError::IntegrityFailure)?;
        chunk.validate()?;
        Ok(chunk)
    }

    pub fn validate(&self) -> Result<(), DaError> {
        if self.magic != CHUNK_PAYLOAD_MAGIC || self.version != CHUNK_PAYLOAD_VERSION {
            return Err(SemanticError::IntegrityFailure.into());
        }
        if self.chunk_count == 0 || self.chunk_index >= self.chunk_count {
            return Err(SemanticError::IntegrityFailure.into());
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn verify_against(
        &self,
        blob_hash: &[u8; 32],
        chunk_index: usize,
        chunk_count: usize,
    ) -> Result<(), DaError> {
        let expected_index =
            u32::try_from(chunk_index).map_err(|_| SemanticError::IntegrityFailure)?;
        let expected_count =
            u32::try_from(chunk_count).map_err(|_| SemanticError::IntegrityFailure)?;
        if &self.blob_hash != blob_hash
            || self.chunk_index != expected_index
            || self.chunk_count != expected_count
        {
            return Err(SemanticError::IntegrityFailure.into());
        }
        Ok(())
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

pub(crate) fn encode_chunk_payloads(
    payload: &[u8],
    chunk_size: usize,
) -> Result<Vec<Vec<u8>>, DaError> {
    let blob_hash = ChunkPayload::blob_hash_from_payload(payload);
    let chunk_count = payload.chunks(chunk_size).count();
    payload
        .chunks(chunk_size)
        .enumerate()
        .map(|(chunk_index, chunk)| {
            ChunkPayload::new(blob_hash, chunk_index, chunk_count, chunk)?.encode()
        })
        .collect()
}

#[cfg(test)]
pub(crate) fn verify_blob_hash(blob_hash: &[u8; 32], payload: &[u8]) -> Result<(), DaError> {
    let digest: [u8; 32] = Sha256::digest(payload).into();
    if &digest != blob_hash {
        return Err(SemanticError::IntegrityFailure.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_payload_round_trips_and_validates() {
        let chunk = ChunkPayload::new([0x33; 32], 1, 3, b"hello chunk")
            .expect("chunk payload should build");
        let encoded = chunk.encode().expect("chunk payload should encode");
        let decoded =
            ChunkPayload::decode(encoded.as_slice()).expect("chunk payload should decode");

        assert_eq!(decoded, chunk);
        assert_eq!(decoded.payload(), b"hello chunk");
        assert_eq!(decoded.chunk_index, 1);
        assert_eq!(decoded.chunk_count, 3);
        decoded
            .verify_against(&[0x33; 32], 1, 3)
            .expect("chunk payload should match expected metadata");
    }

    #[test]
    fn blob_hash_matches_payload_hash() {
        let payload = b"hello chunked world";
        let blob_hash = ChunkPayload::blob_hash_from_payload(payload);

        verify_blob_hash(&blob_hash, payload).expect("blob hash should match payload");
        assert!(verify_blob_hash(&[0u8; 32], payload).is_err());
    }
}
