// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! Integration: transport types + semantics types in one flow (domain info, stream event, error mapping).

mod common;

use char_sdk::{
    CharRpcTransport, DecisionRollEventKind, DecisionRollStreamEvent, DomainId, GapReason,
    SemanticsError, TransportError,
};

#[tokio::test]
async fn get_domain_info_via_transport_then_domain_id_semantics() {
    let transport = common::MockTransport;
    let info = transport.get_domain_info("deadbeef").await.unwrap();
    assert_eq!(info.next_ballot, 1);
    assert!(info.is_next_leader_mine);
    assert_eq!(
        info.latest_decision_roll_hash,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );

    let id = DomainId::from_preimage_hex("deadbeef").unwrap();
    assert_eq!(id.to_string().len(), 64);
}

#[test]
fn stream_event_kind_and_gap_reason_from_semantics() {
    let domain = DomainId::from_preimage(b"d");
    let ev = DecisionRollStreamEvent {
        domain,
        ballot: 1,
        kind: DecisionRollEventKind::Observed {
            serialized: vec![0, 1],
            payload: vec![2, 3],
            tag: 0,
        },
    };
    assert_eq!(ev.ballot, 1);
    let gap = GapReason::MissingBallot { expected: 2 };
    let err = SemanticsError::Gap(gap);
    assert!(err.to_string().contains("gap"));
}

#[test]
fn transport_error_maps_to_semantics_error() {
    let te = TransportError::Timeout;
    let se: SemanticsError = te.into();
    assert!(matches!(se, SemanticsError::Transport(_)));
}
