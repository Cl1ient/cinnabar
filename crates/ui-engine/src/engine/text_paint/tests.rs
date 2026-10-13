use super::*;
use assets::{FontPixels, FontRendering, FontTexturePage, GlyphMetrics};
use sha2::Digest;
use ui::DpiScale;

/// Builds a proportional outline face at a selectable raster resolution.
fn title_font(em: u32) -> RuntimeFontCatalog {
    let face = assets::carriers::FONT_TEN.font_face.unwrap();
    let pixels = vec![255; (em * em * 4) as usize].into_boxed_slice();
    let page = FontTexturePage {
        source_path: "font/title.png".into(),
        source_bytes: pixels.len() as u32,
        source_sha256: [1; 32],
        pixels_sha256: sha2::Sha256::digest(&pixels).into(),
        width: em,
        height: em,
        pixels: FontPixels::Rgba8(pixels.into()),
    };
    let glyphs = ['0', '\u{fffd}'].map(|codepoint| GlyphMetrics {
        codepoint,
        page: 0,
        uv: [0, 0, (em / 2) as u16, em as u16],
        bearing: [0, -(em as i16)],
        advance_64: (em * 64 / 2) as i16,
    });
    let bytes = assets::encode_font_catalog([7; 32], &glyphs, &[page]).unwrap();
    let font = RuntimeFontCatalog::decode(&bytes, [7; 32])
        .unwrap()
        .with_line_metrics(face.line_metrics_at(em))
        .unwrap()
        .with_rendering(FontRendering::NativeSdf);
    crate::test_support::fixture_font()
        .with_named_font(face.name, &font)
        .unwrap()
}

#[test]
fn pack_title_size_is_independent_of_outline_raster_resolution() {
    for (physical, dpi) in [([1280, 720], 1.0), ([1920, 1080], 1.5)] {
        compare_title_resolutions(physical, dpi);
    }
}

/// Checks painting, measurement and caret advances for one display configuration.
fn compare_title_resolutions(physical: [u32; 2], dpi: f32) {
    let nominal = assets::carriers::FONT_TEN
        .font_face
        .unwrap()
        .raster_em_pixels();
    let metrics = TextMetrics::for_viewport(physical, DpiScale::new(dpi).unwrap(), None);
    let px = metrics.gui_scale / dpi;
    let options = TextOptions {
        font_type: Some("MinecraftTen".into()),
        ..TextOptions::default()
    };
    let mut extents = Vec::new();
    for em in [nominal, 52] {
        let font = title_font(em);
        let mut layouts = TextLayoutCache::new(32, 1 << 20);
        let request = painted_label_request(
            metrics,
            "0000",
            [0.0, 0.0, 200.0, 80.0],
            &font,
            1.3,
            &options,
            TextAlign::Left,
            px,
        );
        let layout = layouts.layout(request).unwrap();
        let painted = layout.size_64();
        let first = layout.glyphs().first().unwrap().bounds_64;
        assert!(
            first[1] >= -64,
            "title ink must begin inside its label: {first:?}"
        );
        let layouts = RefCell::new(&mut layouts);
        let measure = Measure {
            layouts: &layouts,
            font: &font,
            metrics,
            px,
            translate: &|_| None,
        };
        let measured = measure.named_label(
            "0000",
            "MinecraftTen",
            None,
            LabelShape {
                scale: 1.3,
                line_padding: 0.0,
                hide_hyphen: false,
            },
        );
        for axis in 0..2 {
            assert!(
                (measured[axis] * f64::from(px) * 64.0 - f64::from(painted[axis])).abs() <= 1.0
            );
        }
        drop(layouts);
        let spot = crate::caret::TextSpot {
            field: (),
            bounds: ui::UiRect::from_physical([0.0, 0.0], [200.0, 80.0], metrics.dpi_scale)
                .unwrap(),
            left: 0.0,
            factor: 1.3,
            font: Some("MinecraftTen".into()),
            letter_spacing_64: 0,
            metrics,
        };
        let mut caret_layouts = TextLayoutCache::new(32, 1 << 20);
        assert!(
            (spot.width(&mut caret_layouts, &font, "0000") * 64.0 - painted[0] as f32).abs() <= 1.0
        );
        extents.push(painted);
    }
    for axis in 0..2 {
        assert!(
            (i64::from(extents[0][axis]) - i64::from(extents[1][axis])).abs() <= 4,
            "same design at different raster resolutions ({physical:?}, DPI {dpi}): {extents:?}"
        );
    }
}

#[test]
fn explicit_named_faces_and_missing_substitutes_keep_their_geometry() {
    let font = title_font(52);
    let face = assets::carriers::FONT_TEN.font_face.unwrap();
    assert_eq!(font.font_named_for_json_ui(face.name).1, None);
    let custom = font.with_named_font("MinecraftTen", &font).unwrap();
    assert_eq!(custom.font_named_for_json_ui("MinecraftTen").1, None);
    let default = crate::test_support::fixture_font();
    for name in ["MinecraftTen", "default", "unknown"] {
        let (selected, factor) = default.font_named_for_json_ui(name);
        assert!(std::ptr::eq(selected, default.as_ref()));
        assert_eq!(factor, None);
    }
}
