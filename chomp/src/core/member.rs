use serde::{Deserialize, Serialize};
use std::fmt;

/// Identifier for one member in a data-availability composition.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "label", rename_all = "snake_case")]
pub enum MemberId {
    Bitcoin,
    Liquid,
    Council(String),
}

impl MemberId {
    /// Build a validated council member identifier from a user-provided label.
    pub fn council(label: impl Into<String>) -> Result<Self, crate::core::UsageError> {
        let label = label.into().trim().to_string();
        if label.is_empty() {
            return Err(crate::core::UsageError::BadMemberId(
                "council member label cannot be empty".to_string(),
            ));
        }

        Ok(Self::Council(label))
    }

    /// Return the backend/provider kind that should serve this member.
    pub fn provider_kind(&self) -> &'static str {
        match self {
            Self::Bitcoin => "bitcoin",
            Self::Liquid => "liquid",
            Self::Council(_) => "council",
        }
    }

    /// Return `true` when this member id expects the given provider kind.
    pub fn matches_provider_kind(&self, provider_kind: &str) -> bool {
        self.provider_kind() == provider_kind
    }
}

impl fmt::Display for MemberId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bitcoin => f.write_str("bitcoin"),
            Self::Liquid => f.write_str("liquid"),
            Self::Council(label) => write!(f, "council:{label}"),
        }
    }
}
