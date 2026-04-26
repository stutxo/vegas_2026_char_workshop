use super::traits::DaMember;
use crate::core::{DaError, MemberId, UsageError};
use crate::da::PolicyKey;
use serde::{Deserialize, Serialize};

/// Composition policy for one or more DA members.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Policy {
    Leaf(MemberId),
    And(Vec<Policy>),
    Or(Vec<Policy>),
}

impl Policy {
    pub fn validate(&self) -> Result<(), DaError> {
        match self {
            Self::Leaf(_) => Ok(()),
            Self::And(children) => {
                if children.is_empty() {
                    return Err(UsageError::InvalidComposition(
                        "Policy::And must not be empty".to_string(),
                    )
                    .into());
                }
                for child in children {
                    child.validate()?;
                }
                Ok(())
            }
            Self::Or(children) => {
                if children.is_empty() {
                    return Err(UsageError::InvalidComposition(
                        "Policy::Or must not be empty".to_string(),
                    )
                    .into());
                }
                for child in children {
                    child.validate()?;
                }
                Ok(())
            }
        }
    }

    pub fn referenced_members(&self) -> Vec<MemberId> {
        fn walk(policy: &Policy, output: &mut Vec<MemberId>) {
            match policy {
                Policy::Leaf(member_id) => output.push(member_id.clone()),
                Policy::And(children) | Policy::Or(children) => {
                    for child in children {
                        walk(child, output);
                    }
                }
            }
        }

        let mut output = Vec::new();
        walk(self, &mut output);
        output
    }
}

/// Build-time policy DSL that owns live backend members.
pub enum PolicySpec {
    Leaf(DaMember),
    And(Vec<PolicySpec>),
    Or(Vec<PolicySpec>),
}

impl PolicySpec {
    pub(crate) fn into_parts(self) -> Result<(Vec<DaMember>, Policy), DaError> {
        fn inner(spec: PolicySpec) -> Result<(Vec<DaMember>, Policy), DaError> {
            match spec {
                PolicySpec::Leaf(member) => {
                    let policy = Policy::Leaf(member.id().clone());
                    Ok((vec![member], policy))
                }
                PolicySpec::And(branches) => {
                    if branches.is_empty() {
                        return Err(UsageError::InvalidComposition(
                            "PolicySpec::And must not be empty".to_string(),
                        )
                        .into());
                    }

                    let mut members = Vec::new();
                    let mut policies = Vec::with_capacity(branches.len());
                    for branch in branches {
                        let (branch_members, branch_policy) = inner(branch)?;
                        members.extend(branch_members);
                        policies.push(branch_policy);
                    }
                    Ok((members, Policy::And(policies)))
                }
                PolicySpec::Or(branches) => {
                    if branches.is_empty() {
                        return Err(UsageError::InvalidComposition(
                            "PolicySpec::Or must not be empty".to_string(),
                        )
                        .into());
                    }

                    let mut members = Vec::new();
                    let mut policies = Vec::with_capacity(branches.len());
                    for branch in branches {
                        let (branch_members, branch_policy) = inner(branch)?;
                        members.extend(branch_members);
                        policies.push(branch_policy);
                    }
                    Ok((members, Policy::Or(policies)))
                }
            }
        }

        inner(self)
    }
}

impl PolicyKey {
    pub fn validate_against(&self, policy: &Policy) -> Result<(), DaError> {
        fn inner(key: &PolicyKey, policy: &Policy) -> Result<(), DaError> {
            match (key, policy) {
                (PolicyKey::Leaf(leaf), Policy::Leaf(member_id)) => {
                    leaf.validate()?;

                    if leaf.member_id() != member_id {
                        return Err(UsageError::BadPolicyKey(format!(
                            "policy leaf member '{}' does not match key member '{}'",
                            member_id,
                            leaf.member_id()
                        ))
                        .into());
                    }

                    Ok(())
                }
                (PolicyKey::And(keys), Policy::And(policies)) => {
                    if keys.len() != policies.len() {
                        return Err(UsageError::BadPolicyKey(format!(
                            "PolicyKey::And arity {} does not match Policy::And arity {}",
                            keys.len(),
                            policies.len()
                        ))
                        .into());
                    }

                    for (key, policy) in keys.iter().zip(policies.iter()) {
                        inner(key, policy)?;
                    }

                    Ok(())
                }
                (PolicyKey::Or(keys), Policy::Or(policies)) => {
                    if keys.is_empty() {
                        return Err(UsageError::BadPolicyKey(
                            "PolicyKey::Or must not be empty".to_string(),
                        )
                        .into());
                    }

                    let mut next_index = 0usize;
                    for key in keys {
                        let mut matched = false;
                        while next_index < policies.len() {
                            if inner(key, &policies[next_index]).is_ok() {
                                matched = true;
                                next_index += 1;
                                break;
                            }
                            next_index += 1;
                        }

                        if !matched {
                            return Err(UsageError::BadPolicyKey(
                                "PolicyKey::Or does not match a policy-ordered subset".to_string(),
                            )
                            .into());
                        }
                    }

                    Ok(())
                }
                _ => Err(UsageError::BadPolicyKey(
                    "policy key shape does not match policy".to_string(),
                )
                .into()),
            }
        }

        inner(self, policy)
    }
}

/// Fee selection policy for Bitcoin and Liquid inscription writes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum FeePolicy {
    /// Ask the backend to estimate a fee rate for a target confirmation window.
    Target {
        confirmation_blocks: u16,
        max_sat_per_vb: Option<f64>,
    },
    /// Use an explicit fee rate, still respecting chain relay floors at runtime.
    Manual { sat_per_vb: f64 },
}

impl FeePolicy {
    /// Build a target-fee policy for next-block inclusion.
    pub fn next_block() -> Self {
        Self::Target {
            confirmation_blocks: 1,
            max_sat_per_vb: None,
        }
    }

    /// Build a target-fee policy for inclusion within `confirmation_blocks`.
    pub fn within_blocks(confirmation_blocks: u16) -> Self {
        Self::Target {
            confirmation_blocks,
            max_sat_per_vb: None,
        }
    }

    /// Build a manual fee policy in sat/vB.
    pub fn manual(sat_per_vb: f64) -> Self {
        Self::Manual { sat_per_vb }
    }
}

impl Default for FeePolicy {
    fn default() -> Self {
        Self::next_block()
    }
}

/// Large-payload handling policy for Bitcoin and Liquid inscription writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum OversizePolicy {
    /// Reject payloads that do not fit in one standard write flow.
    #[default]
    Reject,
    /// Chunk oversized payloads across multiple ordered transactions.
    Chunked {
        initial_chunk_target_bytes: usize,
        min_chunk_bytes: usize,
    },
}

impl OversizePolicy {
    /// Build the default chunking policy used by the examples.
    pub fn chunked_default() -> Self {
        Self::Chunked {
            initial_chunk_target_bytes: 397_000,
            min_chunk_bytes: 8_192,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::OversizePolicy;

    #[test]
    fn default_oversize_policy_rejects() {
        assert_eq!(OversizePolicy::default(), OversizePolicy::Reject);
    }
}
