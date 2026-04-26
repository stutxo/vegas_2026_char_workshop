// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! Real ZMQ subscriber: connects to a Char node ZMQ endpoint and implements [ZmqSubscriber].
//! Enabled with the `zmq` feature. Bitcoin Core / Char multipart format: (topic, body, 4-byte LE sequence).

use crate::error::{TransportError, ZmqMultipartFormatError};
use crate::zmq::{ZmqMessage, ZmqSubscriber};
use std::io;
use tokio::sync::mpsc;

type ZmqItem = Result<ZmqMessage, TransportError>;

/// Subscriber that receives ZMQ messages from a background thread over a channel.
/// Create with [ZmqSubSocket::subscribe].
#[derive(Debug)]
pub struct ZmqSubSocket {
    topic: String,
    rx: mpsc::UnboundedReceiver<ZmqItem>,
}

impl ZmqSubSocket {
    /// Connect to `addr` (e.g. `tcp://127.0.0.1:28332`), subscribe to `topic` (e.g. `"leader"` or `"decisionroll"`),
    /// and return a subscriber that implements [ZmqSubscriber]. The ZMQ socket runs in a blocking thread;
    /// when this value is dropped, the thread will exit when the channel is closed.
    pub fn subscribe(addr: impl Into<String>, topic: impl AsRef<str>) -> Self {
        let addr = addr.into();
        let topic_str = topic.as_ref().to_string();
        let topic_subscribe = topic_str.clone();
        let (tx, rx) = mpsc::unbounded_channel::<ZmqItem>();
        std::thread::spawn(move || {
            let ctx = zmq::Context::new();
            let socket = match ctx.socket(zmq::SUB) {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.send(Err(TransportError::Network(Box::new(e))));
                    return;
                }
            };
            if let Err(e) = socket.connect(&addr) {
                let _ = tx.send(Err(TransportError::Network(Box::new(io::Error::new(
                    io::ErrorKind::ConnectionRefused,
                    format!("zmq connect ({addr}): {e}"),
                )))));
                return;
            }
            if let Err(e) = socket.set_subscribe(topic_subscribe.as_bytes()) {
                let _ = tx.send(Err(TransportError::Network(Box::new(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("zmq subscribe ({}): {e}", topic_subscribe),
                )))));
                return;
            }
            loop {
                let frames: Vec<Vec<u8>> = match socket.recv_multipart(0) {
                    Ok(f) => f,
                    Err(e) => {
                        let _ = tx.send(Err(TransportError::Network(Box::new(e))));
                        return;
                    }
                };
                let [topic, body, seq_frame] = frames.as_slice() else {
                    let _ = tx.send(Err(TransportError::ZmqMultipart(
                        ZmqMultipartFormatError::WrongFrameCount {
                            got: frames.len(),
                        },
                    )));
                    continue;
                };
                if seq_frame.len() != 4 {
                    let _ = tx.send(Err(TransportError::ZmqMultipart(
                        ZmqMultipartFormatError::SequenceFrameNotFourBytes {
                            got_len: seq_frame.len(),
                        },
                    )));
                    continue;
                }
                let mut sequence = [0u8; 4];
                sequence.copy_from_slice(seq_frame.as_slice());
                let msg = ZmqMessage {
                    topic: topic.clone(),
                    body: body.clone(),
                    sequence,
                };
                if tx.send(Ok(msg)).is_err() {
                    return;
                }
            }
        });
        Self { topic: topic_str, rx }
    }
}

#[async_trait::async_trait]
impl ZmqSubscriber for ZmqSubSocket {
    fn topic(&self) -> &str {
        &self.topic
    }

    async fn recv(&mut self) -> Result<ZmqMessage, TransportError> {
        match self.rx.recv().await {
            Some(Ok(msg)) => Ok(msg),
            Some(Err(e)) => Err(e),
            None => Err(TransportError::Network(Box::new(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "zmq receiver closed",
            )))),
        }
    }
}
