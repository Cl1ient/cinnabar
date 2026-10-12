//! A copied Rust helper outlives both the client and the package it replaces.
use super::{InstallLayout, Ready};
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
};

/// Resolves only a supported native installed package; never a source checkout or unpacked image.
pub(super) fn installed_target() -> Result<PathBuf> {
    update_installer::target::resolve(
        std::env::consts::OS,
        &std::env::current_exe()?,
        std::env::var_os("APPIMAGE").map(PathBuf::from).as_deref(),
    )
}

/// Copies the signed, installed helper out of the app before starting its parent-exit wait.
pub(super) fn spawn(layout: &InstallLayout, ready: &Ready) -> Result<()> {
    let executable = std::env::current_exe()?;
    let source = executable
        .parent()
        .context("missing installed executable directory")?
        .join(update_installer::helper_filename());
    ensure!(source.is_file(), "installed update helper is missing");
    let root = layout.update_dir();
    let directory = tempfile::Builder::new()
        .prefix("helper-")
        .tempdir_in(&root)?
        .keep();
    let helper = directory.join(update_installer::helper_filename());
    fs::copy(source, &helper)?;
    let log = fs::File::create(root.join("apply.log"))?;
    let mut command = Command::new(helper);
    command
        .arg(&ready.stage)
        .arg(installed_target()?)
        .arg(std::process::id().to_string())
        .current_dir(directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(log);
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
    // The pipe must remain open until the process exits, including any late shutdown work.
    if let Some(pipe) = child.stdin.take() {
        std::mem::forget(pipe);
    }
    Ok(())
}
