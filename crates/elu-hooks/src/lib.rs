pub mod error;
pub mod interpolate;
pub mod mode;
pub mod ops;
pub mod path;

use camino::Utf8Path;
use elu_manifest::HookOp;

pub use error::HookError;

#[derive(Clone, Debug)]
pub struct PackageContext<'a> {
    pub namespace: &'a str,
    pub name: &'a str,
    pub version: &'a str,
    pub kind: &'a str,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum HookMode {
    /// Run every op in the manifest. v1 default.
    Safe,
    /// Run no ops at all.
    Off,
}

pub struct HookRunner<'a> {
    staging: &'a Utf8Path,
    package: &'a PackageContext<'a>,
    mode: HookMode,
}

#[derive(Debug, Default)]
pub struct HookStats {
    pub ops_run: usize,
    pub files_changed: u64,
}

impl<'a> HookRunner<'a> {
    pub fn new(
        staging: &'a Utf8Path,
        package: &'a PackageContext<'a>,
        mode: HookMode,
    ) -> Self {
        Self { staging, package, mode }
    }

    /// Execute ops against the staging directory in order.
    pub fn run(&self, ops: &[HookOp]) -> Result<HookStats, HookError> {
        if matches!(self.mode, HookMode::Off) {
            return Ok(HookStats::default());
        }
        let mut stats = HookStats::default();
        for (i, op) in ops.iter().enumerate() {
            let r = match op {
                HookOp::Chmod { paths, mode } => ops::chmod::run(self.staging, paths, mode),
                HookOp::Mkdir { path, mode, parents } => ops::mkdir::run(self.staging, path, mode.as_deref(), *parents),
                HookOp::Symlink { from, to, replace } => ops::symlink::run(self.staging, from, to, *replace),
                HookOp::Write { path, content, mode, replace } => ops::write::run(self.staging, self.package, path, content, mode.as_deref(), *replace),
                HookOp::Template { input, output, vars, mode } => ops::template::run(self.staging, self.package, input, output, vars, mode.as_deref()),
                HookOp::Copy { from, to } => ops::copy::run(self.staging, from, to),
                HookOp::Move { from, to } => ops::move_op::run(self.staging, from, to),
                HookOp::Delete { paths } => ops::delete::run(self.staging, paths),
                HookOp::Index { root, output, format } => ops::index::run(self.staging, root, output, format),
                HookOp::Patch { file, source, fuzz: _ } => ops::patch::run(self.staging, file, source),
            };
            r.map_err(|e| HookError::Op { index: i, source: Box::new(e) })?;
            stats.ops_run += 1;
        }
        Ok(stats)
    }
}
