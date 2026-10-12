//! Copies one offline helper or opens a verified package for a manual install.
use super::*;
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
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
    let source = std::env::current_exe()?
        .parent()
        .context("missing installed directory")?
        .join(update_installer::helper_filename());
    // The lock covers the copy until its helper holds the directory itself.
    let (helper, _spawning) = update_installer::helper::copy(&layout.update_dir(), &source)?;
    let directory = helper.parent().context("missing helper directory")?;
    let mut command = Command::new(&helper);
    command
        .arg(&ready.stage)
        .arg(installed_target()?)
        .arg(std::process::id().to_string())
        .arg(ready.channel.name())
        .arg(ready.switch_now.to_string())
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
        let result = manual_package(&ready).and_then(|path| {
            let argument = if cfg!(windows) {
                path.to_str()
                    .context("invalid downloaded package path")?
                    .to_owned()
            } else {
                url::Url::from_file_path(&path)
                    .map_err(|_| anyhow::anyhow!("invalid download path"))?
                    .to_string()
            };
            ensure!(
                crate::desktop::open_with_default(&argument),
                "could not open the downloaded update"
            );
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
fn manual_package(ready: &Ready) -> Result<PathBuf> {
    let keys = update_manifest::parse_keys(update_installer::TRUSTED_KEYS)?;
    let platform = update_manifest::platform_key(std::env::consts::OS, std::env::consts::ARCH)
        .context("unsupported platform")?;
    let selected = update_installer::stage::verify(
        &ready.stage,
        &keys,
        Check {
            channel: ready.channel.name(),
            platform: &platform,
            current: env!("CARGO_PKG_VERSION"),
            current_build: update_manifest::build_number(),
            switch_now: ready.switch_now,
        },
        OffsetDateTime::now_utc(),
    )?;
    let name =
        update_manifest::platforms::artifact_name(std::env::consts::OS, std::env::consts::ARCH)
            .context("unknown native artifact")?;
    let destination = ready.stage.join(name);
    fs::copy(
        ready.stage.join(update_installer::stage::ARTIFACT_FILE),
        &destination,
    )?;
    update_installer::stage::verify_file(&destination, &selected.artifact)?;
    if cfg!(target_os = "linux") {
        Ok(ready.stage.clone())
    } else {
        Ok(destination)
    }
}
