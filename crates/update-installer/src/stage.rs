//! Bounded, restartable downloads become visible only after their signed digest matches.

use crate::OffsetDateTime;
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use update_manifest::{Artifact, TrustedKeys};

/// Absolute artifact bound, even when a trusted manifest accidentally advertises more.
pub const MAX_ARTIFACT_BYTES: u64 = 4 * 1024 * 1024 * 1024;
pub const ENVELOPE_FILE: &str = "manifest.json";
pub const ARTIFACT_FILE: &str = "artifact";

/// A manifest-selected artifact. Construction always verifies its signed platform and version.
pub struct Selection {
    pub version: String,
    pub artifact: Artifact,
    envelope: Vec<u8>,
}

impl Selection {
    /// Rejects expired signatures, missing platforms, unchanged versions and downgrades.
    pub fn verify(
        envelope: &[u8],
        keys: &TrustedKeys,
        platform: &str,
        current: &str,
        now: OffsetDateTime,
    ) -> Result<Self> {
        ensure!(
            envelope.len() <= update_manifest::MAX_ENVELOPE_BYTES,
            "manifest exceeds size limit"
        );
        let manifest = update_manifest::verify(envelope, keys, now)?;
        let verdict = update_manifest::evaluate(&manifest, crate::CHANNEL, platform, current)?;
        let artifact = verdict
            .artifact
            .context("no newer artifact for this platform")?;
        update_manifest::validate_artifact(&artifact)?;
        ensure!(
            artifact.size as u64 <= MAX_ARTIFACT_BYTES,
            "artifact exceeds size limit"
        );
        Ok(Self {
            version: verdict.latest,
            artifact,
            envelope: envelope.to_vec(),
        })
    }
}

/// A private partial download; dropping it never publishes an incomplete artifact.
pub struct Download {
    directory: tempfile::TempDir,
    file: File,
    digest: Sha256,
    written: u64,
    selection: Selection,
}

impl Download {
    /// Starts a fresh attempt. A failed or interrupted transfer is safe to restart from zero.
    pub fn begin(root: &Path, selection: Selection) -> Result<Self> {
        fs::create_dir_all(root)?;
        let directory = tempfile::Builder::new()
            .prefix("download-")
            .tempdir_in(root)?;
        let file = File::create(directory.path().join(ARTIFACT_FILE))?;
        Ok(Self {
            directory,
            file,
            digest: Sha256::new(),
            written: 0,
            selection,
        })
    }

    /// Writes only bytes within the signed size, with constant memory use.
    pub fn write(&mut self, bytes: &[u8]) -> Result<()> {
        ensure!(
            bytes.len() as u64 <= self.selection.artifact.size as u64 - self.written,
            "artifact exceeds signed size"
        );
        self.file.write_all(bytes)?;
        self.digest.update(bytes);
        self.written += bytes.len() as u64;
        Ok(())
    }

    /// Publishes the envelope and artifact together after both signed size and hash agree.
    pub fn finish(self, destination: &Path) -> Result<PathBuf> {
        ensure!(
            self.written == self.selection.artifact.size as u64,
            "artifact is truncated"
        );
        ensure!(
            format!("{:x}", self.digest.finalize())
                .eq_ignore_ascii_case(&self.selection.artifact.sha256),
            "artifact hash mismatch"
        );
        self.file.sync_all()?;
        drop(self.file);
        let mut manifest = File::create(self.directory.path().join(ENVELOPE_FILE))?;
        manifest.write_all(&self.selection.envelope)?;
        manifest.sync_all()?;
        drop(manifest);
        ensure!(!destination.exists(), "a staged update already exists");
        fs::rename(self.directory.path(), destination)?;
        Ok(destination.to_owned())
    }
}

/// Checks a staged artifact again before copying, extracting or executing any installer.
pub fn verify(
    directory: &Path,
    keys: &TrustedKeys,
    platform: &str,
    current: &str,
    now: OffsetDateTime,
) -> Result<Selection> {
    let mut bytes = Vec::new();
    File::open(directory.join(ENVELOPE_FILE))?
        .take(update_manifest::MAX_ENVELOPE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    let selection = Selection::verify(&bytes, keys, platform, current, now)?;
    verify_file(&directory.join(ARTIFACT_FILE), &selection.artifact)?;
    Ok(selection)
}

/// Streams a regular file through its signed digest and rejects size changes while reading.
pub fn verify_file(path: &Path, artifact: &Artifact) -> Result<()> {
    update_manifest::validate_artifact(artifact)?;
    ensure!(
        artifact.size as u64 <= MAX_ARTIFACT_BYTES,
        "artifact exceeds size limit"
    );
    ensure!(
        fs::symlink_metadata(path)?.file_type().is_file(),
        "artifact must be a regular file"
    );
    let file = File::open(path)?;
    ensure!(
        file.metadata()?.len() == artifact.size as u64,
        "artifact size mismatch"
    );
    let mut reader = file.take(artifact.size as u64 + 1);
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    let mut size = 0;
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        size += count as u64;
        hash.update(&buffer[..count]);
    }
    ensure!(size == artifact.size as u64, "artifact size changed");
    ensure!(
        format!("{:x}", hash.finalize()).eq_ignore_ascii_case(&artifact.sha256),
        "artifact hash mismatch"
    );
    Ok(())
}

#[cfg(test)]
mod tests;
