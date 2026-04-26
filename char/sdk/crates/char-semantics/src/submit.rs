// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! Submit model: vote submit, read-after-write, reject reasons.
//!
//! **Submit retry backoff** (transport errors only) is passed explicitly via [`SubmitRetryConfig`] to
//! [`submit_vote`]. Integrators typically read `CHAR_SUBMIT_*` (or their own settings) in `main` and
//! build that struct there, consistent with other process configuration.

use crate::error::SemanticsError;
use crate::retry::{RetryClass, classify_transport_error};
use bitcoin::hashes::Hash as _;
use char_transport::{AddReferendumVoteMode, CharRpcTransport};
use char_utils::{bytes_to_hex, hex_to_bytes, strip_0x_prefix};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use thiserror::Error;

/// Backoff and retry limits for transport errors during submit and read-after-write RPCs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubmitRetryConfig {
    pub max_retries: u32,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
}

impl Default for SubmitRetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_millis(1_000),
        }
    }
}

impl SubmitRetryConfig {
    /// Clamp `max_retries` to at most 50 and ensure `initial_backoff` does not exceed `max_backoff`.
    pub fn normalized(self) -> Self {
        let max_backoff = self.max_backoff;
        let initial_backoff = self.initial_backoff.min(max_backoff);
        Self {
            max_retries: self.max_retries.min(50),
            initial_backoff,
            max_backoff,
        }
    }
}

static IDEMPOTENCY_RESULTS: OnceLock<Mutex<HashMap<[u8; 32], SubmitResult>>> = OnceLock::new();

fn idempotency_store() -> &'static Mutex<HashMap<[u8; 32], SubmitResult>> {
    IDEMPOTENCY_RESULTS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Clears the process-global idempotency cache. **Tests only** — avoids cross-test pollution.
#[cfg(test)]
fn reset_idempotency_store_for_tests() {
    idempotency_store()
        .lock()
        .expect("idempotency mutex poisoned")
        .clear();
}

fn derive_idempotency_key(request: &SubmitRequest) -> [u8; 32] {
    if let Some(key) = request.idempotency_key {
        return key;
    }
    let mut key_material = Vec::with_capacity(
        request.domain_preimage_hex.len()
            + request.payload.len()
            + std::mem::size_of::<u64>() * 2
            + 1,
    );
    key_material.extend_from_slice(request.domain_preimage_hex.as_bytes());
    key_material.push(0);
    key_material.extend_from_slice(&request.ballot.to_le_bytes());
    key_material.extend_from_slice(&(request.payload.len() as u64).to_le_bytes());
    key_material.extend_from_slice(&request.payload);
    char_utils::domain_hash(&key_material).to_byte_array()
}

fn next_backoff(current: Duration, max_backoff: Duration) -> Duration {
    let doubled = current.saturating_add(current);
    if doubled > max_backoff {
        max_backoff
    } else {
        doubled
    }
}

/// Result of a submit_vote call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmitResult {
    /// RPC accepted the vote; not yet verified by read-after-write.
    Submitted,
    /// RPC accepted and we observed/verified the outcome.
    VerifiedObserved,
    /// RPC rejected (e.g. not leader, invalid format).
    Rejected(RejectReason),
    /// Unknown (e.g. timeout before confirmation).
    Unknown,
}

/// Why the node rejected the vote.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum RejectReason {
    #[error("not leader")]
    NotLeader,

    #[error("invalid vote format")]
    InvalidVoteFormat,

    /// Node accepted the RPC call but reported `false` for this domain (vote not applied).
    #[error("referendum vote not accepted for domain")]
    VoteNotAccepted,
}

/// Request for submit_vote.
#[derive(Debug, Clone)]
pub struct SubmitRequest {
    pub domain_preimage_hex: String,
    pub ballot: u64,
    pub payload: Vec<u8>,
    pub idempotency_key: Option<[u8; 32]>,
    pub leader_verification: bool,
    pub read_after_write: ReadAfterWriteConfig,
}

/// When to poll for decision after submit.
#[derive(Debug, Clone)]
pub struct ReadAfterWriteConfig {
    pub enabled: bool,
    pub max_wait: Duration,
    pub poll_interval: Duration,
}

/// Submit a referendum vote. If leader_verification, checks leader first; sends **raw payload hex** per
/// RPC contract; optionally polls until decided.
pub async fn submit_vote(
    transport: &impl CharRpcTransport,
    request: SubmitRequest,
    bond_id_hex: &str,
    retry: SubmitRetryConfig,
) -> Result<SubmitResult, SemanticsError> {
    let idempotency_key = derive_idempotency_key(&request);
    if let Some(previous) = idempotency_store()
        .lock()
        .expect("idempotency mutex poisoned")
        .get(&idempotency_key)
        .cloned()
    {
        return Ok(previous);
    }

    if request.leader_verification {
        let check = crate::leader::check_leader(
            transport,
            &request.domain_preimage_hex,
            request.ballot,
            bond_id_hex,
        )
        .await?;
        if !check.is_mine {
            return Ok(SubmitResult::Rejected(RejectReason::NotLeader));
        }
    }

    let vote_hex = bytes_to_hex(&request.payload);
    let mode = if request.leader_verification {
        Some(AddReferendumVoteMode::IsLeader)
    } else {
        Some(AddReferendumVoteMode::PlzFind)
    };
    let votes = vec![(request.domain_preimage_hex.clone(), vote_hex)];
    let policy = retry.normalized();
    let mut submit_attempt = 0u32;
    let mut backoff = policy.initial_backoff;
    let result = loop {
        match transport.add_referendum_vote(&votes, mode).await {
            Ok(result) => break result,
            Err(e) => {
                let retryable = matches!(classify_transport_error(&e), RetryClass::Retryable);
                if !retryable || submit_attempt >= policy.max_retries {
                    return Err(SemanticsError::Transport(e));
                }
                submit_attempt += 1;
                tokio::time::sleep(backoff).await;
                backoff = next_backoff(backoff, policy.max_backoff);
            }
        }
    };
    let accepted = result
        .get(&request.domain_preimage_hex)
        .copied()
        .unwrap_or(false);
    if !accepted {
        let rejected = SubmitResult::Rejected(RejectReason::VoteNotAccepted);
        idempotency_store()
            .lock()
            .expect("idempotency mutex poisoned")
            .insert(idempotency_key, rejected.clone());
        return Ok(rejected);
    }

    if !request.read_after_write.enabled {
        let submitted = SubmitResult::Submitted;
        idempotency_store()
            .lock()
            .expect("idempotency mutex poisoned")
            .insert(idempotency_key, submitted.clone());
        return Ok(submitted);
    }

    let deadline = std::time::Instant::now() + request.read_after_write.max_wait;
    while std::time::Instant::now() < deadline {
        let mut read_attempt = 0u32;
        let mut read_backoff = policy.initial_backoff;
        let entries = loop {
            match transport
                .get_referendum_decision_roll(
                    &request.domain_preimage_hex,
                    request.ballot,
                    request.ballot,
                    char_transport::DecisionRollVerbosity::Standard,
                )
                .await
            {
                Ok(entries) => break entries,
                Err(e) => {
                    let retryable = matches!(classify_transport_error(&e), RetryClass::Retryable);
                    if !retryable || read_attempt >= policy.max_retries {
                        return Err(SemanticsError::Transport(e));
                    }
                    read_attempt += 1;
                    tokio::time::sleep(read_backoff).await;
                    read_backoff = next_backoff(read_backoff, policy.max_backoff);
                }
            }
        };
        if let Some(entry) = entries.first() {
            if entry.found && entry.ballot_number == request.ballot {
                let mut observed_payload = entry
                    .decision_roll
                    .as_ref()
                    .and_then(|r| r.data.as_deref())
                    .and_then(|data_hex| hex_to_bytes(strip_0x_prefix(data_hex.trim())).ok())
                    .filter(|p| !p.is_empty());

                if observed_payload.as_deref() != Some(request.payload.as_slice()) {
                    if let Ok(domain_id) =
                        crate::domain::DomainId::from_preimage_hex(&request.domain_preimage_hex)
                    {
                        if let Ok(head) = transport
                            .get_domain_info(&request.domain_preimage_hex)
                            .await
                        {
                            if let Ok(Some(p)) = crate::leader::try_payload_via_attestation_chain(
                                transport,
                                &request.domain_preimage_hex,
                                domain_id,
                                request.ballot,
                                &head,
                            )
                            .await
                            {
                                observed_payload = Some(p);
                            }
                        }
                    }
                }

                if observed_payload.as_deref() == Some(request.payload.as_slice()) {
                    let verified = SubmitResult::VerifiedObserved;
                    idempotency_store()
                        .lock()
                        .expect("idempotency mutex poisoned")
                        .insert(idempotency_key, verified.clone());
                    return Ok(verified);
                }
            }
        }
        tokio::time::sleep(request.read_after_write.poll_interval).await;
    }

    let unknown = SubmitResult::Unknown;
    idempotency_store()
        .lock()
        .expect("idempotency mutex poisoned")
        .insert(idempotency_key, unknown.clone());
    Ok(unknown)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::MockTransport;
    use bitcoin::Txid;
    use char_transport::{
        AddReferendumVoteMode, AttestationForBondBallot, BondInfo, CharRpcTransport,
        DecisionRollEntry, DecisionRollVerbosity, DomainInfo, DomainRegistryScheduleResult,
        KeyRange, LeaderSlotEntry, TransportError,
    };
    use char_utils::ShortDiag;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    /// First N `add_referendum_vote` calls return [`TransportError::Timeout`], then [`MockTransport`] behavior.
    struct FlakyTimeoutAddVote {
        inner: MockTransport,
        timeouts_before_ok: AtomicU32,
    }

    /// Every `add_referendum_vote` returns deserialization error (terminal for retry policy).
    struct DeserFailAddVote {
        inner: MockTransport,
    }

    /// Counts `add_referendum_vote` invocations, then delegates to [`MockTransport`].
    struct CountingAddVote {
        inner: MockTransport,
        add_calls: Arc<AtomicU32>,
    }

    #[async_trait::async_trait]
    impl CharRpcTransport for FlakyTimeoutAddVote {
        async fn get_domain_info(&self, h: &str) -> Result<DomainInfo, TransportError> {
            self.inner.get_domain_info(h).await
        }
        async fn get_referendum_decision_roll(
            &self,
            d: &str,
            s: u64,
            e: u64,
            v: DecisionRollVerbosity,
        ) -> Result<Vec<DecisionRollEntry>, TransportError> {
            self.inner.get_referendum_decision_roll(d, s, e, v).await
        }
        async fn add_referendum_vote(
            &self,
            votes: &[(String, String)],
            mode: Option<AddReferendumVoteMode>,
        ) -> Result<HashMap<String, bool>, TransportError> {
            let left = self.timeouts_before_ok.load(Ordering::SeqCst);
            if left > 0 {
                self.timeouts_before_ok.fetch_sub(1, Ordering::SeqCst);
                return Err(TransportError::Timeout);
            }
            self.inner.add_referendum_vote(votes, mode).await
        }
        async fn get_leader_for_slot_current_block(
            &self,
            keys: &[KeyRange],
        ) -> Result<Vec<LeaderSlotEntry>, TransportError> {
            self.inner.get_leader_for_slot_current_block(keys).await
        }
        async fn get_all_char_bonds(&self, verbosity: u8) -> Result<Vec<BondInfo>, TransportError> {
            self.inner.get_all_char_bonds(verbosity).await
        }
        async fn get_attestation_for_bond_at_ballot(
            &self,
            bond_id: &Txid,
            ballot_number: u64,
        ) -> Result<AttestationForBondBallot, TransportError> {
            self.inner
                .get_attestation_for_bond_at_ballot(bond_id, ballot_number)
                .await
        }
        async fn domain_registry_schedule(
            &self,
            domain: &str,
            info: &str,
        ) -> Result<DomainRegistryScheduleResult, TransportError> {
            self.inner.domain_registry_schedule(domain, info).await
        }
    }

    #[async_trait::async_trait]
    impl CharRpcTransport for DeserFailAddVote {
        async fn get_domain_info(&self, h: &str) -> Result<DomainInfo, TransportError> {
            self.inner.get_domain_info(h).await
        }
        async fn get_referendum_decision_roll(
            &self,
            d: &str,
            s: u64,
            e: u64,
            v: DecisionRollVerbosity,
        ) -> Result<Vec<DecisionRollEntry>, TransportError> {
            self.inner.get_referendum_decision_roll(d, s, e, v).await
        }
        async fn add_referendum_vote(
            &self,
            _votes: &[(String, String)],
            _mode: Option<AddReferendumVoteMode>,
        ) -> Result<HashMap<String, bool>, TransportError> {
            Err(TransportError::Deserialization(ShortDiag::truncate("bad")))
        }
        async fn get_leader_for_slot_current_block(
            &self,
            keys: &[KeyRange],
        ) -> Result<Vec<LeaderSlotEntry>, TransportError> {
            self.inner.get_leader_for_slot_current_block(keys).await
        }
        async fn get_all_char_bonds(&self, verbosity: u8) -> Result<Vec<BondInfo>, TransportError> {
            self.inner.get_all_char_bonds(verbosity).await
        }
        async fn get_attestation_for_bond_at_ballot(
            &self,
            bond_id: &Txid,
            ballot_number: u64,
        ) -> Result<AttestationForBondBallot, TransportError> {
            self.inner
                .get_attestation_for_bond_at_ballot(bond_id, ballot_number)
                .await
        }
        async fn domain_registry_schedule(
            &self,
            domain: &str,
            info: &str,
        ) -> Result<DomainRegistryScheduleResult, TransportError> {
            self.inner.domain_registry_schedule(domain, info).await
        }
    }

    #[async_trait::async_trait]
    impl CharRpcTransport for CountingAddVote {
        async fn get_domain_info(&self, h: &str) -> Result<DomainInfo, TransportError> {
            self.inner.get_domain_info(h).await
        }
        async fn get_referendum_decision_roll(
            &self,
            d: &str,
            s: u64,
            e: u64,
            v: DecisionRollVerbosity,
        ) -> Result<Vec<DecisionRollEntry>, TransportError> {
            self.inner.get_referendum_decision_roll(d, s, e, v).await
        }
        async fn add_referendum_vote(
            &self,
            votes: &[(String, String)],
            mode: Option<AddReferendumVoteMode>,
        ) -> Result<HashMap<String, bool>, TransportError> {
            self.add_calls.fetch_add(1, Ordering::SeqCst);
            self.inner.add_referendum_vote(votes, mode).await
        }
        async fn get_leader_for_slot_current_block(
            &self,
            keys: &[KeyRange],
        ) -> Result<Vec<LeaderSlotEntry>, TransportError> {
            self.inner.get_leader_for_slot_current_block(keys).await
        }
        async fn get_all_char_bonds(&self, verbosity: u8) -> Result<Vec<BondInfo>, TransportError> {
            self.inner.get_all_char_bonds(verbosity).await
        }
        async fn get_attestation_for_bond_at_ballot(
            &self,
            bond_id: &Txid,
            ballot_number: u64,
        ) -> Result<AttestationForBondBallot, TransportError> {
            self.inner
                .get_attestation_for_bond_at_ballot(bond_id, ballot_number)
                .await
        }
        async fn domain_registry_schedule(
            &self,
            domain: &str,
            info: &str,
        ) -> Result<DomainRegistryScheduleResult, TransportError> {
            self.inner.domain_registry_schedule(domain, info).await
        }
    }

    fn fast_retry() -> SubmitRetryConfig {
        SubmitRetryConfig {
            max_retries: 5,
            initial_backoff: Duration::ZERO,
            max_backoff: Duration::ZERO,
        }
    }

    fn base_req(domain: &str) -> SubmitRequest {
        SubmitRequest {
            domain_preimage_hex: domain.into(),
            ballot: 1,
            payload: vec![1, 2, 3],
            idempotency_key: None,
            leader_verification: false,
            read_after_write: ReadAfterWriteConfig {
                enabled: false,
                max_wait: Duration::from_secs(1),
                poll_interval: Duration::from_millis(10),
            },
        }
    }

    #[test]
    fn reject_reason_display() {
        let r = RejectReason::NotLeader;
        assert!(r.to_string().contains("leader"));
    }

    #[tokio::test]
    async fn submit_vote_mock_submitted() {
        reset_idempotency_store_for_tests();
        let t = MockTransport;
        let res = submit_vote(&t, base_req("deadbeef"), "bond", fast_retry())
            .await
            .unwrap();
        assert!(matches!(res, SubmitResult::Submitted));
    }

    #[tokio::test]
    async fn submit_vote_retries_transport_timeout_then_succeeds() {
        reset_idempotency_store_for_tests();
        let t = FlakyTimeoutAddVote {
            inner: MockTransport,
            timeouts_before_ok: AtomicU32::new(2),
        };
        let res = submit_vote(&t, base_req("cafef00d"), "bond", fast_retry())
            .await
            .unwrap();
        assert!(matches!(res, SubmitResult::Submitted));
        assert_eq!(t.timeouts_before_ok.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn submit_vote_does_not_retry_terminal_transport_error() {
        reset_idempotency_store_for_tests();
        let t = DeserFailAddVote {
            inner: MockTransport,
        };
        let err = submit_vote(&t, base_req("b16b00b5"), "bond", fast_retry())
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            SemanticsError::Transport(TransportError::Deserialization(_))
        ));
    }

    #[tokio::test]
    async fn submit_vote_stops_after_max_retries() {
        reset_idempotency_store_for_tests();
        let t = FlakyTimeoutAddVote {
            inner: MockTransport,
            timeouts_before_ok: AtomicU32::new(100),
        };
        let policy = SubmitRetryConfig {
            max_retries: 2,
            initial_backoff: Duration::ZERO,
            max_backoff: Duration::ZERO,
        };
        let err = submit_vote(&t, base_req("10aded"), "bond", policy)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            SemanticsError::Transport(TransportError::Timeout)
        ));
        assert_eq!(t.timeouts_before_ok.load(Ordering::SeqCst), 97);
    }

    #[tokio::test]
    async fn submit_vote_idempotent_second_call_skips_rpc() {
        reset_idempotency_store_for_tests();
        let calls = Arc::new(AtomicU32::new(0));
        let t = CountingAddVote {
            inner: MockTransport,
            add_calls: Arc::clone(&calls),
        };
        let req = base_req("1dempotent");
        let r1 = submit_vote(&t, req.clone(), "bond", fast_retry())
            .await
            .unwrap();
        let r2 = submit_vote(&t, req, "bond", fast_retry()).await.unwrap();
        assert_eq!(r1, r2);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn submit_vote_explicit_idempotency_key_hits_cache() {
        reset_idempotency_store_for_tests();
        let key = [7u8; 32];
        let calls = Arc::new(AtomicU32::new(0));
        let t = CountingAddVote {
            inner: MockTransport,
            add_calls: Arc::clone(&calls),
        };
        let mut req = base_req("ignored_domain_for_key");
        req.idempotency_key = Some(key);
        let r1 = submit_vote(&t, req.clone(), "bond", fast_retry())
            .await
            .unwrap();
        let r2 = submit_vote(&t, req, "bond", fast_retry()).await.unwrap();
        assert_eq!(r1, r2);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
