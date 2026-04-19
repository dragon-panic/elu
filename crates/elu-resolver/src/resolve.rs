use std::collections::{HashMap, HashSet};

use elu_manifest::types::{PackageRef, VersionSpec};
use elu_store::hash::{DiffId, ManifestHash};
use elu_store::store::Store;
use semver::VersionReq;

use crate::error::{Chain, ChainStep, ResolverError};
use crate::lockfile::Lockfile;
use crate::source::{FetchedManifest, VersionSource};
use crate::types::{FetchItem, FetchKind, FetchPlan, ResolvedManifest, Resolution, RootRef};
use crate::version::highest_match;

/// Resolve `roots` against `source` (and optional `lockfile` and `store`).
pub async fn resolve<S: VersionSource>(
    roots: &[RootRef],
    source: &S,
    lockfile: Option<&Lockfile>,
    store: Option<&dyn Store>,
) -> Result<Resolution, ResolverError> {
    let mut walker = Walker::new(source, lockfile);
    for root in roots {
        let parent_chain = vec![ChainStep {
            package: root.package.clone(),
            spec: root.version.clone(),
        }];
        walker
            .visit(&root.package, &root.version, &parent_chain)
            .await?;
    }

    walker.check_conflicts()?;
    let layers = flatten_layers(&walker.ordered);
    let fetch_plan = build_fetch_plan(&walker.ordered, &walker.fetched_urls, &layers, store)?;
    Ok(Resolution {
        manifests: walker.ordered,
        layers,
        fetch_plan,
    })
}

struct Walker<'a, S: VersionSource> {
    source: &'a S,
    lockfile: Option<&'a Lockfile>,
    ordered: Vec<ResolvedManifest>,
    visited: HashSet<ManifestHash>,
    by_name: HashMap<PackageRef, Vec<(Chain, ManifestHash)>>,
    /// URLs reported by the source for fetched manifests/layers.
    fetched_urls: FetchedUrls,
}

#[derive(Default)]
struct FetchedUrls {
    manifests: HashMap<ManifestHash, url::Url>,
    layers: HashMap<DiffId, url::Url>,
}

impl<'a, S: VersionSource> Walker<'a, S> {
    fn new(source: &'a S, lockfile: Option<&'a Lockfile>) -> Self {
        Self {
            source,
            lockfile,
            ordered: Vec::new(),
            visited: HashSet::new(),
            by_name: HashMap::new(),
            fetched_urls: FetchedUrls::default(),
        }
    }

    async fn visit(
        &mut self,
        package: &PackageRef,
        spec: &VersionSpec,
        chain: &[ChainStep],
    ) -> Result<(), ResolverError> {
        let fetched = resolve_one(self.source, package, spec, self.lockfile).await?;
        self.by_name
            .entry(package.clone())
            .or_default()
            .push((Chain(chain.to_vec()), fetched.hash.clone()));

        if !self.visited.insert(fetched.hash.clone()) {
            return Ok(());
        }

        if let Some(u) = &fetched.manifest_url {
            self.fetched_urls
                .manifests
                .insert(fetched.hash.clone(), u.clone());
        }
        for (diff_str, url) in &fetched.layer_urls {
            if let Ok(d) = diff_str.parse::<DiffId>() {
                self.fetched_urls.layers.insert(d, url.clone());
            }
        }

        // Walk deps in alphabetical order for stable output across runs.
        let mut deps = fetched.manifest.dependencies.clone();
        deps.sort_by(|a, b| a.reference.as_str().cmp(b.reference.as_str()));

        for d in deps {
            let mut child_chain = chain.to_vec();
            child_chain.push(ChainStep {
                package: d.reference.clone(),
                spec: d.version.clone(),
            });
            Box::pin(self.visit(&d.reference, &d.version, &child_chain)).await?;
        }

        self.ordered.push(ResolvedManifest {
            package: package.clone(),
            hash: fetched.hash,
            manifest: fetched.manifest,
        });
        Ok(())
    }

    fn check_conflicts(&self) -> Result<(), ResolverError> {
        for (name, entries) in &self.by_name {
            let unique: HashSet<&ManifestHash> = entries.iter().map(|(_, h)| h).collect();
            if unique.len() > 1 {
                return Err(ResolverError::Conflict {
                    package: name.clone(),
                    chains: entries.clone(),
                });
            }
        }
        Ok(())
    }
}

async fn resolve_one<S: VersionSource>(
    source: &S,
    package: &PackageRef,
    spec: &VersionSpec,
    lockfile: Option<&Lockfile>,
) -> Result<FetchedManifest, ResolverError> {
    if let VersionSpec::Pinned(hash) = spec {
        return source.fetch_by_hash(hash).await;
    }
    let req = match spec {
        VersionSpec::Range(r) => r.clone(),
        VersionSpec::Any => VersionReq::STAR,
        VersionSpec::Pinned(_) => unreachable!(),
    };
    let (ns, name) = split_pkg(package);
    if let Some(lock) = lockfile.and_then(|l| l.lookup(ns, name)) {
        if req.matches(&lock.version) {
            return source.fetch_by_hash(&lock.hash).await;
        } else {
            return Err(ResolverError::LockMismatch {
                package: package.clone(),
                pinned: lock.version.clone(),
                spec: req.to_string(),
            });
        }
    }
    let candidates = source.list_versions(package).await?;
    let chosen = highest_match(&req, &candidates).ok_or_else(|| ResolverError::NoMatch {
        package: package.clone(),
        spec: req.to_string(),
    })?;
    source.fetch_manifest(package, &chosen).await
}

fn split_pkg(p: &PackageRef) -> (&str, &str) {
    let s = p.as_str();
    s.split_once('/').expect("PackageRef invariant: contains '/'")
}

/// Walk manifests in order; collect layer diff_ids in first-seen order.
fn flatten_layers(manifests: &[ResolvedManifest]) -> Vec<DiffId> {
    let mut seen: HashSet<DiffId> = HashSet::new();
    let mut out = Vec::new();
    for m in manifests {
        for layer in &m.manifest.layers {
            if let Some(d) = &layer.diff_id
                && seen.insert(d.clone())
            {
                out.push(d.clone());
            }
        }
    }
    out
}

/// Diff manifests + layers against the local store; report what's missing.
fn build_fetch_plan(
    manifests: &[ResolvedManifest],
    urls: &FetchedUrls,
    layers: &[DiffId],
    store: Option<&dyn Store>,
) -> Result<FetchPlan, ResolverError> {
    let Some(store) = store else {
        return Ok(FetchPlan::default());
    };
    let mut items = Vec::new();
    for m in manifests {
        let present = store
            .get_manifest(&m.hash)
            .map_err(|e| ResolverError::Source(format!("store get_manifest: {e}")))?
            .is_some();
        if !present {
            items.push(FetchItem {
                kind: FetchKind::Manifest(m.hash.clone()),
                url: urls.manifests.get(&m.hash).cloned(),
            });
        }
    }
    for d in layers {
        let present = store
            .has_diff(d)
            .map_err(|e| ResolverError::Source(format!("store has_diff: {e}")))?;
        if !present {
            items.push(FetchItem {
                kind: FetchKind::Layer(d.clone()),
                url: urls.layers.get(d).cloned(),
            });
        }
    }
    Ok(FetchPlan { items })
}

