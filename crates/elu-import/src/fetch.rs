use crate::error::ImportError;

/// Abstraction over HTTP fetching for testability.
pub trait Fetcher {
    /// Fetch the content at `url` and return the bytes.
    fn get(&self, url: &str) -> Result<Vec<u8>, ImportError>;
}

/// Real HTTP fetcher using reqwest blocking client.
pub struct HttpFetcher {
    client: reqwest::blocking::Client,
}

impl Default for HttpFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpFetcher {
    pub fn new() -> Self {
        Self {
            client: reqwest::blocking::Client::new(),
        }
    }
}

impl Fetcher for HttpFetcher {
    fn get(&self, url: &str) -> Result<Vec<u8>, ImportError> {
        let resp = self
            .client
            .get(url)
            .send()
            .map_err(|e| ImportError::Fetch(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ImportError::Fetch(format!(
                "HTTP {} for {}",
                resp.status(),
                url
            )));
        }

        resp.bytes()
            .map(|b| b.to_vec())
            .map_err(|e| ImportError::Fetch(e.to_string()))
    }
}
