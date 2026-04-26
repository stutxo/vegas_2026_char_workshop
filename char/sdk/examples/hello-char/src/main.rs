// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! Hello Char - minimal example using the Char SDK + transport.
//! RPC or ZMQ path selected by `--zmq` / `CHAR_USE_ZMQ`; each path lives in its own file.
//!
//! [`HelloCharApp`] implements [`CharBallotHandlers`]: your ballot payload + roll verification logic.
//!
//! **Payload when `decision_roll.data` is missing:** `char-semantics` falls back to
//! `getattestationforbondatballot` (leader bond first, then all bonds from `getallcharbonds(1)`); see
//! `try_payload_via_attestation_chain` in `char-semantics/src/leader.rs`.

mod rpc_example;
mod zmq_example;

use char_sdk::*;
use std::collections::HashSet;
use std::error::Error as StdError;
use std::path::PathBuf;
use std::time::Duration;

const COOKIE_ERR: &str = "Cookie file not found. Set CHAR_COOKIE_PATH or BITCOIND_COOKIE to your node's .cookie (e.g. char_data/regtest/.cookie).";

/// Example integrator: fixed UTF-8 payload per ballot (`hello 0`, `hello 1`, …).
///
/// [`CharBallotHandlers::produce_payload`] submits that payload when **this** wallet is leader.
/// [`CharBallotHandlers::on_roll_observed`] only **strictly** checks rolls for ballots where we recorded
/// a successful leader submit ([`CharBallotHandlers::on_leader_submit_accepted`]); other bonds can win
/// earlier ballots with different payload bytes — that is normal Char behavior, not a bug.
pub struct HelloCharApp {
    /// Ballots for which `addreferendumvote` succeeded in this process (leader path).
    leader_submitted: HashSet<u64>,
    next_ballot: u64,
}

impl HelloCharApp {
    pub fn new() -> Self {
        Self {
            leader_submitted: HashSet::new(),
            next_ballot: 0,
        }
    }

    /// `run-regtest.sh` calls `addreferendumvote` for ballot **0** before this process starts, so the
    /// SDK never fires [`CharBallotHandlers::on_leader_submit_accepted`] for 0. Call this so ballot 0
    /// is still strict-checked against `hello 0` when you use that script (the script must seed the
    /// same UTF-8 bytes as `format!("hello {}", 0)` (hex `68656c6c6f2030` in `run-regtest.sh`).
    pub fn trust_script_seeded_ballot_zero(&mut self) {
        self.leader_submitted.insert(0);
    }

    fn expected_payload(ballot: u64) -> Vec<u8> {
        format!("hello {}", ballot).into_bytes()
    }
}

#[async_trait]
impl CharBallotHandlers for HelloCharApp {
    // Write your ballot payload for your app here.
    // 'produce_payload' is called by the SDK when it needs to produce a payload for a given ballot.
    // it's the one of two functions you need to implement the CharBallotHandlers trait for either RPC or ZMQ.
    async fn produce_payload(&mut self, ballot: u64) -> Vec<u8> {
        Self::expected_payload(ballot)
    }

    async fn on_leader_submit_accepted(&mut self, ballot: u64) {
        self.leader_submitted.insert(ballot);
    }

    // Read the roll for a given ballot here.
    // 'on_roll_observed' is called by the SDK when it has observed a decision roll for a given ballot.
    // it's the other function you need to implement the CharBallotHandlers trait for either RPC or ZMQ.
    async fn on_roll_observed(
        &mut self,
        roll: ObservedRoll,
    ) -> Result<(), Box<dyn StdError + Send + Sync>> {
        // Always print what finalized on-chain so demos show `hello N` even when this process
        // did not submit that ballot (e.g. older ballots after `latest_decided_ballot + 1` cursor).
        let ballot = roll.ballot;
        let payload = roll.payload.as_slice();
        let as_utf8 = String::from_utf8_lossy(payload);
        println!("[hello-char] ballot {ballot} payload on chain: {as_utf8}");

        if !self.leader_submitted.contains(&ballot) {
            // Another bond was leader / different vote won; not our payload to assert on.
            return Ok(());
        }
        let expected = Self::expected_payload(ballot);
        if payload != expected.as_slice() {
            return Err(format!(
                "decision roll payload mismatch for ballot {}: expected {:?} got {:?}",
                ballot, expected, payload
            )
            .into());
        }
        println!("[hello-char] ballot {ballot}: matches our leader submit (hello {ballot})");
        Ok(())
    }
}

#[async_trait]
impl CharReconcileCursor for HelloCharApp {
    async fn next_ballot(&mut self) -> Result<u64, Box<dyn StdError + Send + Sync>> {
        Ok(self.next_ballot)
    }

    async fn advance_cursor(
        &mut self,
        next_ballot: u64,
    ) -> Result<(), Box<dyn StdError + Send + Sync>> {
        self.next_ballot = next_ballot;
        Ok(())
    }
}

fn env_rpc_url() -> Result<String, std::io::Error> {
    std::env::var("CHAR_RPC_URL")
        .or_else(|_| std::env::var("BITCOIND_URL"))
        .map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "CHAR_RPC_URL or BITCOIND_URL required",
            )
        })
}

fn env_cookie_path() -> Result<String, std::io::Error> {
    std::env::var("CHAR_COOKIE_PATH")
        .or_else(|_| std::env::var("BITCOIND_COOKIE"))
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, COOKIE_ERR))
}

fn env_domain_hex() -> String {
    std::env::var("CHAR_DOMAIN_HEX")
        .or_else(|_| std::env::var("DOMAIN_PREIMAGE_HEX"))
        .unwrap_or_else(|_| "636861722e6e6574776f726b2f68656c6c6f".into())
}

fn env_submit_max_retries() -> u32 {
    std::env::var("CHAR_SUBMIT_MAX_RETRIES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3)
        .min(50)
}

fn env_submit_initial_backoff_ms() -> Duration {
    let ms: u64 = std::env::var("CHAR_SUBMIT_INITIAL_BACKOFF_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100)
        .clamp(1, 3_600_000);
    Duration::from_millis(ms)
}

fn env_submit_max_backoff_ms() -> Duration {
    let ms: u64 = std::env::var("CHAR_SUBMIT_MAX_BACKOFF_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1_000)
        .clamp(1, 3_600_000);
    Duration::from_millis(ms)
}

fn submit_retry_from_env() -> SubmitRetryConfig {
    SubmitRetryConfig {
        max_retries: env_submit_max_retries(),
        initial_backoff: env_submit_initial_backoff_ms(),
        max_backoff: env_submit_max_backoff_ms(),
    }
}

fn env_run_rpc_poll_delay() -> Duration {
    let secs: u64 = std::env::var("CHAR_RUN_RPC_POLL_DELAY_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3)
        .max(1);
    Duration::from_secs(secs)
}

fn env_run_rpc_roll_poll_interval() -> Duration {
    let ms: u64 = std::env::var("CHAR_RUN_RPC_ROLL_POLL_INTERVAL_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(200)
        .clamp(1, 3_600_000);
    Duration::from_millis(ms)
}

fn env_run_rpc_roll_timeout() -> Duration {
    let secs: u64 = std::env::var("CHAR_RUN_RPC_ROLL_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5)
        .max(1);
    Duration::from_secs(secs)
}

fn rpc_poll_from_env() -> RpcPollConfig {
    RpcPollConfig {
        poll_delay: env_run_rpc_poll_delay(),
        roll_poll_interval: env_run_rpc_roll_poll_interval(),
        roll_timeout: env_run_rpc_roll_timeout(),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let url = env_rpc_url()?;
    let cookie_path = env_cookie_path()?;
    let path = PathBuf::from(&cookie_path);
    if !path.exists() {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, COOKIE_ERR).into());
    }
    let transport = BitcoindAsyncTransport::from_cookie_file(&url, &path)?;
    let domain_hex = env_domain_hex();
    let domain_id = DomainId::from_preimage_hex(&domain_hex)?;
    let _ = transport
        .domain_registry_schedule(&domain_hex, "hello-char")
        .await;
    let submit_retry = submit_retry_from_env().normalized();

    let use_zmq = std::env::args().any(|a| a == "--zmq")
        || std::env::var("CHAR_USE_ZMQ")
            .map(|v| v != "0")
            .unwrap_or(false);

    let mut app = HelloCharApp::new();
    app.trust_script_seeded_ballot_zero();

    if use_zmq {
        eprintln!(
            "[hello-char] mode=zmq submit_retries={} backoff {:?}..{:?}; roll lines on stdout; CHAR_ZMQ_VERBOSE=1 for ZMQ decode logs",
            submit_retry.max_retries, submit_retry.initial_backoff, submit_retry.max_backoff,
        );
        zmq_example::run_zmq(&transport, &domain_hex, &mut app).await
    } else {
        let rpc_poll = rpc_poll_from_env().normalized();
        eprintln!(
            "[hello-char] mode=rpc_poll poll_delay={:?} roll_poll={:?} roll_timeout={:?} submit_retries={} backoff {:?}..{:?}",
            rpc_poll.poll_delay,
            rpc_poll.roll_poll_interval,
            rpc_poll.roll_timeout,
            submit_retry.max_retries,
            submit_retry.initial_backoff,
            submit_retry.max_backoff,
        );
        rpc_example::run_rpc_loop(&transport, &domain_hex, domain_id, &mut app, rpc_poll).await
    }
}
