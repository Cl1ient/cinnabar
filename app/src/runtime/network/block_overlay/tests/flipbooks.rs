use super::*;
use assets::flipbook::FlipbookSource;

/// Admits a pack containing only the synthetic files named by the test.
fn pack(files: &[(&str, &[u8])]) -> LayeredPackView {
    let id = "00000000-0000-0000-0000-000000000019";
    let manifest = format!(
        r#"{{"format_version":2,"header":{{"uuid":"{id}","version":[1,0,0]}},"modules":[{{"type":"resources"}}]}}"#
    );
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (path, bytes) in
        std::iter::once(("manifest.json", manifest.as_bytes())).chain(files.iter().copied())
    {
        writer
            .start_file(path, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    LayeredPackView::new(resource_pack::validate_handoff(
        protocol::ResourcePackHandoff::from_archives(vec![
            protocol::ResourcePackArchive::unencrypted(
                id.parse().unwrap(),
                "1.0.0".into(),
                String::new(),
                writer.finish().unwrap().into_inner(),
            ),
        ]),
    ))
}

/// Returns a frame's first pixel from the compiled overlay's physical page.
fn pixel(overlay: &assets::BlockOverlay, frame: assets::TextureRef) -> [u8; 4] {
    let mip = &overlay.texture.as_ref().unwrap().mips[0];
    let start = frame.layer() as usize * (mip.size * mip.size * 4) as usize;
    mip.rgba8[start..start + 4].try_into().unwrap()
}

/// Builds a tiny immutable carrier with one base animation and a replaceable material.
fn animated_base() -> Arc<RuntimeAssets> {
    use assets::*;
    let diagnostic = RuntimeAssets::diagnostic();
    let compiled = CompiledAssets {
        visuals: vec![BlockVisual::diagnostic(
            BlockFlags::empty(),
            ContributorRole::Primary,
        )]
        .into(),
        light_properties: vec![LightProperties::default()].into(),
        hashed: Box::new([]),
        materials: vec![
            diagnostic.material(0),
            Material {
                animation: 0,
                ..diagnostic.material(0)
            },
        ]
        .into(),
        model_templates: Box::new([]),
        model_quads: Box::new([]),
        animations: vec![Animation {
            frame_start: 0,
            frame_count: 2,
            ticks_per_frame: 7,
            atlas_index: 0,
            atlas_tile_variant: 0,
            replicate: 1,
            flags: 0,
        }]
        .into(),
        animation_frames: vec![TextureRef::DIAGNOSTIC; 2].into(),
        texture_pages: diagnostic.texture_pages().to_vec().into(),
        biomes: CompiledBiomeAssets::diagnostic(),
        provenance: BlobProvenance {
            source_manifest_sha256: [1; 32],
            block_registry_sha256: [2; 32],
            light_registry_sha256: [3; 32],
            biome_registry_sha256: [4; 32],
        },
    };
    Arc::new(RuntimeAssets::decode(&encode_blob(&compiled).unwrap()).unwrap())
}

#[test]
fn raster_only_pack_replaces_all_base_frames_and_removal_restores_the_carrier() {
    let source: FlipbookSource = serde_json::from_value(serde_json::json!({
        "flipbook_texture":"textures/water", "atlas_tile":"water",
        "frames":[1,0,1], "ticks_per_frame":7, "blend_frames":false
    }))
    .unwrap();
    let keys = assets::MaterialKeys::from_entries([(1, "water")])
        .with_aliases([("water", "textures/water")])
        .with_flipbooks(&[source]);
    let strip = png(16, 32, |_, y| {
        if y < 16 {
            [250, 20, 10, 255]
        } else {
            [10, 30, 240, 255]
        }
    });
    let compiled = Arc::new(
        compile_block_overlay(
            &pack(&[("textures/water.png", &strip)]),
            &CustomBlocks::default(),
            false,
            Some(&keys),
        )
        .unwrap(),
    );
    let animation = &compiled.overlay.animations[0];
    assert_eq!(
        (
            animation.frame_count,
            animation.ticks_per_frame,
            animation.flags
        ),
        (3, 7, 0)
    );
    assert_eq!(
        compiled
            .overlay
            .animation_frames
            .iter()
            .map(|&frame| pixel(&compiled.overlay, frame))
            .collect::<Vec<_>>(),
        [[10, 30, 240, 255], [250, 20, 10, 255], [10, 30, 240, 255]]
    );
    let base = animated_base();
    let session =
        super::super::super::resource_packs::session_runtime_assets(&base, None, Some(&compiled));
    assert_eq!(session.material(1).texture.page(), 1);
    assert_eq!(session.material(1).animation, 1);
    assert_eq!(
        session.animations()[1].ticks_per_frame,
        base.animations()[0].ticks_per_frame
    );

    let removed = compile_block_overlay(&pack(&[]), &CustomBlocks::default(), false, Some(&keys));
    assert!(removed.is_none());
    let restored = super::super::super::resource_packs::session_runtime_assets(&base, None, None);
    assert!(Arc::ptr_eq(&restored, &base));
    assert_eq!(restored.material(1).animation, 0);
    assert_eq!(restored.material(1).texture.page(), 0);
}

#[test]
fn independent_sources_and_full_selectors_keep_distinct_replicated_frames() {
    let keys = assets::MaterialKeys::from_entries([(1, "tile"), (2, "tile")])
        .with_aliases([("tile", "textures/static")])
        .with_flipbook_selectors([(1, (1, 0)), (2, (1, 1))])
        .with_flipbook_destinations([("tile", [1, 2])]);
    let first = png(32, 64, |x, y| {
        if x < 16 && y % 32 < 16 {
            if y < 32 {
                [230, 10, 20, 255]
            } else {
                [20, 10, 230, 255]
            }
        } else {
            [0, 0, 0, 255]
        }
    });
    let second = png(32, 64, |x, y| {
        if x < 16 && y % 32 < 16 {
            if y < 32 {
                [230, 210, 20, 255]
            } else {
                [20, 210, 230, 255]
            }
        } else {
            [0, 0, 0, 255]
        }
    });
    let static_tile = png(16, 16, |_, _| [0, 180, 0, 255]);
    let descriptions = br#"[
        {"atlas_tile":"tile","atlas_index":1,"atlas_tile_variant":0,"flipbook_texture":"textures/first","replicate":2,"frames":[1,0],"ticks_per_frame":3},
        {"atlas_tile":"tile","atlas_index":1,"atlas_tile_variant":1,"flipbook_texture":"textures/second","replicate":2,"ticks_per_frame":5,"blend_frames":false}
    ]"#;
    let compiled = compile_block_overlay(
        &pack(&[
            ("textures/static.png", &static_tile),
            ("textures/first.png", &first),
            ("textures/second.png", &second),
            ("textures/flipbook_textures.json", descriptions),
        ]),
        &CustomBlocks::default(),
        false,
        Some(&keys),
    )
    .unwrap();
    assert_eq!(compiled.overlay.material_overrides.len(), 2);
    for (index, expected) in [[20, 10, 230, 255], [230, 210, 20, 255]]
        .into_iter()
        .enumerate()
    {
        let replacement = &compiled.overlay.material_overrides[index];
        let animation = &compiled.overlay.animations[replacement.animation as usize];
        assert_eq!(
            (
                animation.atlas_index,
                animation.atlas_tile_variant,
                animation.replicate
            ),
            (1, index as u32, 2)
        );
        assert_eq!(animation.ticks_per_frame, if index == 0 { 3 } else { 5 });
        assert_eq!(
            animation.flags,
            if index == 0 {
                assets::ANIMATION_FLAG_BLEND
            } else {
                0
            }
        );
        assert_eq!(pixel(&compiled.overlay, replacement.texture), expected);
        assert_eq!(
            compiled.overlay.texture_source_sizes[replacement.texture.layer() as usize],
            [16, 16]
        );
    }
}

#[test]
fn omitted_variant_reaches_every_destination_without_crossing_atlas_indices() {
    let view = pack(&[(
        "textures/flipbook_textures.json",
        br#"[
        {"atlas_tile":"tile","atlas_index":1,"flipbook_texture":"textures/strip"}
    ]"#,
    )]);
    let keys = assets::MaterialKeys::from_entries([(1, "tile")])
        .with_aliases([("tile", "textures/static")])
        .with_flipbook_destinations([("tile", [1, 3])]);
    let catalog = super::super::textures::TextureCatalog::new(&view, Some(&keys));
    assert!(catalog.flipbook("tile", (1, 0)).is_some());
    assert!(catalog.flipbook("tile", (1, 2)).is_some());
    assert!(catalog.flipbook("tile", (0, 0)).is_none());
}

#[test]
fn explicit_frames_clamp_to_the_last_physical_frame_before_replication() {
    let source: FlipbookSource = serde_json::from_value(serde_json::json!({
        "atlas_tile":"tile", "flipbook_texture":"textures/strip", "frames":[9,0], "replicate":2
    }))
    .unwrap();
    let strip = pack_runtime::pack_textures::DecodedTexture {
        width: 4,
        height: 8,
        rgba8: (0..8)
            .flat_map(|y| (0..4).flat_map(move |x| [x, y, 9, 255]))
            .collect(),
    };
    let frames = super::super::textures::flipbook_frames(&strip, &source, 2, 16);
    assert_eq!((frames[0].width, frames[0].height), (2, 2));
    assert_eq!(&frames[0].rgba8[..4], &[0, 4, 9, 255]);
    assert_eq!(&frames[1].rgba8[..4], &[0, 0, 9, 255]);
}

#[test]
fn invalid_uv_variants_fall_back_in_creation_order_and_invalid_indices_skip() {
    let keys = assets::MaterialKeys::from_entries([(1, "tile")])
        .with_aliases([("tile", "textures/static")])
        .with_flipbook_destinations([("tile", [2])]);
    let view = pack(&[(
        "textures/flipbook_textures.json",
        br#"[
        {"atlas_tile":"tile","atlas_tile_variant":9,"flipbook_texture":"textures/first"},
        {"atlas_tile":"tile","atlas_tile_variant":4,"flipbook_texture":"textures/second"},
        {"atlas_tile":"tile","atlas_index":1,"flipbook_texture":"textures/invalid"}
    ]"#,
    )]);
    let catalog = super::super::textures::TextureCatalog::new(&view, Some(&keys));
    assert_eq!(
        catalog
            .flipbook("tile", (0, 0))
            .unwrap()
            .texture_path
            .as_ref(),
        "textures/second"
    );
    assert!(catalog.flipbook("tile", (0, 1)).is_none());
    assert!(catalog.flipbook("tile", (1, 0)).is_none());
}

#[test]
fn a_later_all_variant_description_overwrites_an_earlier_exact_destination() {
    let keys = assets::MaterialKeys::from_entries([(1, "tile")])
        .with_aliases([("tile", "textures/static")])
        .with_flipbook_destinations([("tile", [2])]);
    let view = pack(&[(
        "textures/flipbook_textures.json",
        br#"[
        {"atlas_tile":"tile","atlas_tile_variant":1,"flipbook_texture":"textures/exact"},
        {"atlas_tile":"tile","flipbook_texture":"textures/all"}
    ]"#,
    )]);
    let catalog = super::super::textures::TextureCatalog::new(&view, Some(&keys));
    assert_eq!(
        catalog
            .flipbook("tile", (0, 1))
            .unwrap()
            .texture_path
            .as_ref(),
        "textures/all"
    );
}
