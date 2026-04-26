// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! Integration: semantics + transport (mock): pending_ballot, check_leader, reconcile, submit, progress, retry.

mod common;

use char_sdk::{
    check_leader, classify_semantics_error, classify_transport_error, pending_ballot, reconcile,
    submit_vote, DomainId, Progress, ReconcileError, ReconcileRequest, RetryClass, ShortDiag,
    SubmitRequest, SubmitResult, SubmitRetryConfig, TransportError, Txid,
};
use std::str::FromStr;
use std::time::Duration;

#[tokio::test]
async fn pending_ballot_and_leader_check() {
    let transport = common::MockTransport;
    let domain_hex = "deadbeef";
    let my_bond_id_hex = "1111111111111111111111111111111111111111111111111111111111111111";
    let info = pending_ballot(&transport, domain_hex, my_bond_id_hex).await.unwrap();
    assert_eq!(info.pending_ballot, 1);
    assert!(info.leader_is_mine);

    let check = check_leader(&transport, domain_hex, 0, my_bond_id_hex).await.unwrap();
    assert_eq!(check.ballot, 0);
    assert!(check.is_mine);
    assert_eq!(
        check.leader_bond_id,
        Some(Txid::from_str(my_bond_id_hex).unwrap())
    );
}

#[tokio::test]
async fn reconcile_returns_rolls_or_gap() {
    let transport = common::MockTransport;
    let req = ReconcileRequest {
        domain: DomainId::from_preimage(b"d"),
        from_ballot: 0,
        to_ballot: 4,
        max_fetch: 10,
    };
    let res = reconcile(&transport, "deadbeef", req).await.unwrap();
    assert!(res.gap_detected || !res.rolls.is_empty(), "reconcile should return rolls or signal gap");
}

#[tokio::test]
async fn submit_vote_accepted() {
    let transport = common::MockTransport;
    let req = SubmitRequest {
        domain_preimage_hex: "deadbeef".into(),
        ballot: 1,
        payload: vec![1, 2, 3],
        idempotency_key: None,
        leader_verification: false,
        read_after_write: char_sdk::ReadAfterWriteConfig {
            enabled: false,
            max_wait: Duration::from_secs(1),
            poll_interval: Duration::from_millis(10),
        },
    };
    let res = submit_vote(&transport, req, "bond", SubmitRetryConfig::default())
        .await
        .unwrap();
    assert!(matches!(res, SubmitResult::Submitted));
}

#[test]
fn progress_advance_and_next_ballot() {
    let mut p = Progress::default();
    assert_eq!(p.next_ballot_to_verify(), 0);
    p.advance_verified(0).unwrap();
    p.advance_verified(1).unwrap();
    assert_eq!(p.next_ballot_to_verify(), 2);
}

#[test]
fn retry_classify_transport_and_semantics() {
    assert_eq!(
        classify_transport_error(&TransportError::Timeout),
        RetryClass::Retryable
    );
    assert_eq!(
        classify_transport_error(&TransportError::Deserialization(ShortDiag::truncate("bad"))),
        RetryClass::Terminal
    );
    let e = char_sdk::SemanticsError::Reconcile(ReconcileError::MissingDecisionRollWire);
    assert_eq!(classify_semantics_error(&e), RetryClass::Retryable);
}
