use crate::bundle::{BorshPayload, decode_borsh, encode_borsh};
use crate::core::{
    BranchFailure, BranchFailureClass, DaError, MemberId, PolicyFailureSummary, SemanticError,
    UsageError, WriteIncomplete,
};
use crate::da::{BlobWriteReceipt, DaMember, DaVerifyReport, Policy, PolicyKey, PolicySpec};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Policy-driven composite DA evaluator.
pub struct MultiDa {
    members: HashMap<MemberId, Arc<dyn crate::da::DataAvailability>>,
    policy: Policy,
}

impl MultiDa {
    /// Build a composite backend from a build-time member/policy tree.
    pub fn from_spec(spec: PolicySpec) -> Result<Self, DaError> {
        let (members, policy) = spec.into_parts()?;
        Self::new(members, policy)
    }

    /// Internal constructor used by public helpers like `all_of`, `any_of`, and `from_spec`.
    pub(crate) fn new(members: Vec<DaMember>, policy: Policy) -> Result<Self, DaError> {
        policy.validate()?;

        let mut seen = HashSet::with_capacity(members.len());
        let mut member_map = HashMap::with_capacity(members.len());

        for member in members {
            if !seen.insert(member.id().clone()) {
                return Err(UsageError::DuplicateMemberId(member.id().clone()).into());
            }

            let backend_member_id = member.backend().member_id();
            if member.id() != &backend_member_id {
                return Err(UsageError::InvalidComposition(format!(
                    "member '{}' does not match backend member '{}'",
                    member.id(),
                    backend_member_id
                ))
                .into());
            }

            member_map.insert(member.id().clone(), member.backend_arc());
        }

        if member_map.is_empty() {
            return Err(UsageError::InvalidComposition(
                "MultiDa requires at least one member".to_string(),
            )
            .into());
        }

        for member_id in policy.referenced_members() {
            if !member_map.contains_key(&member_id) {
                return Err(UsageError::UnknownMember(member_id).into());
            }
        }

        Ok(Self {
            members: member_map,
            policy,
        })
    }

    /// Build a composite backend that writes to all members in registry order.
    pub fn all_of<TMember>(members: Vec<TMember>) -> Result<Self, DaError>
    where
        TMember: Into<DaMember>,
    {
        let members = members.into_iter().map(Into::into).collect::<Vec<_>>();
        let policy = policy_from_members(&members, PolicyMode::All);
        Self::new(members, policy)
    }

    /// Build a composite backend that can use any member in registry order.
    pub fn any_of<TMember>(members: Vec<TMember>) -> Result<Self, DaError>
    where
        TMember: Into<DaMember>,
    {
        let members = members.into_iter().map(Into::into).collect::<Vec<_>>();
        let policy = policy_from_members(&members, PolicyMode::Any);
        Self::new(members, policy)
    }

    pub fn policy(&self) -> &Policy {
        &self.policy
    }

    fn member(
        &self,
        member_id: &MemberId,
    ) -> Result<&Arc<dyn crate::da::DataAvailability>, DaError> {
        self.members
            .get(member_id)
            .ok_or_else(|| UsageError::UnknownMember(member_id.clone()).into())
    }

    pub async fn write_blob(&self, data: &[u8]) -> Result<BlobWriteReceipt, DaError> {
        let key = self.eval_write(&self.policy, data).await?;
        Ok(BlobWriteReceipt::new(key, data.len()))
    }

    pub async fn read_blob(&self, key: &PolicyKey) -> Result<Vec<u8>, DaError> {
        key.validate_against(&self.policy)?;
        self.eval_read(&self.policy, key).await
    }

    pub async fn verify(&self, key: &PolicyKey) -> Result<DaVerifyReport, DaError> {
        key.validate_against(&self.policy)?;
        self.eval_verify(&self.policy, key).await
    }

    pub async fn write<T>(&self, value: &T) -> Result<BlobWriteReceipt, DaError>
    where
        T: BorshPayload + Sync,
    {
        let bytes = encode_borsh(value).map_err(map_codec_error)?;
        self.write_blob(&bytes).await
    }

    pub async fn read<T>(&self, key: &PolicyKey) -> Result<T, DaError>
    where
        T: BorshPayload + Send,
    {
        let bytes = self.read_blob(key).await?;
        decode_borsh(bytes.as_slice()).map_err(map_codec_error)
    }

    fn eval_write<'a>(
        &'a self,
        policy: &'a Policy,
        data: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<PolicyKey, DaError>> + Send + 'a>> {
        Box::pin(async move {
            match policy {
                Policy::Leaf(member_id) => {
                    let backend = self.member(member_id)?;
                    let receipt = backend.write_blob(data).await?;
                    let leaf = receipt.key().as_leaf_for_member(member_id)?;
                    Ok(PolicyKey::leaf(member_id.clone(), leaf.clone()))
                }
                Policy::And(children) => {
                    let mut keys = Vec::with_capacity(children.len());

                    for child in children {
                        match self.eval_write(child, data).await {
                            Ok(key) => keys.push(key),
                            Err(err) => {
                                if keys.is_empty() {
                                    return Err(err);
                                }

                                if let Some(incomplete) = extract_write_incomplete(&err) {
                                    keys.push(incomplete.partial_key().clone());
                                    return Err(SemanticError::WriteIncomplete(
                                        WriteIncomplete::new(
                                            PolicyKey::And(keys),
                                            incomplete.failures().to_vec(),
                                        ),
                                    )
                                    .into());
                                }

                                let failure = BranchFailure::new(
                                    first_member(child),
                                    classify_error(&err),
                                    err.to_string(),
                                );
                                return Err(SemanticError::WriteIncomplete(WriteIncomplete::new(
                                    PolicyKey::And(keys),
                                    vec![failure],
                                ))
                                .into());
                            }
                        }
                    }

                    Ok(PolicyKey::And(keys))
                }
                Policy::Or(children) => {
                    let mut failures = Vec::new();

                    for child in children {
                        match self.eval_write(child, data).await {
                            Ok(key) => return Ok(PolicyKey::Or(vec![key])),
                            Err(err @ DaError::Usage(_)) => return Err(err),
                            Err(err) => failures.push(BranchFailure::new(
                                first_member(child),
                                classify_error(&err),
                                err.to_string(),
                            )),
                        }
                    }

                    Err(
                        SemanticError::UnavailableAcrossPolicy(summary_from_failures(&failures))
                            .into(),
                    )
                }
            }
        })
    }

    fn eval_read<'a>(
        &'a self,
        policy: &'a Policy,
        key: &'a PolicyKey,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, DaError>> + Send + 'a>> {
        Box::pin(async move {
            match (policy, key) {
                (Policy::Leaf(member_id), PolicyKey::Leaf(leaf)) => {
                    let backend = self.member(member_id)?;
                    let decoded = backend.decode_key(leaf.locator())?;
                    backend.read_blob(decoded.as_ref()).await
                }
                (Policy::And(policies), PolicyKey::And(keys)) => {
                    let mut summary = PolicyFailureSummary::default();

                    for (policy, key) in policies.iter().zip(keys.iter()) {
                        match self.eval_read(policy, key).await {
                            Ok(bytes) => return Ok(bytes),
                            Err(err) if is_retryable(&err) => {
                                summary.record(&classify_error(&err));
                            }
                            Err(err) => return Err(err),
                        }
                    }

                    Err(SemanticError::UnavailableAcrossPolicy(summary).into())
                }
                (Policy::Or(policies), PolicyKey::Or(keys)) => {
                    let mut summary = PolicyFailureSummary::default();

                    for (policy, key) in match_or_pairs(policies, keys)? {
                        match self.eval_read(policy, key).await {
                            Ok(bytes) => return Ok(bytes),
                            Err(err) if is_retryable(&err) => {
                                summary.record(&classify_error(&err));
                            }
                            Err(err) => return Err(err),
                        }
                    }

                    Err(SemanticError::UnavailableAcrossPolicy(summary).into())
                }
                _ => Err(UsageError::BadPolicyKey(
                    "policy key shape does not match policy".to_string(),
                )
                .into()),
            }
        })
    }

    fn eval_verify<'a>(
        &'a self,
        policy: &'a Policy,
        key: &'a PolicyKey,
    ) -> Pin<Box<dyn Future<Output = Result<DaVerifyReport, DaError>> + Send + 'a>> {
        Box::pin(async move {
            match (policy, key) {
                (Policy::Leaf(member_id), PolicyKey::Leaf(leaf)) => {
                    let backend = self.member(member_id)?;
                    let decoded = backend.decode_key(leaf.locator())?;
                    let report = backend.verify_key(decoded.as_ref()).await?;
                    if report.is_read_guaranteed() {
                        Ok(report)
                    } else {
                        Err(SemanticError::VerificationFailed(
                            report.note().unwrap_or("verification failed").to_string(),
                        )
                        .into())
                    }
                }
                (Policy::And(policies), PolicyKey::And(keys)) => {
                    for (policy, key) in policies.iter().zip(keys.iter()) {
                        let report = self.eval_verify(policy, key).await?;
                        if !report.is_read_guaranteed() {
                            return Err(SemanticError::VerificationFailed(
                                "child verification failed".to_string(),
                            )
                            .into());
                        }
                    }

                    Ok(DaVerifyReport::new(
                        true,
                        Some("all AND branches verified".to_string()),
                    ))
                }
                (Policy::Or(policies), PolicyKey::Or(keys)) => {
                    let mut summary = PolicyFailureSummary::default();

                    for (policy, key) in match_or_pairs(policies, keys)? {
                        match self.eval_verify(policy, key).await {
                            Ok(report) => return Ok(report),
                            Err(err) if is_retryable(&err) => {
                                summary.record(&classify_error(&err));
                            }
                            Err(err) => return Err(err),
                        }
                    }

                    Err(SemanticError::UnavailableAcrossPolicy(summary).into())
                }
                _ => Err(UsageError::BadPolicyKey(
                    "policy key shape does not match policy".to_string(),
                )
                .into()),
            }
        })
    }
}

fn match_or_pairs<'a>(
    policies: &'a [Policy],
    keys: &'a [PolicyKey],
) -> Result<Vec<(&'a Policy, &'a PolicyKey)>, DaError> {
    let mut pairs = Vec::with_capacity(keys.len());
    let mut next_index = 0usize;

    for key in keys {
        let mut matched = None;
        while next_index < policies.len() {
            if key.validate_against(&policies[next_index]).is_ok() {
                matched = Some((&policies[next_index], key));
                next_index += 1;
                break;
            }
            next_index += 1;
        }

        let Some(pair) = matched else {
            return Err(UsageError::BadPolicyKey(
                "PolicyKey::Or did not match policy subset".to_string(),
            )
            .into());
        };
        pairs.push(pair);
    }

    Ok(pairs)
}

fn summary_from_failures(failures: &[BranchFailure]) -> PolicyFailureSummary {
    let mut summary = PolicyFailureSummary::default();
    for failure in failures {
        summary.record(failure.class());
    }
    summary
}

fn classify_error(err: &DaError) -> BranchFailureClass {
    match err {
        DaError::Runtime(_) => BranchFailureClass::Runtime,
        DaError::Semantic(SemanticError::NotFound) => BranchFailureClass::NotFound,
        DaError::Semantic(_) => BranchFailureClass::Semantic,
        DaError::Usage(_) => BranchFailureClass::Usage,
    }
}

fn is_retryable(err: &DaError) -> bool {
    matches!(
        err,
        DaError::Runtime(_) | DaError::Semantic(SemanticError::NotFound)
    )
}

fn extract_write_incomplete(err: &DaError) -> Option<&WriteIncomplete> {
    match err {
        DaError::Semantic(SemanticError::WriteIncomplete(incomplete)) => Some(incomplete),
        _ => None,
    }
}

fn first_member(policy: &Policy) -> MemberId {
    match policy {
        Policy::Leaf(member_id) => member_id.clone(),
        Policy::And(children) | Policy::Or(children) => first_member(&children[0]),
    }
}

fn map_codec_error(err: crate::bundle::CodecError) -> DaError {
    match err {
        crate::bundle::CodecError::Serialize(message) => UsageError::InvalidRequest(message).into(),
        crate::bundle::CodecError::Deserialize(message) => {
            SemanticError::DecodeFailure(message).into()
        }
        crate::bundle::CodecError::EmptyBundle => {
            SemanticError::DecodeFailure("bundle must contain at least one item".to_string()).into()
        }
    }
}

#[derive(Clone, Copy)]
enum PolicyMode {
    All,
    Any,
}

fn policy_from_members(members: &[DaMember], mode: PolicyMode) -> Policy {
    let leaves = members
        .iter()
        .map(|member| Policy::Leaf(member.id().clone()))
        .collect::<Vec<_>>();

    if leaves.len() == 1 {
        return leaves
            .into_iter()
            .next()
            .expect("single-element policy list should contain one leaf");
    }

    match mode {
        PolicyMode::All => Policy::And(leaves),
        PolicyMode::Any => Policy::Or(leaves),
    }
}
