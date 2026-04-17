use std::collections::BTreeMap;

use camino::Utf8Path;

use crate::error::HookError;
use crate::interpolate::interpolate;
use crate::ops::write::atomic_write;
use crate::path::{resolve_in_staging, verify_under_staging};
use crate::PackageContext;

pub fn run(
    staging: &Utf8Path,
    pkg: &PackageContext,
    input: &str,
    output: &str,
    vars: &BTreeMap<String, String>,
    mode: Option<&str>,
) -> Result<(), HookError> {
    let src = resolve_in_staging(staging, input)?;
    let dest = resolve_in_staging(staging, output)?;
    verify_under_staging(staging, &src)?;
    verify_under_staging(staging, &dest)?;
    let tpl = std::fs::read_to_string(&src)?;
    let out = interpolate(&tpl, pkg, vars)?;
    atomic_write(&dest, out.as_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let target_mode = match mode {
            Some(m) => crate::mode::ModeSpec::parse(m)?.apply(0o644),
            None => std::fs::metadata(&src)?.permissions().mode(),
        };
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(target_mode))?;
    }
    #[cfg(not(unix))]
    let _ = mode;
    Ok(())
}
