// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! Char SDK - the one crate integrators depend on.
//! Re-exports public API from char-utils, char-framing, char-transport & char-semantics

pub use async_trait::async_trait;
pub use bitcoin::{BlockHash, Txid};
pub use char_framing::{
    decode_referendum_vote, decode_referendum_vote_hex, encode_referendum_vote,
    encode_referendum_vote_hex, FramingError, REFERENDUM_VOTE_LEAF_TYPE,
};
#[cfg(feature = "zmq")]
pub use char_semantics::{run_zmq, run_zmq_with_address, run_zmq_with_addresses};
pub use char_semantics::{
    check_leader, classify_semantics_error, classify_transport_error, decode_leader_zmq_body,
    next_ballot_leader_is_wallet_owned, next_decision_roll_event, pending_ballot,
    process_zmq_decision_roll_message, reconcile, run_rpc, submit_vote,
    try_payload_via_attestation_chain, zmq_sequence_from_core, CharBallotHandlers,
    CharReconcileCursor, CursorError, DecisionRollEventKind, DecisionRollParseError,
    DecisionRollStreamEvent, DomainError, DomainId, GapReason, LeaderCheck, LeaderCheckError,
    ObservedRoll, PendingBallotError, PendingBallotInfo, Progress, ProgressError,
    ReadAfterWriteConfig, ReconcileError, ReconcileRequest, ReconcileResult, RejectReason,
    RetryClass, RollHash, RpcPollConfig, SemanticsConfig, SemanticsError, SubmitRequest,
    SubmitResult, SubmitRetryConfig, VerifiedRoll,
};
/// Mock transport for tests. Re-exported from char-transport so integration tests can use it.
pub use char_transport::MockTransport;
#[cfg(feature = "zmq")]
pub use char_transport::ZmqSubSocket;
pub use char_transport::{
    AddReferendumVoteMode, AttestationEntryWire, AttestationForBondBallot, BitcoindAsyncTransport,
    BondAttestationsInfo, BondInfo, CharRpcTransport, DecisionRollEntry, DecisionRollVerbosity,
    DecisionRollWire, DomainInfo, DomainRegistryScheduleResult, KeyRange, LeaderSlotEntry,
    SlotSelection, TransportError, ZmqAddress, ZmqMessage, ZmqMultipartFormatError, ZmqSubscriber,
};
pub use char_utils::{
    bytes_to_hex, domain_hash, domain_hash_from_hex, domain_hash_to_hex, hex_to_bytes,
    strip_0x_prefix, DomainHash, HexParseError, ShortDiag, UtilsError,
    GET_REFERENDUM_DECISION_ROLL_MAX_INCLUSIVE_SPAN, GET_REFERENDUM_DECISION_ROLL_MAX_RANGE,
    MAX_CHAR_BAMBOO_SIZE,
};
