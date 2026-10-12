//! Installed-pack image lookup with bounded worker reads for large sources.

use super::{IMAGE_EXTENSIONS, MAX_EXTRA, MAX_PACK_TEXTURE_BYTES, ServerAtlas, Source, source};
use crate::sources::LocalPack;

/// Tries extensions in precedence order; an inline lookup stops before reading a large file.
pub(super) fn read(
    root: &dyn LocalPack,
    relative: &str,
    limit: u64,
    inline: bool,
) -> (Option<Source>, bool) {
    for extension in IMAGE_EXTENSIONS {
        let path = format!("{relative}{extension}");
        let Some(len) = root.file_size(&path) else {
            continue;
        };
        if len > resource_pack::MAX_FILE_BYTES {
            continue;
        }
        if inline && len > MAX_PACK_TEXTURE_BYTES {
            return (None, false);
        }
        if len > limit {
            continue;
        }
        #[cfg(test)]
        super::LOCAL_READS.with(|reads| reads.borrow_mut().push(path.clone()));
        let limit = if inline {
            limit.min(MAX_PACK_TEXTURE_BYTES)
        } else {
            limit
        };
        let Some(bytes) = root.read(&path, limit) else {
            continue;
        };
        if let Some(found) = source(bytes.into()) {
            return (Some(found), true);
        }
    }
    (None, true)
}

impl ServerAtlas {
    /// Resolves carrier bytes first and defers newly large installed images without caching a miss.
    pub(super) fn local_source(&self, key: &str) -> (Option<Source>, bool) {
        let relative = key.strip_prefix(super::VANILLA_IN_PACKAGE).unwrap_or(key);
        if !key.starts_with("textures/") && relative == key {
            return (None, true);
        }
        if let Some(found) = self.carrier.as_ref().and_then(|carrier| {
            IMAGE_EXTENSIONS.iter().find_map(|extension| {
                source(carrier.ui_file(&format!("{relative}{extension}"))?.into())
            })
        }) {
            return (Some(found), true);
        }
        let Some(root) = &self.vanilla else {
            return (None, true);
        };
        let limit = self.source_read_limit();
        let result = read(root.as_ref(), relative, limit, true);
        if !result.1 {
            self.local_reads.request_local(key, root, relative, limit);
        }
        result
    }

    /// The shared encoded-source allowance excludes small images and shadowed pack copies.
    pub(super) fn source_read_limit(&self) -> u64 {
        let extra: usize = self
            .extra
            .borrow()
            .values()
            .flatten()
            .filter(|source| source.bytes.len() as u64 > MAX_PACK_TEXTURE_BYTES)
            .map(|source| source.bytes.len())
            .sum();
        let pack: usize = self.pack.as_ref().map_or(0, |pack| {
            pack.loaded
                .borrow()
                .values()
                .flatten()
                .filter(|source| source.bytes.len() as u64 > MAX_PACK_TEXTURE_BYTES)
                .map(|source| source.bytes.len())
                .sum()
        });
        render_model::MAX_UI_NATIVE_SHEET_BYTES
            .saturating_sub(extra + pack)
            .max(MAX_PACK_TEXTURE_BYTES as usize)
            .min(resource_pack::MAX_FILE_BYTES as usize) as u64
    }

    /// Publishes a completed installed image only if its source still fits the aggregate allowance.
    pub(super) fn collect_local_source(&self) -> bool {
        let Some((key, found)) = self.local_reads.collect() else {
            return false;
        };
        let found = found.filter(|source| source.bytes.len() as u64 <= self.source_read_limit());
        let mut extra = self.extra.borrow_mut();
        if extra.len() >= MAX_EXTRA {
            extra.pop_first();
        }
        extra.insert(key, found);
        true
    }
}

#[cfg(test)]
mod tests;
