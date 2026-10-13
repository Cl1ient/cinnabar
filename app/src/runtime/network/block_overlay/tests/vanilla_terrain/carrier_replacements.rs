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
    let mut cube = super::super::super::diagnostic_visual();
    cube.kind = VisualKind::Cube;
    cube.support = VisualSupport::Exact;
    cube.flags = assets::BlockFlags::CUBE_GEOMETRY | assets::BlockFlags::OCCLUDES_FULL_FACE;
    cube.faces = faces;
    let materials = std::iter::once(Material::unvaried())
        .chain(flags.iter().enumerate().map(|(index, &flags)| Material {
            texture: assets::TextureRef::new(0, index as u32 % colors.len() as u32).unwrap(),
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
                    baked_tint: None,
                },
            ),
            (
                2,
                MaterialTextureSource {
                    key: "pad".into(),
                    variant: 0,
                    path: "textures/pad".into(),
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
