// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! JSON-RPC 1.0 over HTTP with cookie auth (Bitcoin Core / Char).
//!
//! Parses the response body **even when HTTP status is not 2xx**, so JSON-RPC `error.code` /
//! `error.message` surface as [`TransportError::Rpc`] instead of a useless `"Internal Server Error"`.

use crate::error::TransportError;
use crate::rpc::{
    AddReferendumVoteMode, AttestationForBondBallot, BondInfo, CharRpcTransport, DecisionRollEntry,
    DecisionRollVerbosity, DomainInfo, DomainRegistryScheduleResult, KeyRange, LeaderSlotEntry,
};
use bitcoin::Txid;
use char_utils::ShortDiag;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::io::{self, Read};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

const DEFAULT_TIMEOUT_SECS: u64 = 30;
const MAX_RETRIES: u8 = 3;
const RETRY_INTERVAL_MS: u64 = 1_000;

#[derive(Deserialize)]
struct RpcErrorWire {
    code: i32,
    message: String,
}

#[derive(Deserialize)]
struct RpcResponseWire {
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<RpcErrorWire>,
    #[serde(default)]
    #[allow(dead_code)]
    id: Value,
}

fn read_cookie_user_pass(path: &Path) -> io::Result<(String, String)> {
    let mut s = String::new();
    std::fs::File::open(path)?.read_to_string(&mut s)?;
    let line = s.lines().next().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "empty cookie file")
    })?;
    let colon = line
        .find(':')
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "cookie missing ':'"))?;
    Ok((line[..colon].to_string(), line[colon + 1..].to_string()))
}

fn retryable_reqwest(e: &reqwest::Error) -> bool {
    e.is_timeout() || e.is_connect()
}

fn snippet(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s.chars().take(max).collect::<String>())
    }
}

/// Transport: HTTP JSON-RPC + `.cookie` Basic auth.
#[derive(Clone)]
pub struct BitcoindAsyncTransport {
    http: reqwest::Client,
    url: String,
    user: String,
    pass: String,
    id: Arc<AtomicU64>,
}

impl BitcoindAsyncTransport {
    /// Create from RPC URL and explicit Basic auth credentials.
    pub fn from_user_pass(
        rpc_url: &str,
        user: &str,
        pass: &str,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
        Ok(Self {
            http,
            url: rpc_url.trim_end_matches('/').to_string(),
            user: user.to_string(),
            pass: pass.to_string(),
            id: Arc::new(AtomicU64::new(0)),
        })
    }

    /// Create from RPC URL and cookie file path (one line `USER:PASS`).
    pub fn from_cookie_file<P: AsRef<Path>>(
        rpc_url: &str,
        cookie_path: P,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let (user, pass) = read_cookie_user_pass(cookie_path.as_ref())
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
        Self::from_user_pass(rpc_url, &user, &pass)
    }

    /// Create from RPC URL and datadir; looks for `.cookie` in `datadir` or `datadir/regtest`.
    pub fn from_datadir<P: AsRef<Path>>(
        rpc_url: &str,
        datadir: P,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let datadir = datadir.as_ref();
        let direct = datadir.join(".cookie");
        if direct.exists() {
            return Self::from_cookie_file(rpc_url, direct);
        }
        let regtest = datadir.join("regtest").join(".cookie");
        if regtest.exists() {
            return Self::from_cookie_file(rpc_url, regtest);
        }
        Err(Box::new(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "no .cookie in {} or {}/regtest",
                datadir.display(),
                datadir.display()
            ),
        )))
    }

    fn next_id(&self) -> u64 {
        self.id.fetch_add(1, Ordering::AcqRel)
    }

    async fn call_raw(&self, method: &str, params: &[Value]) -> Result<Value, TransportError> {
        let body = serde_json::json!({
            "jsonrpc": "1.0",
            "id": self.next_id(),
            "method": method,
            "params": params,
        });
        let body_str = serde_json::to_string(&body).map_err(|e| {
            TransportError::Deserialization(ShortDiag::truncate(&e.to_string()))
        })?;

        let mut attempt = 0u8;
        loop {
            let resp = match self
                .http
                .post(&self.url)
                .header("Content-Type", "application/json")
                .basic_auth(&self.user, Some(&self.pass))
                .body(body_str.clone())
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    attempt += 1;
                    if attempt < MAX_RETRIES && retryable_reqwest(&e) {
                        tokio::time::sleep(Duration::from_millis(RETRY_INTERVAL_MS)).await;
                        continue;
                    }
                    return Err(TransportError::Network(Box::new(e)));
                }
            };

            let status = resp.status();
            let text = resp
                .text()
                .await
                .map_err(|e| TransportError::Network(Box::new(e)))?;

            let parsed: RpcResponseWire = match serde_json::from_str(&text) {
                Ok(p) => p,
                Err(e) => {
                    return Err(TransportError::Deserialization(ShortDiag::truncate(&format!(
                        "http {} json parse: {}; body {}",
                        status.as_u16(),
                        e,
                        snippet(&text, 200)
                    ))));
                }
            };

            if let Some(err) = parsed.error {
                return Err(TransportError::Rpc {
                    code: err.code,
                    message: ShortDiag::truncate(&err.message),
                });
            }

            return parsed.result.ok_or_else(|| {
                TransportError::Deserialization(ShortDiag::truncate(
                    "rpc response missing result and error",
                ))
            });
        }
    }

    fn to_value<T: serde::Serialize>(v: T) -> Result<Value, TransportError> {
        serde_json::to_value(v).map_err(|e| {
            TransportError::Deserialization(ShortDiag::truncate(&e.to_string()))
        })
    }
}

#[async_trait::async_trait]
impl CharRpcTransport for BitcoindAsyncTransport {
    async fn get_domain_info(&self, domain_preimage_hex: &str) -> Result<DomainInfo, TransportError> {
        let params = [Self::to_value(domain_preimage_hex)?];
        let v = self.call_raw("getdomaininfo", &params).await?;
        serde_json::from_value(v)
            .map_err(|e| TransportError::Deserialization(ShortDiag::truncate(&e.to_string())))
    }

    async fn get_referendum_decision_roll(
        &self,
        domain_preimage_hex: &str,
        start_ballot: u64,
        end_ballot: u64,
        verbosity: DecisionRollVerbosity,
    ) -> Result<Vec<DecisionRollEntry>, TransportError> {
        let params = [
            Self::to_value(domain_preimage_hex)?,
            Self::to_value(start_ballot)?,
            Self::to_value(end_ballot)?,
            Self::to_value(u8::from(verbosity))?,
        ];
        let v = self.call_raw("getreferendumresolution", &params).await?;
        serde_json::from_value(v)
            .map_err(|e| TransportError::Deserialization(ShortDiag::truncate(&e.to_string())))
    }

    async fn add_referendum_vote(
        &self,
        votes: &[(String, String)],
        mode: Option<AddReferendumVoteMode>,
    ) -> Result<HashMap<String, bool>, TransportError> {
        let referendumvote: Vec<Value> = votes
            .iter()
            .map(|(k, v)| {
                let mut obj = serde_json::Map::new();
                obj.insert(k.clone(), Value::String(v.clone()));
                Value::Object(obj)
            })
            .collect();
        let mode_str = match mode.unwrap_or_default() {
            AddReferendumVoteMode::IsLeader => "is_leader",
            AddReferendumVoteMode::Init => "init",
            AddReferendumVoteMode::PlzFind => "plzfind",
        };
        let params = [Self::to_value(referendumvote)?, Self::to_value(mode_str)?];
        let v = self.call_raw("addreferendumvote", &params).await?;
        let map = v.as_object().ok_or_else(|| {
            TransportError::Deserialization(ShortDiag::truncate(
                "addreferendumvote result not object",
            ))
        })?;
        let mut out = HashMap::new();
        for (k, val) in map {
            out.insert(k.clone(), val.as_bool().unwrap_or(false));
        }
        Ok(out)
    }

    async fn get_leader_for_slot_current_block(
        &self,
        key_ranges: &[KeyRange],
    ) -> Result<Vec<LeaderSlotEntry>, TransportError> {
        let key_ranges_json: Vec<Value> = key_ranges
            .iter()
            .map(|kr| {
                serde_json::json!({
                    "key": kr.key,
                    "start_ballot": kr.start_slot,
                    "end_ballot": kr.end_slot
                })
            })
            .collect();
        let params = [Self::to_value(key_ranges_json)?];
        let v = self
            .call_raw("get_leader_for_ballot_current_block", &params)
            .await?;
        serde_json::from_value(v)
            .map_err(|e| TransportError::Deserialization(ShortDiag::truncate(&e.to_string())))
    }

    async fn get_all_char_bonds(&self, verbosity: u8) -> Result<Vec<BondInfo>, TransportError> {
        let params = [Self::to_value(verbosity)?];
        let v = self.call_raw("getallcharbonds", &params).await?;
        serde_json::from_value(v)
            .map_err(|e| TransportError::Deserialization(ShortDiag::truncate(&e.to_string())))
    }

    async fn get_attestation_for_bond_at_ballot(
        &self,
        bond_id: &Txid,
        ballot_number: u64,
    ) -> Result<AttestationForBondBallot, TransportError> {
        let params = [Self::to_value(bond_id.to_string())?, Self::to_value(ballot_number)?];
        let v = self
            .call_raw("getattestationforbondatballot", &params)
            .await?;
        serde_json::from_value(v)
            .map_err(|e| TransportError::Deserialization(ShortDiag::truncate(&e.to_string())))
    }

    async fn domain_registry_schedule(
        &self,
        domain_preimage_hex: &str,
        info: &str,
    ) -> Result<DomainRegistryScheduleResult, TransportError> {
        let params = [
            Self::to_value("schedule")?,
            Self::to_value(domain_preimage_hex)?,
            Self::to_value(info)?,
        ];
        let v = self.call_raw("domain_registry", &params).await?;
        serde_json::from_value(v)
            .map_err(|e| TransportError::Deserialization(ShortDiag::truncate(&e.to_string())))
    }
}
