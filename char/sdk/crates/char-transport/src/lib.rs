// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! Char SDK - typed RPC client and ZMQ primitives only. No protocol logic.

mod error;
mod rpc;
mod zmq;

#[cfg(feature = "zmq")]
mod zmq_socket;

pub use error::{TransportError, ZmqMultipartFormatError};
pub use rpc::{
    AddReferendumVoteMode, AttestationEntryWire, AttestationForBondBallot, BondAttestationsInfo,
    BondInfo, CharRpcTransport, DecisionRollEntry, DecisionRollVerbosity, DecisionRollWire,
    DomainInfo, DomainRegistryScheduleResult, KeyRange, LeaderSlotEntry, SlotSelection,
};
/// Mock transport for tests. Used by char-semantics and char-sdk tests.
pub use rpc::mock::MockTransport;
#[cfg(feature = "bitcoind-client")]
pub use rpc::bitcoind_async::BitcoindAsyncTransport;
pub use zmq::{ZmqAddress, ZmqMessage, ZmqSubscriber};

#[cfg(feature = "zmq")]
pub use zmq_socket::ZmqSubSocket;
