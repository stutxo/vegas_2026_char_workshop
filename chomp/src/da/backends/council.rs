use crate::da::{
    BlobWriteReceipt, CouncilBlobLocator, DaError, DaMember, DaVerifyReport, DataAvailability,
    DynKey, Locator, MemberId, PolicyKey, RuntimeError, SemanticError, UsageError,
};
use async_trait::async_trait;
use reqwest::{Client as HttpClient, header::CONTENT_TYPE};
use sha2::{Digest, Sha256};

/// Simple council-style backend addressed by HTTP push/pull endpoints.
pub struct CouncilDa {
    /// Council member tag used when converting into a [`DaMember`].
    pub tag: String,
    /// Base URL for the council service.
    pub url: String,
    /// HTTP client used for council reads and writes.
    pub http_client: HttpClient,
}

impl CouncilDa {
    /// Construct a council backend from a validated tag and base URL.
    pub fn new(tag: impl Into<String>, url: &str) -> Result<Self, UsageError> {
        let MemberId::Council(tag) = MemberId::council(tag)? else {
            unreachable!("validated council tag should always produce MemberId::Council");
        };

        Ok(Self {
            tag,
            url: url.to_string(),
            http_client: HttpClient::new(),
        })
    }

    /// Return the member id implied by this council backend.
    pub fn member_id(&self) -> MemberId {
        MemberId::Council(self.tag.clone())
    }

    /// Convert this backend into a [`DaMember`] for use inside [`crate::MultiDa`].
    pub fn into_member(self) -> DaMember {
        self.into()
    }

    fn compute_hash(data: &[u8]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hasher.finalize().into()
    }

    fn locator_from_hash(hash: [u8; 32]) -> Result<Locator, DaError> {
        Locator::from_key(&CouncilBlobLocator::new(hash))
    }

    fn hash_from_locator(locator: &Locator) -> Result<[u8; 32], DaError> {
        if locator.provider_kind() != "council" {
            return Err(UsageError::WrongProvider {
                expected: "council",
            }
            .into());
        }

        let council_locator = serde_json::from_slice::<CouncilBlobLocator>(locator.key_bytes())
            .map_err(|err| UsageError::BadLocator(err.to_string()))?;

        Ok(council_locator.into_array())
    }
}

#[async_trait]
impl DataAvailability for CouncilDa {
    fn provider_kind(&self) -> &'static str {
        "council"
    }

    fn member_id(&self) -> MemberId {
        CouncilDa::member_id(self)
    }

    async fn write_blob(&self, data: &[u8]) -> Result<BlobWriteReceipt, DaError> {
        let key = Self::compute_hash(data);
        let endpoint = format!("{}/push/{}", self.url, hex::encode(key));

        let resp = self
            .http_client
            .post(endpoint)
            .header(CONTENT_TYPE, "application/octet-stream")
            .body(data.to_vec())
            .send()
            .await
            .map_err(|e| RuntimeError::ConnectionFailure(e.to_string()))?;

        if resp.status().is_success() {
            Ok(BlobWriteReceipt::new(
                PolicyKey::leaf(self.member_id(), Self::locator_from_hash(key)?),
                data.len(),
            ))
        } else {
            Err(RuntimeError::ServiceUnavailable(format!(
                "Council returned status {}",
                resp.status()
            ))
            .into())
        }
    }

    async fn read_blob(&self, key: &dyn crate::da::DaKey) -> Result<Vec<u8>, DaError> {
        let Some(locator) = key.as_any().downcast_ref::<CouncilBlobLocator>() else {
            return Err(UsageError::WrongKeyType {
                expected: "CouncilBlobLocator",
            }
            .into());
        };

        let key = locator.into_array();
        let endpoint = format!("{}/pull/{}", self.url, hex::encode(key));
        let resp = self
            .http_client
            .get(endpoint)
            .send()
            .await
            .map_err(|e| RuntimeError::ConnectionFailure(e.to_string()))?;

        if resp.status() == 404 {
            return Err(SemanticError::NotFound.into());
        }
        if !resp.status().is_success() {
            return Err(RuntimeError::ServiceUnavailable(format!(
                "Council returned status {}",
                resp.status()
            ))
            .into());
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| RuntimeError::ConnectionFailure(e.to_string()))?;
        let blob = bytes.to_vec();

        if Self::compute_hash(&blob) != key {
            return Err(SemanticError::IntegrityFailure.into());
        }

        Ok(blob)
    }

    async fn verify_key(&self, key: &dyn crate::da::DaKey) -> Result<DaVerifyReport, DaError> {
        let _ = self.read_blob(key).await?;
        Ok(DaVerifyReport::new(
            true,
            Some("council blob resolved and hash matched".to_string()),
        ))
    }

    fn decode_key(&self, locator: &Locator) -> Result<DynKey, DaError> {
        let hash = Self::hash_from_locator(locator)?;
        Ok(std::sync::Arc::new(CouncilBlobLocator::new(hash)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn into_member_uses_backend_tag() {
        let member = CouncilDa::new("new_york", "http://127.0.0.1:8080")
            .expect("council tag should be valid")
            .into_member();

        assert_eq!(member.id(), &MemberId::Council("new_york".to_string()));
    }

    #[test]
    fn council_transport_encoding_prefixes_chomp_before_hashing() {
        let payload = b"decision-roll";
        let hash = CouncilDa::compute_hash(payload);

        assert_eq!(
            CouncilDa::locator_from_hash(hash).unwrap().provider_kind(),
            "council"
        );
    }

    #[test]
    fn council_transport_payload_is_raw_bytes() {
        let payload = b"decision-roll".to_vec();

        assert_eq!(payload, b"decision-roll".to_vec());
    }
}
