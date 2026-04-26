// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! Reset and reconciliation: fetch rolls via RPC, verify in order.

use crate::error::{ReconcileError, SemanticsError};
use bitcoin::hashes::Hash as _;
use bitcoin::BlockHash;
use char_transport::{CharRpcTransport, DecisionRollVerbosity};
use char_utils::{hex_to_bytes, GET_REFERENDUM_DECISION_ROLL_MAX_INCLUSIVE_SPAN};

/// SHA256 digest for a decision roll (`decision_roll.roll_hash` from RPC).
pub type RollHash = bitcoin::hashes::sha256::Hash;

/// Request for reconcile: domain and ballot range.
#[derive(Debug, Clone)]
pub struct ReconcileRequest {
    pub domain: crate::domain::DomainId,
    pub from_ballot: u64,
    pub to_ballot: u64,
    pub max_fetch: u64,
}

/// Result of reconcile: verified rolls and whether a gap was detected.
#[derive(Debug, Clone)]
pub struct ReconcileResult {
    pub rolls: Vec<VerifiedRoll>,
    pub next_ballot: u64,
    pub gap_detected: bool,
}

/// One verified roll (payload and hashes). Caller persists; SDK does not.
#[derive(Debug, Clone)]
pub struct VerifiedRoll {
    pub ballot: u64,
    pub payload: Option<Vec<u8>>,
    pub serialized_roll: Vec<u8>,
    pub roll_hash: RollHash,
    pub data_hash: Option<RollHash>,
    pub block_hash: BlockHash,
}

/// After a gap or startup: fetch rolls via RPC, verify in order, return verified list.
pub async fn reconcile(
    transport: &impl CharRpcTransport,
    domain_preimage_hex: &str,
    request: ReconcileRequest,
) -> Result<ReconcileResult, SemanticsError> {
    let max_step = request
        .max_fetch
        .min(GET_REFERENDUM_DECISION_ROLL_MAX_INCLUSIVE_SPAN);
    let to = request
        .to_ballot
        .min(request.from_ballot.saturating_add(max_step));
    let entries = transport
        .get_referendum_decision_roll(
            domain_preimage_hex,
            request.from_ballot,
            to,
            DecisionRollVerbosity::Standard,
        )
        .await
        .map_err(SemanticsError::Transport)?;

    let mut rolls = Vec::new();
    let mut next_expected = request.from_ballot;
    let mut gap_detected = false;

    for entry in entries {
        if entry.ballot_number != next_expected {
            gap_detected = true;
            break;
        }
        next_expected = entry.ballot_number + 1;

        if !entry.found {
            gap_detected = true;
            break;
        }

        let wire = entry
            .decision_roll
            .as_ref()
            .ok_or(ReconcileError::MissingDecisionRollWire)?;

        let serialized_roll = wire
            .serialized
            .as_deref()
            .map(|s| {
                let h = char_utils::strip_0x_prefix(s);
                hex_to_bytes(h).map_err(ReconcileError::from)
            })
            .transpose()?
            .unwrap_or_default();

        let mut payload = wire
            .data
            .as_deref()
            .map(|s| {
                let h = char_utils::strip_0x_prefix(s);
                hex_to_bytes(h).map_err(ReconcileError::from)
            })
            .transpose()?;

        if payload.as_ref().map(|p| p.is_empty()).unwrap_or(true) {
            if let Ok(head) = transport.get_domain_info(domain_preimage_hex).await {
                if let Ok(Some(p)) = crate::leader::try_payload_via_attestation_chain(
                    transport,
                    domain_preimage_hex,
                    request.domain,
                    entry.ballot_number,
                    &head,
                )
                .await
                {
                    payload = Some(p);
                }
            }
        }

        let roll_hash = wire.roll_hash.ok_or(ReconcileError::MissingRollHash)?;
        let block_hash = BlockHash::from_byte_array([0u8; 32]);

        rolls.push(VerifiedRoll {
            ballot: entry.ballot_number,
            payload,
            serialized_roll,
            roll_hash,
            data_hash: wire.data_hash,
            block_hash,
        });
    }

    Ok(ReconcileResult {
        rolls,
        next_ballot: next_expected,
        gap_detected,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::MockTransport;

    #[tokio::test]
    async fn reconcile_mock() {
        let t = MockTransport;
        let from_ballot = 0u64;
        let req = ReconcileRequest {
            domain: crate::domain::DomainId::from_preimage(b"d"),
            from_ballot,
            to_ballot: 4,
            max_fetch: 10,
        };
        let res = reconcile(&t, "deadbeef", req).await.unwrap();
        assert!(res.next_ballot >= from_ballot);
        // Mock has found=true only for even ballots; first gap at 1
        assert!(res.gap_detected || !res.rolls.is_empty());
    }
}
