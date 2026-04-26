// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! Typed transport errors. No `anyhow` at library boundary.

use char_utils::ShortDiag;
use std::fmt;

/// ZMQ multipart`(topic, body, 4-byte LE sequence)` layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZmqMultipartFormatError {
    /// Core sends exactly three frames.
    WrongFrameCount { got: usize },
    /// Third frame must be `sizeof(uint32_t)` sequence bytes.
    SequenceFrameNotFourBytes { got_len: usize },
}

impl fmt::Display for ZmqMultipartFormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongFrameCount { got } => write!(
                f,
                "expected 3 ZMQ multipart frames (topic, body, sequence), got {got}"
            ),
            Self::SequenceFrameNotFourBytes { got_len } => write!(
                f,
                "expected 4-byte LE sequence frame, got {got_len} bytes"
            ),
        }
    }
}

impl std::error::Error for ZmqMultipartFormatError {}

/// Transport-layer errors: network, deserialization, RPC, timeout.
#[derive(Debug)]
pub enum TransportError {
    /// Network or I/O failure.
    Network(Box<dyn std::error::Error + Send + Sync>),
    /// Response could not be deserialized (bounded diagnostic only).
    Deserialization(ShortDiag),
    /// RPC returned an error object (`message` truncated for safety).
    Rpc {
        code: i32,
        message: ShortDiag,
    },
    /// Request timed out.
    Timeout,
    /// ZMQ multipart layout did not match Core's publisher format.
    ZmqMultipart(ZmqMultipartFormatError),
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportError::Network(e) => write!(f, "network: {e}"),
            TransportError::Deserialization(s) => write!(f, "deserialization: {s}"),
            TransportError::Rpc { code, message } => {
                write!(f, "rpc error code {code}: {message}")
            }
            TransportError::Timeout => f.write_str("timeout"),
            TransportError::ZmqMultipart(e) => write!(f, "zmq multipart: {e}"),
        }
    }
}

impl std::error::Error for TransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TransportError::Network(e) => Some(e.as_ref()),
            TransportError::ZmqMultipart(e) => Some(e),
            _ => None,
        }
    }
}

impl From<Box<dyn std::error::Error + Send + Sync>> for TransportError {
    fn from(e: Box<dyn std::error::Error + Send + Sync>) -> Self {
        TransportError::Network(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_error_display() {
        assert_eq!(TransportError::Timeout.to_string(), "timeout");
        assert!(
            TransportError::Deserialization(ShortDiag::truncate("bad"))
                .to_string()
                .contains("bad")
        );
        let e = TransportError::Rpc {
            code: -32600,
            message: ShortDiag::truncate("Invalid request"),
        };
        assert!(e.to_string().contains("-32600"));
        assert!(e.to_string().contains("Invalid request"));
    }

    #[test]
    fn transport_error_from_box() {
        let b: Box<dyn std::error::Error + Send + Sync> =
            Box::new(std::io::Error::from(std::io::ErrorKind::NotFound));
        let te = TransportError::from(b);
        assert!(matches!(te, TransportError::Network(_)));
    }

    #[test]
    fn zmq_multipart_format_display() {
        let e = ZmqMultipartFormatError::WrongFrameCount { got: 2 };
        assert!(e.to_string().contains("got 2"));
        let e = ZmqMultipartFormatError::SequenceFrameNotFourBytes { got_len: 8 };
        assert!(e.to_string().contains("8"));
    }
}
