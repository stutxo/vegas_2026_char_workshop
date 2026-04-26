use crate::da::{DaError, FeePolicy, RuntimeError};

pub(crate) const TARGET_FEE_RETRY_MULTIPLIER: f64 = 1.25;
pub(crate) const TARGET_FEE_MAX_RETRIES: usize = 5;

pub(crate) fn validate_fee_rate(rate: f64) -> Result<f64, DaError> {
    if rate.is_finite() && rate > 0.0 {
        Ok(rate)
    } else {
        Err(RuntimeError::Misconfigured(format!(
            "fee rate must be finite and positive, got {}",
            rate
        ))
        .into())
    }
}

fn validate_nonnegative_fee_rate(rate: f64, context: &str) -> Result<f64, DaError> {
    if rate.is_finite() && rate >= 0.0 {
        Ok(rate)
    } else {
        Err(RuntimeError::Misconfigured(format!(
            "{context} must be finite and non-negative, got {rate}"
        ))
        .into())
    }
}

pub(crate) fn validate_confirmation_blocks(confirmation_blocks: u16) -> Result<u16, DaError> {
    if confirmation_blocks == 0 {
        Err(RuntimeError::Misconfigured(
            "confirmation_blocks must be greater than zero".to_string(),
        )
        .into())
    } else {
        Ok(confirmation_blocks)
    }
}

pub(crate) fn validate_fee_policy(policy: &FeePolicy) -> Result<(), DaError> {
    match policy {
        FeePolicy::Target {
            confirmation_blocks,
            max_sat_per_vb,
        } => {
            let _ = validate_confirmation_blocks(*confirmation_blocks)?;
            if let Some(max_sat_per_vb) = max_sat_per_vb {
                let _ = validate_fee_rate(*max_sat_per_vb)?;
            }
            Ok(())
        }
        FeePolicy::Manual { sat_per_vb } => validate_fee_rate(*sat_per_vb).map(|_| ()),
    }
}

pub(crate) fn clamp_fee_rate_to_policy(
    policy: &FeePolicy,
    estimated_sat_per_vb: f64,
    relay_floor_sat_per_vb: f64,
) -> Result<f64, DaError> {
    let relay_floor_sat_per_vb =
        validate_nonnegative_fee_rate(relay_floor_sat_per_vb, "relay floor fee rate")?;
    match policy {
        FeePolicy::Target {
            confirmation_blocks,
            max_sat_per_vb,
        } => {
            let _ = validate_confirmation_blocks(*confirmation_blocks)?;
            let estimated_sat_per_vb =
                validate_nonnegative_fee_rate(estimated_sat_per_vb, "estimated fee rate")?;
            let capped = if let Some(max_sat_per_vb) = max_sat_per_vb {
                estimated_sat_per_vb.min(validate_fee_rate(*max_sat_per_vb)?)
            } else {
                estimated_sat_per_vb
            };
            Ok(capped.max(relay_floor_sat_per_vb))
        }
        FeePolicy::Manual { sat_per_vb } => {
            Ok(validate_fee_rate(*sat_per_vb)?.max(relay_floor_sat_per_vb))
        }
    }
}

pub(crate) fn fee_policy_rates(
    policy: &FeePolicy,
    estimated_sat_per_vb: f64,
    relay_floor_sat_per_vb: f64,
) -> Result<Vec<f64>, DaError> {
    let initial_rate =
        clamp_fee_rate_to_policy(policy, estimated_sat_per_vb, relay_floor_sat_per_vb)?;
    match policy {
        FeePolicy::Manual { .. } => Ok(vec![initial_rate]),
        FeePolicy::Target { max_sat_per_vb, .. } => {
            let mut rates = Vec::with_capacity(TARGET_FEE_MAX_RETRIES.saturating_add(1));
            let max_rate = match max_sat_per_vb {
                Some(max_sat_per_vb) => Some(validate_fee_rate(*max_sat_per_vb)?),
                None => None,
            };
            let relay_floor_sat_per_vb =
                validate_nonnegative_fee_rate(relay_floor_sat_per_vb, "relay floor fee rate")?;
            let mut current = initial_rate;
            rates.push(current);
            for _ in 0..TARGET_FEE_MAX_RETRIES {
                current *= TARGET_FEE_RETRY_MULTIPLIER;
                if let Some(max_rate) = max_rate {
                    current = current.min(max_rate);
                }
                current = current.max(relay_floor_sat_per_vb);
                if current <= *rates.last().expect("rates is never empty") {
                    break;
                }
                rates.push(current);
            }
            Ok(rates)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_fee_policy_builds_retry_schedule() {
        let rates = fee_policy_rates(&FeePolicy::next_block(), 1.5, 1.0)
            .expect("target fee policy should build rates");

        assert_eq!(
            rates,
            vec![1.5, 1.875, 2.34375, 2.9296875, 3.662109375, 4.57763671875]
        );
    }

    #[test]
    fn manual_fee_policy_stays_at_relay_floor_or_requested_rate() {
        let raised = fee_policy_rates(&FeePolicy::manual(0.5), 0.5, 1.0)
            .expect("manual policy should honor relay floor");
        let exact = fee_policy_rates(&FeePolicy::manual(2.5), 2.5, 1.0)
            .expect("manual policy should use requested rate");

        assert_eq!(raised, vec![1.0]);
        assert_eq!(exact, vec![2.5]);
    }

    #[test]
    fn target_fee_policy_respects_optional_cap() {
        let rates = fee_policy_rates(
            &FeePolicy::Target {
                confirmation_blocks: 2,
                max_sat_per_vb: Some(2.0),
            },
            1.8,
            1.0,
        )
        .expect("target fee policy should cap retry rates");

        assert_eq!(rates, vec![1.8, 2.0]);
    }

    #[test]
    fn zero_confirmation_target_is_invalid() {
        let err = validate_fee_policy(&FeePolicy::within_blocks(0))
            .expect_err("zero confirmation target should be invalid");

        assert!(matches!(
            err,
            DaError::Runtime(RuntimeError::Misconfigured(_))
        ));
    }

    #[test]
    fn target_fee_policy_allows_zero_runtime_fee_observations() {
        let rates = fee_policy_rates(&FeePolicy::next_block(), 0.0, 0.0)
            .expect("zero runtime rates should be allowed");

        assert_eq!(rates, vec![0.0]);
    }
}
