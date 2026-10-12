//! Offline application with a retained previous install and a startup acknowledgement.

use crate::{OffsetDateTime, platform, stage, target};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
};
use update_manifest::TrustedKeys;

const RECEIPT: &str = "pending.json";

#[derive(Deserialize, Serialize)]
struct Pending {
    version: String,
    target: PathBuf,
}

/// Verified update inputs supplied by the installed helper, never by the network.
pub struct Request<'a> {
    pub stage_dir: &'a Path,
    pub target: &'a Path,
    pub keys: &'a TrustedKeys,
    pub platform_key: &'a str,
    pub current: &'a str,
    pub now: OffsetDateTime,
}

/// Applies a staged artifact only after the caller's parent-exit barrier has succeeded.
pub fn after_exit(request: Request<'_>, wait: impl FnOnce() -> Result<()>) -> Result<()> {
    let Request {
        stage_dir,
        target,
        keys,
        platform_key,
        current,
        now,
    } = request;
    wait()?;
    let root = stage_dir.parent().context("missing update root")?;
    let lock = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(root.join("apply.lock"))?;
    lock.try_lock().context("another updater is applying")?;
    ensure!(
        !root.join(RECEIPT).exists(),
        "previous update has not started successfully yet"
    );
    let selection = stage::verify(stage_dir, keys, platform_key, current, now)?;
    ensure!(
        target.is_absolute() && !fs::symlink_metadata(target)?.file_type().is_symlink(),
        "unsafe install target"
    );
    let os = platform_key
        .split_once('-')
        .context("invalid platform key")?
        .0;
    ensure!(
        os == std::env::consts::OS,
        "helper cannot apply another platform"
    );
    let backup = backup_path(root, target, os)?;
    ensure!(!backup.exists(), "previous installation must be retained");
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
    // The copy is private and on the destination filesystem. Verify exactly that copy.
    stage::verify_file(&artifact, &selection.artifact)?;
    let receipt = Pending {
        version: selection.version,
        target: target.to_owned(),
    };
    write_receipt(root, &receipt)?;
    let outcome = platform::install(&artifact, work.path(), target, &backup, &selection.artifact);
    if let Err(error) = outcome {
        // Keep the backup even when recovery itself failed; never destroy the only old copy.
        let _ = fs::remove_file(root.join(RECEIPT));
        return Err(error);
    }
    fs::remove_dir_all(stage_dir)?;
    Ok(())
}

/// Atomically records the expected new version before touching the installed application.
fn write_receipt(root: &Path, pending: &Pending) -> Result<()> {
    use std::io::Write;
    let mut file = tempfile::NamedTempFile::new_in(root)?;
    file.write_all(&serde_json::to_vec(pending)?)?;
    file.as_file().sync_all()?;
    file.persist(root.join(RECEIPT))?;
    Ok(())
}

/// Selects the backup location without trusting a path stored in a writable receipt.
fn backup_path(root: &Path, target: &Path, os: &str) -> Result<PathBuf> {
    if os == "windows" {
        Ok(root.join("previous-install"))
    } else {
        target::backup(target)
    }
}

/// Removes the previous version only when this exact installed target has started successfully.
pub fn acknowledge(root: &Path, installed_target: &Path, current: &str, os: &str) -> Result<()> {
    let receipt = root.join(RECEIPT);
    if !receipt.exists() {
        return Ok(());
    }
    let lock = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(root.join("apply.lock"))?;
    lock.try_lock()?;
    use std::io::Read;
    let mut bytes = Vec::new();
    File::open(&receipt)?
        .take(16 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 16 * 1024, "invalid update receipt");
    let pending: Pending = serde_json::from_slice(&bytes)?;
    ensure!(
        pending.version == current && pending.target == installed_target,
        "pending update does not match this running install"
    );
    remove(&backup_path(root, installed_target, os)?)?;
    fs::remove_file(receipt)?;
    Ok(())
}

/// Replaces a file or entire bundle and restores the old one if promotion fails.
#[cfg(any(unix, test))]
pub(crate) fn swap(target: &Path, candidate: &Path, backup: &Path) -> Result<()> {
    swap_with(target, candidate, backup, |from, to| fs::rename(from, to))
}

/// Shares the real replacement transaction with failure-injection tests.
#[cfg(any(unix, test))]
fn swap_with(
    target: &Path,
    candidate: &Path,
    backup: &Path,
    mut rename: impl FnMut(&Path, &Path) -> std::io::Result<()>,
) -> Result<()> {
    ensure!(
        !backup.exists(),
        "previous installation still awaits startup"
    );
    rename(target, backup).context("save previous installation")?;
    if let Err(error) = rename(candidate, target) {
        rename(backup, target).context("restore previous installation after failed swap")?;
        return Err(error).context("install update (previous version restored)");
    }
    Ok(())
}

/// Deletes only the caller's known backup, handling both files and whole bundles.
fn remove(path: &Path) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if metadata.is_dir() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
