use super::locator::PolicyKey;

/// Result of a successful write operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobWriteReceipt {
    key: PolicyKey,
    size: usize,
}

impl BlobWriteReceipt {
    /// Build a write receipt from a policy key and byte size.
    pub fn new(key: PolicyKey, size: usize) -> Self {
        Self { key, size }
    }

    /// Borrow the persisted key returned by the write.
    pub fn key(&self) -> &PolicyKey {
        &self.key
    }

    /// Return the size of the encoded bytes written to the backend.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Consume the receipt and return its `(key, size)` parts.
    pub fn into_parts(self) -> (PolicyKey, usize) {
        (self.key, self.size)
    }
}

/// Result of a verification call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaVerifyReport {
    read_guaranteed: bool,
    note: Option<String>,
}

impl DaVerifyReport {
    /// Build a verification report from a read guarantee flag and optional note.
    pub fn new(read_guaranteed: bool, note: Option<String>) -> Self {
        Self {
            read_guaranteed,
            note,
        }
    }

    /// Return `true` when the backend verified that reads should succeed.
    pub fn is_read_guaranteed(&self) -> bool {
        self.read_guaranteed
    }

    /// Optional provider-specific note that explains the verification result.
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::DaVerifyReport;

    #[test]
    fn verify_report_primary_accessor_matches_constructor() {
        let report = DaVerifyReport::new(true, Some("ok".to_string()));

        assert!(report.is_read_guaranteed());
        assert_eq!(report.note(), Some("ok"));
    }
}
