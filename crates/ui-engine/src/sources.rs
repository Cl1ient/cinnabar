//! Host-owned reads the texture atlas draws through, keeping filesystem and network access out
//! of the engine.

use std::sync::Arc;

/// The installed vanilla resource pack's files by pack-relative path; the host owns the reads.
pub trait LocalPack: Send + Sync {
    /// The size of the file at `relative`, spelled exactly so; `None` when absent or outside
    /// the pack.
    fn file_size(&self, relative: &str) -> Option<u64>;

    /// The file's bytes when it holds at most `limit`; `None` when absent, unreadable or larger.
    fn read(&self, relative: &str, limit: u64) -> Option<Vec<u8>>;
}

/// Images named by URL; the host owns the downloads and their cache.
pub trait RemoteSource: Send + Sync {
    /// Where `url`'s download stands, starting it on first sight.
    fn state(&self, url: &str) -> RemoteImage;
}

/// One downloaded image's progress.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteImage {
    Loading,
    Ready(Arc<[u8]>),
    Failed,
}

/// Whether `path` names a downloaded image rather than a pack file.
pub fn is_remote(path: &str) -> bool {
    path.starts_with("https://") || path.starts_with("http://")
}
