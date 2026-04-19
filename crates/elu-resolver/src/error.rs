use elu_manifest::types::{PackageRef, VersionSpec};
use elu_store::hash::ManifestHash;
use semver::Version;
use thiserror::Error;

/// One link in a dependency chain — `parent → ref@constraint`.
#[derive(Debug, Clone)]
pub struct ChainStep {
    pub package: PackageRef,
    pub spec: VersionSpec,
}

/// A full chain from a root to a particular resolved package.
#[derive(Debug, Clone)]
pub struct Chain(pub Vec<ChainStep>);

impl std::fmt::Display for Chain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, step) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(" → ")?;
            }
            write!(f, "{}@{}", step.package, render_spec(&step.spec))?;
        }
        Ok(())
    }
}

pub(crate) fn render_spec(spec: &VersionSpec) -> String {
    match spec {
        VersionSpec::Range(r) => r.to_string(),
        VersionSpec::Pinned(h) => h.to_string(),
        VersionSpec::Any => "*".to_string(),
    }
}

#[derive(Debug, Error)]
pub enum ResolverError {
    #[error("no version of {package} satisfies {spec}")]
    NoMatch { package: PackageRef, spec: String },

    #[error("conflict on {package}: {}", render_chains(.chains))]
    Conflict {
        package: PackageRef,
        chains: Vec<(Chain, ManifestHash)>,
    },

    #[error("lockfile pin {pinned} for {package} does not satisfy {spec}")]
    LockMismatch {
        package: PackageRef,
        pinned: Version,
        spec: String,
    },

    #[error("{package}@{} not in local store (offline mode)", render_spec(version))]
    NotInLocalStore {
        package: PackageRef,
        version: VersionSpec,
    },

    #[error("update target '{name}' not in manifest")]
    UnknownUpdateTarget { name: String },

    #[error("source error: {0}")]
    Source(String),

    #[error("manifest decode error: {0}")]
    ManifestDecode(String),

    #[error("lockfile decode error: {0}")]
    LockfileDecode(String),
}

fn render_chains(chains: &[(Chain, ManifestHash)]) -> String {
    chains
        .iter()
        .map(|(c, h)| format!("[{}] → {}", c, h))
        .collect::<Vec<_>>()
        .join("; ")
}
