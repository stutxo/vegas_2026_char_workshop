mod chunking;
mod fees;
mod instructions;
mod rpc;
mod selection;

pub(crate) use chunking::{ChunkPayload, encode_chunk_payloads};
pub(crate) use fees::{fee_policy_rates, validate_fee_policy};
pub(crate) use instructions::{collect_instructions, extract_blob_from_inscription_instructions};
pub(crate) use rpc::map_rpc_client_error;
pub(crate) use selection::{chunk_size_bounds, validate_oversize_policy};

pub(crate) const CHOMP_PROTOCOL_ID: &[u8] = b"chomp";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chomp_protocol_id_matches_expected_bytes() {
        assert_eq!(CHOMP_PROTOCOL_ID, b"chomp");
    }
}
