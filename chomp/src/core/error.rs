use super::member::MemberId;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BranchFailureClass {
    Runtime,
    NotFound,
    Semantic,
    Usage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchFailure {
    member_id: MemberId,
    class: BranchFailureClass,
    message: String,
}

impl BranchFailure {
    pub fn new(member_id: MemberId, class: BranchFailureClass, message: impl Into<String>) -> Self {
        Self {
            member_id,
            class,
            message: message.into(),
        }
    }

    pub fn member_id(&self) -> &MemberId {
        &self.member_id
    }

    pub fn class(&self) -> &BranchFailureClass {
        &self.class
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PolicyFailureSummary {
    runtime_failures: usize,
    not_found_failures: usize,
    semantic_failures: usize,
    usage_failures: usize,
}

impl PolicyFailureSummary {
    pub fn record(&mut self, class: &BranchFailureClass) {
        match class {
            BranchFailureClass::Runtime => self.runtime_failures += 1,
            BranchFailureClass::NotFound => self.not_found_failures += 1,
            BranchFailureClass::Semantic => self.semantic_failures += 1,
            BranchFailureClass::Usage => self.usage_failures += 1,
        }
    }

    pub fn runtime_failures(&self) -> usize {
        self.runtime_failures
    }

    pub fn not_found_failures(&self) -> usize {
        self.not_found_failures
    }

    pub fn semantic_failures(&self) -> usize {
        self.semantic_failures
    }

    pub fn usage_failures(&self) -> usize {
        self.usage_failures
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WriteIncomplete {
    partial_key: crate::da::PolicyKey,
    failures: Vec<BranchFailure>,
}

impl WriteIncomplete {
    pub fn new(partial_key: crate::da::PolicyKey, failures: Vec<BranchFailure>) -> Self {
        Self {
            partial_key,
            failures,
        }
    }

    pub fn partial_key(&self) -> &crate::da::PolicyKey {
        &self.partial_key
    }

    pub fn failures(&self) -> &[BranchFailure] {
        &self.failures
    }
}

/// Runtime failures returned by backend implementations.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    #[error("connection failure: {0}")]
    ConnectionFailure(String),

    #[error("request timed out: {0}")]
    Timeout(String),

    #[error("service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("provider misconfigured: {0}")]
    Misconfigured(String),

    #[error("unexpected runtime failure: {0}")]
    Internal(String),
}

/// Semantic failures about the requested data or operation.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum SemanticError {
    #[error("data not found")]
    NotFound,

    #[error("data unavailable across policy")]
    UnavailableAcrossPolicy(PolicyFailureSummary),

    #[error("data integrity failure")]
    IntegrityFailure,

    #[error("data mismatch across policy branches")]
    DataMismatch,

    #[error("payload decode failure: {0}")]
    DecodeFailure(String),

    #[error("verification failed: {0}")]
    VerificationFailed(String),

    #[error("write completed only partially")]
    WriteIncomplete(WriteIncomplete),

    #[error("request exceeds provider limits: {0}")]
    ExceedsLimit(String),

    #[error("operation precondition failed: {0}")]
    PreconditionFailed(String),

    #[error("unsupported operation: {0}")]
    Unsupported(String),
}

/// Caller or configuration mistakes detected by the SDK.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum UsageError {
    #[error("wrong provider for locator: expected {expected}")]
    WrongProvider { expected: &'static str },

    #[error("wrong key type for provider: expected {expected}")]
    WrongKeyType { expected: &'static str },

    #[error("malformed locator: {0}")]
    BadLocator(String),

    #[error("malformed policy key: {0}")]
    BadPolicyKey(String),

    #[error("invalid member id: {0}")]
    BadMemberId(String),

    #[error("duplicate member id: {0}")]
    DuplicateMemberId(MemberId),

    #[error("unknown member id in locator: {0}")]
    UnknownMember(MemberId),

    #[error("member id {member_id} does not match locator kind {locator_kind}")]
    MemberLocatorMismatch {
        member_id: MemberId,
        locator_kind: String,
    },

    #[error("invalid request: {0}")]
    InvalidRequest(String),

    #[error("invalid composition: {0}")]
    InvalidComposition(String),
}

/// Underlying unified CHOMP error enum.
///
/// Prefer the [`DaError`] alias in application-facing code.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum SdkError {
    #[error(transparent)]
    Runtime(#[from] RuntimeError),

    #[error(transparent)]
    Semantic(#[from] SemanticError),

    #[error(transparent)]
    Usage(#[from] UsageError),
}

/// Preferred crate-wide error alias for CHOMP operations.
pub type DaError = SdkError;
