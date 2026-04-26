use super::CHOMP_PROTOCOL_ID;
use crate::da::{DaError, SemanticError};
use std::fmt;

pub(crate) fn collect_instructions<T, E, I>(
    instructions: I,
    context: &str,
) -> Result<Vec<T>, DaError>
where
    I: IntoIterator<Item = std::result::Result<T, E>>,
    E: fmt::Display,
{
    instructions
        .into_iter()
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|err| SemanticError::Unsupported(format!("{context}: {err}")).into())
}

pub(crate) fn extract_blob_from_inscription_instructions<T>(
    instructions: &[T],
    is_empty_push: impl Fn(&T) -> bool,
    is_if: impl Fn(&T) -> bool,
    is_endif: impl Fn(&T) -> bool,
    pushed_bytes: impl Fn(&T) -> Option<Vec<u8>>,
) -> Option<Vec<u8>> {
    for start in 0..instructions.len() {
        if start + 3 >= instructions.len() {
            break;
        }
        if !is_empty_push(&instructions[start]) || !is_if(&instructions[start + 1]) {
            continue;
        }

        let Some(header) = pushed_bytes(&instructions[start + 2]) else {
            continue;
        };
        if header.as_slice() != CHOMP_PROTOCOL_ID {
            continue;
        }

        let mut payload = Vec::new();
        for instruction in instructions.iter().skip(start + 3) {
            if let Some(bytes) = pushed_bytes(instruction) {
                payload.extend_from_slice(&bytes);
                continue;
            }
            if is_endif(instruction) {
                return Some(payload);
            }
            return None;
        }
    }

    None
}
