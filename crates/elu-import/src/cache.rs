use std::fs;
use std::path::{Path, PathBuf};

use crate::error::ImportError;

/// Cache for upstream tarballs, organized by importer type.
pub struct Cache {
    root: PathBuf,
}

impl Cache {
    /// Create a new cache rooted at `root`. Creates the directory if needed.
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, ImportError> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// Get the cached artifact bytes, if present.
    pub fn get(&self, importer: &str, name: &str, version: &str) -> Option<Vec<u8>> {
        let path = self.path_for(importer, name, version);
        fs::read(&path).ok()
    }

    /// Store artifact bytes in the cache.
    pub fn put(
        &self,
        importer: &str,
        name: &str,
        version: &str,
        data: &[u8],
    ) -> Result<(), ImportError> {
        let path = self.path_for(importer, name, version);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, data)?;
        Ok(())
    }

    fn path_for(&self, importer: &str, name: &str, version: &str) -> PathBuf {
        self.root.join(importer).join(name).join(version)
    }

    /// Root directory of the cache.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_get_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::new(dir.path().join("cache")).unwrap();

        assert!(cache.get("apt", "curl", "8.1.2").is_none());

        cache.put("apt", "curl", "8.1.2", b"fake deb data").unwrap();
        let data = cache.get("apt", "curl", "8.1.2").unwrap();
        assert_eq!(data, b"fake deb data");
    }

    #[test]
    fn miss_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::new(dir.path().join("cache")).unwrap();
        assert!(cache.get("npm", "lodash", "4.17.21").is_none());
    }
}
