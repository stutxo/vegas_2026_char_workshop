// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! Leader verification: slot leader RPC, optional explicit bond check, and wallet bond scan.

use crate::domain::DomainId;
use crate::error::{LeaderCheckError, SemanticsError};
use bitcoin::Txid;
use char_transport::{CharRpcTransport, DomainInfo, KeyRange};
use char_utils::{hex_to_bytes, strip_0x_prefix};
use std::collections::HashSet;
use std::str::FromStr;

/// Result of checking who is leader for a ballot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaderCheck {
    pub ballot: u64,
    /// Bond transaction id for the leader of `ballot`, if the RPC returned a selection for that slot.
    pub leader_bond_id: Option<Txid>,
    pub is_mine: bool,
}

/// Check who is leader for the given (domain, ballot) and whether it's my bond.
pub async fn check_leader(
    transport: &impl CharRpcTransport,
    domain_preimage_hex: &str,
    ballot: u64,
    my_bond_id_hex: &str,
) -> Result<LeaderCheck, SemanticsError> {
    let my_bond = Txid::from_str(strip_0x_prefix(my_bond_id_hex.trim()))
        .map_err(|_| LeaderCheckError::InvalidBondTxid)?;

    let key_ranges = vec![KeyRange {
        key: domain_preimage_hex.to_string(),
        start_slot: ballot,
        end_slot: ballot,
    }];
    let entries = transport
        .get_leader_for_slot_current_block(&key_ranges)
        .await
        .map_err(SemanticsError::Transport)?;
    let entry = entries
        .into_iter()
        .next()
        .ok_or(LeaderCheckError::NoEntry)?;
    let leader_bond_id = entry
        .selections
        .iter()
        .find(|s| s.slot == ballot)
        .map(|s| s.bond);
    let is_mine = leader_bond_id == Some(my_bond);
    Ok(LeaderCheck {
        ballot,
        leader_bond_id,
        is_mine,
    })
}

/// Whether this node should treat itself as the next leader for the domain: no client-supplied bond.
///
/// Intended for polling loops (**no caching**): after a fresh [`CharRpcTransport::get_domain_info`], call this
/// to re-query the node. Confirms, in order:
/// 1. [`DomainInfo::is_next_leader_mine`] (wallet view on the node).
/// 2. [`CharRpcTransport::get_leader_for_slot_current_block`] for `head.next_ballot` matches [`DomainInfo::next_leader_bond`].
/// 3. [`CharRpcTransport::get_all_char_bonds`] with verbosity **0** (wallet bonds only) includes that leader bond.
pub async fn next_ballot_leader_is_wallet_owned(
    transport: &impl CharRpcTransport,
    domain_preimage_hex: &str,
    head: &DomainInfo,
) -> Result<bool, SemanticsError> {
    let ballot = head.next_ballot;
    let expected = head.next_leader_bond;

    let key_ranges = vec![KeyRange {
        key: domain_preimage_hex.to_string(),
        start_slot: ballot,
        end_slot: ballot,
    }];
    let entries = transport
        .get_leader_for_slot_current_block(&key_ranges)
        .await
        .map_err(SemanticsError::Transport)?;
    let entry = match entries.into_iter().next() {
        Some(e) => e,
        None => return Ok(false),
    };
    let slot_leader = entry
        .selections
        .iter()
        .find(|s| s.slot == ballot)
        .map(|s| s.bond);

    let bonds = transport
        .get_all_char_bonds(0)
        .await
        .map_err(SemanticsError::Transport)?;
    let leader_in_wallet = bonds.iter().any(|b| b.txid == expected);

    let Some(slot_leader) = slot_leader else {
        return Ok(false);
    };

    Ok(head.is_next_leader_mine && slot_leader == expected && leader_in_wallet)
}

/// Recover referendum payload bytes when `getreferendumdecisionroll` omits `decision_roll.data`.
///
/// Tries [`DomainInfo::next_leader_bond`] first, then every bond from [`CharRpcTransport::get_all_char_bonds`]
/// with verbosity **1** (network bonds). For each bond, calls [`CharRpcTransport::get_attestation_for_bond_at_ballot`];
/// uses the attestation only when `ballot_number` matches `ballot`, then scans `entries` for the domain key
/// (same hex as [`DomainId`]'s `Display`) and hex-decodes `value`.
pub async fn try_payload_via_attestation_chain(
    transport: &impl CharRpcTransport,
    domain_preimage_hex: &str,
    domain_id: DomainId,
    ballot: u64,
    head: &DomainInfo,
) -> Result<Option<Vec<u8>>, SemanticsError> {
    let _ = domain_preimage_hex;
    let domain_key_hex = domain_id.to_string();
    let leader_bond = head.next_leader_bond;

    let mut seen = HashSet::new();
    let mut bonds_order: Vec<Txid> = Vec::new();
    if seen.insert(leader_bond) {
        bonds_order.push(leader_bond);
    }

    let all = transport
        .get_all_char_bonds(1)
        .await
        .map_err(SemanticsError::Transport)?;
    for b in all {
        if seen.insert(b.txid) {
            bonds_order.push(b.txid);
        }
    }

    for bond in bonds_order {
        let att = match transport
            .get_attestation_for_bond_at_ballot(&bond, ballot)
            .await
        {
            Ok(a) => a,
            Err(_) => continue,
        };
        if att.ballot_number != ballot {
            continue;
        }
        for entry in &att.entries {
            if !entry.key.eq_ignore_ascii_case(&domain_key_hex) {
                continue;
            }
            let stripped = strip_0x_prefix(entry.value.trim());
            if stripped.is_empty() {
                continue;
            }
            let payload = match hex_to_bytes(stripped) {
                Ok(p) => p,
                Err(_) => continue,
            };
            if !payload.is_empty() {
                return Ok(Some(payload));
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::MockTransport;

    #[tokio::test]
    async fn next_ballot_leader_wallet_owned_mock() {
        let t = MockTransport;
        let head = t.get_domain_info("d").await.unwrap();
        assert!(
            next_ballot_leader_is_wallet_owned(&t, "d", &head)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn check_leader_mock() {
        let t = MockTransport;
        let my_bond_id_hex = "1111111111111111111111111111111111111111111111111111111111111111";
        let c = check_leader(&t, "domain", 0, my_bond_id_hex).await.unwrap();
        assert_eq!(c.ballot, 0);
        assert_eq!(
            c.leader_bond_id,
            Some(Txid::from_str(my_bond_id_hex).unwrap())
        );
        assert!(c.is_mine);
    }
}
