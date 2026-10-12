//! Retained helper and manual-package copies, independent of the replaceable update stage.

use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

const PREFIX: &str = "helper-";
const MANUAL_PREFIX: &str = "manual-";
/// Held while a helper process or a manual package handoff uses its copy.
const LOCK: &str = "helper.lock";
/// An idle copy is kept this long while its helper starts or the desktop handler opens it.
pub const HANDOFF_WINDOW: Duration = Duration::from_secs(60);

/// Copies `source` into a fresh directory and returns the copied helper.
pub fn copy(update_dir: &Path, source: &Path) -> io::Result<PathBuf> {
    let directory = tempfile::Builder::new()
        .prefix(PREFIX)
        .tempdir_in(update_dir)?
        .keep();
    let helper = directory.join(crate::helper_filename());
    fs::copy(source, &helper)?;
    Ok(helper)
}

/// Creates a manual package directory and holds it against pruning until desktop handoff ends.
/// Keep the directory after verification and stamp the lease's modification time at handoff.
pub fn manual_directory(update_dir: &Path) -> io::Result<(tempfile::TempDir, File)> {
    let directory = tempfile::Builder::new()
        .prefix(MANUAL_PREFIX)
        .tempdir_in(update_dir)?;
    let lease = open_lock(directory.path())?;
    lease.lock()?;
    Ok((directory, lease))
}

/// Holds the running helper's own directory; the updater never runs from the install it replaces.
pub fn hold(executable: &Path) -> io::Result<File> {
    let directory = executable
        .parent()
        .filter(|directory| {
            directory
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(PREFIX))
        })
        .ok_or_else(|| io::Error::other("the updater runs only from its copied directory"))?;
    let lock = open_lock(directory)?;
    lock.lock()?;
    Ok(lock)
}

/// Removes idle helper and manual copies as of `now`, keeping held leases and recent handoffs.
pub fn prune(update_dir: &Path, now: SystemTime) {
    let Ok(entries) = fs::read_dir(update_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let copy = entry.file_type().is_ok_and(|kind| kind.is_dir())
            && entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(PREFIX) || name.starts_with(MANUAL_PREFIX));
        if !copy {
            continue;
        }
        // A clock that moved backwards leaves the copy for a later pass.
        let path = entry.path();
        let handing_off = fs::metadata(path.join(LOCK))
            .or_else(|_| entry.metadata())
            .and_then(|metadata| metadata.modified())
            .is_ok_and(|modified| {
                now.duration_since(modified)
                    .map_or(true, |age| age < HANDOFF_WINDOW)
            });
        if handing_off {
            continue;
        }
        let idle = match File::options().write(true).open(path.join(LOCK)) {
            Ok(lock) => lock.try_lock().is_ok(),
            Err(error) => error.kind() == io::ErrorKind::NotFound,
        };
        if idle {
            let _ = fs::remove_dir_all(path);
        }
    }
}

/// Opens the lease shared by a retained copy's owner and the retention scan.
fn open_lock(directory: &Path) -> io::Result<File> {
    File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(directory.join(LOCK))
}

#[cfg(test)]
mod tests;
