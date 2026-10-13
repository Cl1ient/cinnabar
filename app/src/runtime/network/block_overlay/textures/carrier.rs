//! Carrier pixels supplied to the shared texture preparation pipeline.

use super::{DecodedTexture, TextureImage};
use assets::{RuntimeAssets, material_keys::MaterialKeys};

/// Reads the resolved raster's frames without applying catalog tint, grid, or storage policy.
pub(in super::super) fn carrier_image(
    base: &RuntimeAssets,
    keys: &MaterialKeys,
    path: &str,
    tint: Option<[u8; 3]>,
    max_frames: usize,
) -> Option<TextureImage> {
    let (id, source) = keys
        .material_sources()
        .filter(|(_, source)| {
            source.raster.as_ref() == path
                && (source.baked_tint.is_none() || source.baked_tint == tint)
        })
        .min_by_key(|(_, source)| source.baked_tint != tint)?;
    let material = base.material(id);
    let animation = base.animations().get(material.animation as usize).copied();
    let references = match animation {
        Some(animation) => base
            .animation_frames()
            .get(
                animation.frame_start as usize
                    ..animation.frame_start as usize + animation.frame_count as usize,
            )?
            .to_vec(),
        None => vec![material.texture],
    };
    let mut rgba8 = Vec::new();
    let mut side = None;
    let mut frames = 0;
    for reference in references.into_iter().take(max_frames) {
        let mip = base
            .texture_pages()
            .get(reference.page() as usize)?
            .texture
            .mips
            .first()?;
        if side.is_some_and(|size| size != mip.size) {
            return None;
        }
        side = Some(mip.size);
        let bytes = (mip.size * mip.size * 4) as usize;
        if rgba8.len() + bytes > super::super::MAX_SUBSCRIBER_BYTES {
            return None;
        }
        let start = reference.layer() as usize * bytes;
        rgba8.extend_from_slice(mip.rgba8.get(start..start + bytes)?);
        frames += 1;
    }
    let side = side?;
    Some(TextureImage {
        texture: DecodedTexture {
            width: side,
            height: side * frames,
            rgba8: rgba8.into_boxed_slice(),
        },
        baked_tint: source.baked_tint,
        animation,
    })
}
