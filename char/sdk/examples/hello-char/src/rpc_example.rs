// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! RPC path: SDK [`char_sdk::run_rpc`] wired to the example [`crate::HelloCharApp`].

use char_sdk::{CharBallotHandlers, CharRpcTransport, DomainId, RpcPollConfig};

pub async fn run_rpc_loop<D: CharBallotHandlers>(
    transport: &impl CharRpcTransport,
    domain_hex: &str,
    domain_id: DomainId,
    app: &mut D,
    timing: RpcPollConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    char_sdk::run_rpc(transport, domain_hex, domain_id, app, timing)
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
}
