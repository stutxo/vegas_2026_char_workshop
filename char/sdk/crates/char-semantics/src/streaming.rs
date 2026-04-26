// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! Decision roll streaming: events and gap detection.
//!
//! Uses Bitcoin Core's ZMQ sequencing when available: each message is sent as
//! multipart (topic, body, 4-byte LE sequence number). The sequence increments
//! per message per topic; we detect gaps via `GapReason::SequenceDiscontinuity`.
//!
//! Body format: domain (32 bytes) + **CompactSize** leaf tag + **CompactSize** ballot
//! + **CompactSize** payload length + payload bytes + serialized roll.

use crate::domain::DomainId;
use crate::error::SemanticsError;
use char_framing::read_compact_size_from_slice;
use char_transport::{ZmqMessage, ZmqSubscriber};
use thiserror::Error;

/// One event from the decision roll stream (ZMQ or RPC-driven).
#[derive(Debug, Clone)]
pub struct DecisionRollStreamEvent {
    pub domain: DomainId,
    pub ballot: u64,
    pub kind: DecisionRollEventKind,
}

/// Observed roll data or a gap/error that requires reset and reconcile.
#[derive(Debug, Clone)]
pub enum DecisionRollEventKind {
    /// Roll data received from ZMQ; payload is app-owned and not yet verified by the app.
    Observed {
        serialized: Vec<u8>,
        payload: Vec<u8>,
        tag: u8,
    },
    /// Gap or invalid sequence; caller must reconcile and reset.
    Gap(GapReason),
}

/// Reason for a gap in the stream.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum GapReason {
    #[error("duplicate ballot {ballot}")]
    DuplicateBallot { ballot: u64 },

    #[error("missing ballot {expected}")]
    MissingBallot { expected: u64 },

    #[error("sequence discontinuity: expected {expected}, got {got}")]
    SequenceDiscontinuity { expected: u64, got: u64 },

    #[error("parse error: {0}")]
    ParseError(DecisionRollParseError),
}

/// Finite classification for malformed ZMQ decisionroll bodies.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum DecisionRollParseError {
    #[error("decisionroll body too short (< 32 bytes)")]
    BodyTooShort,

    #[error("decisionroll domain slice invalid")]
    DomainSlice,

    #[error("decisionroll tag CompactSize")]
    TagCompactSize,

    #[error("decisionroll ballot CompactSize")]
    BallotCompactSize,

    #[error("decisionroll payload length CompactSize")]
    PayloadLengthCompactSize,

    #[error("decisionroll payload length overflows usize")]
    PayloadLengthOverflow,

    #[error("decisionroll payload truncated")]
    PayloadTruncated,
}

/// Decode Bitcoin Core ZMQ sequence from the 4-byte LE suffix.
/// Core sends: multipart (topic, body, 4-byte LE uint32 sequence).
#[inline]
pub fn zmq_sequence_from_core(seq: &[u8; 4]) -> u32 {
    u32::from_le_bytes(*seq)
}

/// Process the next ZMQ message with Bitcoin Core sequence logic.
///
/// Decodes `message.sequence` as 4-byte LE and checks against `expected_sequence`.
/// On mismatch returns `Err(Gap(SequenceDiscontinuity))`.
/// Updates `expected_sequence` to `got + 1` for the next call.
///
/// Body format (Bitcoin Core decisionroll): first 32 bytes = domain hash, then **CompactSize** leaf tag,
/// **CompactSize** ballot, **CompactSize** payload length, payload bytes, and serialized roll.
pub fn process_zmq_decision_roll_message(
    message: ZmqMessage,
    expected_sequence: &mut Option<u32>,
) -> Result<DecisionRollStreamEvent, SemanticsError> {
    let got = zmq_sequence_from_core(&message.sequence);
    if let Some(exp) = *expected_sequence {
        if got != exp {
            return Err(SemanticsError::Gap(GapReason::SequenceDiscontinuity {
                expected: exp as u64,
                got: got as u64,
            }));
        }
    } else {
        *expected_sequence = Some(got);
    }
    *expected_sequence = Some(got.wrapping_add(1));

    let body = message.body;
    if body.len() < 32 {
        return Err(SemanticsError::Gap(GapReason::ParseError(
            DecisionRollParseError::BodyTooShort,
        )));
    }
    let domain: [u8; 32] = body[0..32].try_into().map_err(|_| {
        SemanticsError::Gap(GapReason::ParseError(DecisionRollParseError::DomainSlice))
    })?;
    let domain = DomainId(domain);
    let (tag_val, tag_len) = read_compact_size_from_slice(&body[32..]).ok_or_else(|| {
        SemanticsError::Gap(GapReason::ParseError(
            DecisionRollParseError::TagCompactSize,
        ))
    })?;
    let tag = tag_val.min(255) as u8;
    let after_tag = 32 + tag_len;
    let (ballot, ballot_len) =
        read_compact_size_from_slice(&body[after_tag..]).ok_or_else(|| {
            SemanticsError::Gap(GapReason::ParseError(
                DecisionRollParseError::BallotCompactSize,
            ))
        })?;
    let after_ballot = after_tag + ballot_len;
    let (payload_len, payload_len_len) = read_compact_size_from_slice(&body[after_ballot..])
        .ok_or_else(|| {
            SemanticsError::Gap(GapReason::ParseError(
                DecisionRollParseError::PayloadLengthCompactSize,
            ))
        })?;
    let payload_len: usize = payload_len.try_into().map_err(|_| {
        SemanticsError::Gap(GapReason::ParseError(
            DecisionRollParseError::PayloadLengthOverflow,
        ))
    })?;
    let payload_start = after_ballot + payload_len_len;
    let payload_end = payload_start.checked_add(payload_len).ok_or_else(|| {
        SemanticsError::Gap(GapReason::ParseError(
            DecisionRollParseError::PayloadLengthOverflow,
        ))
    })?;
    if payload_end > body.len() {
        return Err(SemanticsError::Gap(GapReason::ParseError(
            DecisionRollParseError::PayloadTruncated,
        )));
    }
    let payload = body[payload_start..payload_end].to_vec();
    let serialized = body[payload_end..].to_vec();

    Ok(DecisionRollStreamEvent {
        domain,
        ballot,
        kind: DecisionRollEventKind::Observed {
            serialized,
            payload,
            tag,
        },
    })
}

/// Receive and process the next decision-roll ZMQ message with sequence checking.
///
/// Calls `subscriber.recv()`, then `process_zmq_decision_roll_message`. Transport errors
/// (e.g. disconnect) are mapped to `SemanticsError::Transport`.
pub async fn next_decision_roll_event<S: ZmqSubscriber + ?Sized>(
    subscriber: &mut S,
    expected_sequence: &mut Option<u32>,
) -> Result<DecisionRollStreamEvent, SemanticsError> {
    let message = subscriber.recv().await.map_err(SemanticsError::Transport)?;
    process_zmq_decision_roll_message(message, expected_sequence)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_event_observed() {
        let domain = DomainId::from_preimage(b"d");
        let ev = DecisionRollStreamEvent {
            domain,
            ballot: 1,
            kind: DecisionRollEventKind::Observed {
                serialized: vec![0, 1, 2],
                payload: vec![3, 4, 5],
                tag: 0,
            },
        };
        assert_eq!(ev.ballot, 1);
    }

    #[test]
    fn gap_reason_display() {
        let g = GapReason::MissingBallot { expected: 5 };
        assert!(g.to_string().contains("5"));
        let p = DecisionRollParseError::BodyTooShort;
        assert!(
            GapReason::ParseError(p.clone())
                .to_string()
                .contains("parse")
        );
    }

    #[test]
    fn zmq_sequence_from_core_le() {
        assert_eq!(zmq_sequence_from_core(&[1, 0, 0, 0]), 1);
        assert_eq!(zmq_sequence_from_core(&[0, 0, 0, 0]), 0);
        assert_eq!(zmq_sequence_from_core(&[0xff, 0xff, 0xff, 0xff]), u32::MAX);
    }

    #[test]
    fn process_zmq_message_sequence_discontinuity() {
        let mut expected = Some(5u32);
        let msg = ZmqMessage {
            topic: b"decisionroll".to_vec(),
            body: vec![0u8; 33],
            sequence: 7u32.to_le_bytes(),
        };
        let err = process_zmq_decision_roll_message(msg, &mut expected).unwrap_err();
        match &err {
            SemanticsError::Gap(GapReason::SequenceDiscontinuity {
                expected: e,
                got: g,
            }) => {
                assert_eq!(*e, 5);
                assert_eq!(*g, 7);
            }
            _ => panic!("expected SequenceDiscontinuity, got {:?}", err),
        }
    }

    #[test]
    fn process_zmq_message_sequence_ok() {
        let mut expected = Some(2u32);
        let mut body = vec![0u8; 32];
        body.push(1);
        body.push(7);
        body.push(3);
        body.extend_from_slice(&[4, 5, 6]);
        body.extend_from_slice(&[9, 8]);
        let msg = ZmqMessage {
            topic: b"decisionroll".to_vec(),
            body,
            sequence: 2u32.to_le_bytes(),
        };
        let ev = process_zmq_decision_roll_message(msg, &mut expected).unwrap();
        assert_eq!(expected, Some(3));
        assert_eq!(ev.ballot, 7);
        assert!(matches!(
            ev.kind,
            DecisionRollEventKind::Observed { tag: 1, .. }
        ));
    }
}
