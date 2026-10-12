//! Supported installed targets; unpacked developer builds are never replaced.
use anyhow::{Context, Result, bail, ensure};
use std::path::{Path, PathBuf};

/// Resolves the replaceable bundle, MSI directory, or mounted AppImage source.
pub fn resolve(os: &str, executable: &Path, appimage: Option<&Path>) -> Result<PathBuf> {
    ensure!(
        executable.is_absolute(),
        "installed executable must be absolute"
    );
    match os {
        "macos" => executable
            .ancestors()
            .nth(3)
            .filter(|path| path.extension().is_some_and(|extension| extension == "app"))
            .filter(|_| {
                executable
                    .parent()
                    .is_some_and(|p| p.ends_with("Contents/MacOS"))
            })
            .map(Path::to_owned)
            .context("executable is not in an app bundle"),
        "windows" => executable
            .parent()
            .map(Path::to_owned)
            .context("missing MSI install directory"),
        "linux" => appimage
            .filter(|path| path.is_absolute())
            .map(Path::to_owned)
            .context("automatic updates require an AppImage install"),
        _ => bail!("unsupported update platform"),
    }
}

/// Sibling retained until the replacement reaches its first successful startup.
pub fn backup(target: &Path) -> Result<PathBuf> {
    let mut name = target
        .file_name()
        .context("missing install name")?
        .to_os_string();
    name.push(".previous");
    Ok(target.with_file_name(name))
}

/// Whether the helper can write this install without changing user privileges.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Access {
    Writable,
    Manual,
}

/// Probes the install filesystem; permission failures become the manual installer action.
pub fn access(target: &Path) -> Result<Access> {
    ensure!(
        target.is_absolute() && target.exists(),
        "installed target is missing"
    );
    #[cfg(windows)]
    if !machine_install_rights() {
        return Ok(Access::Manual);
    }
    let directory = if cfg!(windows) {
        target
    } else {
        target.parent().context("missing install parent")?
    };
    Ok(access_with(|| {
        tempfile::NamedTempFile::new_in(directory).is_ok()
    }))
}

#[cfg(windows)]
/// Requires existing machine-install rights for the per-machine MSI, without requesting them.
fn machine_install_rights() -> bool {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation},
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };
    let mut token = std::ptr::null_mut();
    let mut elevation = TOKEN_ELEVATION::default();
    let mut length = 0;
    // SAFETY: valid output buffers; the opened token is checked and closed exactly once.
    unsafe {
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let read = GetTokenInformation(
            token,
            TokenElevation,
            std::ptr::from_mut(&mut elevation).cast(),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut length,
        );
        CloseHandle(token);
        read != 0 && elevation.TokenIsElevated != 0
    }
}

/// Keeps permission decisions testable without chmod, elevation, or a platform-specific account.
fn access_with(probe: impl FnOnce() -> bool) -> Access {
    if probe() {
        Access::Writable
    } else {
        Access::Manual
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_target_is_the_whole_install_directory() {
        let root = tempfile::tempdir().unwrap();
        let executable = root.path().join("cinnabar.exe");
        assert_eq!(resolve("windows", &executable, None).unwrap(), root.path());
    }

    #[cfg(unix)]
    #[test]
    fn resolves_only_native_install_shapes() {
        assert_eq!(
            resolve(
                "macos",
                Path::new("/Applications/Client.app/Contents/MacOS/cinnabar"),
                None
            )
            .unwrap(),
            Path::new("/Applications/Client.app")
        );
        assert!(resolve("macos", Path::new("/tmp/Client.app/wrong/bin/client"), None).is_err());
        assert!(resolve("linux", Path::new("/opt/cinnabar/bin/client"), None).is_err());
        assert!(
            resolve(
                "linux",
                Path::new("/opt/cinnabar/bin/client"),
                Some(Path::new("relative.AppImage"))
            )
            .is_err()
        );
        assert_eq!(
            resolve(
                "linux",
                Path::new("/tmp/mount/usr/bin/client"),
                Some(Path::new("/opt/Client.AppImage"))
            )
            .unwrap(),
            Path::new("/opt/Client.AppImage")
        );
    }
    #[test]
    fn not_writable_installs_offer_the_manual_download() {
        assert_eq!(access_with(|| false), Access::Manual);
        assert_eq!(access_with(|| true), Access::Writable);
    }
}
