use url::Url;

use crate::error::RegistryError;
use crate::types::PackageRecord;

/// A registry client that tries multiple registries in order (fallback chain).
/// Configured via `ELU_REGISTRY` env var (comma-separated URLs).
pub struct RegistryClient {
    registries: Vec<Url>,
    http: reqwest::Client,
}

impl RegistryClient {
    /// Create a client from a comma-separated list of registry URLs.
    pub fn from_env_str(registry_str: &str) -> Result<Self, RegistryError> {
        let registries: Vec<Url> = registry_str
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| {
                Url::parse(s)
                    .map_err(|e| RegistryError::InvalidManifest {
                        reason: format!("invalid registry URL '{s}': {e}"),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;

        if registries.is_empty() {
            return Err(RegistryError::InvalidManifest {
                reason: "no registry URLs provided".into(),
            });
        }

        Ok(Self {
            registries,
            http: reqwest::Client::new(),
        })
    }

    /// Create a client with explicit registry URLs.
    pub fn new(registries: Vec<Url>) -> Self {
        Self {
            registries,
            http: reqwest::Client::new(),
        }
    }

    /// Fetch a package record, trying each registry in order.
    /// Returns the first successful result, or the last error.
    pub async fn fetch_package(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> Result<PackageRecord, RegistryError> {
        let path = format!("api/v1/packages/{namespace}/{name}/{version}");
        let mut last_err = RegistryError::InvalidManifest {
            reason: "no registries configured".into(),
        };

        for base in &self.registries {
            let url = base.join(&path).map_err(|e| RegistryError::BlobBackend(e.to_string()))?;

            match self.http.get(url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    let record: PackageRecord = resp
                        .json()
                        .await
                        .map_err(|e| RegistryError::InvalidManifest {
                            reason: format!("failed to parse package record: {e}"),
                        })?;
                    return Ok(record);
                }
                Ok(resp) if resp.status().as_u16() == 404 => {
                    last_err = RegistryError::VersionNotFound {
                        namespace: namespace.to_string(),
                        name: name.to_string(),
                        version: version.to_string(),
                    };
                    continue;
                }
                Ok(resp) => {
                    last_err = RegistryError::BlobBackend(format!(
                        "registry returned status {}",
                        resp.status()
                    ));
                    continue;
                }
                Err(e) => {
                    last_err =
                        RegistryError::BlobBackend(format!("failed to connect to registry: {e}"));
                    continue;
                }
            }
        }

        Err(last_err)
    }

    /// Fetch a package record by manifest hash, trying each registry in order.
    /// Returns the first successful result, or the last error.
    pub async fn fetch_package_by_hash(
        &self,
        _hash: &elu_store::hash::ManifestHash,
    ) -> Result<PackageRecord, RegistryError> {
        unimplemented!("fetch_package_by_hash — implemented in green slice")
    }

    /// Fetch raw bytes from a URL (for manifest or blob downloads).
    pub async fn fetch_bytes(&self, url: &Url) -> Result<Vec<u8>, RegistryError> {
        let resp = self
            .http
            .get(url.clone())
            .send()
            .await
            .map_err(|e| RegistryError::BlobBackend(format!("fetch failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(RegistryError::BlobBackend(format!(
                "fetch returned status {}",
                resp.status()
            )));
        }

        resp.bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| RegistryError::BlobBackend(format!("failed to read bytes: {e}")))
    }

    pub fn registries(&self) -> &[Url] {
        &self.registries
    }

    /// Search the registry index. Tries each registry in order.
    pub async fn search(
        &self,
        query: &crate::types::SearchQuery,
    ) -> Result<crate::types::SearchResponse, RegistryError> {
        let mut last_err = RegistryError::InvalidManifest {
            reason: "no registries configured".into(),
        };
        for base in &self.registries {
            let mut url = base
                .join("api/v1/search")
                .map_err(|e| RegistryError::BlobBackend(e.to_string()))?;
            {
                let mut q = url.query_pairs_mut();
                if let Some(s) = &query.q {
                    q.append_pair("q", s);
                }
                if let Some(s) = &query.kind {
                    q.append_pair("kind", s);
                }
                if let Some(s) = &query.tag {
                    q.append_pair("tag", s);
                }
                if let Some(s) = &query.namespace {
                    q.append_pair("namespace", s);
                }
            }
            match self.http.get(url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    return resp
                        .json()
                        .await
                        .map_err(|e| RegistryError::InvalidManifest {
                            reason: format!("search response: {e}"),
                        });
                }
                Ok(resp) => {
                    last_err = RegistryError::BlobBackend(format!(
                        "registry returned status {}",
                        resp.status()
                    ));
                    continue;
                }
                Err(e) => {
                    last_err =
                        RegistryError::BlobBackend(format!("failed to connect to registry: {e}"));
                    continue;
                }
            }
        }
        Err(last_err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_fallback_chain() {
        let client = RegistryClient::from_env_str(
            "https://registry.acme.internal, https://registry.elu.dev",
        )
        .unwrap();

        assert_eq!(client.registries().len(), 2);
        assert_eq!(
            client.registries()[0].as_str(),
            "https://registry.acme.internal/"
        );
        assert_eq!(
            client.registries()[1].as_str(),
            "https://registry.elu.dev/"
        );
    }

    #[test]
    fn parse_single_registry() {
        let client = RegistryClient::from_env_str("https://registry.elu.dev").unwrap();
        assert_eq!(client.registries().len(), 1);
    }

    #[test]
    fn parse_empty_fails() {
        let result = RegistryClient::from_env_str("");
        assert!(result.is_err());
    }

    #[test]
    fn parse_ignores_whitespace_and_empty() {
        let client =
            RegistryClient::from_env_str("  https://a.com  ,  , https://b.com  ").unwrap();
        assert_eq!(client.registries().len(), 2);
    }
}
