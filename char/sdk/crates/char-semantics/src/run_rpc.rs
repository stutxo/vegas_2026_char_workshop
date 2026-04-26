// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! RPC poll runner: each iteration calls **`getdomaininfo`**, then [`crate::leader::next_ballot_leader_is_wallet_owned`]
//! (**`get_leader_for_slot_current_block`** + **`getallcharbonds(0)`**; no client bond parameter, no caching of RPC results between steps).
//! When that chain says the wallet owns the next leader bond, produce → submit → verify roll via [`CharBallotHandlers`].
//!
//! Loop timing is [`crate::config::RpcPollConfig`].

use crate::ballot_handlers::{CharBallotHandlers, ObservedRoll};
use crate::config::RpcPollConfig;
use crate::error::{ReconcileError, SemanticsError};
use crate::leader::next_ballot_leader_is_wallet_owned;
use char_transport::{AddReferendumVoteMode, CharRpcTransport, DecisionRollVerbosity};
use char_utils::{bytes_to_hex, hex_to_bytes, strip_0x_prefix};
use std::collections::HashSet;

/// Run the integrator's handlers over RPC polling: poll domain head, submit when leader, verify roll.
pub async fn run_rpc<D: CharBallotHandlers>(
    transport: &impl CharRpcTransport,
    domain_hex: &str,
    domain_id: crate::domain::DomainId,
    handlers: &mut D,
    timing: RpcPollConfig,
) -> Result<(), SemanticsError> {
    let timing = timing.normalized();
    let mut submitted = HashSet::new();
    loop {
        let head = match transport.get_domain_info(domain_hex).await {
            Ok(d) => d,
            Err(_) => {
                tokio::time::sleep(timing.poll_delay).await;
                continue;
            }
        };
        let ballot = head.next_ballot;
        let ours = match next_ballot_leader_is_wallet_owned(transport, domain_hex, &head).await {
            Ok(v) => v,
            Err(_) => {
                tokio::time::sleep(timing.poll_delay).await;
                continue;
            }
        };
        if !ours {
            tokio::time::sleep(timing.poll_delay).await;
            continue;
        }
        if !submitted.insert(ballot) {
            tokio::time::sleep(timing.poll_delay).await;
            continue;
        }
        let payload = handlers.produce_payload(ballot).await;
        // RPC expects raw payload hex; node binds to the current pending ballot.
        let vote_hex = bytes_to_hex(&payload);
        let votes = vec![(domain_hex.to_string(), vote_hex)];
        let ok = transport
            .add_referendum_vote(&votes, Some(AddReferendumVoteMode::IsLeader))
            .await?
            .get(domain_hex)
            == Some(&true);
        if !ok {
            submitted.remove(&ballot);
            tokio::time::sleep(timing.poll_delay).await;
            continue;
        }
        handlers.on_leader_submit_accepted(ballot).await;
        let deadline = std::time::Instant::now() + timing.roll_timeout;
        while std::time::Instant::now() < deadline {
            let entries = transport
                .get_referendum_decision_roll(
                    domain_hex,
                    ballot,
                    ballot,
                    DecisionRollVerbosity::Standard,
                )
                .await?;
            let entry = match entries.into_iter().next() {
                Some(e) => e,
                None => {
                    tokio::time::sleep(timing.roll_poll_interval).await;
                    continue;
                }
            };
            if !entry.found {
                tokio::time::sleep(timing.roll_poll_interval).await;
                continue;
            }
            if entry.ballot_number != ballot {
                tokio::time::sleep(timing.roll_poll_interval).await;
                continue;
            }
            let roll_wire = entry.decision_roll.as_ref();
            let serialized_roll = roll_wire
                .and_then(|r| r.serialized.as_deref())
                .map(|serialized_hex| {
                    let stripped = strip_0x_prefix(serialized_hex.trim());
                    if stripped.is_empty() {
                        Ok(Vec::new())
                    } else {
                        hex_to_bytes(stripped).map_err(ReconcileError::from)
                    }
                })
                .transpose()
                .map_err(SemanticsError::from)?;
            let roll_hash = roll_wire.and_then(|r| r.roll_hash.as_ref().cloned());
            let data_hash = roll_wire.and_then(|r| r.data_hash.as_ref().cloned());
            let mut payload_opt = entry
                .decision_roll
                .as_ref()
                .and_then(|r| r.data.as_deref())
                .and_then(|data_hex| {
                    let stripped = strip_0x_prefix(data_hex.trim());
                    if stripped.is_empty() {
                        None
                    } else {
                        hex_to_bytes(stripped).ok()
                    }
                });

            if payload_opt.as_ref().map(|p| p.is_empty()).unwrap_or(true) {
                if let Ok(head) = transport.get_domain_info(domain_hex).await {
                    if let Ok(Some(p)) = crate::leader::try_payload_via_attestation_chain(
                        transport, domain_hex, domain_id, ballot, &head,
                    )
                    .await
                    {
                        payload_opt = Some(p);
                    }
                }
            }

            let Some(payload) = payload_opt.filter(|p| !p.is_empty()) else {
                tokio::time::sleep(timing.roll_poll_interval).await;
                continue;
            };
            handlers
                .on_roll_observed(ObservedRoll {
                    ballot,
                    payload,
                    serialized_roll,
                    roll_hash,
                    data_hash,
                    tag: None,
                })
                .await
                .map_err(|e| {
                    SemanticsError::Reconcile(ReconcileError::HandlerDenied(
                        char_utils::ShortDiag::truncate(&e.to_string()),
                    ))
                })?;
            break;
        }
        tokio::time::sleep(timing.poll_delay).await;
    }
}
