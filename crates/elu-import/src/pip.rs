use std::io::{Cursor, Read as _};
use std::path::Path;

use elu_store::hash::ManifestHash;
use elu_store::store::Store;
use semver::Version;

use crate::cache::Cache;
use crate::error::ImportError;
use crate::fetch::Fetcher;
use crate::manifest_build::{self, ManifestParams};
use crate::tar_layer;
use crate::{ImportOptions, Importer};

pub struct PipImporter;

impl Importer for PipImporter {
    fn import(
        &self,
        name: &str,
        options: &ImportOptions,
        store: &dyn Store,
        cache: &Cache,
        fetcher: &dyn Fetcher,
    ) -> Result<ManifestHash, ImportError> {
        if options.closure {
            return self.import_closure(name, options, store, cache, fetcher);
        }
        self.import_single(name, options, store, cache, fetcher)
    }
}

/// Metadata parsed from PyPI JSON API for a specific version.
struct PypiVersionInfo {
    version: String,
    description: String,
    wheel_url: String,
    /// Dependencies from Requires-Dist
    requires_dist: Vec<String>,
    /// Original METADATA content
    metadata_raw: String,
    /// Wheel tags
    wheel_tags: String,
}

impl PipImporter {
    fn import_single(
        &self,
        name: &str,
        options: &ImportOptions,
        store: &dyn Store,
        cache: &Cache,
        fetcher: &dyn Fetcher,
    ) -> Result<ManifestHash, ImportError> {
        let info = self.fetch_version_info(name, options, fetcher)?;
        self.import_from_info(name, &info, store, cache, fetcher)
    }

    fn import_from_info(
        &self,
        name: &str,
        info: &PypiVersionInfo,
        store: &dyn Store,
        cache: &Cache,
        fetcher: &dyn Fetcher,
    ) -> Result<ManifestHash, ImportError> {
        let elu_name = normalize_pip_name(name);

        // Fetch or use cached wheel
        let wheel_bytes = match cache.get("pip", &elu_name, &info.version) {
            Some(bytes) => bytes,
            None => {
                let bytes = fetcher.get(&info.wheel_url)?;
                cache.put("pip", &elu_name, &info.version, &bytes)?;
                bytes
            }
        };

        // Extract wheel to staging at site-packages/<package>/
        let staging = tempfile::tempdir()?;
        extract_wheel(&wheel_bytes, staging.path(), &elu_name)?;

        // Pack into store
        let packed = tar_layer::pack_dir(staging.path(), store)?;

        let version = parse_pip_version(&info.version)?;

        // Parse deps from Requires-Dist (skip markers/extras for v1)
        let deps = parse_requires_dist(&info.requires_dist);

        // Build metadata
        let mut meta = toml::value::Table::new();
        let mut pip_meta = toml::value::Table::new();
        pip_meta.insert("metadata".into(), toml::Value::String(info.metadata_raw.clone()));
        pip_meta.insert("wheel-tags".into(), toml::Value::String(info.wheel_tags.clone()));
        meta.insert("pip".into(), toml::Value::Table(pip_meta));

        let manifest = manifest_build::build_manifest(ManifestParams {
            namespace: "pip".into(),
            name: elu_name,
            version,
            kind: "pip".into(),
            description: if info.description.is_empty() {
                "Python package".into()
            } else {
                info.description.clone()
            },
            diff_id: packed.diff_id,
            layer_size: packed.size,
            dependencies: deps,
            metadata: meta,
        })
        .map_err(ImportError::InvalidMetadata)?;

        manifest_build::store_manifest(&manifest, store)
    }

    fn fetch_version_info(
        &self,
        name: &str,
        options: &ImportOptions,
        fetcher: &dyn Fetcher,
    ) -> Result<PypiVersionInfo, ImportError> {
        let url = format!("https://pypi.org/pypi/{name}/json");
        let bytes = fetcher.get(&url)?;
        let pypi: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|e| ImportError::InvalidMetadata(format!("invalid PyPI JSON: {e}")))?;

        let target = options.target.as_deref().unwrap_or("py3-none-any");
        parse_pypi_json(&pypi, options.version.as_deref(), target)
    }

    fn import_closure(
        &self,
        name: &str,
        options: &ImportOptions,
        store: &dyn Store,
        cache: &Cache,
        fetcher: &dyn Fetcher,
    ) -> Result<ManifestHash, ImportError> {
        let mut imported: std::collections::HashMap<String, ManifestHash> =
            std::collections::HashMap::new();
        let mut queue: std::collections::VecDeque<String> =
            std::collections::VecDeque::new();
        queue.push_back(name.to_string());

        let target = options.target.as_deref().unwrap_or("py3-none-any");

        while let Some(pkg_name) = queue.pop_front() {
            let elu_name = normalize_pip_name(&pkg_name);
            if imported.contains_key(&elu_name) {
                continue;
            }

            let pkg_options = if pkg_name == name {
                ImportOptions {
                    version: options.version.clone(),
                    target: Some(target.to_string()),
                    ..Default::default()
                }
            } else {
                ImportOptions {
                    target: Some(target.to_string()),
                    ..Default::default()
                }
            };

            let info = match self.fetch_version_info(&pkg_name, &pkg_options, fetcher) {
                Ok(info) => info,
                Err(_) => continue, // Skip unavailable transitive deps
            };

            // Queue transitive deps
            for req in &info.requires_dist {
                let dep_name = req
                    .split(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_' && c != '.')
                    .next()
                    .unwrap_or(req)
                    .trim();
                if !dep_name.is_empty() {
                    let dep_elu = normalize_pip_name(dep_name);
                    if !imported.contains_key(&dep_elu) {
                        queue.push_back(dep_name.to_string());
                    }
                }
            }

            // Fetch wheel
            let wheel_bytes = match cache.get("pip", &elu_name, &info.version) {
                Some(bytes) => bytes,
                None => {
                    let bytes = fetcher.get(&info.wheel_url)?;
                    cache.put("pip", &elu_name, &info.version, &bytes)?;
                    bytes
                }
            };

            let staging = tempfile::tempdir()?;
            extract_wheel(&wheel_bytes, staging.path(), &elu_name)?;
            let packed = tar_layer::pack_dir(staging.path(), store)?;
            let version = parse_pip_version(&info.version)?;

            let deps: Vec<(String, elu_manifest::VersionSpec)> =
                parse_requires_dist(&info.requires_dist)
                    .into_iter()
                    .map(|(ref_str, _)| {
                        let dep_elu = ref_str.strip_prefix("pip/").unwrap_or(&ref_str);
                        if let Some(hash) = imported.get(dep_elu) {
                            (ref_str, elu_manifest::VersionSpec::Pinned(hash.clone()))
                        } else {
                            (ref_str, elu_manifest::VersionSpec::Any)
                        }
                    })
                    .collect();

            let mut meta = toml::value::Table::new();
            let mut pip_meta = toml::value::Table::new();
            pip_meta.insert("metadata".into(), toml::Value::String(info.metadata_raw.clone()));
            pip_meta.insert("wheel-tags".into(), toml::Value::String(info.wheel_tags.clone()));
            meta.insert("pip".into(), toml::Value::Table(pip_meta));

            let manifest = manifest_build::build_manifest(ManifestParams {
                namespace: "pip".into(),
                name: elu_name.clone(),
                version,
                kind: "pip".into(),
                description: if info.description.is_empty() {
                    "Python package".into()
                } else {
                    info.description.clone()
                },
                diff_id: packed.diff_id,
                layer_size: packed.size,
                dependencies: deps,
                metadata: meta,
            })
            .map_err(ImportError::InvalidMetadata)?;

            let hash = manifest_build::store_manifest(&manifest, store)?;
            imported.insert(elu_name, hash);
        }

        let top_elu = normalize_pip_name(name);
        imported
            .get(&top_elu)
            .cloned()
            .ok_or_else(|| ImportError::NotFound(format!("failed to import {name}")))
    }
}

/// Parse PyPI JSON API response for version info.
fn parse_pypi_json(
    pypi: &serde_json::Value,
    version: Option<&str>,
    target: &str,
) -> Result<PypiVersionInfo, ImportError> {
    let info = pypi
        .get("info")
        .ok_or_else(|| ImportError::InvalidMetadata("missing 'info' in PyPI JSON".into()))?;

    let pkg_version = match version {
        Some(v) => v.to_string(),
        None => info
            .get("version")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ImportError::InvalidMetadata("missing version in PyPI JSON".into()))?
            .to_string(),
    };

    let description = info
        .get("summary")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();
    let description = description.lines().next().unwrap_or(&description).to_string();

    // Find wheel URL for the requested version
    let releases = pypi
        .get("releases")
        .and_then(|r| r.as_object())
        .ok_or_else(|| ImportError::InvalidMetadata("missing 'releases' in PyPI JSON".into()))?;

    let version_files = releases
        .get(&pkg_version)
        .and_then(|v| v.as_array())
        .ok_or_else(|| ImportError::NoVersion {
            name: info
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("unknown")
                .to_string(),
            detail: format!("version {pkg_version} not found"),
        })?;

    // Find a wheel matching the target tag, or fall back to any wheel
    let wheel_file = version_files
        .iter()
        .find(|f| {
            f.get("packagetype")
                .and_then(|p| p.as_str())
                == Some("bdist_wheel")
                && f.get("filename")
                    .and_then(|n| n.as_str())
                    .is_some_and(|n| n.contains(target))
        })
        .or_else(|| {
            version_files.iter().find(|f| {
                f.get("packagetype")
                    .and_then(|p| p.as_str())
                    == Some("bdist_wheel")
            })
        })
        .ok_or_else(|| ImportError::NoVersion {
            name: info
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("unknown")
                .to_string(),
            detail: format!("no wheel found for version {pkg_version}"),
        })?;

    let wheel_url = wheel_file
        .get("url")
        .and_then(|u| u.as_str())
        .ok_or_else(|| ImportError::InvalidMetadata("missing wheel URL".into()))?
        .to_string();

    let wheel_tags = wheel_file
        .get("filename")
        .and_then(|f| f.as_str())
        .unwrap_or("")
        .to_string();

    // Extract requires_dist from info
    let requires_dist = info
        .get("requires_dist")
        .and_then(|r| r.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    // Build metadata string
    let metadata_raw = format!(
        "Name: {}\nVersion: {}\nSummary: {}",
        info.get("name").and_then(|n| n.as_str()).unwrap_or(""),
        pkg_version,
        description,
    );

    Ok(PypiVersionInfo {
        version: pkg_version,
        description,
        wheel_url,
        requires_dist,
        metadata_raw,
        wheel_tags,
    })
}

/// Normalize pip package name per PEP 503: lowercase, non-alphanumerics to hyphens.
fn normalize_pip_name(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// Parse pip version to semver.
fn parse_pip_version(version: &str) -> Result<Version, ImportError> {
    // Pip versions like "2.31.0" are usually valid semver.
    // For versions with fewer segments (e.g. "1.0"), pad with zeros.
    let v = version.trim();
    // Strip any pre-release/local suffixes that aren't semver-compatible
    let v = v.split(|c: char| !c.is_ascii_digit() && c != '.').next().unwrap_or(v);

    let parts: Vec<u64> = v
        .split('.')
        .filter_map(|s| s.parse().ok())
        .collect();

    let major = parts.first().copied().unwrap_or(0);
    let minor = parts.get(1).copied().unwrap_or(0);
    let patch = parts.get(2).copied().unwrap_or(0);

    Ok(Version::new(major, minor, patch))
}

/// Parse Requires-Dist entries into elu dependencies.
/// v1: ignore environment markers and extras.
fn parse_requires_dist(requires: &[String]) -> Vec<(String, elu_manifest::VersionSpec)> {
    requires
        .iter()
        .filter_map(|req| {
            // Skip entries with markers (contain ";")
            // v1 simplification: import all, ignore markers
            let req = req.split(';').next().unwrap_or(req).trim();

            // Extract package name (before any version specifier)
            let name = req
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_' && c != '.')
                .next()
                .unwrap_or(req)
                .trim();

            if name.is_empty() {
                return None;
            }

            let elu_name = normalize_pip_name(name);
            let ref_str = format!("pip/{elu_name}");

            if ref_str.parse::<elu_manifest::PackageRef>().is_ok() {
                Some((ref_str, elu_manifest::VersionSpec::Any))
            } else {
                None
            }
        })
        .collect()
}

/// Extract a wheel (zip file) into staging at site-packages/<package>/.
fn extract_wheel(wheel_bytes: &[u8], dest: &Path, name: &str) -> Result<(), ImportError> {
    let cursor = Cursor::new(wheel_bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| ImportError::Archive(format!("invalid wheel zip: {e}")))?;

    let site_packages = dest.join("site-packages").join(name);
    std::fs::create_dir_all(&site_packages)?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| ImportError::Archive(format!("wheel entry {i}: {e}")))?;

        let outpath = site_packages.join(file.name());

        if file.is_dir() {
            std::fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)?;
            std::fs::write(&outpath, &buf)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::Fetcher;
    use elu_manifest::validate::validate_stored;

    struct MockFetcher {
        data: std::collections::HashMap<String, Vec<u8>>,
    }

    impl MockFetcher {
        fn new() -> Self {
            Self {
                data: std::collections::HashMap::new(),
            }
        }

        fn add(&mut self, url_fragment: &str, data: Vec<u8>) {
            self.data.insert(url_fragment.to_string(), data);
        }
    }

    impl Fetcher for MockFetcher {
        fn get(&self, url: &str) -> Result<Vec<u8>, ImportError> {
            let mut matches: Vec<_> = self
                .data
                .iter()
                .filter(|(fragment, _)| url.contains(fragment.as_str()))
                .collect();
            matches.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
            if let Some((_, data)) = matches.first() {
                return Ok(data.to_vec());
            }
            Err(ImportError::NotFound(format!("mock: no data for {url}")))
        }
    }

    fn test_store() -> (tempfile::TempDir, elu_store::fs_store::FsStore) {
        let dir = tempfile::TempDir::new().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let store =
            elu_store::fs_store::FsStore::init_with_fsync(root, elu_store::atomic::FsyncMode::Never)
                .unwrap();
        (dir, store)
    }

    /// Build a minimal wheel (.whl = zip file).
    fn build_test_wheel(files: &[(&str, &[u8])]) -> Vec<u8> {
        let buf = Vec::new();
        let cursor = Cursor::new(buf);
        let mut zip = zip::ZipWriter::new(cursor);
        let options = zip::write::SimpleFileOptions::default();

        for (path, content) in files {
            zip.start_file(path.to_string(), options).unwrap();
            std::io::Write::write_all(&mut zip, content).unwrap();
        }

        zip.finish().unwrap().into_inner()
    }

    /// Build mock PyPI JSON for a package.
    fn build_pypi_json(
        name: &str,
        version: &str,
        summary: &str,
        wheel_url: &str,
        wheel_filename: &str,
        requires_dist: &[&str],
    ) -> Vec<u8> {
        let requires: Vec<serde_json::Value> = requires_dist
            .iter()
            .map(|s| serde_json::Value::String(s.to_string()))
            .collect();

        let pypi = serde_json::json!({
            "info": {
                "name": name,
                "version": version,
                "summary": summary,
                "requires_dist": requires,
            },
            "releases": {
                version: [
                    {
                        "packagetype": "bdist_wheel",
                        "url": wheel_url,
                        "filename": wheel_filename,
                    }
                ]
            }
        });
        serde_json::to_vec(&pypi).unwrap()
    }

    #[test]
    fn import_single_pip_produces_valid_manifest_and_ref() {
        let (_dir, store) = test_store();
        let cache_dir = tempfile::TempDir::new().unwrap();
        let cache = Cache::new(cache_dir.path().join("cache")).unwrap();

        let wheel = build_test_wheel(&[
            ("requests/__init__.py", b"# requests\n"),
            ("requests/api.py", b"def get(): pass\n"),
        ]);

        let pypi_json = build_pypi_json(
            "requests",
            "2.31.0",
            "Python HTTP for Humans",
            "https://files.pythonhosted.org/packages/requests-2.31.0-py3-none-any.whl",
            "requests-2.31.0-py3-none-any.whl",
            &["urllib3>=1.21.1", "certifi>=2017.4.17"],
        );

        let mut fetcher = MockFetcher::new();
        fetcher.add("files.pythonhosted.org", wheel);
        fetcher.add("pypi.org/pypi/requests", pypi_json);

        let importer = PipImporter;
        let options = ImportOptions {
            version: Some("2.31.0".into()),
            ..Default::default()
        };

        let hash = importer
            .import("requests", &options, &store, &cache, &fetcher)
            .unwrap();

        // Verify ref
        let ref_hash = store.get_ref("pip", "requests", "2.31.0").unwrap();
        assert_eq!(ref_hash, Some(hash.clone()));

        // Verify manifest
        let manifest_bytes = store.get_manifest(&hash).unwrap().unwrap();
        let manifest: elu_manifest::Manifest =
            serde_json::from_slice(&manifest_bytes).unwrap();

        assert_eq!(manifest.package.namespace, "pip");
        assert_eq!(manifest.package.name, "requests");
        assert_eq!(manifest.package.kind, "pip");
        assert_eq!(manifest.package.description, "Python HTTP for Humans");
        assert_eq!(manifest.layers.len(), 1);
        assert!(manifest.layers[0].diff_id.is_some());

        // Check dependencies
        assert_eq!(manifest.dependencies.len(), 2);
        let dep_refs: Vec<&str> = manifest
            .dependencies
            .iter()
            .map(|d| d.reference.as_str())
            .collect();
        assert!(dep_refs.contains(&"pip/urllib3"));
        assert!(dep_refs.contains(&"pip/certifi"));

        // Check metadata
        assert!(manifest.metadata.0.contains_key("pip"));

        validate_stored(&manifest).unwrap();
    }

    #[test]
    fn normalize_pip_name_follows_pep503() {
        assert_eq!(normalize_pip_name("Requests"), "requests");
        assert_eq!(normalize_pip_name("Flask_RESTful"), "flask-restful");
        assert_eq!(normalize_pip_name("my.package"), "my-package");
    }

    #[test]
    fn parse_requires_dist_extracts_names() {
        let reqs = vec![
            "urllib3>=1.21.1".to_string(),
            "certifi>=2017.4.17".to_string(),
            "PySocks!=1.5.7 ; extra == 'socks'".to_string(),
        ];
        let deps = parse_requires_dist(&reqs);
        assert_eq!(deps.len(), 3);
        assert_eq!(deps[0].0, "pip/urllib3");
        assert_eq!(deps[1].0, "pip/certifi");
        assert_eq!(deps[2].0, "pip/pysocks");
    }

    #[test]
    fn extract_wheel_puts_files_under_site_packages() {
        let wheel = build_test_wheel(&[
            ("mylib/__init__.py", b"# init"),
            ("mylib/core.py", b"# core"),
        ]);

        let dest = tempfile::tempdir().unwrap();
        extract_wheel(&wheel, dest.path(), "mylib").unwrap();

        assert!(dest.path().join("site-packages/mylib/mylib/__init__.py").exists());
        assert!(dest.path().join("site-packages/mylib/mylib/core.py").exists());
    }

    #[test]
    fn import_closure_imports_transitive_pip_deps() {
        let (_dir, store) = test_store();
        let cache_dir = tempfile::TempDir::new().unwrap();
        let cache = Cache::new(cache_dir.path().join("cache")).unwrap();

        // top -> mid -> leaf
        let leaf_wheel = build_test_wheel(&[("leaf/__init__.py", b"leaf")]);
        let leaf_json = build_pypi_json(
            "leaf",
            "1.0.0",
            "leaf pkg",
            "https://files.pythonhosted.org/leaf-1.0.0-py3-none-any.whl",
            "leaf-1.0.0-py3-none-any.whl",
            &[],
        );

        let mid_wheel = build_test_wheel(&[("mid/__init__.py", b"mid")]);
        let mid_json = build_pypi_json(
            "mid",
            "1.0.0",
            "mid pkg",
            "https://files.pythonhosted.org/mid-1.0.0-py3-none-any.whl",
            "mid-1.0.0-py3-none-any.whl",
            &["leaf>=1.0"],
        );

        let top_wheel = build_test_wheel(&[("top/__init__.py", b"top")]);
        let top_json = build_pypi_json(
            "top",
            "1.0.0",
            "top pkg",
            "https://files.pythonhosted.org/top-1.0.0-py3-none-any.whl",
            "top-1.0.0-py3-none-any.whl",
            &["mid>=1.0"],
        );

        let mut fetcher = MockFetcher::new();
        fetcher.add("pythonhosted.org/top-1.0.0", top_wheel);
        fetcher.add("pypi.org/pypi/top", top_json);
        fetcher.add("pythonhosted.org/mid-1.0.0", mid_wheel);
        fetcher.add("pypi.org/pypi/mid", mid_json);
        fetcher.add("pythonhosted.org/leaf-1.0.0", leaf_wheel);
        fetcher.add("pypi.org/pypi/leaf", leaf_json);

        let importer = PipImporter;
        let options = ImportOptions {
            version: Some("1.0.0".into()),
            closure: true,
            ..Default::default()
        };

        let top_hash = importer
            .import("top", &options, &store, &cache, &fetcher)
            .unwrap();

        // All three should be in the store
        assert!(store.get_ref("pip", "top", "1.0.0").unwrap().is_some());
        assert!(store.get_ref("pip", "mid", "1.0.0").unwrap().is_some());
        assert!(store.get_ref("pip", "leaf", "1.0.0").unwrap().is_some());

        // Top should depend on mid
        let top_bytes = store.get_manifest(&top_hash).unwrap().unwrap();
        let top_manifest: elu_manifest::Manifest =
            serde_json::from_slice(&top_bytes).unwrap();
        assert_eq!(top_manifest.dependencies.len(), 1);
        assert_eq!(top_manifest.dependencies[0].reference.as_str(), "pip/mid");
    }
}
