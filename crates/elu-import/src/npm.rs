use std::io::Read as _;
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

pub struct NpmImporter;

impl Importer for NpmImporter {
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

/// Metadata parsed from npm registry JSON for a specific version.
struct NpmVersionInfo {
    version: String,
    description: String,
    tarball_url: String,
    /// Runtime dependencies: name -> version requirement
    dependencies: Vec<(String, String)>,
    /// Full package.json as string for metadata
    package_json: String,
}

impl NpmImporter {
    fn import_single(
        &self,
        name: &str,
        options: &ImportOptions,
        store: &dyn Store,
        cache: &Cache,
        fetcher: &dyn Fetcher,
    ) -> Result<ManifestHash, ImportError> {
        let info = self.fetch_version_info(name, options.version.as_deref(), fetcher)?;
        self.import_from_info(name, &info, store, cache, fetcher)
    }

    fn import_from_info(
        &self,
        name: &str,
        info: &NpmVersionInfo,
        store: &dyn Store,
        cache: &Cache,
        fetcher: &dyn Fetcher,
    ) -> Result<ManifestHash, ImportError> {
        // Normalize name for elu: @scope/pkg -> scope-pkg
        let elu_name = normalize_npm_name(name);

        // Fetch or use cached tarball
        let tgz_bytes = match cache.get("npm", &elu_name, &info.version) {
            Some(bytes) => bytes,
            None => {
                let bytes = fetcher.get(&info.tarball_url)?;
                cache.put("npm", &elu_name, &info.version, &bytes)?;
                bytes
            }
        };

        // Extract tarball to staging, re-rooted at <name>/
        let staging = tempfile::tempdir()?;
        extract_npm_tarball(&tgz_bytes, staging.path(), &elu_name)?;

        // Pack into store
        let packed = tar_layer::pack_dir(staging.path(), store)?;

        let version = parse_npm_version(&info.version)?;

        // Build deps — runtime only
        let deps: Vec<(String, elu_manifest::VersionSpec)> = info
            .dependencies
            .iter()
            .filter_map(|(dep_name, _)| {
                let dep_elu = normalize_npm_name(dep_name);
                let ref_str = format!("npm/{dep_elu}");
                // Validate the ref is valid
                if ref_str.parse::<elu_manifest::PackageRef>().is_ok() {
                    Some((ref_str, elu_manifest::VersionSpec::Any))
                } else {
                    None
                }
            })
            .collect();

        // Build metadata
        let mut meta = toml::value::Table::new();
        let mut npm_meta = toml::value::Table::new();
        npm_meta.insert(
            "package-json".into(),
            toml::Value::String(info.package_json.clone()),
        );
        npm_meta.insert(
            "original-name".into(),
            toml::Value::String(name.to_string()),
        );
        meta.insert("npm".into(), toml::Value::Table(npm_meta));

        let manifest = manifest_build::build_manifest(ManifestParams {
            namespace: "npm".into(),
            name: elu_name,
            version,
            kind: "npm".into(),
            description: if info.description.is_empty() {
                "npm package".into()
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
        version: Option<&str>,
        fetcher: &dyn Fetcher,
    ) -> Result<NpmVersionInfo, ImportError> {
        let url = format!("https://registry.npmjs.org/{name}");
        let bytes = fetcher.get(&url)?;
        let registry: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|e| ImportError::InvalidMetadata(format!("invalid npm registry JSON: {e}")))?;

        parse_npm_registry_json(&registry, version)
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
        let mut queue: std::collections::VecDeque<(String, Option<String>)> =
            std::collections::VecDeque::new();
        queue.push_back((name.to_string(), options.version.clone()));

        while let Some((pkg_name, pkg_version)) = queue.pop_front() {
            let elu_name = normalize_npm_name(&pkg_name);
            if imported.contains_key(&elu_name) {
                continue;
            }

            let info = match self.fetch_version_info(&pkg_name, pkg_version.as_deref(), fetcher) {
                Ok(info) => info,
                Err(_) => continue, // Skip unavailable transitive deps
            };

            // Queue transitive deps before importing (so we can pin later)
            for (dep_name, _) in &info.dependencies {
                let dep_elu = normalize_npm_name(dep_name);
                if !imported.contains_key(&dep_elu) {
                    queue.push_back((dep_name.clone(), None));
                }
            }

            // Import this package
            let tgz_bytes = match cache.get("npm", &elu_name, &info.version) {
                Some(bytes) => bytes,
                None => {
                    let bytes = fetcher.get(&info.tarball_url)?;
                    cache.put("npm", &elu_name, &info.version, &bytes)?;
                    bytes
                }
            };

            let staging = tempfile::tempdir()?;
            extract_npm_tarball(&tgz_bytes, staging.path(), &elu_name)?;
            let packed = tar_layer::pack_dir(staging.path(), store)?;
            let version = parse_npm_version(&info.version)?;

            let deps: Vec<(String, elu_manifest::VersionSpec)> = info
                .dependencies
                .iter()
                .filter_map(|(dep_name, _)| {
                    let dep_elu = normalize_npm_name(dep_name);
                    let ref_str = format!("npm/{dep_elu}");
                    if ref_str.parse::<elu_manifest::PackageRef>().is_ok() {
                        let vs = if let Some(hash) = imported.get(&dep_elu) {
                            elu_manifest::VersionSpec::Pinned(hash.clone())
                        } else {
                            elu_manifest::VersionSpec::Any
                        };
                        Some((ref_str, vs))
                    } else {
                        None
                    }
                })
                .collect();

            let mut meta = toml::value::Table::new();
            let mut npm_meta = toml::value::Table::new();
            npm_meta.insert(
                "package-json".into(),
                toml::Value::String(info.package_json.clone()),
            );
            npm_meta.insert(
                "original-name".into(),
                toml::Value::String(pkg_name.clone()),
            );
            meta.insert("npm".into(), toml::Value::Table(npm_meta));

            let manifest = manifest_build::build_manifest(ManifestParams {
                namespace: "npm".into(),
                name: elu_name.clone(),
                version,
                kind: "npm".into(),
                description: if info.description.is_empty() {
                    "npm package".into()
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

        let top_elu = normalize_npm_name(name);
        imported
            .get(&top_elu)
            .cloned()
            .ok_or_else(|| ImportError::NotFound(format!("failed to import {name}")))
    }
}

/// Parse npm registry JSON to extract version info.
fn parse_npm_registry_json(
    registry: &serde_json::Value,
    version: Option<&str>,
) -> Result<NpmVersionInfo, ImportError> {
    let versions = registry
        .get("versions")
        .and_then(|v| v.as_object())
        .ok_or_else(|| ImportError::InvalidMetadata("missing 'versions' in registry JSON".into()))?;

    // Resolve version
    let version_key = match version {
        Some(v) => v.to_string(),
        None => {
            // Use dist-tags.latest
            registry
                .get("dist-tags")
                .and_then(|dt| dt.get("latest"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    ImportError::InvalidMetadata("no dist-tags.latest in registry JSON".into())
                })?
                .to_string()
        }
    };

    let version_data = versions
        .get(&version_key)
        .ok_or_else(|| ImportError::NoVersion {
            name: registry
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("unknown")
                .to_string(),
            detail: format!("version {version_key} not found"),
        })?;

    let description = version_data
        .get("description")
        .and_then(|d| d.as_str())
        .unwrap_or("")
        .to_string();
    // Ensure single-line description
    let description = description.lines().next().unwrap_or(&description).to_string();

    let tarball_url = version_data
        .get("dist")
        .and_then(|d| d.get("tarball"))
        .and_then(|t| t.as_str())
        .ok_or_else(|| ImportError::InvalidMetadata("missing dist.tarball".into()))?
        .to_string();

    let dependencies = version_data
        .get("dependencies")
        .and_then(|d| d.as_object())
        .map(|deps| {
            deps.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("*").to_string()))
                .collect()
        })
        .unwrap_or_default();

    let package_json =
        serde_json::to_string(version_data).unwrap_or_default();

    Ok(NpmVersionInfo {
        version: version_key,
        description,
        tarball_url,
        dependencies,
        package_json,
    })
}

/// Normalize npm package name for elu: @scope/pkg -> scope-pkg
/// Per proposal option (b): normalize, store original in metadata.
fn normalize_npm_name(name: &str) -> String {
    let name = name.trim();
    if let Some(scoped) = name.strip_prefix('@') {
        scoped.replace('/', "-")
    } else {
        name.to_string()
    }
}

/// Parse npm version string to semver.
fn parse_npm_version(version: &str) -> Result<Version, ImportError> {
    version
        .parse()
        .map_err(|e| ImportError::InvalidMetadata(format!("invalid npm version '{version}': {e}")))
}

/// Extract npm tarball (.tgz), strip top-level `package/` dir,
/// re-root at `<name>/` for node_modules-style stacking.
fn extract_npm_tarball(tgz: &[u8], dest: &Path, name: &str) -> Result<(), ImportError> {
    let gz = flate2::read::GzDecoder::new(tgz);
    let mut archive = tar::Archive::new(gz);

    let target_dir = dest.join(name);
    std::fs::create_dir_all(&target_dir)?;

    for entry in archive
        .entries()
        .map_err(|e| ImportError::Archive(format!("npm tarball entries: {e}")))?
    {
        let mut entry =
            entry.map_err(|e| ImportError::Archive(format!("npm tarball entry: {e}")))?;

        let path = entry
            .path()
            .map_err(|e| ImportError::Archive(format!("npm tarball path: {e}")))?
            .to_path_buf();

        // Strip the top-level directory (usually "package/")
        let stripped = path
            .components()
            .skip(1)
            .collect::<std::path::PathBuf>();

        if stripped.as_os_str().is_empty() {
            continue;
        }

        let dest_path = target_dir.join(&stripped);
        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        if entry.header().entry_type().is_dir() {
            std::fs::create_dir_all(&dest_path)?;
        } else if entry.header().entry_type().is_file() {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            std::fs::write(&dest_path, &buf)?;
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
            // Try longest matching fragment first to avoid ambiguous matches
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

    /// Build a minimal npm tarball (.tgz) with package/ prefix.
    fn build_npm_tgz(files: &[(&str, &[u8])]) -> Vec<u8> {
        let buf = Vec::new();
        let mut ar = tar::Builder::new(buf);
        for (path, content) in files {
            let full_path = format!("package/{path}");
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            ar.append_data(&mut header, &full_path, *content).unwrap();
        }
        let tar_bytes = ar.into_inner().unwrap();

        let mut gz_buf = Vec::new();
        {
            let mut encoder =
                flate2::write::GzEncoder::new(&mut gz_buf, flate2::Compression::fast());
            std::io::Write::write_all(&mut encoder, &tar_bytes).unwrap();
            encoder.finish().unwrap();
        }
        gz_buf
    }

    /// Build mock npm registry JSON for a package.
    fn build_registry_json(
        name: &str,
        version: &str,
        description: &str,
        tarball_url: &str,
        deps: &[(&str, &str)],
    ) -> Vec<u8> {
        let dep_obj: serde_json::Map<String, serde_json::Value> = deps
            .iter()
            .map(|(k, v)| (k.to_string(), serde_json::Value::String(v.to_string())))
            .collect();

        let registry = serde_json::json!({
            "name": name,
            "dist-tags": { "latest": version },
            "versions": {
                version: {
                    "name": name,
                    "version": version,
                    "description": description,
                    "dist": {
                        "tarball": tarball_url,
                    },
                    "dependencies": dep_obj,
                }
            }
        });
        serde_json::to_vec(&registry).unwrap()
    }

    #[test]
    fn import_single_npm_produces_valid_manifest_and_ref() {
        let (_dir, store) = test_store();
        let cache_dir = tempfile::TempDir::new().unwrap();
        let cache = Cache::new(cache_dir.path().join("cache")).unwrap();

        let tgz = build_npm_tgz(&[
            ("index.js", b"module.exports = {};\n"),
            ("package.json", b"{\"name\":\"test-pkg\",\"version\":\"1.0.0\"}"),
        ]);

        let registry = build_registry_json(
            "test-pkg",
            "1.0.0",
            "a test package",
            "https://registry.npmjs.org/test-pkg/-/test-pkg-1.0.0.tgz",
            &[("dep-a", "^1.0.0")],
        );

        let mut fetcher = MockFetcher::new();
        fetcher.add("registry.npmjs.org/test-pkg/-/", tgz);
        fetcher.add("registry.npmjs.org/test-pkg", registry);

        let importer = NpmImporter;
        let options = ImportOptions {
            version: Some("1.0.0".into()),
            ..Default::default()
        };

        let hash = importer
            .import("test-pkg", &options, &store, &cache, &fetcher)
            .unwrap();

        // Verify ref
        let ref_hash = store.get_ref("npm", "test-pkg", "1.0.0").unwrap();
        assert_eq!(ref_hash, Some(hash.clone()));

        // Verify manifest
        let manifest_bytes = store.get_manifest(&hash).unwrap().unwrap();
        let manifest: elu_manifest::Manifest =
            serde_json::from_slice(&manifest_bytes).unwrap();

        assert_eq!(manifest.package.namespace, "npm");
        assert_eq!(manifest.package.name, "test-pkg");
        assert_eq!(manifest.package.kind, "npm");
        assert_eq!(manifest.package.description, "a test package");
        assert_eq!(manifest.layers.len(), 1);
        assert!(manifest.layers[0].diff_id.is_some());

        // Check dependency
        assert_eq!(manifest.dependencies.len(), 1);
        assert_eq!(manifest.dependencies[0].reference.as_str(), "npm/dep-a");

        // Check metadata
        assert!(manifest.metadata.0.contains_key("npm"));

        validate_stored(&manifest).unwrap();
    }

    #[test]
    fn normalize_npm_name_handles_scoped() {
        assert_eq!(normalize_npm_name("@babel/core"), "babel-core");
        assert_eq!(normalize_npm_name("lodash"), "lodash");
        assert_eq!(normalize_npm_name("@types/node"), "types-node");
    }

    #[test]
    fn extract_npm_tarball_re_roots() {
        let tgz = build_npm_tgz(&[
            ("index.js", b"hello"),
            ("lib/util.js", b"util"),
        ]);

        let dest = tempfile::tempdir().unwrap();
        extract_npm_tarball(&tgz, dest.path(), "my-pkg").unwrap();

        assert!(dest.path().join("my-pkg/index.js").exists());
        assert!(dest.path().join("my-pkg/lib/util.js").exists());
        assert_eq!(
            std::fs::read_to_string(dest.path().join("my-pkg/index.js")).unwrap(),
            "hello"
        );
    }

    #[test]
    fn import_closure_imports_transitive_npm_deps() {
        let (_dir, store) = test_store();
        let cache_dir = tempfile::TempDir::new().unwrap();
        let cache = Cache::new(cache_dir.path().join("cache")).unwrap();

        // top -> mid -> leaf
        let leaf_tgz = build_npm_tgz(&[("index.js", b"leaf")]);
        let leaf_reg = build_registry_json(
            "leaf",
            "1.0.0",
            "leaf pkg",
            "https://registry.npmjs.org/leaf/-/leaf-1.0.0.tgz",
            &[],
        );

        let mid_tgz = build_npm_tgz(&[("index.js", b"mid")]);
        let mid_reg = build_registry_json(
            "mid",
            "1.0.0",
            "mid pkg",
            "https://registry.npmjs.org/mid/-/mid-1.0.0.tgz",
            &[("leaf", "^1.0.0")],
        );

        let top_tgz = build_npm_tgz(&[("index.js", b"top")]);
        let top_reg = build_registry_json(
            "top",
            "1.0.0",
            "top pkg",
            "https://registry.npmjs.org/top/-/top-1.0.0.tgz",
            &[("mid", "^1.0.0")],
        );

        let mut fetcher = MockFetcher::new();
        fetcher.add("registry.npmjs.org/top/-/", top_tgz);
        fetcher.add("registry.npmjs.org/top", top_reg);
        fetcher.add("registry.npmjs.org/mid/-/", mid_tgz);
        fetcher.add("registry.npmjs.org/mid", mid_reg);
        fetcher.add("registry.npmjs.org/leaf/-/", leaf_tgz);
        fetcher.add("registry.npmjs.org/leaf", leaf_reg);

        let importer = NpmImporter;
        let options = ImportOptions {
            version: Some("1.0.0".into()),
            closure: true,
            ..Default::default()
        };

        let top_hash = importer
            .import("top", &options, &store, &cache, &fetcher)
            .unwrap();

        // All three should be in the store
        assert!(store.get_ref("npm", "top", "1.0.0").unwrap().is_some());
        assert!(store.get_ref("npm", "mid", "1.0.0").unwrap().is_some());
        assert!(store.get_ref("npm", "leaf", "1.0.0").unwrap().is_some());

        // Top should depend on mid
        let top_bytes = store.get_manifest(&top_hash).unwrap().unwrap();
        let top_manifest: elu_manifest::Manifest =
            serde_json::from_slice(&top_bytes).unwrap();
        assert_eq!(top_manifest.dependencies.len(), 1);
        assert_eq!(top_manifest.dependencies[0].reference.as_str(), "npm/mid");
    }
}
