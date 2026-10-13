use super::*;

/// Builds carrier layers and preserves their literal pixels at every mip level.
fn carrier(colors: &[[u8; 4]], faces: [u32; 6], flags: &[u32]) -> RuntimeAssets {
    let diagnostic = RuntimeAssets::diagnostic();
    let mut texture = diagnostic.texture_array().clone();
    texture.layers = colors.len() as u32;
    for mip in &mut texture.mips {
        mip.rgba8 = colors
            .iter()
            .flat_map(|color| color.repeat((mip.size * mip.size) as usize))
            .collect::<Vec<_>>()
            .into();
    }
    carrier_with_texture(texture, faces, flags)
}

/// Builds an immutable carrier around original pixel data and the supplied cube materials.
fn carrier_with_texture(
    texture: assets::TextureArray,
    faces: [u32; 6],
    flags: &[u32],
) -> RuntimeAssets {
    let diagnostic = RuntimeAssets::diagnostic();
    let layers = texture.layers;
    let mut cube = super::super::super::diagnostic_visual();
    cube.kind = VisualKind::Cube;
    cube.support = VisualSupport::Exact;
    cube.flags = assets::BlockFlags::CUBE_GEOMETRY | assets::BlockFlags::OCCLUDES_FULL_FACE;
    cube.faces = faces;
    let materials = std::iter::once(Material::unvaried())
        .chain(flags.iter().enumerate().map(|(index, &flags)| Material {
            texture: assets::TextureRef::new(0, index as u32 % layers).unwrap(),
            flags,
            ..Material::unvaried()
        }))
        .collect::<Vec<_>>();
    let compiled = assets::CompiledAssets {
        visuals: vec![cube; 2].into(),
        light_properties: vec![assets::LightProperties::OPAQUE_DARK; 2].into(),
        hashed: Box::new([]),
        materials: materials.into(),
        model_templates: Box::new([]),
        model_quads: Box::new([]),
        animations: Box::new([]),
        animation_frames: Box::new([]),
        texture_pages: vec![assets::TexturePage::new(texture)].into(),
        biomes: assets::CompiledBiomeAssets::diagnostic(),
        provenance: diagnostic.provenance(),
    };
    RuntimeAssets::decode(&assets::encode_blob(&compiled).unwrap()).unwrap()
}

/// Reads optional raster multipliers through the same public atlas operation as carrier creation.
fn tinted(mut pixel: [u8; 4], tint: Option<[u8; 3]>) -> [u8; 4] {
    if let Some(tint) = tint {
        pack_compiler::apply_atlas_tint(&mut pixel, tint);
    }
    pixel
}

#[test]
fn carrier_fallback_preserves_baked_tint_and_applies_a_changed_winning_tint_once() {
    let pixel = [200, 100, 240, 128];
    let original_tint = [128, 255, 64];
    let base = carrier(
        &[pixel, tinted(pixel, Some(original_tint))],
        [2; 6],
        &[0, 0],
    );
    let keys = MaterialKeys::from_entries([(1, "pad"), (2, "pad")]).with_sources(
        [(
            "pad".into(),
            json!({"textures":{"path":"textures/pad","tint_color":"#80ff40"}}),
        )],
        [
            (
                1,
                MaterialTextureSource {
                    key: "pad".into(),
                    variant: 0,
                    path: "textures/pad".into(),
                    raster: "textures/pad.png".into(),
                    baked_tint: None,
                },
            ),
            (
                2,
                MaterialTextureSource {
                    key: "pad".into(),
                    variant: 0,
                    path: "textures/pad".into(),
                    raster: "textures/pad.png".into(),
                    baked_tint: Some(original_tint),
                },
            ),
        ],
        [],
    );
    for (entry, tint) in [
        (
            json!({"path":"textures/pad","tint_color":"#80ff40"}),
            Some(original_tint),
        ),
        (
            json!({"path":"textures/pad","tint_color":"#ff8040"}),
            Some([255, 128, 64]),
        ),
        (json!("textures/pad"), None),
    ] {
        let view = stack(&[(
            json!({"texture_data":{"pad":{"textures":entry}}}),
            json!({}),
            vec![],
        )]);
        let overlay = compile(&view, &base, &keys);
        let runtime = base.with_block_overlay(2, &overlay.overlay).unwrap();
        for material in [1, 2] {
            assert_eq!(material_color(&runtime, material), tinted(pixel, tint));
        }
    }
    assert_eq!(material_color(&base, 2), tinted(pixel, Some(original_tint)));
}

#[test]
fn carrier_fallback_caches_different_tints_of_the_same_raw_raster_separately() {
    let pixel = [200, 100, 240, 128];
    let base = carrier(&[pixel], [1; 6], &[0]);
    let keys = MaterialKeys::from_entries([(1, "fixture")]).with_sources(
        [("fixture".into(), json!({"textures":"textures/raw"}))],
        [(
            1,
            MaterialTextureSource {
                key: "fixture".into(),
                variant: 0,
                path: "textures/raw".into(),
                raster: "textures/raw.png".into(),
                baked_tint: None,
            },
        )],
        [],
    );
    let view = stack(&[(
        json!({"texture_data":{"fixture":{"textures":{"variations":[
            {"path":"textures/raw","tint_color":"#80ff40","weight":1},
            {"path":"textures/raw","tint_color":"#ff8040","weight":1}
        ]}}}}),
        json!({}),
        vec![],
    )]);
    let overlay = compile(&view, &base, &keys);
    let runtime = base.with_block_overlay(2, &overlay.overlay).unwrap();
    let selector = runtime.material(1);
    assert_eq!(selector.variation_count, 2);
    for (index, tint) in [[128, 255, 64], [255, 128, 64]].into_iter().enumerate() {
        assert_eq!(
            material_color(&runtime, selector.variation_start + index as u32),
            tinted(pixel, Some(tint))
        );
    }
}

#[test]
fn cube_bindings_resolve_new_catalog_keys_from_existing_carrier_raster_paths() {
    let pixel = [120, 80, 40, 255];
    let base = carrier(&[pixel], [1; 6], &[0, 0]);
    let keys = keys();
    let view = stack(&[(
        json!({"texture_data":{"replacement":{"textures":"textures/base_a"}}}),
        json!({"fixture":{"textures":"replacement","carried_textures":"replacement"}}),
        vec![],
    )]);
    let overlay = compile(&view, &base, &keys);
    let runtime = base.with_block_overlay(2, &overlay.overlay).unwrap();
    let visual = runtime.resolve(NetworkIdMode::Sequential, 0);
    for face in BlockFace::ALL {
        assert_eq!(
            material_color(&runtime, visual.face(face).material_id()),
            pixel
        );
        assert_ne!(visual.face(face).material_id(), 1);
    }
    let carried = &overlay.vanilla_carried[0];
    let sheet =
        render_model::equipment::blocks::overlay_face_sheet(&overlay.overlay, carried.faces)
            .unwrap();
    assert!(
        sheet
            .rgba8
            .as_chunks::<4>()
            .0
            .iter()
            .all(|sample| *sample == pixel)
    );
}

#[test]
fn scalar_and_per_face_pillar_bindings_recompute_rotation_in_both_directions() {
    for (mapping, rotations) in [
        ([2, 3, 1, 0, 4, 5], [false, false, true, true, true, true]),
        ([0, 1, 5, 4, 2, 3], [true, true, true, true, false, false]),
    ] {
        for original_per_face in [false, true] {
            let flags = rotations.map(|rotate| {
                if original_per_face && rotate {
                    assets::MATERIAL_FLAG_ROTATE_UV
                } else {
                    0
                }
            });
            let base = carrier(&[[100, 120, 140, 255]], [1, 2, 3, 4, 5, 6], &flags);
            let original_keys = keys();
            let mut blocks = original_keys.block_sources().to_vec();
            blocks[0].source_faces = mapping;
            blocks[0].source_rotations = rotations;
            let keys = MaterialKeys::from_entries((1..=6).map(|id| (id, "state"))).with_sources(
                [("state".into(), json!({"textures":"textures/base_a"}))],
                (1..=6).map(|id| {
                    (
                        id,
                        MaterialTextureSource {
                            key: "state".into(),
                            variant: 0,
                            path: "textures/base_a".into(),
                            raster: "textures/base_a.png".into(),
                            baked_tint: None,
                        },
                    )
                }),
                blocks,
            );
            for per_face in [false, true] {
                let binding = if per_face {
                    json!({"west":"replacement","east":"replacement","down":"replacement","up":"replacement","north":"replacement","south":"replacement"})
                } else {
                    json!("replacement")
                };
                let view = stack(&[(
                    json!({"texture_data":{"replacement":{"textures":"textures/new"}}}),
                    json!({"fixture":{"textures":binding,"carried_textures":"replacement"}}),
                    vec![("textures/new", [140, 90, 50, 255])],
                )]);
                let overlay = compile(&view, &base, &keys);
                let runtime = base.with_block_overlay(2, &overlay.overlay).unwrap();
                let visual = runtime.resolve(NetworkIdMode::Sequential, 0);
                for face in BlockFace::ALL {
                    let material = runtime.material(visual.face(face).material_id());
                    assert_eq!(
                        material.flags & assets::MATERIAL_FLAG_ROTATE_UV != 0,
                        per_face && rotations[face as usize]
                    );
                }
                assert!(
                    overlay.vanilla_carried[0]
                        .faces
                        .iter()
                        .all(|&id| overlay.overlay.materials[id as usize].flags
                            & assets::MATERIAL_FLAG_ROTATE_UV
                            == 0)
                );
            }
        }
    }
}

/// Supplies a PNG containing exactly the same pixels as the immutable carrier layer.
fn raster_pack(terrain: Value, blocks: Value, pixels: &assets::TextureMip) -> LayeredPackView {
    let id = "00000000-0000-0000-0000-000000000001";
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let manifest = json!({"format_version":2,"header":{"uuid":id,"version":[1,0,0]},"modules":[{"type":"resources"}]}).to_string();
    let bytes = png(pixels.size, pixels.size, |x, y| {
        let start = ((y * pixels.size + x) * 4) as usize;
        pixels.rgba8[start..start + 4].try_into().unwrap()
    });
    for (path, bytes) in [
        ("manifest.json", manifest.into_bytes()),
        (
            "textures/terrain_texture.json",
            terrain.to_string().into_bytes(),
        ),
        ("blocks.json", blocks.to_string().into_bytes()),
        ("textures/base_a.png", bytes),
    ] {
        writer
            .start_file(path, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(&bytes).unwrap();
    }
    let archive = protocol::ResourcePackArchive::unencrypted(
        id.parse().unwrap(),
        "1.0.0".into(),
        String::new(),
        writer.finish().unwrap().into_inner(),
    );
    LayeredPackView::new(resource_pack::validate_handoff(
        protocol::ResourcePackHandoff::from_archives(vec![archive]),
    ))
}

#[test]
fn carrier_and_pack_sources_share_grid_tint_and_carried_art_preparation() {
    let mut texture = RuntimeAssets::diagnostic().texture_array().clone();
    for mip in &mut texture.mips {
        let size = mip.size;
        mip.rgba8 = (0..size)
            .flat_map(|y| {
                (0..size).flat_map(move |x| match (x < size / 2, y < size / 2) {
                    (true, true) => [200, 0, 0, 255],
                    (false, true) => [0, 200, 0, 255],
                    (true, false) => [0, 0, 200, 255],
                    _ => [200, 200, 200, 255],
                })
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
    }
    let base = carrier_with_texture(texture.clone(), [1; 6], &[0, 0]);
    let keys = keys();
    let terrain = json!({"texture_data":{
        "quad":{"quad":1,"textures":"./textures/base_a.png"},
        "plain":{"quad":0,"textures":"textures/base_a"},
        "tinted":{"quad":1,"textures":{"path":"textures/base_a","tint_color":"#80ffff"}}
    }});
    let blocks = json!({"fixture":{"textures":"tinted","carried_textures":"quad"},"other":{"textures":"plain","carried_textures":"plain"}});
    let pack = compile(
        &raster_pack(terrain.clone(), blocks.clone(), &texture.mips[0]),
        &base,
        &keys,
    );
    let fallback = compile(&stack(&[(terrain, blocks, vec![])]), &base, &keys);
    assert_eq!(
        fallback.overlay.texture_source_grids,
        pack.overlay.texture_source_grids
    );
    assert_eq!(
        fallback.overlay.texture_source_sizes,
        pack.overlay.texture_source_sizes
    );
    assert_eq!(fallback.overlay.texture, pack.overlay.texture);
    let runtime = base.with_block_overlay(2, &fallback.overlay).unwrap();
    for (id, grid) in [(0, 1), (1, 0)] {
        let material = runtime
            .resolve(NetworkIdMode::Sequential, id)
            .face(BlockFace::Up)
            .material_id();
        assert_eq!(
            runtime.texture_source_grid(runtime.material(material).texture),
            grid
        );
    }
    let world = runtime
        .resolve(NetworkIdMode::Sequential, 0)
        .face(BlockFace::Up)
        .material_id();
    assert_eq!(material_color(&runtime, world), [100, 0, 0, 255]);
    let faces = fallback
        .vanilla_carried
        .iter()
        .find(|entry| entry.visual == 0)
        .unwrap()
        .faces;
    let sheet =
        render_model::equipment::blocks::overlay_face_sheet(&fallback.overlay, faces).unwrap();
    assert!(
        sheet
            .rgba8
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == [200, 0, 0, 255])
    );
    let icon = pack_compiler::overlay_cube_icon(&fallback.overlay, faces).unwrap();
    assert!(
        icon.rgba8
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] != 0)
            .all(|pixel| pixel[1] == 0 && pixel[2] == 0)
    );
}

#[test]
fn overlay_color_carried_bindings_leave_world_and_carried_routes_unchanged() {
    let base = carrier(&[[100, 120, 140, 128]], [1; 6], &[0, 0]);
    for carried in [
        json!({"path":"textures/base_a","overlay_color":"#20ff30"}),
        json!({"variations":[{"path":"textures/base_a","overlay_color":"#20ff30","weight":1}]}),
    ] {
        let terrain = json!({"texture_data":{"world":{"textures":"textures/base_a"},"carry":{"textures":carried}}});
        let blocks = json!({"fixture":{"textures":"world","carried_textures":"carry"}});
        for view in [
            stack(&[(terrain.clone(), blocks.clone(), vec![])]),
            raster_pack(terrain, blocks, &base.texture_array().mips[0]),
        ] {
            assert!(
                super::super::super::compile_block_overlay(
                    &view,
                    &CustomBlocks::default(),
                    false,
                    Some(&keys()),
                    assets::MAX_TEXTURE_LAYERS as u32,
                    Some(&base)
                )
                .is_none()
            );
        }
    }
}

#[test]
fn carrier_sources_resolve_explicit_png_and_relative_path_spellings() {
    let pixel = [90, 120, 30, 255];
    let base = carrier(&[pixel], [1; 6], &[0, 0]);
    for path in [
        "textures/base_a.png",
        "./textures/base_a.png",
        "././textures/base_a",
    ] {
        let view = stack(&[(
            json!({"texture_data":{"replacement":{"textures":path}}}),
            json!({"fixture":{"textures":"replacement","carried_textures":"replacement"}}),
            vec![],
        )]);
        let overlay = compile(&view, &base, &keys());
        let runtime = base.with_block_overlay(2, &overlay.overlay).unwrap();
        for face in BlockFace::ALL {
            let material = runtime
                .resolve(NetworkIdMode::Sequential, 0)
                .face(face)
                .material_id();
            assert_ne!(material, 1);
            assert_eq!(material_color(&runtime, material), pixel);
        }
    }
}
