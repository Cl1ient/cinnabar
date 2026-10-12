//! Immutable, independently retained stage names for client and desktop handoffs.

use super::{ARTIFACT_FILE, ENVELOPE_FILE, Selection, verify_file};
use crate::lease::Lease;
use anyhow::{Result, ensure};
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
};

/// A verified stage whose files and shared reader lease survive replacement of `ready`.
pub struct Snapshot {
    directory: PathBuf,
    lease: File,
    selected: Selection,
}

impl Snapshot {
    /// Retains an authenticated selection while its immutable source is protected by the writer lease.
    pub fn capture(source: &Path, lease: &Lease, selected: Selection) -> Result<Self> {
        ensure!(
            source.parent() == Some(lease.root()),
            "stage is outside the leased directory"
        );
        let (directory, reader) = crate::helper::snapshot_directory(lease.root())?;
        fs::write(directory.path().join(ENVELOPE_FILE), &selected.envelope)?;
        let artifact = directory.path().join(ARTIFACT_FILE);
        // Published artifacts are immutable. Separate links survive stage deletion without copying gigabytes.
        if fs::hard_link(source.join(ARTIFACT_FILE), &artifact).is_err() {
            fs::copy(source.join(ARTIFACT_FILE), &artifact)?;
        }
        verify_file(&artifact, &selected.artifact)?;
        Ok(Self {
            directory: directory.keep(),
            lease: reader,
            selected,
        })
    }

    /// Transfers the retained directory, its reader lease and the verified release to its consumer.
    pub fn into_parts(self) -> (PathBuf, File, Selection) {
        (self.directory, self.lease, self.selected)
    }
}
