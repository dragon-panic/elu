use elu_manifest::types::{Manifest, VersionSpec};
use elu_store::hash::ManifestHash;
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};

use crate::error::ResolverError;
use crate::resolve::resolve;
use crate::source::VersionSource;
use crate::types::{Resolution, RootRef};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Lockfile {
    pub schema: u32,
    #[serde(rename = "package", default, skip_serializing_if = "Vec::is_empty")]
    pub packages: Vec<LockfileEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LockfileEntry {
    pub namespace: String,
    pub name: String,
    pub version: Version,
    pub hash: ManifestHash,
}

impl Lockfile {
    pub fn from_toml_str(s: &str) -> Result<Self, ResolverError> {
        toml::from_str(s).map_err(|e| ResolverError::LockfileDecode(e.to_string()))
    }

    pub fn to_toml_string(&self) -> Result<String, ResolverError> {
        toml::to_string_pretty(self).map_err(|e| ResolverError::LockfileDecode(e.to_string()))
    }

    pub fn lookup(&self, namespace: &str, name: &str) -> Option<&LockfileEntry> {
        self.packages
            .iter()
            .find(|p| p.namespace == namespace && p.name == name)
    }
}

/// Resolve `manifest`'s dependencies against `source` and serialize the
/// resolution as a lockfile.
pub async fn lock<S: VersionSource>(
    manifest: &Manifest,
    source: &S,
) -> Result<Lockfile, ResolverError> {
    let roots = roots_from_manifest(manifest);
    let resolution = resolve(&roots, source, None, None).await?;
    Ok(resolution_to_lockfile(&resolution))
}

/// Check that every direct dep of `manifest` has a satisfying entry in `lockfile`.
pub fn verify(manifest: &Manifest, lockfile: &Lockfile) -> Result<(), Vec<ResolverError>> {
    let mut errors = Vec::new();
    for dep in &manifest.dependencies {
        let (ns, name) = dep
            .reference
            .as_str()
            .split_once('/')
            .expect("PackageRef invariant");
        match lockfile.lookup(ns, name) {
            None => errors.push(ResolverError::NoMatch {
                package: dep.reference.clone(),
                spec: format!(
                    "no lockfile entry for {} (constraint {})",
                    dep.reference,
                    crate::error::render_spec(&dep.version)
                ),
            }),
            Some(entry) => {
                let req: VersionReq = match &dep.version {
                    VersionSpec::Range(r) => r.clone(),
                    VersionSpec::Any => VersionReq::STAR,
                    VersionSpec::Pinned(_) => continue,
                };
                if !req.matches(&entry.version) {
                    errors.push(ResolverError::LockMismatch {
                        package: dep.reference.clone(),
                        pinned: entry.version.clone(),
                        spec: req.to_string(),
                    });
                }
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Re-resolve the named packages (and their transitive deps) while keeping
/// every other package pinned to its current lockfile entry. `names=None`
/// re-resolves the whole graph, equivalent to `lock(manifest, source)`.
pub async fn update<S: VersionSource>(
    manifest: &Manifest,
    lockfile: &Lockfile,
    names: Option<&[String]>,
    source: &S,
) -> Result<Lockfile, ResolverError> {
    if let Some(names) = names {
        for name in names {
            let in_manifest = manifest.dependencies.iter().any(|d| {
                d.reference
                    .as_str()
                    .split_once('/')
                    .map(|(_, n)| n == name)
                    .unwrap_or(false)
            });
            if !in_manifest {
                return Err(ResolverError::UnknownUpdateTarget {
                    name: name.clone(),
                });
            }
        }
    }

    let pin_filter = match names {
        None => Lockfile::default(),
        Some(names) => Lockfile {
            schema: lockfile.schema,
            packages: lockfile
                .packages
                .iter()
                .filter(|p| !names.iter().any(|n| n == &p.name))
                .cloned()
                .collect(),
        },
    };

    let roots = roots_from_manifest(manifest);
    let resolution = resolve(&roots, source, Some(&pin_filter), None).await?;
    Ok(resolution_to_lockfile(&resolution))
}

fn roots_from_manifest(manifest: &Manifest) -> Vec<RootRef> {
    manifest
        .dependencies
        .iter()
        .map(|d| RootRef {
            package: d.reference.clone(),
            version: d.version.clone(),
        })
        .collect()
}

fn resolution_to_lockfile(r: &Resolution) -> Lockfile {
    Lockfile {
        schema: 1,
        packages: r
            .manifests
            .iter()
            .map(|m| LockfileEntry {
                namespace: m.manifest.package.namespace.clone(),
                name: m.manifest.package.name.clone(),
                version: m.manifest.package.version.clone(),
                hash: m.hash.clone(),
            })
            .collect(),
    }
}
