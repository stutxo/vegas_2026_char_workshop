// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! Retry classification: retryable vs terminal.

use crate::error::SemanticsError;
use char_transport::TransportError;

// JSON-RPC 2.0 reserved errors (same codes over HTTP or any JSON-RPC transport).
// https://www.jsonrpc.org/specification#error_object
const JSON_RPC_INVALID_REQUEST: i32 = -32600;
const JSON_RPC_METHOD_NOT_FOUND: i32 = -32601;

#[inline]
fn is_terminal_json_rpc_client_error(code: i32) -> bool {
    matches!(code, JSON_RPC_INVALID_REQUEST | JSON_RPC_METHOD_NOT_FOUND)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryClass {
    Retryable,
    Terminal,
}

/// Classify transport errors for retry policy.
pub fn classify_transport_error(e: &TransportError) -> RetryClass {
    use char_transport::TransportError as TE;
    match e {
        TE::Timeout => RetryClass::Retryable,
        TE::Network(_) => RetryClass::Retryable,
        TE::Deserialization(_) => RetryClass::Terminal,
        TE::Rpc { code, .. } => {
            if is_terminal_json_rpc_client_error(*code) {
                RetryClass::Terminal
            } else {
                RetryClass::Retryable
            }
        }
        TE::ZmqMultipart(_) => RetryClass::Terminal,
    }
}

/// Classify semantics errors for retry policy.
pub fn classify_semantics_error(e: &SemanticsError) -> RetryClass {
    use crate::error::SemanticsError as SE;
    match e {
        SE::Transport(te) => classify_transport_error(te),
        SE::Domain(_) | SE::Framing(_) | SE::Utils(_) => RetryClass::Terminal,
        SE::Progress(_) => RetryClass::Terminal,
        SE::Gap(_) => RetryClass::Terminal,
        SE::LeaderCheck(_) => RetryClass::Terminal,
        SE::PendingBallot(_) => RetryClass::Terminal,
        SE::Cursor(_) => RetryClass::Terminal,
        SE::SubmitRejected(_) => RetryClass::Terminal,
        SE::Reconcile(_) => RetryClass::Retryable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{CursorError, LeaderCheckError, PendingBallotError, ReconcileError};
    use crate::progress::ProgressError;
    use crate::streaming::GapReason;
    use crate::submit::RejectReason;
    use char_framing::FramingError;
    use char_transport::ZmqMultipartFormatError;
    use char_utils::{HexParseError, ShortDiag};

    // --- classify_transport_error (full matrix) ---

    #[test]
    fn transport_timeout_is_retryable() {
        assert_eq!(
            classify_transport_error(&TransportError::Timeout),
            RetryClass::Retryable
        );
    }

    #[test]
    fn transport_network_is_retryable() {
        let e: Box<dyn std::error::Error + Send + Sync> =
            Box::new(std::io::Error::from(std::io::ErrorKind::ConnectionRefused));
        assert_eq!(
            classify_transport_error(&TransportError::Network(e)),
            RetryClass::Retryable
        );
    }

    #[test]
    fn transport_deserialization_is_terminal() {
        assert_eq!(
            classify_transport_error(&TransportError::Deserialization(ShortDiag::truncate("bad"))),
            RetryClass::Terminal
        );
    }

    #[test]
    fn transport_rpc_invalid_request_and_method_not_found_are_terminal() {
        for code in [JSON_RPC_INVALID_REQUEST, JSON_RPC_METHOD_NOT_FOUND] {
            assert_eq!(
                classify_transport_error(&TransportError::Rpc {
                    code,
                    message: ShortDiag::truncate("x"),
                }),
                RetryClass::Terminal
            );
        }
    }

    #[test]
    fn transport_rpc_other_codes_are_retryable() {
        for code in [-32603i32, -32000, 0, 1] {
            assert_eq!(
                classify_transport_error(&TransportError::Rpc {
                    code,
                    message: ShortDiag::truncate("x"),
                }),
                RetryClass::Retryable
            );
        }
    }

    #[test]
    fn transport_zmq_multipart_variants_are_terminal() {
        assert_eq!(
            classify_transport_error(&TransportError::ZmqMultipart(
                ZmqMultipartFormatError::WrongFrameCount { got: 2 }
            )),
            RetryClass::Terminal
        );
        assert_eq!(
            classify_transport_error(&TransportError::ZmqMultipart(
                ZmqMultipartFormatError::SequenceFrameNotFourBytes { got_len: 8 }
            )),
            RetryClass::Terminal
        );
    }

    // --- classify_semantics_error (every variant) ---

    #[test]
    fn semantics_transport_delegates_to_transport_classifier() {
        let e: SemanticsError = TransportError::Timeout.into();
        assert_eq!(classify_semantics_error(&e), RetryClass::Retryable);
    }

    #[test]
    fn semantics_domain_framing_utils_progress_gap_leader_pending_and_cursor_are_terminal() {
        assert_eq!(
            classify_semantics_error(&SemanticsError::Domain(
                crate::domain::DomainError::EmptyPreimage
            )),
            RetryClass::Terminal
        );
        assert_eq!(
            classify_semantics_error(&SemanticsError::Framing(FramingError::ConsensusDecode)),
            RetryClass::Terminal
        );
        assert_eq!(
            classify_semantics_error(&SemanticsError::Utils(
                char_utils::UtilsError::EmptyPreimage
            )),
            RetryClass::Terminal
        );
        assert_eq!(
            classify_semantics_error(&SemanticsError::Progress(ProgressError::Overflow)),
            RetryClass::Terminal
        );
        assert_eq!(
            classify_semantics_error(&SemanticsError::Gap(GapReason::MissingBallot {
                expected: 1
            })),
            RetryClass::Terminal
        );
        assert_eq!(
            classify_semantics_error(&SemanticsError::LeaderCheck(LeaderCheckError::NoEntry)),
            RetryClass::Terminal
        );
        assert_eq!(
            classify_semantics_error(&SemanticsError::PendingBallot(
                PendingBallotError::BondNotNextLeader
            )),
            RetryClass::Terminal
        );
        assert_eq!(
            classify_semantics_error(&SemanticsError::Cursor(CursorError::LoadDenied(
                ShortDiag::truncate("cursor")
            ))),
            RetryClass::Terminal
        );
    }

    #[test]
    fn semantics_submit_rejected_all_reasons_are_terminal() {
        for reason in [
            RejectReason::NotLeader,
            RejectReason::InvalidVoteFormat,
            RejectReason::VoteNotAccepted,
        ] {
            assert_eq!(
                classify_semantics_error(&SemanticsError::SubmitRejected(reason)),
                RetryClass::Terminal
            );
        }
    }

    #[test]
    fn semantics_reconcile_is_retryable() {
        let cases = [
            ReconcileError::MissingDecisionRollWire,
            ReconcileError::MissingRollHash,
            ReconcileError::RollEntryBallotMismatch {
                expected: 1,
                got: 2,
            },
            ReconcileError::MissingRollVoteHex,
            ReconcileError::RollHashLength,
            ReconcileError::VoteBallotMismatch {
                entry: 1,
                framed: 2,
            },
            ReconcileError::HexDecode(HexParseError::OddLength),
            ReconcileError::HandlerDenied(ShortDiag::truncate("denied")),
        ];
        for inner in cases {
            assert_eq!(
                classify_semantics_error(&SemanticsError::Reconcile(inner)),
                RetryClass::Retryable
            );
        }
    }
}
