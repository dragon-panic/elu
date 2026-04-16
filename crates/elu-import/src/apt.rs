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

pub struct AptImporter;

impl Importer for AptImporter {
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

impl AptImporter {
    fn import_single(
        &self,
        name: &str,
        options: &ImportOptions,
        store: &dyn Store,
        cache: &Cache,
        fetcher: &dyn Fetcher,
    ) -> Result<ManifestHash, ImportError> {
        // For now, we expect the caller (or test) to provide the .deb data via the fetcher
        // In a real scenario we'd parse Packages index first
        let dist = options.dist.as_deref().unwrap_or("bookworm");
        let version_str = options.version.as_deref().unwrap_or("latest");

        // Check cache first
        let deb_bytes = match cache.get("apt", name, version_str) {
            Some(bytes) => bytes,
            None => {
                // Fetch the .deb
                let url = format!(
                    "https://deb.debian.org/debian/pool/main/{prefix}/{name}/{name}_{version}.deb",
                    prefix = &name[..1],
                    name = name,
                    version = version_str,
                );
                let bytes = fetcher.get(&url)?;
                cache.put("apt", name, version_str, &bytes)?;
                bytes
            }
        };

        // Parse the .deb and extract data
        let deb_info = parse_deb(&deb_bytes)?;

        // Use parsed version if available, fall back to version_str
        let version = parse_deb_version(&deb_info.version)?;

        // Extract data.tar into staging dir
        let staging = tempfile::tempdir()?;
        extract_data_tar(&deb_info.data_tar, &deb_info.data_compression, staging.path())?;

        // Pack into store
        let packed = tar_layer::pack_dir(staging.path(), store)?;

        // Parse dependencies
        let deps = parse_depends(&deb_info.depends);

        // Build metadata
        let mut meta = toml::value::Table::new();
        let mut apt_meta = toml::value::Table::new();
        apt_meta.insert("control".into(), toml::Value::String(deb_info.control_raw.clone()));
        apt_meta.insert("distribution".into(), toml::Value::String(dist.to_string()));
        meta.insert("apt".into(), toml::Value::Table(apt_meta));

        let manifest = manifest_build::build_manifest(ManifestParams {
            namespace: "debian".into(),
            name: name.into(),
            version,
            kind: "debian".into(),
            description: deb_info.description.clone(),
            diff_id: packed.diff_id,
            layer_size: packed.size,
            dependencies: deps,
            metadata: meta,
        })
        .map_err(ImportError::InvalidMetadata)?;

        manifest_build::store_manifest(&manifest, store)
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

        let dist = options.dist.as_deref().unwrap_or("bookworm");

        while let Some(pkg_name) = queue.pop_front() {
            if imported.contains_key(&pkg_name) {
                continue;
            }

            // Check if already in store
            // We don't know the version ahead of time for transitive deps,
            // so we always try to import
            let version_str = if pkg_name == name {
                options.version.as_deref().unwrap_or("latest")
            } else {
                "latest"
            };

            let deb_bytes = match cache.get("apt", &pkg_name, version_str) {
                Some(bytes) => bytes,
                None => {
                    let url = format!(
                        "https://deb.debian.org/debian/pool/main/{prefix}/{pkg}/{pkg}_{ver}.deb",
                        prefix = &pkg_name[..1],
                        pkg = pkg_name,
                        ver = version_str,
                    );
                    match fetcher.get(&url) {
                        Ok(bytes) => {
                            cache.put("apt", &pkg_name, version_str, &bytes)?;
                            bytes
                        }
                        Err(_) => {
                            // Transitive dep not available — skip gracefully
                            continue;
                        }
                    }
                }
            };

            let deb_info = parse_deb(&deb_bytes)?;
            let version = parse_deb_version(&deb_info.version)?;

            let staging = tempfile::tempdir()?;
            extract_data_tar(&deb_info.data_tar, &deb_info.data_compression, staging.path())?;
            let packed = tar_layer::pack_dir(staging.path(), store)?;

            // Parse raw dep names for queue
            let dep_names = parse_dep_names(&deb_info.depends);
            for dep_name in &dep_names {
                if !imported.contains_key(dep_name) {
                    queue.push_back(dep_name.clone());
                }
            }

            // Build deps as pinned refs to already-imported packages, or Any for not-yet-imported
            let deps: Vec<(String, elu_manifest::VersionSpec)> = parse_depends(&deb_info.depends)
                .into_iter()
                .map(|(ref_str, _)| {
                    let dep_pkg = ref_str.strip_prefix("debian/").unwrap_or(&ref_str);
                    if let Some(hash) = imported.get(dep_pkg) {
                        (ref_str, elu_manifest::VersionSpec::Pinned(hash.clone()))
                    } else {
                        (ref_str, elu_manifest::VersionSpec::Any)
                    }
                })
                .collect();

            let mut meta = toml::value::Table::new();
            let mut apt_meta = toml::value::Table::new();
            apt_meta.insert("control".into(), toml::Value::String(deb_info.control_raw.clone()));
            apt_meta.insert("distribution".into(), toml::Value::String(dist.to_string()));
            meta.insert("apt".into(), toml::Value::Table(apt_meta));

            let manifest = manifest_build::build_manifest(ManifestParams {
                namespace: "debian".into(),
                name: pkg_name.clone(),
                version,
                kind: "debian".into(),
                description: deb_info.description.clone(),
                diff_id: packed.diff_id,
                layer_size: packed.size,
                dependencies: deps,
                metadata: meta,
            })
            .map_err(ImportError::InvalidMetadata)?;

            let hash = manifest_build::store_manifest(&manifest, store)?;
            imported.insert(pkg_name, hash);
        }

        imported
            .get(name)
            .cloned()
            .ok_or_else(|| ImportError::NotFound(format!("failed to import {name}")))
    }
}

/// Parsed information from a .deb file.
struct DebInfo {
    version: String,
    description: String,
    depends: String,
    control_raw: String,
    data_tar: Vec<u8>,
    data_compression: DataCompression,
}

enum DataCompression {
    Gzip,
    Xz,
    None,
}

/// Parse a .deb archive (ar format) and extract control info + data tar.
fn parse_deb(deb_bytes: &[u8]) -> Result<DebInfo, ImportError> {
    let mut archive = ar::Archive::new(Cursor::new(deb_bytes));

    let mut control_tar: Option<(Vec<u8>, DataCompression)> = None;
    let mut data_tar: Option<(Vec<u8>, DataCompression)> = None;

    while let Some(entry) = archive.next_entry() {
        let mut entry = entry.map_err(|e| ImportError::Archive(format!("ar entry: {e}")))?;
        let name = String::from_utf8_lossy(entry.header().identifier()).to_string();

        let mut buf = Vec::new();
        entry
            .read_to_end(&mut buf)
            .map_err(|e| ImportError::Archive(format!("reading {name}: {e}")))?;

        if name.starts_with("control.tar") {
            let comp = detect_compression(&name);
            control_tar = Some((buf, comp));
        } else if name.starts_with("data.tar") {
            let comp = detect_compression(&name);
            data_tar = Some((buf, comp));
        }
    }

    let (control_bytes, control_comp) =
        control_tar.ok_or_else(|| ImportError::Archive("missing control.tar in .deb".into()))?;
    let (data_bytes, data_comp) =
        data_tar.ok_or_else(|| ImportError::Archive("missing data.tar in .deb".into()))?;

    // Extract control file from control.tar
    let control_raw = extract_control_file(&control_bytes, &control_comp)?;
    let fields = parse_control(&control_raw);

    let version = fields
        .get("Version")
        .cloned()
        .unwrap_or_default();
    let description = fields
        .get("Description")
        .cloned()
        .unwrap_or_else(|| "imported Debian package".into());
    // Take only the first line of description for the manifest
    let description = description.lines().next().unwrap_or(&description).to_string();
    let depends = fields.get("Depends").cloned().unwrap_or_default();

    Ok(DebInfo {
        version,
        description,
        depends,
        control_raw,
        data_tar: data_bytes,
        data_compression: data_comp,
    })
}

fn detect_compression(filename: &str) -> DataCompression {
    if filename.ends_with(".gz") {
        DataCompression::Gzip
    } else if filename.ends_with(".xz") {
        DataCompression::Xz
    } else {
        DataCompression::None
    }
}

/// Extract the `control` file from a control.tar.* archive.
fn extract_control_file(
    bytes: &[u8],
    compression: &DataCompression,
) -> Result<String, ImportError> {
    let decompressed = decompress(bytes, compression)?;
    let mut archive = tar::Archive::new(Cursor::new(decompressed));

    for entry in archive
        .entries()
        .map_err(|e| ImportError::Archive(format!("control tar entries: {e}")))?
    {
        let mut entry =
            entry.map_err(|e| ImportError::Archive(format!("control tar entry: {e}")))?;
        let path = entry
            .path()
            .map_err(|e| ImportError::Archive(format!("control tar path: {e}")))?
            .to_string_lossy()
            .to_string();

        if path == "./control" || path == "control" {
            let mut content = String::new();
            entry
                .read_to_string(&mut content)
                .map_err(|e| ImportError::Archive(format!("reading control: {e}")))?;
            return Ok(content);
        }
    }

    Err(ImportError::Archive(
        "control file not found in control.tar".into(),
    ))
}

/// Extract data.tar into a staging directory.
fn extract_data_tar(
    bytes: &[u8],
    compression: &DataCompression,
    dest: &Path,
) -> Result<(), ImportError> {
    let decompressed = decompress(bytes, compression)?;
    let mut archive = tar::Archive::new(Cursor::new(decompressed));
    archive
        .unpack(dest)
        .map_err(|e| ImportError::Archive(format!("extracting data.tar: {e}")))?;
    Ok(())
}

fn decompress(bytes: &[u8], compression: &DataCompression) -> Result<Vec<u8>, ImportError> {
    match compression {
        DataCompression::Gzip => {
            let mut decoder = flate2::read::GzDecoder::new(bytes);
            let mut buf = Vec::new();
            decoder.read_to_end(&mut buf)?;
            Ok(buf)
        }
        DataCompression::Xz => {
            let mut decoder = xz2::read::XzDecoder::new(bytes);
            let mut buf = Vec::new();
            decoder.read_to_end(&mut buf)?;
            Ok(buf)
        }
        DataCompression::None => Ok(bytes.to_vec()),
    }
}

/// Parse Debian control file into key-value pairs.
fn parse_control(control: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let mut current_key = String::new();
    let mut current_value = String::new();

    for line in control.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            // Continuation line
            if !current_key.is_empty() {
                current_value.push('\n');
                current_value.push_str(line.trim());
            }
        } else if let Some((key, value)) = line.split_once(':') {
            // Save previous field
            if !current_key.is_empty() {
                map.insert(current_key.clone(), current_value.trim().to_string());
            }
            current_key = key.trim().to_string();
            current_value = value.trim().to_string();
        }
    }
    if !current_key.is_empty() {
        map.insert(current_key, current_value.trim().to_string());
    }
    map
}

/// Parse Debian `Depends:` field into elu dependency entries.
/// Returns `(package_ref, version_spec)` pairs.
fn parse_depends(depends: &str) -> Vec<(String, elu_manifest::VersionSpec)> {
    if depends.trim().is_empty() {
        return vec![];
    }

    depends
        .split(',')
        .filter_map(|dep| {
            let dep = dep.trim();
            if dep.is_empty() {
                return None;
            }
            // Handle alternatives: take the first one
            let dep = dep.split('|').next().unwrap().trim();
            // Extract package name (ignore version constraints and arch qualifiers)
            let name = dep
                .split_whitespace()
                .next()
                .unwrap_or(dep)
                .split(':')
                .next()
                .unwrap_or(dep);

            // Validate it makes a valid package ref segment
            if name.is_empty()
                || !name.as_bytes()[0].is_ascii_alphanumeric()
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            {
                return None;
            }

            Some((
                format!("debian/{name}"),
                elu_manifest::VersionSpec::Any,
            ))
        })
        .collect()
}

/// Extract raw package names from a Depends field (for closure queue).
fn parse_dep_names(depends: &str) -> Vec<String> {
    parse_depends(depends)
        .into_iter()
        .filter_map(|(ref_str, _)| {
            ref_str.strip_prefix("debian/").map(|s| s.to_string())
        })
        .collect()
}

/// Parse a Debian version string into a semver Version.
/// Debian versions like "8.1.2-3" or "1:8.1.2-3" don't map cleanly to semver.
/// We extract up to 3 numeric segments and use them as major.minor.patch.
fn parse_deb_version(deb_version: &str) -> Result<Version, ImportError> {
    let v = deb_version.trim();
    if v.is_empty() {
        return Err(ImportError::InvalidMetadata("empty version".into()));
    }

    // Strip epoch (e.g. "1:8.1.2-3" -> "8.1.2-3")
    let v = v.split_once(':').map_or(v, |(_, rest)| rest);
    // Strip debian revision (e.g. "8.1.2-3" -> "8.1.2")
    let v = v.split_once('-').map_or(v, |(upstream, _)| upstream);
    // Also strip any non-numeric suffix like "~deb12u2"
    let v = v.split_once('~').map_or(v, |(upstream, _)| upstream);
    // Also handle + suffix
    let v = v.split_once('+').map_or(v, |(upstream, _)| upstream);

    let parts: Vec<u64> = v
        .split('.')
        .filter_map(|s| {
            // Take only leading digits from each segment
            let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.parse().ok()
        })
        .collect();

    let major = parts.first().copied().unwrap_or(0);
    let minor = parts.get(1).copied().unwrap_or(0);
    let patch = parts.get(2).copied().unwrap_or(0);

    Ok(Version::new(major, minor, patch))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::Fetcher;
    use elu_manifest::validate::validate_stored;

    /// Build a minimal valid .deb file for testing.
    fn build_test_deb(
        name: &str,
        version: &str,
        depends: &str,
        description: &str,
        files: &[(&str, &[u8])],
    ) -> Vec<u8> {
        // Build control file
        let mut control = format!(
            "Package: {name}\nVersion: {version}\nArchitecture: amd64\nDescription: {description}\n"
        );
        if !depends.is_empty() {
            // Insert Depends before Description
            control = format!(
                "Package: {name}\nVersion: {version}\nArchitecture: amd64\nDepends: {depends}\nDescription: {description}\n"
            );
        }

        // Build control.tar.gz
        let control_tar = {
            let buf = Vec::new();
            let mut ar = tar::Builder::new(buf);
            let control_bytes = control.as_bytes();
            let mut header = tar::Header::new_gnu();
            header.set_size(control_bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            ar.append_data(&mut header, "./control", control_bytes)
                .unwrap();
            ar.into_inner().unwrap()
        };

        let mut gz_buf = Vec::new();
        {
            let mut encoder =
                flate2::write::GzEncoder::new(&mut gz_buf, flate2::Compression::fast());
            std::io::Write::write_all(&mut encoder, &control_tar).unwrap();
            encoder.finish().unwrap();
        }

        // Build data.tar.gz
        let data_tar = {
            let buf = Vec::new();
            let mut ar = tar::Builder::new(buf);
            for (path, content) in files {
                let mut header = tar::Header::new_gnu();
                header.set_size(content.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                ar.append_data(&mut header, path, *content).unwrap();
            }
            ar.into_inner().unwrap()
        };

        let mut data_gz_buf = Vec::new();
        {
            let mut encoder =
                flate2::write::GzEncoder::new(&mut data_gz_buf, flate2::Compression::fast());
            std::io::Write::write_all(&mut encoder, &data_tar).unwrap();
            encoder.finish().unwrap();
        }

        // Build .deb (ar archive)
        let mut deb_buf = Vec::new();
        {
            let mut ar_builder = ar::Builder::new(&mut deb_buf);

            // debian-binary
            let debian_binary = b"2.0\n";
            let header =
                ar::Header::new(b"debian-binary".to_vec(), debian_binary.len() as u64);
            ar_builder.append(&header, &debian_binary[..]).unwrap();

            // control.tar.gz
            let header =
                ar::Header::new(b"control.tar.gz".to_vec(), gz_buf.len() as u64);
            ar_builder.append(&header, &gz_buf[..]).unwrap();

            // data.tar.gz
            let header =
                ar::Header::new(b"data.tar.gz".to_vec(), data_gz_buf.len() as u64);
            ar_builder.append(&header, &data_gz_buf[..]).unwrap();
        }

        deb_buf
    }

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
            // Match on any registered URL fragment
            for (fragment, data) in &self.data {
                if url.contains(fragment) {
                    return Ok(data.clone());
                }
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

    #[test]
    fn import_single_deb_produces_valid_manifest_and_ref() {
        let (_dir, store) = test_store();
        let cache_dir = tempfile::TempDir::new().unwrap();
        let cache = Cache::new(cache_dir.path().join("cache")).unwrap();

        let deb = build_test_deb(
            "hello",
            "1.0.0-1",
            "libc6, libgcc-s1",
            "a test package",
            &[("usr/bin/hello", b"#!/bin/sh\necho hello\n")],
        );

        let mut fetcher = MockFetcher::new();
        fetcher.add("hello", deb);

        let importer = AptImporter;
        let options = ImportOptions {
            version: Some("1.0.0-1".into()),
            ..Default::default()
        };

        let hash = importer
            .import("hello", &options, &store, &cache, &fetcher)
            .unwrap();

        // Verify ref exists
        let ref_hash = store.get_ref("debian", "hello", "1.0.0").unwrap();
        assert_eq!(ref_hash, Some(hash.clone()));

        // Verify manifest is valid
        let manifest_bytes = store.get_manifest(&hash).unwrap().unwrap();
        let manifest: elu_manifest::Manifest =
            serde_json::from_slice(&manifest_bytes).unwrap();

        assert_eq!(manifest.package.namespace, "debian");
        assert_eq!(manifest.package.name, "hello");
        assert_eq!(manifest.package.kind, "debian");
        assert_eq!(manifest.package.description, "a test package");
        assert_eq!(manifest.layers.len(), 1);
        assert!(manifest.layers[0].diff_id.is_some());
        assert!(manifest.layers[0].size.is_some());

        // Check dependencies
        assert_eq!(manifest.dependencies.len(), 2);
        let dep_refs: Vec<&str> = manifest
            .dependencies
            .iter()
            .map(|d| d.reference.as_str())
            .collect();
        assert!(dep_refs.contains(&"debian/libc6"));
        assert!(dep_refs.contains(&"debian/libgcc-s1"));

        // Check metadata
        assert!(!manifest.metadata.is_empty());
        assert!(manifest.metadata.0.contains_key("apt"));

        // Validate stored form
        validate_stored(&manifest).unwrap();
    }

    #[test]
    fn parse_deb_version_strips_epoch_and_revision() {
        assert_eq!(parse_deb_version("8.1.2-3").unwrap(), Version::new(8, 1, 2));
        assert_eq!(
            parse_deb_version("1:8.1.2-3").unwrap(),
            Version::new(8, 1, 2)
        );
        assert_eq!(
            parse_deb_version("3.0.11-1~deb12u2").unwrap(),
            Version::new(3, 0, 11)
        );
        assert_eq!(parse_deb_version("1.0").unwrap(), Version::new(1, 0, 0));
        assert_eq!(parse_deb_version("42").unwrap(), Version::new(42, 0, 0));
    }

    #[test]
    fn parse_depends_handles_alternatives_and_arch() {
        let deps = parse_depends("libc6, libssl3 | libssl1.1, libfoo:amd64 (>= 1.0)");
        assert_eq!(deps.len(), 3);
        assert_eq!(deps[0].0, "debian/libc6");
        assert_eq!(deps[1].0, "debian/libssl3");
        assert_eq!(deps[2].0, "debian/libfoo");
    }

    #[test]
    fn parse_depends_skips_invalid_names() {
        let deps = parse_depends("libc6, INVALID_Name, good-pkg");
        // INVALID_Name has uppercase, should be skipped
        assert_eq!(deps.len(), 2);
    }

    #[test]
    fn cached_deb_skips_fetch() {
        let (_dir, store) = test_store();
        let cache_dir = tempfile::TempDir::new().unwrap();
        let cache = Cache::new(cache_dir.path().join("cache")).unwrap();

        let deb = build_test_deb("cached", "2.0.0", "", "cached package", &[("file", b"data")]);

        // Pre-populate cache
        cache.put("apt", "cached", "2.0.0", &deb).unwrap();

        // Empty fetcher — should still work via cache
        let fetcher = MockFetcher::new();
        let importer = AptImporter;
        let options = ImportOptions {
            version: Some("2.0.0".into()),
            ..Default::default()
        };

        let hash = importer
            .import("cached", &options, &store, &cache, &fetcher)
            .unwrap();
        assert!(store.get_ref("debian", "cached", "2.0.0").unwrap().is_some());
        assert!(store.get_manifest(&hash).unwrap().is_some());
    }

    #[test]
    fn import_closure_imports_transitive_deps() {
        let (_dir, store) = test_store();
        let cache_dir = tempfile::TempDir::new().unwrap();
        let cache = Cache::new(cache_dir.path().join("cache")).unwrap();

        // top depends on mid, mid depends on leaf
        let leaf_deb = build_test_deb("leaf", "1.0.0", "", "leaf package", &[("leaf", b"leaf")]);
        let mid_deb = build_test_deb("mid", "1.0.0", "leaf", "mid package", &[("mid", b"mid")]);
        let top_deb =
            build_test_deb("top", "1.0.0", "mid", "top package", &[("top", b"top")]);

        let mut fetcher = MockFetcher::new();
        fetcher.add("top", top_deb);
        fetcher.add("mid", mid_deb);
        fetcher.add("leaf", leaf_deb);

        let importer = AptImporter;
        let options = ImportOptions {
            version: Some("1.0.0".into()),
            closure: true,
            ..Default::default()
        };

        let top_hash = importer
            .import("top", &options, &store, &cache, &fetcher)
            .unwrap();

        // All three packages should be in the store
        assert!(store.get_ref("debian", "top", "1.0.0").unwrap().is_some());
        assert!(store.get_ref("debian", "mid", "1.0.0").unwrap().is_some());
        assert!(store.get_ref("debian", "leaf", "1.0.0").unwrap().is_some());

        // Top manifest should have mid as dependency
        let top_manifest_bytes = store.get_manifest(&top_hash).unwrap().unwrap();
        let top_manifest: elu_manifest::Manifest =
            serde_json::from_slice(&top_manifest_bytes).unwrap();
        assert_eq!(top_manifest.dependencies.len(), 1);
        assert_eq!(top_manifest.dependencies[0].reference.as_str(), "debian/mid");

        // Mid should have leaf as dependency
        let mid_hash = store.get_ref("debian", "mid", "1.0.0").unwrap().unwrap();
        let mid_manifest_bytes = store.get_manifest(&mid_hash).unwrap().unwrap();
        let mid_manifest: elu_manifest::Manifest =
            serde_json::from_slice(&mid_manifest_bytes).unwrap();
        assert_eq!(mid_manifest.dependencies.len(), 1);
        assert_eq!(mid_manifest.dependencies[0].reference.as_str(), "debian/leaf");
    }
}
