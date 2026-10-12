//! Copies one offline helper or opens a verified package for a manual install.
use super::*;
use anyhow::{Context, Result, ensure};
use std::{
    ffi::OsString,
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::SystemTime,
};
use time::OffsetDateTime;
use update_manifest::select::Check;

/// Resolves only native app bundles, MSI layouts and mounted AppImage source files.
pub(super) fn installed_target() -> Result<PathBuf> {
    update_installer::target::resolve(
        std::env::consts::OS,
        &std::env::current_exe()?,
        std::env::var_os("APPIMAGE").map(PathBuf::from).as_deref(),
    )
}

/// Starts the copied helper with the exact parent PID and the user's channel-switch choice.
pub(super) fn spawn(layout: &InstallLayout, ready: &Ready) -> Result<()> {
    spawn_with_args(
        layout,
        &[
            ready.stage.as_os_str().to_owned(),
            installed_target()?.into_os_string(),
            std::process::id().to_string().into(),
            ready.channel.name().into(),
            ready.switch_now.to_string().into(),
        ],
    )
}

/// Starts rollback recovery with the same exact-parent exit barrier as an ordinary apply.
#[cfg(unix)]
pub(super) fn spawn_recovery(layout: &InstallLayout, target: PathBuf) -> Result<()> {
    spawn_with_args(
        layout,
        &[
            update_installer::recovery::RECOVER_FLAG.into(),
            layout.update_dir().into_os_string(),
            target.into_os_string(),
            std::process::id().to_string().into(),
        ],
    )
}

/// Runs a copied helper and retains its stdin pipe until the client actually exits.
fn spawn_with_args(layout: &InstallLayout, args: &[OsString]) -> Result<()> {
    let source = std::env::current_exe()?
        .parent()
        .context("missing installed directory")?
        .join(update_installer::helper_filename());
    update_installer::helper::prune(&layout.update_dir(), std::time::SystemTime::now());
    let helper = update_installer::helper::copy(&layout.update_dir(), &source)?;
    let directory = helper.parent().context("missing helper directory")?;
    let mut command = Command::new(&helper);
    command
        .args(args)
        .current_dir(directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(fs::File::create(layout.update_dir().join("apply.log"))?);
    #[cfg(target_os = "linux")]
    for name in ["APPIMAGE", "APPDIR", "OWD", "LD_LIBRARY_PATH", "LD_PRELOAD"] {
        command.env_remove(name);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x00000008 | 0x08000000);
    }
    let mut child = command.spawn()?;
    if let Some(pipe) = child.stdin.take() {
        std::mem::forget(pipe);
    }
    Ok(())
}

/// Revalidates before exposing a native package to the desktop; this never requests elevation.
pub(super) fn open_download(ready: Ready) {
    std::thread::spawn(move || {
        let result = manual_package(&ready).and_then(|package| {
            let path = if cfg!(target_os = "linux") {
                package.path.parent().context("missing package directory")?
            } else {
                &package.path
            };
            let argument = if cfg!(windows) {
                path.to_str()
                    .context("invalid downloaded package path")?
                    .to_owned()
            } else {
                url::Url::from_file_path(path)
                    .map_err(|_| anyhow::anyhow!("invalid download path"))?
                    .to_string()
            };
            ensure!(
                crate::desktop::open_with_default(&argument),
                "could not open the downloaded update"
            );
            // Retain the copy for a full handoff window after the desktop accepts the path.
            package.lease.set_modified(SystemTime::now())?;
            Ok(())
        });
        if let Err(error) = result {
            if let Ok(mut state) = STATE.lock() {
                state.message = Some(format!("Could not open update: {error:#}"));
            }
        }
    });
}

/// Gives a verified file its published extension, so macOS and Windows open the correct installer.
fn manual_package(ready: &Ready) -> Result<ManualPackage> {
    let keys = update_manifest::parse_keys(update_installer::TRUSTED_KEYS)?;
    let platform = update_manifest::platform_key(std::env::consts::OS, std::env::consts::ARCH)
        .context("unsupported platform")?;
    let name =
        update_manifest::platforms::artifact_name(std::env::consts::OS, std::env::consts::ARCH)
            .context("unknown native artifact")?;
    snapshot_package(
        &ready.stage,
        name,
        &keys,
        Check {
            channel: ready.channel.name(),
            platform: &platform,
            current: env!("CARGO_PKG_VERSION"),
            current_build: update_manifest::build_number(),
            switch_now: ready.switch_now,
        },
        OffsetDateTime::now_utc(),
    )
}

/// A verified manual installer held independently of further stage replacement or deletion.
pub(super) struct ManualPackage {
    pub(super) path: PathBuf,
    lease: File,
}

/// Verifies and copies one stage under the shared writer lease, then retains the independent copy.
pub(super) fn snapshot_package(
    stage: &Path,
    name: &str,
    keys: &update_manifest::TrustedKeys,
    check: Check<'_>,
    now: OffsetDateTime,
) -> Result<ManualPackage> {
    let root = stage.parent().context("missing update directory")?;
    let stage_lease = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(root.join(update_installer::LOCK_FILE))?;
    stage_lease.lock().context("lock the update directory")?;
    let selected = update_installer::stage::verify(stage, keys, check, now)?;
    let (directory, lease) = update_installer::helper::manual_directory(root)?;
    let path = named_package(stage, directory.path(), name, &selected.artifact)?;
    let _ = directory.keep();
    Ok(ManualPackage { path, lease })
}

/// Copies the artifact to a separate directory and verifies it; Linux AppImages become runnable.
pub(super) fn named_package(
    stage: &Path,
    directory: &Path,
    name: &str,
    artifact: &update_manifest::Artifact,
) -> Result<PathBuf> {
    let destination = directory.join(name);
    fs::copy(
        stage.join(update_installer::stage::ARTIFACT_FILE),
        &destination,
    )?;
    update_installer::stage::verify_file(&destination, artifact)?;
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&destination, fs::Permissions::from_mode(0o755))?;
    }
    Ok(destination)
}
