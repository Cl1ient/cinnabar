//! Shared texture layers and destination-specific animation bindings.

use assets::texture::policy::MAX_SUBSCRIBER_BYTES;
use assets::{Animation, NO_ANIMATION, TextureRef, texture::flipbook::FlipbookSource};
use pack_runtime::pack_textures::DecodedTexture;

use super::{
    Builder, Source, TextureSlot,
    textures::{FrameSequence, admit_static_rectangle, flipbook_frames},
};

/// A retained physical frame sequence, reusable by independently timed destinations.
#[derive(Clone, Copy)]
pub(super) struct FrameStorage {
    layer: u32,
    frame_start: u32,
    frame_count: u32,
}

/// Physical failures are shared; timing failures belong only to the requested destination.
enum AdmissionFailure {
    Source,
    Timing,
}

#[cfg(test)]
mod tests;

impl Builder<'_> {
    /// Reuses cached source frames before checking capacity for new image reads.
    pub(super) fn load_texture(&mut self, key: &str, selector: (u32, u32)) -> Option<TextureSlot> {
        let source = self.catalog.flipbook(key, selector).cloned();
        if let Some(source) = source.as_ref()
            && let Some(storage) = self.source_storage(key, Some(source))
            && let Some(slot) = self.bind_frames(storage, Some(source), selector)
        {
            return Some(slot);
        }
        let storage = self.source_storage(key, None)?;
        self.bind_frames(storage, None, selector)
    }

    /// Caches immutable source admission for both paths without caching destination timing errors.
    fn source_storage(
        &mut self,
        key: &str,
        source: Option<&FlipbookSource>,
    ) -> Option<FrameStorage> {
        let storage_key = self.catalog.storage_key(key, source)?;
        if let Some(&storage) = self.frame_storage.get(&storage_key) {
            return storage;
        }
        let available = match source {
            Some(_) => self.max_layers.saturating_sub(self.sources.len()),
            None => self.remaining_capacity(key).unwrap_or(0),
        };
        let admission = (available > 0).then(|| {
            let texture = match source {
                Some(source) => self.catalog.decode_path(key, &source.texture_path),
                None => self.catalog.decode(key),
            }
            .ok_or(AdmissionFailure::Source)?;
            self.store_frames(key, storage_key.grid, texture, source, available)
        });
        let storage = match admission {
            Some(Err(AdmissionFailure::Timing)) => return None,
            Some(Ok(storage)) => Some(storage),
            Some(Err(AdmissionFailure::Source)) | None => None,
        };
        self.frame_storage.insert(storage_key, storage);
        storage
    }

    /// Returns available layers, counting a full-array rejection before any image read.
    fn remaining_capacity(&mut self, key: &str) -> Option<usize> {
        let available = self.max_layers.saturating_sub(self.sources.len());
        if available == 0 {
            self.gaps.saturated_textures += 1;
            self.catalog.report_full(key);
            return None;
        }
        Some(available)
    }

    /// Admits unique pixels and timeline references without retaining a failed timing binding.
    fn store_frames(
        &mut self,
        key: &str,
        grid: u8,
        mut texture: DecodedTexture,
        source: Option<&FlipbookSource>,
        available: usize,
    ) -> Result<FrameStorage, AdmissionFailure> {
        let [width, height] = match source {
            Some(source) => {
                let side = (texture.width.min(texture.height) / source.replicate.max(1)).max(1);
                [side.min(self.texture_policy.max_side()); 2]
            }
            None => {
                admit_static_rectangle(&mut texture);
                [texture.width, texture.height]
            }
        };
        let tile = width
            .max(height)
            .checked_next_power_of_two()
            .ok_or(AdmissionFailure::Source)?
            .max(self.tile_size);
        let Some(layer_bytes) = self.texture_policy.terrain_bytes(tile, 1) else {
            self.catalog.report_budget(key);
            return Err(AdmissionFailure::Source);
        };
        let frame_bytes = self
            .texture_policy
            .source_bytes(width, height, false)
            .ok_or(AdmissionFailure::Source)?;
        let available = available
            .min((MAX_SUBSCRIBER_BYTES / layer_bytes).saturating_sub(self.sources.len()))
            .min(MAX_SUBSCRIBER_BYTES.saturating_sub(self.source_bytes) / frame_bytes);
        if available == 0 {
            self.catalog.report_budget(key);
            return Err(AdmissionFailure::Source);
        }
        let frames = match source {
            Some(source) => {
                let frames = flipbook_frames(
                    &texture,
                    source,
                    available.min(256),
                    self.texture_policy.max_side(),
                );
                drop(texture);
                frames
            }
            None => FrameSequence {
                frames: vec![texture],
                timeline: vec![0],
            },
        };
        if frames
            .frames
            .iter()
            .any(|frame| frame.width >> grid == 0 || frame.height >> grid == 0)
        {
            return Err(AdmissionFailure::Source);
        }
        let added_bytes: usize = frames.frames.iter().map(|frame| frame.rgba8.len()).sum();
        if frames.frames.is_empty() || self.source_bytes + added_bytes > MAX_SUBSCRIBER_BYTES {
            return Err(AdmissionFailure::Source);
        }
        let frame_count = frames.timeline.len() as u32;
        if frame_count > 1
            && self
                .overlay
                .animation_frames
                .len()
                .checked_add(frame_count as usize)
                .is_none_or(|total| total > assets::MAX_ANIMATION_FRAMES)
        {
            self.catalog.report_budget(key);
            return Err(AdmissionFailure::Source);
        }
        if source.is_some_and(|source| frame_count.checked_mul(source.ticks_per_frame).is_none()) {
            return Err(AdmissionFailure::Timing);
        }
        self.tile_size = tile;
        self.source_bytes += added_bytes;
        let layer = self.sources.len() as u32;
        self.sources.extend(frames.frames.into_iter().map(|frame| {
            if grid == 0 {
                Source::Image(frame)
            } else {
                Source::GridImage(frame, grid)
            }
        }));
        let frame_start = self.overlay.animation_frames.len() as u32;
        if frame_count > 1 {
            self.overlay.animation_frames.extend(
                frames
                    .timeline
                    .into_iter()
                    .map(|index| TextureRef::new(1, layer + index).expect("bounded layer")),
            );
        }
        Ok(FrameStorage {
            layer,
            frame_start,
            frame_count,
        })
    }

    /// Associates shared frames with this destination's timing, blending, and selectors.
    fn bind_frames(
        &mut self,
        storage: FrameStorage,
        source: Option<&FlipbookSource>,
        selector: (u32, u32),
    ) -> Option<TextureSlot> {
        let animation = match source {
            Some(source) if storage.frame_count > 1 => {
                storage.frame_count.checked_mul(source.ticks_per_frame)?;
                self.overlay.animations.push(Animation {
                    frame_start: storage.frame_start,
                    frame_count: storage.frame_count,
                    ticks_per_frame: source.ticks_per_frame,
                    atlas_index: selector.0,
                    atlas_tile_variant: selector.1,
                    replicate: source.replicate,
                    flags: u32::from(source.blend_frames) * assets::ANIMATION_FLAG_BLEND,
                });
                self.overlay.animations.len() as u32 - 1
            }
            _ => NO_ANIMATION,
        };
        Some(TextureSlot {
            layer: storage.layer,
            animation,
        })
    }
}
