//! Texture-only packs replace startup artwork without introducing attachable bindings.

use assets::{EquipmentTexture, RuntimeEquipmentCatalog};
use resource_pack::LayeredPackView;

/// Decodes only startup texture paths the accepted stack provides, and retains their source files.
pub(super) fn collect(
    view: &LayeredPackView,
    base: Option<&RuntimeEquipmentCatalog>,
    files: &mut Vec<(Box<str>, Vec<u8>)>,
) -> Vec<EquipmentTexture> {
    let mut textures = Vec::new();
    let mut retained = 0;
    for texture in base.into_iter().flat_map(RuntimeEquipmentCatalog::textures) {
        for (extension, format) in [
            ("png", image::ImageFormat::Png),
            ("tga", image::ImageFormat::Tga),
        ] {
            let path = format!("{}.{extension}", texture.identifier);
            let Some(bytes) = view.read_capped(&path, resource_pack::MAX_FILE_BYTES) else {
                continue;
            };
            if let Some((width, height, rgba8)) = pack_compiler::decode_equipment_raster(
                &bytes,
                format,
                assets::MAX_EQUIPMENT_PIXEL_BYTES - retained,
            ) {
                retained += rgba8.len();
                textures.push(EquipmentTexture {
                    identifier: texture.identifier.clone(),
                    width,
                    height,
                    rgba8,
                });
                if !files.iter().any(|(existing, _)| existing.as_ref() == path) {
                    files.push((path.into(), bytes.into_vec()));
                }
            }
            break;
        }
    }
    textures
}

/// Adds available startup-path overrides without exceeding the equipment catalog's budgets.
pub(super) fn merge(textures: &mut Vec<EquipmentTexture>, overrides: Vec<EquipmentTexture>) {
    let mut retained: usize = textures.iter().map(|texture| texture.rgba8.len()).sum();
    for texture in overrides {
        let previous = textures
            .iter()
            .position(|old| old.identifier == texture.identifier);
        let old_bytes = previous.map_or(0, |index| textures[index].rgba8.len());
        let bytes = retained - old_bytes + texture.rgba8.len();
        if bytes > assets::MAX_EQUIPMENT_PIXEL_BYTES
            || (previous.is_none() && textures.len() == assets::MAX_EQUIPMENT_TEXTURES)
        {
            continue;
        }
        retained = bytes;
        if let Some(index) = previous {
            textures[index] = texture;
        } else {
            textures.push(texture);
        }
    }
    textures.sort_by(|a, b| a.identifier.cmp(&b.identifier));
}
