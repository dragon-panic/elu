//! Output formats: materialize a finalized staging directory into a
//! concrete artifact (dir, tar, qcow2).
//!
//! See `docs/prd/outputs.md` for the contract.

pub mod dir;
pub mod error;
pub mod qcow2;
pub mod tar;

pub use error::OutputError;

use camino::Utf8Path;

/// Name of a supported output format. The set is closed in v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatName {
    Dir,
    Tar,
    Qcow2,
}

/// Per-format options. Each variant is the option struct for one format.
#[derive(Debug, Default)]
pub enum Options {
    #[default]
    DirDefault,
    Dir(DirOpts),
    Tar(TarOpts),
    Qcow2(Qcow2Opts),
}

#[derive(Debug)]
pub struct Qcow2Opts {
    /// Remove a pre-existing target before writing.
    pub force: bool,
    /// Target disk size in bytes. None = fit + 20%.
    pub size: Option<u64>,
    /// On-disk qcow2 format version (default: 3).
    pub format_version: u32,
    /// Skip guest finalization (image may not boot).
    pub no_finalize: bool,
}

impl Default for Qcow2Opts {
    fn default() -> Self {
        Self {
            force: false,
            size: None,
            format_version: 3,
            no_finalize: false,
        }
    }
}

#[derive(Debug, Default)]
pub struct DirOpts {
    /// Remove a pre-existing target before materializing.
    pub force: bool,
    /// Rewrite ownership to `uid:gid` on all files after rename.
    pub owner: Option<(u32, u32)>,
    /// Apply an additional mode mask to all files after rename.
    pub mode_mask: Option<u32>,
}

/// Compression applied as a streaming transform on the tar output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Compression {
    #[default]
    None,
    Gzip,
    Zstd,
    Xz,
}

#[derive(Debug)]
pub struct TarOpts {
    /// Remove a pre-existing target before writing.
    pub force: bool,
    /// Streaming compression applied on top of the tar stream.
    pub compress: Compression,
    /// Compression level (format-specific). None = library default.
    pub level: Option<i32>,
    /// Force mtime=0, uid=0, gid=0 for byte-reproducibility.
    pub deterministic: bool,
}

impl Default for TarOpts {
    fn default() -> Self {
        Self {
            force: false,
            compress: Compression::None,
            level: None,
            deterministic: true,
        }
    }
}

/// Outcome reported by a successful `materialize` call.
#[derive(Debug, Default)]
pub struct Outcome {
    /// Bytes written to the target (dir: tree size; tar/qcow2: artifact size).
    pub bytes: u64,
}

/// List the output formats available in this build.
pub fn list() -> &'static [FormatName] {
    &[FormatName::Dir, FormatName::Tar, FormatName::Qcow2]
}

/// Infer the output format from a target path's suffix. Returns
/// [`FormatName::Dir`] for anything that does not match a known archive
/// or image suffix — the default for interactive use.
pub fn infer_format(target: &Utf8Path) -> Option<FormatName> {
    let name = target.as_str();
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".qcow2") {
        return Some(FormatName::Qcow2);
    }
    if lower.ends_with(".tar")
        || lower.ends_with(".tar.gz")
        || lower.ends_with(".tgz")
        || lower.ends_with(".tar.zst")
        || lower.ends_with(".tar.xz")
    {
        return Some(FormatName::Tar);
    }
    Some(FormatName::Dir)
}

/// Infer streaming compression from a target path's suffix.
pub fn infer_compression(target: &Utf8Path) -> Compression {
    let lower = target.as_str().to_ascii_lowercase();
    if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        Compression::Gzip
    } else if lower.ends_with(".tar.zst") {
        Compression::Zstd
    } else if lower.ends_with(".tar.xz") {
        Compression::Xz
    } else {
        Compression::None
    }
}

/// Materialize a finalized staging tree at `staging` into `target` using
/// the named `format` and `options`.
///
/// The caller is responsible for producing `staging` (via
/// `elu_layers::stage`) and for cleaning up `staging` on error. On success
/// the output takes ownership of `staging` (moves or copies it into the
/// final artifact).
pub fn materialize(
    format: FormatName,
    staging: &Utf8Path,
    target: &Utf8Path,
    options: &Options,
) -> Result<Outcome, OutputError> {
    match format {
        FormatName::Dir => {
            let opts = match options {
                Options::Dir(o) => o,
                _ => &DirOpts::default(),
            };
            dir::materialize(staging, target, opts)
        }
        FormatName::Tar => {
            let default = TarOpts::default();
            let opts = match options {
                Options::Tar(o) => o,
                _ => &default,
            };
            tar::materialize(staging, target, opts)
        }
        FormatName::Qcow2 => Err(OutputError::Unsupported(
            "qcow2 requires a base — call qcow2::materialize directly",
        )),
    }
}
