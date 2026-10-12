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
}
