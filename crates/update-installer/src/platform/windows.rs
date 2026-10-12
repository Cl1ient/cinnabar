//! MSI upgrades retain a recovery copy while Windows Installer owns transaction rollback.
use anyhow::{Context, Result, ensure};
use std::{fs, path::Path};

/// Keeps the old payload until startup, then lets MSI replace registered and in-use files.
pub(super) fn install(
    artifact: &Path,
    work: &Path,
    target: &Path,
    backup: &Path,
    expected: &update_manifest::Artifact,
) -> Result<()> {
    ensure!(
        target.join("cinnabar.exe").is_file() && target.join("resources").is_dir(),
        "target is not an installed MSI layout"
    );
    let msi = work.join("update.msi");
    fs::rename(artifact, &msi)?;
    // Deny writes and deletion while verifying and until the installer exits, including UAC.
    use std::os::windows::fs::OpenOptionsExt;
    let _verified_payload = fs::OpenOptions::new().read(true).share_mode(1).open(&msi)?;
    crate::stage::verify_file(&msi, expected)?;
    retain_install(target, backup, || run_msi(&msi, target))
}

/// Retains a complete old install; a cancelled or rolled-back MSI remains safely retryable.
fn retain_install(
    target: &Path,
    backup: &Path,
    install: impl FnOnce() -> Result<()>,
) -> Result<()> {
    ensure!(
        !backup.exists(),
        "previous installation still awaits startup"
    );
    if let Err(error) = copy_tree(target, backup) {
        // The installer has not started; only this incomplete recovery copy can be removed.
        let _ = fs::remove_dir_all(backup);
        return Err(error);
    }
    if let Err(error) = install() {
        // Never overwrite MSI registration with a raw file restore. Its transaction owns rollback.
        // Prune only after confirming that rollback left every installed byte unchanged.
        if trees_equal(target, backup)? {
            fs::remove_dir_all(backup)?;
        }
        return Err(error)
            .context("Windows Installer failed; any incomplete rollback keeps the recovery copy");
    }
    Ok(())
}

/// Compares complete payloads after MSI rollback without following reparse links.
fn trees_equal(left: &Path, right: &Path) -> Result<bool> {
    use std::io::Read;
    let mut left_entries = fs::read_dir(left)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<std::io::Result<Vec<_>>>()?;
    let mut right_entries = fs::read_dir(right)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<std::io::Result<Vec<_>>>()?;
    left_entries.sort();
    right_entries.sort();
    if left_entries != right_entries {
        return Ok(false);
    }
    for name in left_entries {
        let (a, b) = (left.join(&name), right.join(&name));
        let (am, bm) = (fs::symlink_metadata(&a)?, fs::symlink_metadata(&b)?);
        if am.file_type() != bm.file_type() || am.file_type().is_symlink() {
            return Ok(false);
        }
        if am.is_dir() {
            if !trees_equal(&a, &b)? {
                return Ok(false);
            }
        } else {
            if am.len() != bm.len() {
                return Ok(false);
            }
            let (mut af, mut bf) = (fs::File::open(a)?, fs::File::open(b)?);
            let (mut ab, mut bb) = ([0; 65536], [0; 65536]);
            loop {
                let count = af.read(&mut ab)?;
                if count == 0 {
                    break;
                }
                bf.read_exact(&mut bb[..count])?;
                if ab[..count] != bb[..count] {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
}

/// Copies installed files without following links or including any per-user data.
fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        ensure!(!kind.is_symlink(), "installed payload contains a link");
        let output = destination.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&entry.path(), &output)?;
        } else {
            ensure!(kind.is_file(), "unsupported installed file");
            fs::copy(entry.path(), output)?;
        }
    }
    Ok(())
}

/// Runs the system installer with elevation consent and waits for its transaction to finish.
fn run_msi(msi: &Path, target: &Path) -> Result<()> {
    use std::{ffi::OsStr, os::windows::ffi::OsStrExt};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_OBJECT_0},
        System::{
            SystemInformation::GetSystemDirectoryW,
            Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject},
        },
        UI::{
            Shell::{
                SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW,
            },
            WindowsAndMessaging::SW_SHOWNORMAL,
        },
    };
    let wide = |value: &OsStr| value.encode_wide().chain(Some(0)).collect::<Vec<_>>();
    ensure!(
        !msi.as_os_str()
            .encode_wide()
            .chain(target.as_os_str().encode_wide())
            .any(|unit| unit == 34 || unit == 0),
        "invalid installer path"
    );
    let verb = wide(OsStr::new("runas"));
    let parameters = wide(OsStr::new(&format!(
        "/i \"{}\" INSTALLFOLDER=\"{}\" /passive /norestart REBOOT=ReallySuppress",
        msi.display(),
        target.display()
    )));
    let mut system = [0u16; 32768];
    // SAFETY: buffers and terminated strings outlive the call; the process handle is closed once.
    unsafe {
        let count = GetSystemDirectoryW(system.as_mut_ptr(), system.len() as u32) as usize;
        ensure!(
            count > 0 && count < system.len(),
            "cannot locate Windows Installer"
        );
        let mut executable = system[..count].to_vec();
        executable.extend("\\msiexec.exe".encode_utf16().chain(Some(0)));
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
        info.lpVerb = verb.as_ptr();
        info.lpFile = executable.as_ptr();
        info.lpParameters = parameters.as_ptr();
        info.nShow = SW_SHOWNORMAL;
        ensure!(
            ShellExecuteExW(&mut info) != 0 && !info.hProcess.is_null(),
            "Windows Installer was not started (elevation may have been cancelled)"
        );
        let waited = WaitForSingleObject(info.hProcess, INFINITE);
        let mut code = u32::MAX;
        let queried = GetExitCodeProcess(info.hProcess, &mut code);
        CloseHandle(info.hProcess);
        ensure!(
            waited == WAIT_OBJECT_0 && queried != 0 && matches!(code, 0 | 3010),
            "Windows Installer returned {code}"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal MSI payload with nested resources and no account data.
    fn payload(root: &Path) -> std::path::PathBuf {
        let target = root.join("installed");
        fs::create_dir_all(target.join("resources")).unwrap();
        fs::write(target.join("cinnabar.exe"), "old").unwrap();
        fs::write(target.join("resources/data"), "old resources").unwrap();
        target
    }

    #[test]
    fn msi_success_keeps_complete_previous_payload_until_startup() {
        let root = tempfile::tempdir().unwrap();
        let target = payload(root.path());
        let backup = root.path().join("previous");
        retain_install(&target, &backup, || {
            fs::write(target.join("cinnabar.exe"), "new")?;
            Ok(())
        })
        .unwrap();
        assert_eq!(fs::read(backup.join("cinnabar.exe")).unwrap(), b"old");
        assert_eq!(
            fs::read(backup.join("resources/data")).unwrap(),
            b"old resources"
        );
    }

    #[test]
    fn cancelled_msi_or_complete_native_rollback_allows_retry() {
        let root = tempfile::tempdir().unwrap();
        let target = payload(root.path());
        let backup = root.path().join("previous");
        assert!(retain_install(&target, &backup, || anyhow::bail!("cancelled")).is_err());
        assert!(!backup.exists());
        assert_eq!(fs::read(target.join("cinnabar.exe")).unwrap(), b"old");
        retain_install(&target, &backup, || Ok(())).unwrap();
    }

    #[test]
    fn incomplete_msi_rollback_keeps_recovery_payload() {
        let root = tempfile::tempdir().unwrap();
        let target = payload(root.path());
        let backup = root.path().join("previous");
        assert!(
            retain_install(&target, &backup, || {
                fs::write(target.join("cinnabar.exe"), "damaged")?;
                anyhow::bail!("rollback failed")
            })
            .is_err()
        );
        assert_eq!(fs::read(backup.join("cinnabar.exe")).unwrap(), b"old");
    }
}
