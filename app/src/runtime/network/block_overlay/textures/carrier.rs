//! Carrier pixels supplied to the shared texture preparation pipeline.

use super::{DecodedTexture, TextureImage};
use assets::{RuntimeAssets, material_keys::MaterialKeys};

/// Reads only static donor pixels; baked animation timelines are not physical source strips.
pub(in super::super) fn carrier_image(
    base: &RuntimeAssets,
    keys: &MaterialKeys,
    path: &str,
    tint: Option<[u8; 3]>,
) -> Option<TextureImage> {
    let (id, source) = keys
        .material_sources()
        .filter(|(id, source)| {
            source.raster.eq_ignore_ascii_case(path)
                && base.material(*id).animation == assets::NO_ANIMATION
                && (source.baked_tint.is_none() || source.baked_tint == tint)
        })
        .min_by_key(|(_, source)| source.baked_tint != tint)?;
    let reference = base.material(id).texture;
    let mip = base
        .texture_pages()
        .get(reference.page() as usize)?
        .texture
        .mips
        .first()?;
    let bytes = (mip.size * mip.size * 4) as usize;
    let start = reference.layer() as usize * bytes;
    Some(TextureImage {
        image_path: path.to_ascii_lowercase(),
        texture: DecodedTexture {
            width: mip.size,
            height: mip.size,
            rgba8: mip.rgba8.get(start..start + bytes)?.into(),
        },
        baked_tint: source.baked_tint,
        pack: false,
    })
}
