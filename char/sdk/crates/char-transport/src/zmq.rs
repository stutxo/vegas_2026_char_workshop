// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! ZMQ subscription types. No zeromq dependency in this crate; implementors provide the socket.
//!
//! **Bitcoin Core sequencing:** Core sends each ZMQ message as multipart:
//! `(topic, body, sequence)` where `sequence` is a 4-byte little-endian uint32 that
//! increments per message per topic. Implementors should populate `sequence` from
//! the third frame when connecting to a Core node so the semantics layer can
//! detect gaps via sequence discontinuity.

use crate::error::TransportError;

/// Address for ZMQ connection (e.g. "tcp://127.0.0.1:28332" or "ipc://...").
pub type ZmqAddress = String;

/// One received message: topic, body, sequence (4 bytes LE, Bitcoin Core `SendZmqMessage` format).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZmqMessage {
    pub topic: Vec<u8>,
    pub body: Vec<u8>,
    pub sequence: [u8; 4],
}

/// Subscriber that delivers raw multipart messages. Semantics layer parses body.
#[async_trait::async_trait]
pub trait ZmqSubscriber: Send + Sync {
    /// Topic name this subscriber is subscribed to (e.g. "leader", "decisionroll").
    fn topic(&self) -> &str;
    /// Receive the next message. Blocks until one is available or error.
    async fn recv(&mut self) -> Result<ZmqMessage, TransportError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zmq_message_construct() {
        let m = ZmqMessage {
            topic: b"decisionroll".to_vec(),
            body: vec![0u8; 33],
            sequence: [1, 2, 3, 4],
        };
        assert_eq!(m.topic.len(), 12);
        assert_eq!(m.sequence, [1, 2, 3, 4]);
    }
}
