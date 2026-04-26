use crate::da::{DaError, OversizePolicy, RuntimeError};

pub(crate) fn chunk_size_bounds(policy: &OversizePolicy) -> Option<(usize, usize)> {
    match policy {
        OversizePolicy::Reject => None,
        OversizePolicy::Chunked {
            initial_chunk_target_bytes,
            min_chunk_bytes,
        } => Some((*initial_chunk_target_bytes, *min_chunk_bytes)),
    }
}

pub(crate) fn validate_oversize_policy(policy: &OversizePolicy) -> Result<(), DaError> {
    if let Some((initial_chunk_target_bytes, min_chunk_bytes)) = chunk_size_bounds(policy) {
        if min_chunk_bytes == 0 || initial_chunk_target_bytes == 0 {
            return Err(RuntimeError::Misconfigured(
                "oversize chunk sizes must be positive".to_string(),
            )
            .into());
        }
        if min_chunk_bytes > initial_chunk_target_bytes {
            return Err(RuntimeError::Misconfigured(format!(
                "oversize min chunk size {} exceeds initial chunk size {}",
                min_chunk_bytes, initial_chunk_target_bytes
            ))
            .into());
        }
    }
    Ok(())
}
