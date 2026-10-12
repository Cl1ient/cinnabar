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
