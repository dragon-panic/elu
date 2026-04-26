use std::process::ExitCode;

use elu_author::report::Diagnostic;
use elu_import::error::ImportError;
use elu_manifest::ManifestError;
use elu_registry::error::RegistryError;
use elu_store::error::StoreError;

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)] // Hook/Lockfile variants are wired by future verbs (resolver+hook ops)
pub enum CliError {
    #[error("usage: {0}")]
    Usage(String),
    #[error("resolution: {0}")]
    Resolution(String),
    #[error("network: {0}")]
    Network(String),
    #[error("store: {0}")]
    Store(String),
    #[error("hook: {0}")]
    Hook(String),
    #[error("lockfile: {0}")]
    Lockfile(String),
    #[error("{0}")]
    Generic(String),
}

impl CliError {
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Generic(_) => 1,
            Self::Usage(_) => 2,
            Self::Resolution(_) => 3,
            Self::Network(_) => 4,
            Self::Store(_) => 5,
            Self::Hook(_) => 6,
            Self::Lockfile(_) => 7,
        }
    }

    pub fn code_str(&self) -> &'static str {
        match self {
            Self::Generic(_) => "generic",
            Self::Usage(_) => "usage",
            Self::Resolution(_) => "resolution",
            Self::Network(_) => "network",
            Self::Store(_) => "store",
            Self::Hook(_) => "hook",
            Self::Lockfile(_) => "lockfile",
        }
    }
}

impl From<StoreError> for CliError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::RefNotFound { .. } => CliError::Resolution(e.to_string()),
            other => CliError::Store(other.to_string()),
        }
    }
}

impl From<ManifestError> for CliError {
    fn from(e: ManifestError) -> Self {
        CliError::Usage(e.to_string())
    }
}

impl From<ImportError> for CliError {
    fn from(e: ImportError) -> Self {
        match e {
            ImportError::Store(s) => CliError::Store(s.to_string()),
            ImportError::Manifest(m) => CliError::Usage(m.to_string()),
            ImportError::Fetch(s) => CliError::Network(s),
            ImportError::NotFound(s) => CliError::Resolution(format!("not found: {s}")),
            ImportError::NoVersion { name, detail } => {
                CliError::Resolution(format!("no version for {name}: {detail}"))
            }
            ImportError::Archive(s) | ImportError::InvalidMetadata(s) => CliError::Generic(s),
            ImportError::Io(io) => CliError::Generic(io.to_string()),
        }
    }
}

impl From<RegistryError> for CliError {
    fn from(e: RegistryError) -> Self {
        match e {
            RegistryError::VersionExists { .. }
            | RegistryError::NamespaceAlreadyClaimed { .. }
            | RegistryError::ReservedNamespace { .. } => CliError::Generic(e.to_string()),
            RegistryError::VersionNotFound { .. }
            | RegistryError::ManifestHashNotFound { .. }
            | RegistryError::PackageNotFound { .. }
            | RegistryError::SessionNotFound { .. }
            | RegistryError::NamespaceNotFound { .. }
            | RegistryError::MissingBlobs { .. } => CliError::Resolution(e.to_string()),
            RegistryError::InvalidManifest { .. } => CliError::Usage(e.to_string()),
            RegistryError::NotAuthorized => CliError::Generic(e.to_string()),
            RegistryError::PublicDependsOnPrivate { .. } => CliError::Resolution(e.to_string()),
            RegistryError::Database(_) => CliError::Generic(e.to_string()),
            RegistryError::BlobBackend(_) => CliError::Network(e.to_string()),
        }
    }
}

impl From<Diagnostic> for CliError {
    fn from(d: Diagnostic) -> Self {
        CliError::Usage(format!("{}: {}", d.code, d.message))
    }
}

pub trait IntoExitCode {
    fn into_exit_code(self) -> ExitCode;
}

impl IntoExitCode for Result<(), CliError> {
    fn into_exit_code(self) -> ExitCode {
        match self {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => ExitCode::from(e.exit_code()),
        }
    }
}
