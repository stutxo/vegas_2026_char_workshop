// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! Char semantics: protocol logic (domain, progress, sync, submit, retry, gaps, ballot handlers).

mod ballot;
mod ballot_handlers;
mod config;
mod domain;
mod error;
mod leader;
mod leader_zmq;
mod progress;
mod reconcile;
mod retry;
mod run_rpc;
#[cfg(feature = "zmq")]
mod run_zmq;
mod streaming;
mod submit;

#[cfg(test)]
mod testing;

pub use ballot::{pending_ballot, PendingBallotInfo};
pub use ballot_handlers::{CharBallotHandlers, CharReconcileCursor, ObservedRoll};
pub use config::{
    BackoffConfig, BufferBounds, ConcurrencyLimits, RetryBudget, RpcPollConfig, SemanticsConfig,
    Timeouts,
};
pub use domain::{DomainError, DomainId};
pub use error::{
    CursorError, LeaderCheckError, PendingBallotError, ReconcileError, SemanticsError,
};
pub use leader::{
    check_leader, next_ballot_leader_is_wallet_owned, try_payload_via_attestation_chain,
    LeaderCheck,
};
pub use leader_zmq::decode_leader_zmq_body;
pub use progress::{Progress, ProgressError};
pub use reconcile::{reconcile, ReconcileRequest, ReconcileResult, RollHash, VerifiedRoll};
pub use retry::{classify_semantics_error, classify_transport_error, RetryClass};
pub use run_rpc::run_rpc;
#[cfg(feature = "zmq")]
pub use run_zmq::{run_zmq, run_zmq_with_address, run_zmq_with_addresses};
pub use streaming::{
    next_decision_roll_event, process_zmq_decision_roll_message, zmq_sequence_from_core,
    DecisionRollEventKind, DecisionRollParseError, DecisionRollStreamEvent, GapReason,
};
pub use submit::{
    submit_vote, ReadAfterWriteConfig, RejectReason, SubmitRequest, SubmitResult, SubmitRetryConfig,
};
