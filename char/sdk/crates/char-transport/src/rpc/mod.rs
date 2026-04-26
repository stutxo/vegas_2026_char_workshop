// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! RPC types and trait. Transport via HTTP JSON-RPC (`reqwest`) when `bitcoind-client` is enabled.

#[cfg(feature = "bitcoind-client")]
pub mod bitcoind_async;
pub mod mock;
mod types;

pub use types::{
    AddReferendumVoteMode, AttestationEntryWire, AttestationForBondBallot, BondAttestationsInfo,
    BondInfo, DecisionRollEntry, DecisionRollVerbosity, DecisionRollWire, DomainInfo,
    DomainRegistryScheduleResult, KeyRange, LeaderSlotEntry, SlotSelection,
};

use crate::error::TransportError;
use std::collections::HashMap;

/// Async trait for Char RPC. Implemented by a real HTTP client or by char-mock.
#[async_trait::async_trait]
pub trait CharRpcTransport: Send + Sync {
    async fn get_domain_info(&self, domain_preimage_hex: &str) -> Result<DomainInfo, TransportError>;

    async fn get_referendum_decision_roll(
        &self,
        domain_preimage_hex: &str,
        start_ballot: u64,
        end_ballot: u64,
        verbosity: DecisionRollVerbosity,
    ) -> Result<Vec<DecisionRollEntry>, TransportError>;

    async fn add_referendum_vote(
        &self,
        votes: &[(String, String)],
        mode: Option<AddReferendumVoteMode>,
    ) -> Result<HashMap<String, bool>, TransportError>;

    async fn get_leader_for_slot_current_block(
        &self,
        key_ranges: &[KeyRange],
    ) -> Result<Vec<LeaderSlotEntry>, TransportError>;

    async fn get_all_char_bonds(&self, verbosity: u8) -> Result<Vec<BondInfo>, TransportError>;

    /// RPC `getattestationforbondatballot`.
    async fn get_attestation_for_bond_at_ballot(
        &self,
        bond_id: &bitcoin::Txid,
        ballot_number: u64,
    ) -> Result<AttestationForBondBallot, TransportError>;

    /// Schedule a domain in the node's registry. RPC: domain_registry schedule <domain_preimage_hex> <info>.
    async fn domain_registry_schedule(
        &self,
        domain_preimage_hex: &str,
        info: &str,
    ) -> Result<DomainRegistryScheduleResult, TransportError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::mock::MockTransport;

    #[tokio::test]
    async fn mock_get_domain_info() {
        let t = MockTransport;
        let info = t.get_domain_info("deadbeef").await.unwrap();
        assert_eq!(info.next_ballot, 1);
        assert!(info.is_next_leader_mine);
        assert_eq!(
            info.next_leader_bond.to_string(),
            "1111111111111111111111111111111111111111111111111111111111111111"
        );
    }

    #[tokio::test]
    async fn mock_get_referendum_decision_roll() {
        let t = MockTransport;
        let rolls = t
            .get_referendum_decision_roll("d", 0, 3, DecisionRollVerbosity::Minimal)
            .await
            .unwrap();
        assert_eq!(rolls.len(), 4);
        assert_eq!(rolls[0].ballot_number, 0);
        assert!(!rolls[1].found);
        assert!(rolls[2].found);
    }

    #[tokio::test]
    async fn mock_add_referendum_vote() {
        let t = MockTransport;
        let votes = vec![("domain1".to_string(), "votehex".to_string())];
        let r = t.add_referendum_vote(&votes, None).await.unwrap();
        assert_eq!(r.get("domain1"), Some(&true));
    }

    #[tokio::test]
    async fn mock_get_leader_for_slot_current_block() {
        let t = MockTransport;
        let ranges = vec![KeyRange {
            key: "k".into(),
            start_slot: 0,
            end_slot: 10,
        }];
        let entries = t.get_leader_for_slot_current_block(&ranges).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key, "k");
        assert_eq!(entries[0].selections[0].slot, 0);
    }

    #[tokio::test]
    async fn mock_get_all_char_bonds() {
        let t = MockTransport;
        let bonds = t.get_all_char_bonds(0).await.unwrap();
        assert_eq!(bonds.len(), 1);
        assert_eq!(bonds[0].txid.to_string(), "1111111111111111111111111111111111111111111111111111111111111111");
    }
}
