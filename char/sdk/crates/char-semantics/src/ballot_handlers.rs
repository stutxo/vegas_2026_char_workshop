// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! Integrator-defined behavior for a Char referendum domain: vote payloads, decision rolls,
//! and persisted reconciliation progress.
//!
//! The SDK runners ([`crate::run_rpc`], [`crate::run_zmq`]) own the long-lived loop and all
//! transport I/O (RPC, ZMQ). They call [`CharBallotHandlers`] when they need **your** product logic:
//! what bytes to submit as the referendum payload for a ballot, and how to react when a decision
//! roll for that ballot is observed.
//!
//! [`CharReconcileCursor`] lets the runner load and advance the app's persisted rollout cursor so
//! startup catch-up and ZMQ-gap recovery can be driven by the SDK while storage remains app-owned.
//!
//! Methods are **async** so apps can await I/O (DB, signing, HTTP) without blocking the Tokio runtime.
//! Implement the trait with `#[async_trait::async_trait]` (`char-sdk` re-exports this as `async_trait`).

use async_trait::async_trait;
use std::error::Error as StdError;

use crate::reconcile::RollHash;

/// Fully observed decision-roll data passed to app handlers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedRoll {
    pub ballot: u64,
    pub payload: Vec<u8>,
    pub serialized_roll: Option<Vec<u8>>,
    pub roll_hash: Option<RollHash>,
    pub data_hash: Option<RollHash>,
    pub tag: Option<u8>,
}

/// Handlers your app implements so SDK runners can submit votes and verify observed rolls.
#[async_trait]
pub trait CharBallotHandlers: Send {
    /// Bytes embedded in the referendum vote as the payload for `ballot` (the runner adds ballot
    /// encoding via `char-framing`).
    async fn produce_payload(&mut self, ballot: u64) -> Vec<u8>;

    /// Called after `addreferendumvote` returned success for this `ballot` (RPC or ZMQ runner).
    /// Default: no-op. Examples use this to know which rolls to strict-validate in `on_roll_observed`.
    async fn on_leader_submit_accepted(&mut self, _ballot: u64) {}

    /// A decision roll was observed; verify or record it.
    async fn on_roll_observed(
        &mut self,
        roll: ObservedRoll,
    ) -> Result<(), Box<dyn StdError + Send + Sync>>;
}

/// App-owned persisted cursor for startup catch-up and ZMQ-gap recovery.
#[async_trait]
pub trait CharReconcileCursor: Send {
    /// Return the next Char ballot that still needs reconcile / verification.
    async fn next_ballot(&mut self) -> Result<u64, Box<dyn StdError + Send + Sync>>;

    /// Persist that ballots below `next_ballot` have been reconciled.
    async fn advance_cursor(
        &mut self,
        next_ballot: u64,
    ) -> Result<(), Box<dyn StdError + Send + Sync>>;
}
