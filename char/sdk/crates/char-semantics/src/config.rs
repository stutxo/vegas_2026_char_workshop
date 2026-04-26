// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! Structured configuration for semantics (retry, timeouts, bounds).

use char_utils::{GET_REFERENDUM_DECISION_ROLL_MAX_INCLUSIVE_SPAN, MAX_CHAR_BAMBOO_SIZE};
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct SemanticsConfig {
    pub retry_budget: RetryBudget,
    pub timeouts: Timeouts,
    pub concurrency_limits: ConcurrencyLimits,
    pub buffer_bounds: BufferBounds,
}

#[derive(Debug, Clone)]
pub struct RetryBudget {
    pub max_retries: u32,
    pub backoff: BackoffConfig,
}
impl Default for RetryBudget {
    fn default() -> Self {
        Self {
            max_retries: 3,
            backoff: BackoffConfig::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BackoffConfig {
    pub initial: Duration,
    pub max: Duration,
}
impl Default for BackoffConfig {
    fn default() -> Self {
        Self {
            initial: Duration::from_millis(100),
            max: Duration::from_secs(5),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Timeouts {
    pub rpc_call: Duration,
    pub read_after_write: Duration,
    pub reconcile_batch: Duration,
}
impl Default for Timeouts {
    fn default() -> Self {
        Self {
            rpc_call: Duration::from_secs(30),
            read_after_write: Duration::from_secs(60),
            reconcile_batch: Duration::from_secs(120),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConcurrencyLimits {
    pub max_concurrent_requests: u32,
}
impl Default for ConcurrencyLimits {
    fn default() -> Self {
        Self {
            max_concurrent_requests: 4,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BufferBounds {
    pub max_roll_batch: u64,
    pub max_payload_size: usize,
}
impl Default for BufferBounds {
    fn default() -> Self {
        Self {
            max_roll_batch: GET_REFERENDUM_DECISION_ROLL_MAX_INCLUSIVE_SPAN,
            max_payload_size: MAX_CHAR_BAMBOO_SIZE,
        }
    }
}

/// Timing for [`crate::run_rpc`]: loop delay, decision-roll polling, post-submit wait budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RpcPollConfig {
    pub poll_delay: Duration,
    pub roll_poll_interval: Duration,
    pub roll_timeout: Duration,
}

impl Default for RpcPollConfig {
    fn default() -> Self {
        Self {
            poll_delay: Duration::from_secs(3),
            roll_poll_interval: Duration::from_millis(200),
            roll_timeout: Duration::from_secs(5),
        }
    }
}

impl RpcPollConfig {
    pub fn normalized(self) -> Self {
        let roll_poll_interval = self.roll_poll_interval.max(Duration::from_millis(1));
        let poll_delay = self.poll_delay.max(Duration::from_millis(1));
        let roll_timeout = self.roll_timeout.max(roll_poll_interval);
        Self {
            poll_delay,
            roll_poll_interval,
            roll_timeout,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use char_utils::{GET_REFERENDUM_DECISION_ROLL_MAX_INCLUSIVE_SPAN, MAX_CHAR_BAMBOO_SIZE};
    use std::time::Duration;

    #[test]
    fn config_default() {
        let c = SemanticsConfig::default();
        assert_eq!(c.retry_budget.max_retries, 3);
        assert_eq!(
            c.buffer_bounds.max_roll_batch,
            GET_REFERENDUM_DECISION_ROLL_MAX_INCLUSIVE_SPAN
        );
        assert_eq!(c.buffer_bounds.max_payload_size, MAX_CHAR_BAMBOO_SIZE);
    }

    #[test]
    fn rpc_poll_default_matches_run_rpc_constants() {
        let p = RpcPollConfig::default();
        assert_eq!(p.poll_delay, Duration::from_secs(3));
        assert_eq!(p.roll_poll_interval, Duration::from_millis(200));
        assert_eq!(p.roll_timeout, Duration::from_secs(5));
    }
}
