//! Private helper copies in the update directory, kept only while their helper process runs.

use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
};

const PREFIX: &str = "helper-";
/// Locked for the lifetime of the copy's helper process; an unlocked copy is finished.
const LOCK: &str = "helper.lock";

/// Copies `source` into a fresh directory, returning the copy and a lock to hold until it spawns.
pub fn copy(update_dir: &Path, source: &Path) -> io::Result<(PathBuf, File)> {
    prune(update_dir);
    let directory = tempfile::Builder::new()
        .prefix(PREFIX)
        .tempdir_in(update_dir)?
        .keep();
    let lock = open_lock(&directory)?;
    lock.lock()?;
    let helper = directory.join(crate::helper_filename());
    fs::copy(source, &helper)?;
    Ok((helper, lock))
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

/// Removes completed or failed helper copies, keeping any whose helper still holds its lock.
pub fn prune(update_dir: &Path) {
    let Ok(entries) = fs::read_dir(update_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let copy = entry.file_type().is_ok_and(|kind| kind.is_dir())
            && entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(PREFIX));
        if !copy {
            continue;
        }
        let path = entry.path();
        let idle = match File::options().write(true).open(path.join(LOCK)) {
            Ok(lock) => lock.try_lock().is_ok(),
            Err(error) => error.kind() == io::ErrorKind::NotFound,
        };
        if idle {
            let _ = fs::remove_dir_all(path);
        }
    }
}

fn open_lock(directory: &Path) -> io::Result<File> {
    File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(directory.join(LOCK))
}

#[cfg(test)]
mod tests;
