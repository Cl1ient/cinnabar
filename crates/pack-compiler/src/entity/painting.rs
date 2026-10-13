//! Compiles built-in hanging pictures using the accepted pack's painting rasters.
use super::*;
use assets::painting::{PAINTING_ATLAS, PAINTING_ATLAS_SIDE, PAINTING_MOTIVES, PaintingMotive};
use serde_json::json;

const CONTROLLER: &str = "controller.render.cinnabar.painting";

/// Adds original static bindings for every painting part with a retained raster.
pub(super) fn complete_bindings(
    root: &Path,
    sources: &mut Vec<EntityAssetSource>,
    payloads: &mut SourcePayloads,
    symbols: &mut BTreeMap<(EntityAssetKind, Box<str>, Box<str>), PendingSymbol>,
    geometries: &mut BTreeMap<(Box<str>, Box<str>), PendingGeometry>,
) -> Result<usize, AssetError> {
    let has_texture = |texture: &str| {
        ["png", "tga"]
            .iter()
            .any(|extension| payloads.contains_key(format!("{texture}.{extension}").as_str()))
    };
    let parts: Vec<_> = PAINTING_MOTIVES
        .iter()
        .copied()
        .flat_map(|motive| {
            [false, true].into_iter().filter_map(move |border| {
                let texture = if border {
                    PAINTING_ATLAS.into()
                } else {
                    motive.texture()
                };
                has_texture(&texture).then_some((motive, border, texture))
            })
        })
        .collect();
    if parts.is_empty() {
        return Ok(0);
    }
    let mut bytes_added = 0;
    let mut add = |path: String, value: Value| -> Result<(), AssetError> {
        let bytes =
            serde_json::to_vec(&value).map_err(|_| invalid("painting binding encoding failed"))?;
        parse_source(&path, &root.join(&path), &bytes, symbols, geometries)?;
        sources.push(EntityAssetSource {
            path: path.clone().into(),
            source_bytes: bytes.len() as u32,
            source_sha256: Sha256::digest(&bytes).into(),
        });
        bytes_added += bytes.len();
        payloads.insert(path.into(), bytes.into());
        Ok(())
    };
    add(
        "render_controllers/_cinnabar/paintings.json".into(),
        json!({
            "format_version":"1.8.0", "render_controllers": {CONTROLLER: {
                "geometry":"Geometry.default", "materials":[{"*":"Material.default"}],
                "textures":["Texture.default"]
            }}
        }),
    )?;
    for (motive, border, texture) in parts {
        let identifier = motive.part_identifier(border);
        let part = if border { "border" } else { "front" };
        let geometry = format!("geometry.cinnabar.painting.{}.{}", motive.title, part);
        add(
            format!("entity/_cinnabar/painting_{}_{}.json", motive.title, part),
            json!({
                "format_version":"1.10.0", "minecraft:client_entity":{"description":{
                    "identifier":identifier, "materials":{"default":"entity_alphatest"},
                    "textures":{"default":texture}, "geometry":{"default":geometry},
                    "render_controllers":[CONTROLLER]
                }}
            }),
        )?;
        add(
            format!(
                "models/entity/_cinnabar/painting_{}_{}.json",
                motive.title, part
            ),
            geometry_document(motive, border, &geometry),
        )?;
    }
    Ok(bytes_added)
}

/// Builds the centered front plane or tiled one-pixel border independently of raster resolution.
fn geometry_document(motive: PaintingMotive, border: bool, identifier: &str) -> Value {
    let [width, height] = motive.size.map(f32::from);
    let mut cubes = Vec::new();
    let texture_size;
    let half_depth = assets::painting::PAINTING_HALF_DEPTH * 16.0;
    let [border_u, border_v] = assets::painting::PAINTING_BORDER_ORIGIN;
    let border_sample = assets::painting::PAINTING_BORDER_SAMPLE_PIXELS;
    if border {
        texture_size = [PAINTING_ATLAS_SIDE; 2];
        for x in 0..motive.size[0] / 16 {
            for y in 0..motive.size[1] / 16 {
                cubes.push(json!({
                    "origin":[-width/2.0 + f32::from(x)*16.0, -height/2.0 + f32::from(y)*16.0, -half_depth],
                    "size":[16,16,2.0*half_depth], "uv": {
                        "south":{"uv":[border_u,border_v],"uv_size":[16,16]},
                        "east":{"uv":[border_u+border_sample,border_v],"uv_size":[0,16]},
                        "west":{"uv":[border_u+border_sample,border_v],"uv_size":[0,16]},
                        "up":{"uv":[border_u,border_v],"uv_size":[16,border_sample]},
                        "down":{"uv":[border_u,border_v],"uv_size":[16,border_sample]}
                    }
                }));
            }
        }
    } else {
        let origin = motive.atlas_origin.unwrap_or([0, 0]);
        texture_size = if motive.atlas_origin.is_some() {
            [PAINTING_ATLAS_SIDE; 2]
        } else {
            motive.size.map(u16::from)
        };
        cubes.push(
            json!({"origin":[-width/2.0,-height/2.0,-half_depth],"size":[width,height,0],
            "uv":{"north":{"uv":origin,"uv_size":[width,height]}}}),
        );
    }
    json!({"format_version":"1.12.0", "minecraft:geometry":[{
        "description":{"identifier":identifier,"texture_width":texture_size[0],"texture_height":texture_size[1],
            "visible_bounds_width":width/16.0,"visible_bounds_height":height/16.0,"visible_bounds_offset":[0,0,0]},
        "bones":[{"name":"root","pivot":[0,0,0],"cubes":cubes}]
    }]})
}

#[cfg(test)]
mod tests {

    /// Encodes original artwork with a distinct colour for every logical atlas region.
    fn png(width: u32, height: u32, color: [u8; 4]) -> Vec<u8> {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(width, height, image::Rgba(color))
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        bytes.into_inner()
    }

    #[test]
    fn texture_only_pack_compiles_legacy_and_individual_painting_artwork() {
        let compiled = crate::compile_actor_pack(vec![
            (
                "textures/painting/kz.png".into(),
                png(512, 512, [40, 80, 120, 255]),
            ),
            (
                "textures/painting/meditative.png".into(),
                png(32, 32, [150, 30, 10, 255]),
            ),
        ])
        .unwrap()
        .expect("painting pack");
        let runtime = assets::RuntimeEntityAssets::from_compiled(compiled.entities).unwrap();
        for (title, border, texture) in [
            ("Kebab", false, "textures/painting/kz.png"),
            ("Kebab", true, "textures/painting/kz.png"),
            ("meditative", false, "textures/painting/meditative.png"),
        ] {
            let identifier = assets::painting::motive(title).part_identifier(border);
            let symbol = runtime
                .symbols()
                .iter()
                .position(|symbol| symbol.identifier.as_ref() == identifier)
                .expect("painting part binding");
            let rig = runtime
                .rig_bindings()
                .iter()
                .position(|rig| rig.entity_symbol as usize == symbol)
                .unwrap();
            let binding = compiled
                .bindings
                .iter()
                .find(|binding| binding.rig as usize == rig)
                .expect("painting part has decoded artwork");
            let raster = &compiled.textures[binding.texture as usize];
            assert_eq!(
                runtime.sources()[raster.source as usize].path.as_ref(),
                texture
            );
        }
        assert_eq!(compiled.skipped.unparsable, 0);
    }
}
