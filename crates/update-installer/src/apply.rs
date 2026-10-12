//! One swap, one previous copy, and rollback when the newly launched client cannot start.

use crate::{platform, stage, target};
use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
};
use time::OffsetDateTime;
use update_manifest::{TrustedKeys, select::Check};

/// Local, signed inputs passed by the offline helper after the client chooses to update.
pub struct Request<'a> {
    pub stage_dir: &'a Path,
    pub target: &'a Path,
    pub keys: &'a TrustedKeys,
    pub check: Check<'a>,
    pub now: OffsetDateTime,
}

/// Waits for the old client, verifies the payload, applies it, and restores failed new starts.
pub fn after_exit(
    request: Request<'_>,
    wait: impl FnOnce() -> Result<()>,
    start: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    wait()?;
    let Request {
        stage_dir,
        target,
        keys,
        check,
        now,
    } = request;
    let root = stage_dir.parent().context("missing update directory")?;
    let lock = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(root.join("update.lock"))?;
    lock.try_lock()
        .context("another updater is using this stage")?;
    let selected = stage::verify(stage_dir, keys, check, now)?;
    ensure!(
        target::access(target)? == target::Access::Writable,
        "install is not writable; use Download update"
    );
    ensure!(
        !fs::symlink_metadata(target)?.file_type().is_symlink(),
        "unsafe install target"
    );
    let os = check
        .platform
        .split_once('-')
        .context("invalid platform key")?
        .0;
    ensure!(
        os == std::env::consts::OS,
        "helper cannot apply another platform"
    );
    let parent = if os == "windows" {
        root
    } else {
        target.parent().context("missing install parent")?
    };
    let work = tempfile::Builder::new()
        .prefix(".cinnabar-update-")
        .tempdir_in(parent)?;
    let artifact = work.path().join("payload");
    fs::copy(stage_dir.join(stage::ARTIFACT_FILE), &artifact)?;
    stage::verify_file(&artifact, &selected.artifact)?;
    if os == "windows" {
        platform::install_windows(&artifact, work.path(), target, &selected.artifact)?;
        remember_installer(root, &artifact)?;
        start(target)?;
    } else {
        let backup = target::backup(target)?;
        let candidate = platform::candidate(&artifact, work.path(), target)?;
        swap(target, &candidate, &backup)?;
        if let Err(error) = start(target) {
            rollback(target, &backup, &work.path().join("failed"))?;
            return Err(error).context("new version did not start; previous version restored");
        }
        confirm_start(&backup)?;
        cleanup_confirmed_backup(target, &backup)?;
    }
    fs::remove_dir_all(stage_dir)?;
    Ok(())
}

/// Keeps the most recently downloaded previous MSI for manual repair, without copying Program Files.
fn remember_installer(root: &Path, artifact: &Path) -> Result<()> {
    let current = root.join("installed.msi");
    let previous = root.join("previous.msi");
    if current.exists() {
        if previous.exists() {
            fs::remove_file(&previous)?;
        }
        fs::rename(&current, &previous)?;
    }
    fs::copy(artifact, current)?;
    Ok(())
}

/// Promotes a file or complete app bundle, restoring it if the promotion itself fails.
pub(crate) fn swap(target: &Path, candidate: &Path, backup: &Path) -> Result<()> {
    cleanup_confirmed_backup(target, backup)?;
    ensure!(!backup.exists(), "previous installation is still retained");
    fs::rename(target, backup).context("retain previous installation")?;
    if let Err(error) = fs::rename(candidate, target) {
        fs::rename(backup, target).context("restore previous installation")?;
        return Err(error).context("install update");
    }
    Ok(())
}

/// The receipt beside a backup, written only after its replacement confirms startup.
fn confirmation(backup: &Path) -> PathBuf {
    let mut name = backup.as_os_str().to_os_string();
    name.push(".started");
    PathBuf::from(name)
}

/// Persists successful startup before cleanup, so a later swap can retry failed removal.
fn confirm_start(backup: &Path) -> Result<()> {
    File::options()
        .write(true)
        .create_new(true)
        .open(confirmation(backup))?
        .sync_all()?;
    Ok(())
}

/// Retries only confirmed cleanup; unconfirmed backups remain available for rollback.
fn cleanup_confirmed_backup(target: &Path, backup: &Path) -> Result<()> {
    let receipt = confirmation(backup);
    if !receipt.is_file() {
        return Ok(());
    }
    ensure!(
        target.exists(),
        "confirmed replacement is missing; keep previous installation"
    );
    if backup.exists() {
        remove(backup).context("remove confirmed previous installation")?;
    }
    fs::remove_file(receipt).context("remove startup confirmation")?;
    Ok(())
}

/// Moves a failed new install aside before restoring the previous signed payload.
pub(crate) fn rollback(target: &Path, backup: &Path, failed: &Path) -> Result<()> {
    fs::rename(target, failed).context("move failed new installation aside")?;
    fs::rename(backup, target).context("restore previous installation")?;
    Ok(())
}

/// Deletes only the known previous file or complete bundle after successful startup.
fn remove(path: &Path) -> Result<()> {
    if fs::symlink_metadata(path)?.is_dir() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
