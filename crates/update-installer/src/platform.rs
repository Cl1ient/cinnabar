//! Native package operations. Nothing here downloads or runs the staged app itself.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

use anyhow::Result;
use std::path::Path;

/// Installs the verified native payload and preserves the previous installed version.
pub(crate) fn install(
    artifact: &Path,
    work: &Path,
    target: &Path,
    backup: &Path,
    expected: &update_manifest::Artifact,
) -> Result<()> {
    #[cfg(not(windows))]
    let _ = expected;
    #[cfg(target_os = "macos")]
    {
        macos::install(artifact, work, target, backup)
    }
    #[cfg(windows)]
    {
        windows::install(artifact, work, target, backup, expected)
    }
    #[cfg(target_os = "linux")]
    {
        use std::{fs, os::unix::fs::PermissionsExt};
        anyhow::ensure!(target.is_file(), "AppImage target must be a file");
        let _ = work;
        fs::set_permissions(artifact, fs::Permissions::from_mode(0o755))?;
        crate::apply::swap(target, artifact, backup)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        let _ = (artifact, work, target, backup);
        anyhow::bail!("unsupported installer platform")
    }
}
