// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! ZMQ path: SDK [`char_sdk::run_zmq`] wired to the example [`crate::HelloCharApp`].

use char_sdk::{CharBallotHandlers, CharReconcileCursor, CharRpcTransport};

pub async fn run_zmq<D: CharBallotHandlers + CharReconcileCursor>(
    transport: &impl CharRpcTransport,
    domain_hex: &str,
    app: &mut D,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    char_sdk::run_zmq(transport, domain_hex, app)
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
}
