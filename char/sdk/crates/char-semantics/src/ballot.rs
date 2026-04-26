// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! Ballot lifecycle: next ballot and leader bond for (domain, bond) via `getdomaininfo`.

use crate::error::{LeaderCheckError, PendingBallotError, SemanticsError};
use bitcoin::Txid;
use char_transport::CharRpcTransport;
use char_utils::strip_0x_prefix;
use std::str::FromStr;

/// Semantic view of the next ballot for this domain (from `getdomaininfo`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingBallotInfo {
    pub pending_ballot: u64,
    /// Leader bond for `pending_ballot` (same type as `DomainInfo::next_leader_bond`).
    pub leader: Txid,
    pub leader_is_mine: bool,
}

/// Fetch the next pending ballot for (domain, bond). Uses `getdomaininfo`; `bond_id_hex` must match `next_leader_bond`.
pub async fn pending_ballot(
    transport: &impl CharRpcTransport,
    domain_preimage_hex: &str,
    bond_id_hex: &str,
) -> Result<PendingBallotInfo, SemanticsError> {
    let d = transport
        .get_domain_info(domain_preimage_hex)
        .await
        .map_err(SemanticsError::Transport)?;
    let want = Txid::from_str(strip_0x_prefix(bond_id_hex.trim()))
        .map_err(|_| SemanticsError::LeaderCheck(LeaderCheckError::InvalidBondTxid))?;
    if want != d.next_leader_bond {
        return Err(SemanticsError::PendingBallot(
            PendingBallotError::BondNotNextLeader,
        ));
    }
    Ok(PendingBallotInfo {
        pending_ballot: d.next_ballot,
        leader: d.next_leader_bond,
        leader_is_mine: d.is_next_leader_mine,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::PendingBallotError;
    use crate::testing::MockTransport;
    use bitcoin::Txid;
    use std::str::FromStr;

    #[tokio::test]
    async fn pending_ballot_mock() {
        let t = MockTransport;
        let bond = "1111111111111111111111111111111111111111111111111111111111111111";
        let info = pending_ballot(&t, "domain", bond).await.unwrap();
        assert_eq!(info.pending_ballot, 1);
        assert!(info.leader_is_mine);
        assert_eq!(info.leader, Txid::from_str(bond).unwrap());
    }

    #[tokio::test]
    async fn pending_ballot_bond_mismatch() {
        let t = MockTransport;
        let err = pending_ballot(
            &t,
            "domain",
            "2222222222222222222222222222222222222222222222222222222222222222",
        )
        .await
        .unwrap_err();
        assert!(matches!(
            err,
            SemanticsError::PendingBallot(PendingBallotError::BondNotNextLeader)
        ));
    }
}
