// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! Minimal mock transport for tests. Returns fixed responses; no network.

use super::types::*;
use super::CharRpcTransport;
use crate::error::TransportError;
use bitcoin::hashes::sha256::Hash as Sha256Hash;
use bitcoin::{BlockHash, Txid};
use char_utils::ShortDiag;
use std::collections::HashMap;
use std::str::FromStr;

/// Mock that returns fixed data for tests. Reused by char-transport, char-semantics, and char-sdk tests.
pub struct MockTransport;

fn hex64(ch: char) -> String {
    std::iter::repeat_n(ch, 64).collect()
}

#[async_trait::async_trait]
impl CharRpcTransport for MockTransport {
    async fn get_domain_info(&self, domain_preimage_hex: &str) -> Result<DomainInfo, TransportError> {
        let _ = domain_preimage_hex;
        Ok(DomainInfo {
            next_ballot: 1,
            next_leader_bond: Txid::from_str(&hex64('1')).unwrap(),
            is_next_leader_mine: true,
            latest_decided_ballot: Some(0),
            latest_decision_roll_hash: hex64('a'),
            latest_decision_data_hash: hex64('b'),
            latest_decision_zeitgeist: hex64('c'),
        })
    }

    async fn get_referendum_decision_roll(
        &self,
        _domain: &str,
        start_ballot: u64,
        end_ballot: u64,
        _verbosity: DecisionRollVerbosity,
    ) -> Result<Vec<DecisionRollEntry>, TransportError> {
        let mut out = Vec::new();
        for b in start_ballot..=end_ballot {
            let found = b % 2 == 0;
            let decision_roll = if found {
                Some(DecisionRollWire {
                    roll_hash: Some(Sha256Hash::from_str(&hex64('d')).unwrap()),
                    data_hash: None,
                    serialized: Some("00".to_string()),
                    data: Some("00".to_string()),
                    attestation_hash: None,
                    proofs: None,
                })
            } else {
                None
            };
            out.push(DecisionRollEntry {
                domain_hash: Sha256Hash::from_str(&hex64('e')).unwrap(),
                ballot_number: b,
                found,
                decision_roll,
            });
        }
        Ok(out)
    }

    async fn add_referendum_vote(
        &self,
        votes: &[(String, String)],
        _mode: Option<AddReferendumVoteMode>,
    ) -> Result<HashMap<String, bool>, TransportError> {
        let mut m = HashMap::new();
        for (k, _) in votes {
            m.insert(k.clone(), true);
        }
        Ok(m)
    }

    async fn get_leader_for_slot_current_block(
        &self,
        key_ranges: &[KeyRange],
    ) -> Result<Vec<LeaderSlotEntry>, TransportError> {
        Ok(key_ranges
            .iter()
            .map(|kr| LeaderSlotEntry {
                key: kr.key.clone(),
                blockhash: BlockHash::from_str(&hex64('f')).unwrap(),
                selections: vec![SlotSelection {
                    slot: kr.start_slot,
                    bond: Txid::from_str(&hex64('1')).unwrap(),
                }],
            })
            .collect())
    }

    async fn get_all_char_bonds(&self, _verbosity: u8) -> Result<Vec<BondInfo>, TransportError> {
        // Verbosity 0 = wallet bonds; align with `next_leader_bond` from `get_domain_info` for runner tests.
        Ok(vec![BondInfo {
            txid: Txid::from_str(&hex64('1')).unwrap(),
            issuer: "".into(),
            amount: "0.1".into(),
            closed: false,
            attestations: None,
        }])
    }

    async fn get_attestation_for_bond_at_ballot(
        &self,
        _bond_id: &Txid,
        _ballot_number: u64,
    ) -> Result<AttestationForBondBallot, TransportError> {
        Err(TransportError::Rpc {
            code: -8,
            message: ShortDiag::truncate("mock: no attestation"),
        })
    }

    async fn domain_registry_schedule(
        &self,
        _domain_preimage_hex: &str,
        _info: &str,
    ) -> Result<DomainRegistryScheduleResult, TransportError> {
        Ok(DomainRegistryScheduleResult { success: true })
    }
}
