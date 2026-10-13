//! Successful raster reuse and transactional texture admission.

use super::{Builder, TextureSlot, textures::TextureIdentity};
use std::collections::HashMap;

/// One admitted slot together with its image provider.
pub(super) struct AdmittedTexture {
    pub(super) slot: TextureSlot,
    pub(super) pack: bool,
}

struct CachedTexture {
    slot: Option<TextureSlot>,
    serial: u64,
}

/// Pack successes are shared with carrier-enabled requests; carrier-only pixels stay optional.
#[derive(Default)]
pub(super) struct TextureSlots {
    pack: HashMap<TextureIdentity, CachedTexture>,
    carrier: HashMap<TextureIdentity, CachedTexture>,
    serial: u64,
}

impl TextureSlots {
    /// Returns a cached success or failure for the requested image-source policy.
    pub(super) fn get(&self, key: &TextureIdentity, carrier: bool) -> Option<Option<TextureSlot>> {
        let pack = self.pack.get(key);
        if pack.is_some_and(|entry| entry.slot.is_some()) {
            return pack.map(|entry| entry.slot.clone());
        }
        if carrier {
            self.carrier.get(key).map(|entry| entry.slot.clone())
        } else {
            pack.map(|entry| entry.slot.clone())
        }
    }

    /// Records one result while retaining its source policy and admission order.
    pub(super) fn insert(
        &mut self,
        key: TextureIdentity,
        result: Option<AdmittedTexture>,
        carrier: bool,
    ) -> Option<TextureSlot> {
        self.serial += 1;
        let pack = result.as_ref().is_some_and(|result| result.pack);
        let slot = result.map(|result| result.slot);
        let target = if pack || !carrier {
            &mut self.pack
        } else {
            &mut self.carrier
        };
        target.insert(
            key,
            CachedTexture {
                slot: slot.clone(),
                serial: self.serial,
            },
        );
        slot
    }
}

/// Texture state owned by an in-progress selector, before any material references are published.
pub(super) struct TextureCheckpoint {
    sources: usize,
    bytes: usize,
    tile: u32,
    animations: usize,
    frames: usize,
    serial: u64,
}

impl Builder<'_> {
    /// Saves the append-only texture state before preparing a multi-choice material.
    pub(super) fn texture_checkpoint(&self) -> TextureCheckpoint {
        TextureCheckpoint {
            sources: self.sources.len(),
            bytes: self.source_bytes,
            tile: self.tile_size,
            animations: self.overlay.animations.len(),
            frames: self.overlay.animation_frames.len(),
            serial: self.slots.serial,
        }
    }

    /// Releases unreferenced partial admissions and their cached successes and failures.
    pub(super) fn rollback_textures(&mut self, checkpoint: TextureCheckpoint) {
        self.sources.truncate(checkpoint.sources);
        self.source_bytes = checkpoint.bytes;
        self.tile_size = checkpoint.tile;
        self.overlay.animations.truncate(checkpoint.animations);
        self.overlay.animation_frames.truncate(checkpoint.frames);
        self.slots
            .pack
            .retain(|_, entry| entry.serial <= checkpoint.serial);
        self.slots
            .carrier
            .retain(|_, entry| entry.serial <= checkpoint.serial);
        self.slots.serial = checkpoint.serial;
    }
}
