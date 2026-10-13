use super::*;

/// Reads a small synthetic source pack without compiling any image pixels.
fn pack() -> PackSources {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("textures")).unwrap();
    for (path, bytes) in [
        ("blocks.json", "{}"),
        (
            "textures/terrain_texture.json",
            r#"{"texture_data":{"water":{"textures":"textures/water"}}}"#,
        ),
        (
            "textures/flipbook_textures.json",
            r#"[{"atlas_tile":"water","flipbook_texture":"textures/water","atlas_index":1,"atlas_tile_variant":2,"replicate":2}]"#,
        ),
    ] {
        std::fs::write(root.path().join(path), bytes).unwrap();
    }
    crate::read_pack(root.path()).unwrap()
}

#[test]
fn compiled_animation_destinations_survive_generated_material_copies() {
    let pack = pack();
    let descriptors = BTreeMap::from([(
        Descriptor {
            path: "textures/water".into(),
            texture_key: "water".into(),
            flags: 0,
            state_variant: 0,
            flipbook_selector: None,
        },
        1,
    )]);
    let materials = [Material {
        texture: TextureRef::DIAGNOSTIC,
        flags: 0,
        animation: 0,
        ..Material::unvaried()
    }; 2];
    let animations = [Animation {
        frame_start: 0,
        frame_count: 2,
        ticks_per_frame: 1,
        atlas_index: 1,
        atlas_tile_variant: 2,
        replicate: 2,
        flags: 0,
    }];
    let keys = build(&pack, &descriptors, &materials, &animations);
    assert_eq!(keys.flipbook_selector(1), (1, 2));
    let keys = extend(keys, &[], Vec::<(Box<str>, [u8; 3])>::new(), &[(2, 1)]);
    assert_eq!(keys.materials("water"), [1, 2]);
    assert_eq!(keys.flipbook_selector(2), (1, 2));
    assert_eq!(keys.flipbooks(), pack.flipbooks.as_ref());
}

#[test]
fn unanimated_state_destinations_remain_available_to_new_pack_descriptions() {
    let pack = pack();
    let descriptors = BTreeMap::from([(
        Descriptor {
            path: "textures/water".into(),
            texture_key: "water".into(),
            flags: 0,
            state_variant: 2,
            flipbook_selector: None,
        },
        1,
    )]);
    let materials = [Material {
        texture: TextureRef::DIAGNOSTIC,
        flags: 0,
        animation: NO_ANIMATION,
        ..Material::unvaried()
    }; 2];
    let keys = build(&pack, &descriptors, &materials, &[]);
    assert_eq!(keys.flipbook_selector(1), (2, 0));
}

#[test]
fn weighted_uv_leaves_and_the_representative_selector_are_recorded() {
    use crate::animation::{AnimationLimits, DecodedImage, compile_animation_plan};
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("textures")).unwrap();
    for (path, bytes) in [
        ("blocks.json", "{}"),
        (
            "textures/terrain_texture.json",
            r#"{"texture_data":{"tile":{"textures":["textures/shared",{"variations":[
            {"path":"textures/shared","weight":1},{"path":"textures/shared","weight":3}
        ]}]}}}"#,
        ),
        ("textures/flipbook_textures.json", "[]"),
    ] {
        std::fs::write(root.path().join(path), bytes).unwrap();
    }
    let pack = crate::read_pack(root.path()).unwrap();
    let descriptor = Descriptor {
        path: "textures/shared".into(),
        texture_key: "tile".into(),
        flags: 0,
        state_variant: 1,
        flipbook_selector: None,
    };
    let mut descriptors = BTreeMap::from([(descriptor.clone(), "tile".into())]);
    let groups = super::super::variations::expand(&pack, &mut descriptors);
    let image = DecodedImage {
        source_path: "textures/shared".into(),
        width: assets::TILE_SIZE,
        height: assets::TILE_SIZE,
        rgba8: [31, 47, 59, 255]
            .into_iter()
            .cycle()
            .take((assets::TILE_SIZE * assets::TILE_SIZE * 4) as usize)
            .collect(),
    };
    let plan = compile_animation_plan(
        &pack,
        &[image],
        AnimationLimits {
            max_layers_per_page: 4,
            max_pages: 1,
        },
    )
    .unwrap();
    let (materials, ids) =
        super::super::compile_materials(&descriptors, &plan, &BTreeSet::new()).unwrap();
    let (materials, ids) = super::super::variations::install(materials, ids, groups).unwrap();
    let selector_id = ids[&descriptor];
    let selector = materials[selector_id as usize];
    assert_eq!(selector.variation_count, 2);
    let first = selector.variation_start;
    let second = first + 1;
    let keys = build(&pack, &ids, &materials, &[]);
    assert_eq!(keys.materials("tile"), [first, second, selector_id]);
    assert_eq!(keys.flipbook_selector(selector_id), (1, 0));
    assert_eq!(keys.flipbook_selector(first), (1, 0));
    assert_eq!(keys.flipbook_selector(second), (1, 1));
    assert_eq!(
        materials[first as usize].texture,
        materials[second as usize].texture
    );
    assert_eq!(
        f32::from_bits(materials[first as usize].variation_weight),
        0.25
    );
    assert_eq!(
        f32::from_bits(materials[second as usize].variation_weight),
        0.75
    );
    assert_eq!(materials[selector_id as usize], selector);
    assert_eq!(
        MaterialKeys::from_json(&keys.to_json(materials.len() as u32), materials.len()),
        Some(keys)
    );
}

#[test]
fn matching_pixels_share_storage_without_collapsing_material_destinations() {
    use crate::animation::{AnimationLimits, DecodedImage, compile_animation_plan};
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("textures")).unwrap();
    for (path, bytes) in [
        ("blocks.json", "{}"),
        (
            "textures/terrain_texture.json",
            r#"{"texture_data":{
            "water":{"textures":["textures/shared", "textures/shared"]},
            "alias":{"textures":"textures/shared"}
        }}"#,
        ),
        ("textures/flipbook_textures.json", "[]"),
    ] {
        std::fs::write(root.path().join(path), bytes).unwrap();
    }
    let pack = crate::read_pack(root.path()).unwrap();
    let image = DecodedImage {
        source_path: "textures/shared".into(),
        width: assets::TILE_SIZE,
        height: assets::TILE_SIZE,
        rgba8: [31, 47, 59, 255]
            .into_iter()
            .cycle()
            .take((assets::TILE_SIZE * assets::TILE_SIZE * 4) as usize)
            .collect(),
    };
    let plan = compile_animation_plan(
        &pack,
        &[image],
        AnimationLimits {
            max_layers_per_page: 4,
            max_pages: 1,
        },
    )
    .unwrap();
    let first = Descriptor {
        path: "textures/shared".into(),
        texture_key: "water".into(),
        flags: 0,
        state_variant: 0,
        flipbook_selector: None,
    };
    let second = Descriptor {
        state_variant: 1,
        ..first.clone()
    };
    let same_destination = Descriptor {
        flipbook_selector: Some((0, 0)),
        ..first.clone()
    };
    let alias = Descriptor {
        texture_key: "alias".into(),
        ..first.clone()
    };
    let descriptors = BTreeMap::from([
        (first.clone(), "water".into()),
        (second.clone(), "water".into()),
        (same_destination.clone(), "water".into()),
        (alias.clone(), "alias".into()),
    ]);
    let (materials, ids) =
        super::super::compile_materials(&descriptors, &plan, &BTreeSet::new()).unwrap();
    let (first_id, second_id, alias_id) = (ids[&first], ids[&second], ids[&alias]);
    assert_ne!(first_id, second_id);
    assert_ne!(first_id, alias_id);
    assert_eq!(first_id, ids[&same_destination]);
    assert_eq!(
        materials[first_id as usize].texture,
        materials[second_id as usize].texture
    );
    assert_eq!(
        materials[first_id as usize].texture,
        materials[alias_id as usize].texture
    );
    let keys = build(&pack, &ids, &materials, &[]);
    assert_eq!(keys.materials("water").len(), 2);
    assert_eq!(keys.flipbook_selector(first_id), (0, 0));
    assert_eq!(keys.flipbook_selector(second_id), (1, 0));
    assert_eq!(keys.materials("alias"), [alias_id]);
}
