// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! Typed semantics errors. No `anyhow` at library boundary.

use crate::domain::DomainError;
use crate::streaming::GapReason;
use crate::submit::RejectReason;
use char_framing::FramingError;
use char_transport::TransportError;
use char_utils::{HexParseError, ShortDiag};
use thiserror::Error;

/// `pending_ballot` could not satisfy the (domain, bond) request against `getdomaininfo`.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PendingBallotError {
    #[error("bond id does not match next leader bond")]
    BondNotNextLeader,
}

/// Leader RPC or bond-id parsing failed for the requested domain/slot.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LeaderCheckError {
    #[error("no leader entry for domain/slot")]
    NoEntry,

    #[error("invalid bond txid hex")]
    InvalidBondTxid,
}

/// Runner cursor load / persistence failures from [`crate::ballot_handlers::CharReconcileCursor`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CursorError {
    #[error("reconcile cursor load denied: {0}")]
    LoadDenied(ShortDiag),

    #[error("reconcile cursor advance denied: {0}")]
    AdvanceDenied(ShortDiag),
}

/// Reconciliation / roll fetch failures (structured; no arbitrary strings).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ReconcileError {
    #[error("hex decode: {0}")]
    HexDecode(#[from] HexParseError),
    #[error("roll hash must decode to 32 bytes")]
    RollHashLength,
    #[error("found=true but decision_roll missing")]
    MissingDecisionRollWire,
    #[error("found=true but decision_roll.roll_hash missing")]
    MissingRollHash,
    #[error("decision roll entry ballot {got} != expected {expected}")]
    RollEntryBallotMismatch { expected: u64, got: u64 },
    #[error("decision roll entry missing vote data")]
    MissingRollVoteHex,
    #[error("vote framing ballot {framed} != roll entry {entry}")]
    VoteBallotMismatch { entry: u64, framed: u64 },
    #[error("roll handler rejected: {0}")]
    HandlerDenied(ShortDiag),
}

impl From<char_utils::UtilsError> for ReconcileError {
    fn from(e: char_utils::UtilsError) -> Self {
        match e {
            char_utils::UtilsError::HexParse(h) => ReconcileError::HexDecode(h),
            char_utils::UtilsError::EmptyPreimage | char_utils::UtilsError::PreimageBytesEmpty => {
                ReconcileError::HexDecode(HexParseError::Empty)
            }
        }
    }
}

/// Semantics layer errors; can wrap transport, domain, progress, framing, utils.
#[derive(Debug, Error)]
pub enum SemanticsError {
    #[error("transport: {0}")]
    Transport(#[from] TransportError),

    #[error("domain: {0}")]
    Domain(#[from] DomainError),

    #[error("progress: {0}")]
    Progress(#[from] super::progress::ProgressError),

    #[error("framing: {0}")]
    Framing(#[from] FramingError),

    #[error("utils: {0}")]
    Utils(#[from] char_utils::UtilsError),

    #[error("gap: {0}")]
    Gap(GapReason),

    #[error("leader check: {0}")]
    LeaderCheck(#[from] LeaderCheckError),

    #[error("pending ballot: {0}")]
    PendingBallot(#[from] PendingBallotError),

    #[error("cursor: {0}")]
    Cursor(#[from] CursorError),

    #[error("submit rejected: {0}")]
    SubmitRejected(RejectReason),

    #[error("reconcile: {0}")]
    Reconcile(#[from] ReconcileError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::streaming::GapReason;

    #[test]
    fn semantics_error_display() {
        let e = ReconcileError::MissingDecisionRollWire;
        let se = SemanticsError::from(e);
        assert!(se.to_string().contains("reconcile"));
        assert!(se.to_string().contains("decision_roll"));
    }

    #[test]
    fn semantics_error_from_transport() {
        let te = TransportError::Timeout;
        let se: SemanticsError = te.into();
        assert!(matches!(se, SemanticsError::Transport(_)));
    }

    #[test]
    fn semantics_error_gap() {
        let e = SemanticsError::Gap(GapReason::MissingBallot { expected: 3 });
        assert!(e.to_string().contains("gap"));
    }

    #[test]
    fn semantics_error_cursor() {
        let e = SemanticsError::from(CursorError::AdvanceDenied(ShortDiag::truncate("denied")));
        assert!(matches!(e, SemanticsError::Cursor(_)));
        assert!(e.to_string().contains("cursor"));
    }
}
