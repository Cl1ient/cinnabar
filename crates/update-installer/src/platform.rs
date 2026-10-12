//! Native package preparation starts only after signed artifact verification.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

use anyhow::{Result, bail};
use std::path::{Path, PathBuf};

/// Prepares a complete macOS bundle or an executable AppImage on the install filesystem.
pub(crate) fn candidate(artifact: &Path, work: &Path, target: &Path) -> Result<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        macos::candidate(artifact, work, target)
    }
    #[cfg(target_os = "linux")]
    {
        use std::{fs, os::unix::fs::PermissionsExt};
        let _ = (work, target);
        fs::set_permissions(artifact, fs::Permissions::from_mode(0o755))?;
        Ok(artifact.to_owned())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = (artifact, work, target);
        bail!("this platform uses its installer")
    }
}

/// Runs Windows Installer passively without requesting elevation or forcing a reboot.
pub(crate) fn install_windows(
    artifact: &Path,
    work: &Path,
    target: &Path,
    expected: &update_manifest::Artifact,
) -> Result<()> {
    #[cfg(windows)]
    {
        windows::install(artifact, work, target, expected)
    }
    #[cfg(not(windows))]
    {
        let _ = (artifact, work, target, expected);
        bail!("Windows installer cannot run on this platform")
    }
}
