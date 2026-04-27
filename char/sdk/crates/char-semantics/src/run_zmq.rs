// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! ZMQ runner: leader + decisionroll subs; produce when leader, reconcile from app-owned
//! progress on startup and stream gaps, then verify rolls via [`CharBallotHandlers`].

#[cfg(feature = "zmq")]
mod inner {
    use crate::ballot_handlers::{CharBallotHandlers, CharReconcileCursor, ObservedRoll};
    use crate::error::{CursorError, ReconcileError, SemanticsError};
    use crate::leader::next_ballot_leader_is_wallet_owned;
    use crate::reconcile::{ReconcileRequest, ReconcileResult, reconcile};
    use crate::streaming::{DecisionRollEventKind, process_zmq_decision_roll_message};
    use char_transport::{CharRpcTransport, ZmqSubSocket, ZmqSubscriber};
    use char_utils::{HexParseError, ShortDiag, bytes_to_hex};
    use std::collections::HashSet;

    fn handler_denied(error: impl std::fmt::Display) -> SemanticsError {
        SemanticsError::Reconcile(ReconcileError::HandlerDenied(ShortDiag::truncate(
            &error.to_string(),
        )))
    }

    fn cursor_load_denied(error: impl std::fmt::Display) -> SemanticsError {
        SemanticsError::Cursor(CursorError::LoadDenied(ShortDiag::truncate(
            &error.to_string(),
        )))
    }

    fn cursor_advance_denied(error: impl std::fmt::Display) -> SemanticsError {
        SemanticsError::Cursor(CursorError::AdvanceDenied(ShortDiag::truncate(
            &error.to_string(),
        )))
    }

    async fn advance_next_ballot<D: CharReconcileCursor + ?Sized>(
        handlers: &mut D,
        next_ballot: &mut u64,
        next: u64,
    ) -> Result<(), SemanticsError> {
        handlers
            .advance_cursor(next)
            .await
            .map_err(cursor_advance_denied)?;
        *next_ballot = next;
        Ok(())
    }

    /// Fetch one decided ballot via RPC, apply/skip it, and advance `next_ballot`.
    async fn reconcile_after_zmq_gap<D: CharBallotHandlers + CharReconcileCursor>(
        transport: &impl CharRpcTransport,
        domain_hex: &str,
        handlers: &mut D,
        next_ballot: &mut u64,
    ) -> Result<ReconcileResult, SemanticsError> {
        let from = *next_ballot;
        let domain = crate::domain::DomainId::from_preimage_hex(domain_hex)?;
        let req = ReconcileRequest {
            domain,
            from_ballot: from,
            to_ballot: from,
            max_fetch: 1,
        };
        let res = match reconcile(transport, domain_hex, req).await {
            Ok(res) => res,
            Err(error @ SemanticsError::Reconcile(_))
                if matches!(
                    error,
                    SemanticsError::Reconcile(ReconcileError::MissingDecisionRollWire)
                        | SemanticsError::Reconcile(ReconcileError::HexDecode(
                            HexParseError::Empty
                        ))
                ) =>
            {
                let next = from.saturating_add(1);
                advance_next_ballot(handlers, next_ballot, next).await?;
                return Ok(ReconcileResult {
                    rolls: Vec::new(),
                    next_ballot: next,
                    gap_detected: false,
                });
            }
            Err(error) => return Err(error),
        };
        for roll in &res.rolls {
            let Some(raw) = &roll.payload else {
                advance_next_ballot(handlers, next_ballot, roll.ballot.saturating_add(1)).await?;
                continue;
            };
            if raw.is_empty() {
                advance_next_ballot(handlers, next_ballot, roll.ballot.saturating_add(1)).await?;
                continue;
            }
            handlers
                .on_roll_observed(ObservedRoll {
                    ballot: roll.ballot,
                    payload: raw.clone(),
                    serialized_roll: Some(roll.serialized_roll.clone()),
                    roll_hash: Some(roll.roll_hash),
                    data_hash: roll.data_hash,
                    tag: None,
                })
                .await
                .map_err(handler_denied)?;
            advance_next_ballot(handlers, next_ballot, roll.ballot.saturating_add(1)).await?;
        }
        if !res.gap_detected {
            *next_ballot = res.next_ballot;
        }
        Ok(res)
    }

    /// Walk `next_ballot` via RPC until the domain tip has been reached or a gap is found.
    async fn reconcile_to_tip<D: CharBallotHandlers + CharReconcileCursor>(
        transport: &impl CharRpcTransport,
        domain_hex: &str,
        handlers: &mut D,
        next_ballot: &mut u64,
    ) -> Result<(), SemanticsError> {
        'outer: loop {
            let latest_decided_ballot = transport
                .get_domain_info(domain_hex)
                .await?
                .latest_decided_ballot;
            if latest_decided_ballot.is_none_or(|latest| *next_ballot > latest) {
                break 'outer;
            }

            let from = *next_ballot;
            let res = reconcile_after_zmq_gap(transport, domain_hex, handlers, next_ballot).await?;
            if res.next_ballot <= from || res.gap_detected {
                break 'outer;
            }
        }
        Ok(())
    }

    /// `domain_hex` is the domain **preimage** hex (same string RPC uses for votes). The canonical
    /// domain id for ZMQ filtering is [`crate::domain::DomainId::from_preimage_hex`].
    pub async fn run_zmq<D: CharBallotHandlers + CharReconcileCursor>(
        transport: &impl CharRpcTransport,
        domain_hex: &str,
        handlers: &mut D,
    ) -> Result<(), SemanticsError> {
        let leader_addr = std::env::var("CHAR_ZMQ_LEADER_ADDR")
            .unwrap_or_else(|_| "tcp://127.0.0.1:28332".into());
        let roll_addr =
            std::env::var("CHAR_ZMQ_DECISIONROLL_ADDR").unwrap_or_else(|_| leader_addr.clone());
        run_zmq_with_addresses(transport, domain_hex, handlers, leader_addr, roll_addr).await
    }

    /// Run the ZMQ loop with one endpoint for both leader and decision-roll subscriptions.
    ///
    /// `domain_hex` is the domain **preimage** hex (same string RPC uses for votes). The canonical
    /// domain id for ZMQ filtering is [`crate::domain::DomainId::from_preimage_hex`].
    pub async fn run_zmq_with_address<D: CharBallotHandlers + CharReconcileCursor>(
        transport: &impl CharRpcTransport,
        domain_hex: &str,
        handlers: &mut D,
        addr: impl Into<String>,
    ) -> Result<(), SemanticsError> {
        let addr = addr.into();
        run_zmq_with_addresses(transport, domain_hex, handlers, addr.clone(), addr).await
    }

    /// Run the ZMQ loop with explicit leader and decision-roll endpoints.
    ///
    /// `domain_hex` is the domain **preimage** hex (same string RPC uses for votes). The canonical
    /// domain id for ZMQ filtering is [`crate::domain::DomainId::from_preimage_hex`].
    pub async fn run_zmq_with_addresses<D: CharBallotHandlers + CharReconcileCursor>(
        transport: &impl CharRpcTransport,
        domain_hex: &str,
        handlers: &mut D,
        leader_addr: impl Into<String>,
        decisionroll_addr: impl Into<String>,
    ) -> Result<(), SemanticsError> {
        let leader_addr = leader_addr.into();
        let roll_addr = decisionroll_addr.into();
        let domain_id = crate::domain::DomainId::from_preimage_hex(domain_hex)?;
        let mut leader_sub = ZmqSubSocket::subscribe(leader_addr.clone(), "leader");
        let mut roll_sub = ZmqSubSocket::subscribe(roll_addr.clone(), "decisionroll");
        let (mut submitted, mut seq) = (HashSet::new(), None::<u32>);
        let mut next_ballot = handlers.next_ballot().await.map_err(cursor_load_denied)?;
        let zmq_verbose = std::env::var("CHAR_ZMQ_VERBOSE")
            .map(|v| {
                let v = v.to_ascii_lowercase();
                v != "0" && v != "false" && !v.is_empty()
            })
            .unwrap_or(false);

        println!(
            "Listening for leader notifications on {} (decision rolls on {})",
            leader_addr, roll_addr
        );

        reconcile_to_tip(transport, domain_hex, handlers, &mut next_ballot).await?;

        loop {
            tokio::select! {
                Ok(msg) = leader_sub.recv() => {
                    if let Some((notif_ballot, notif_domain)) = crate::leader_zmq::decode_leader_zmq_body(&msg.body) {
                        if notif_domain != domain_id {
                            continue;
                        }
                        if zmq_verbose {
                            println!("Leader notification for ballot {}", notif_ballot);
                        }
                        let head = match transport.get_domain_info(domain_hex).await {
                            Ok(d) => d,
                            Err(_) => {
                                continue;
                            }
                        };
                        if head.latest_decided_ballot.is_some_and(|latest| next_ballot <= latest) {
                            reconcile_to_tip(transport, domain_hex, handlers, &mut next_ballot).await?;
                        }
                        if head.next_ballot != notif_ballot {
                            continue;
                        }
                        let ours = match next_ballot_leader_is_wallet_owned(transport, domain_hex, &head).await {
                            Ok(v) => v,
                            Err(_) => {
                                continue;
                            }
                        };
                        if !ours {
                            continue;
                        }
                        if submitted.contains(&notif_ballot) {
                            continue;
                        }
                        let payload = handlers.produce_payload(notif_ballot).await;
                        let votes = vec![(domain_hex.to_string(), bytes_to_hex(&payload))];
                        let ok = transport
                            .add_referendum_vote(&votes, Some(char_transport::AddReferendumVoteMode::IsLeader))
                            .await
                            .map_err(SemanticsError::Transport)?
                            .get(domain_hex) == Some(&true);
                        if ok {
                            submitted.insert(notif_ballot);
                            handlers.on_leader_submit_accepted(notif_ballot).await;
                            println!("Submit attempt accepted for ballot {}", notif_ballot);
                        } else {
                            println!("Submit attempt rejected for ballot {}", notif_ballot);
                        }
                    }
                }
                Ok(msg) = roll_sub.recv() => match process_zmq_decision_roll_message(msg, &mut seq) {
                    Ok(e) if e.domain == domain_id => match &e.kind {
                        DecisionRollEventKind::Observed { tag, serialized } => {
                            if zmq_verbose {
                                println!(
                                    "DecodedDecisionRollNotification{{tag:{}, roll_len:{}}}",
                                    tag,
                                    serialized.len()
                                );
                            }
                            reconcile_to_tip(transport, domain_hex, handlers, &mut next_ballot).await?;
                        }
                        DecisionRollEventKind::Gap(_) => {
                            reconcile_to_tip(transport, domain_hex, handlers, &mut next_ballot).await?;
                            seq = None;
                            continue;
                        }
                    },
                    Err(SemanticsError::Gap(_)) => {
                        reconcile_to_tip(transport, domain_hex, handlers, &mut next_ballot).await?;
                        seq = None;
                        continue;
                    }
                    Err(e) => return Err(e),
                    _ => {}
                },
            }
        }
    }
}

#[cfg(feature = "zmq")]
pub use inner::{run_zmq, run_zmq_with_address, run_zmq_with_addresses};
