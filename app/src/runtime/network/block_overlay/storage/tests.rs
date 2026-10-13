use super::super::{MIN_TILE, textures::TextureCatalog};
use super::*;
use resource_pack::LayeredPackView;

/// Admits a resource pack containing only the original synthetic files supplied by a test.
fn view(files: &[(&str, &[u8])]) -> LayeredPackView {
    use std::io::Write;
    let id = "00000000-0000-0000-0000-000000000037";
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

/// Starts with enough occupied layers to reject a 2048px page while small images still fit.
fn builder<'a>(view: &'a LayeredPackView, keys: &assets::MaterialKeys) -> Builder<'a> {
    Builder {
        catalog: TextureCatalog::new(view, Some(keys)),
        geometries: Default::default(),
        overlay: Default::default(),
        sources: (0..3).map(|_| Source::Diagnostic).collect(),
        source_bytes: 0,
        tile_size: MIN_TILE,
        texture_policy: assets::texture::policy::device_policy(),
        max_layers: assets::MAX_TEXTURE_LAYERS,
        textures: Default::default(),
        frame_storage: Default::default(),
        materials: Default::default(),
        visuals: Default::default(),
        gaps: Default::default(),
    }
}

/// Encodes original opaque pixels for decode-work and admission regressions.
fn png(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(width, height, image::Rgba([31, 47, 59, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    bytes.into_inner()
}

#[test]
fn static_decode_and_budget_failures_are_shared_across_selectors_and_aliases() {
    let large = png(2048, 2048);
    let small = png(16, 16);
    for rejected in [large.as_slice(), b"invalid PNG".as_slice(), b"".as_slice()] {
        let view = view(&[
            ("textures/rejected.png", rejected),
            ("textures/small.png", &small),
        ]);
        let keys = assets::MaterialKeys::from_entries([(1, "tile"), (2, "alias"), (3, "small")])
            .with_aliases([
                ("tile", "textures/rejected"),
                ("alias", " ./textures/rejected "),
                ("small", "textures/small"),
            ])
            .with_flipbook_destinations([("tile", vec![1; 16]), ("alias", vec![1; 16])]);
        let mut builder = builder(&view, &keys);
        for index in 0..16 {
            assert!(builder.texture_slot_for("tile", (index, 0)).is_none());
            assert!(builder.texture_slot_for("alias", (index, 0)).is_none());
        }
        assert_eq!(builder.catalog.decode_attempts.get(), 1);
        assert_eq!(builder.sources.len(), 3);
        assert!(builder.texture_slot_for("small", (0, 0)).is_some());
        assert_eq!(builder.catalog.decode_attempts.get(), 2);
        assert_eq!(builder.sources.len(), 4);
    }
}

#[test]
fn failed_static_storage_does_not_hide_an_independent_animation() {
    let large = png(2048, 2048);
    let small = png(16, 32);
    let view = view(&[
        ("textures/rejected.png", &large),
        ("textures/frame.png", &small),
        (
            "textures/flipbook_textures.json",
            br#"[{
            "atlas_tile":"tile","atlas_index":1,"flipbook_texture":"textures/frame"
        }]"#,
        ),
    ]);
    let keys = assets::MaterialKeys::from_entries([(1, "tile")])
        .with_aliases([("tile", "textures/rejected")])
        .with_flipbook_destinations([("tile", vec![1; 2])]);
    let mut builder = builder(&view, &keys);
    assert!(builder.texture_slot_for("tile", (0, 0)).is_none());
    let slot = builder.texture_slot_for("tile", (1, 0)).unwrap();
    assert_ne!(slot.animation, NO_ANIMATION);
    assert_eq!(
        builder.overlay.animations[slot.animation as usize].frame_count,
        2
    );
    assert!(builder.texture_slot_for("tile", (0, 1)).is_none());
    assert_eq!(builder.catalog.decode_attempts.get(), 2);
}

#[test]
fn failed_grid_admission_does_not_poison_another_grid_of_the_same_image() {
    let raster = png(16, 16);
    let view = view(&[
        ("textures/shared.png", &raster),
        (
            "textures/terrain_texture.json",
            br#"{"texture_data":{
            "invalid":{"textures":"textures/shared","quad":5},
            "valid":{"textures":"textures/shared","quad":0}
        }}"#,
        ),
    ]);
    let keys = assets::MaterialKeys::from_entries([(1, "invalid"), (2, "valid")]);
    let mut builder = builder(&view, &keys);
    assert!(builder.texture_slot_for("invalid", (0, 0)).is_none());
    assert!(builder.texture_slot_for("valid", (0, 0)).is_some());
    assert_eq!(builder.catalog.decode_attempts.get(), 2);
}

#[test]
fn animation_source_failures_decode_once_across_wildcard_variants() {
    let large = png(2048, 2048);
    let malformed_strip = png(48, 32);
    let fallback = png(16, 16);
    for rejected in [
        large.as_slice(),
        malformed_strip.as_slice(),
        b"invalid PNG".as_slice(),
        b"".as_slice(),
    ] {
        let view = view(&[
            ("textures/rejected.png", rejected),
            ("textures/fallback.png", &fallback),
            (
                "textures/flipbook_textures.json",
                br#"[{"atlas_tile":"tile","flipbook_texture":"textures/rejected"}]"#,
            ),
        ]);
        let keys = assets::MaterialKeys::from_entries([(1, "tile")])
            .with_aliases([("tile", "textures/fallback")])
            .with_flipbook_destinations([("tile", vec![16])]);
        let mut builder = builder(&view, &keys);
        let first = builder.texture_slot_for("tile", (0, 0)).unwrap();
        for variant in 1..16 {
            let slot = builder.texture_slot_for("tile", (0, variant)).unwrap();
            assert_eq!(slot.layer, first.layer);
            assert_eq!(slot.animation, NO_ANIMATION);
        }
        assert_eq!(builder.catalog.decode_attempts.get(), 2);
        assert_eq!(builder.sources.len(), 4);
    }
}

#[test]
fn destination_timing_failure_does_not_poison_shared_animation_storage() {
    let strip = png(16, 32);
    let fallback = png(16, 16);
    let description = serde_json::to_vec(&(0..3).map(|variant| serde_json::json!({
        "atlas_tile":"tile", "atlas_tile_variant":variant,
        "flipbook_texture":"textures/strip", "ticks_per_frame":if variant == 1 { 2 } else { u32::MAX }
    })).collect::<Vec<_>>()).unwrap();
    let view = view(&[
        ("textures/strip.png", &strip),
        ("textures/fallback.png", &fallback),
        ("textures/flipbook_textures.json", &description),
    ]);
    let keys = assets::MaterialKeys::from_entries([(1, "tile")])
        .with_aliases([("tile", "textures/fallback")])
        .with_flipbook_destinations([("tile", vec![3])]);
    let mut builder = builder(&view, &keys);
    let first = builder.texture_slot_for("tile", (0, 0)).unwrap();
    assert_eq!(first.animation, NO_ANIMATION);
    let animated = builder.texture_slot_for("tile", (0, 1)).unwrap();
    assert_ne!(animated.animation, NO_ANIMATION);
    assert_eq!(
        builder.overlay.animations[animated.animation as usize].frame_count,
        2
    );
    let last = builder.texture_slot_for("tile", (0, 2)).unwrap();
    assert_eq!(last.layer, first.layer);
    assert_eq!(last.animation, NO_ANIMATION);
    assert_eq!(builder.catalog.decode_attempts.get(), 3);
}

#[test]
fn identical_pixels_at_distinct_source_frames_share_a_physical_layer() {
    let raster = png(16, 32);
    let view = view(&[
        ("textures/strip.png", &raster),
        (
            "textures/flipbook_textures.json",
            br#"[{"atlas_tile":"tile","flipbook_texture":"textures/strip","frames":[1,0,1]}]"#,
        ),
    ]);
    let keys = assets::MaterialKeys::from_entries([(1, "tile")])
        .with_aliases([("tile", "textures/strip")]);
    let mut builder = builder(&view, &keys);
    builder.sources.truncate(1);
    builder.max_layers = 2;
    let slot = builder.texture_slot_for("tile", (0, 0)).unwrap();
    assert_eq!(builder.sources.len(), 2);
    let animation = &builder.overlay.animations[slot.animation as usize];
    assert_eq!(animation.frame_count, 3);
    assert!(
        builder
            .overlay
            .animation_frames
            .iter()
            .all(|&frame| frame.layer() == slot.layer)
    );
}
