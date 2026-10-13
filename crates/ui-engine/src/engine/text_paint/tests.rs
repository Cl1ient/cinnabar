use super::*;

use assets::{FontPixels, FontRendering, FontTexturePage, GlyphMetrics, encode_font_catalog};
use sha2::{Digest, Sha256};
use ui::DpiScale;

use crate::test_support::fixture_font;

/// An em-tall rectangle isolates native face sizing from differences in glyph artwork.
fn native_font(em: u16) -> RuntimeFontCatalog {
    let pixels: Arc<[u8]> = vec![255; usize::from(em) * usize::from(em)].into();
    let page = FontTexturePage {
        source_path: "font/native-label-fixture.png".into(),
        source_bytes: pixels.len() as u32,
        source_sha256: [1; 32],
        pixels_sha256: Sha256::digest(&pixels).into(),
        width: u32::from(em),
        height: u32::from(em),
        pixels: FontPixels::Coverage(pixels),
    };
    let face = assets::carriers::FONT_TEN.font_face.unwrap();
    let source = face.line_metrics_at(u32::from(em));
    let glyphs = ['0', '\u{fffd}'].map(|codepoint| GlyphMetrics {
        codepoint,
        page: 0,
        uv: [0, 0, em, em],
        bearing: [0, -(source.ascent_64 as i16 / 64)],
        advance_64: em as i16 * 64,
    });
    let manifest = [2; 32];
    let bytes = encode_font_catalog(manifest, &glyphs, &[page]).unwrap();
    RuntimeFontCatalog::decode(&bytes, manifest)
        .unwrap()
        .with_rendering(FontRendering::NativeSdf)
        .with_line_metrics(source)
        .unwrap()
}

/// Measures the actual glyph height and distance between the two painted lines.
fn line_geometry(layout: &ui::TextLayout) -> [i32; 2] {
    assert_eq!(layout.line_count(), 2);
    assert!(!layout.ellipsized());
    let [first, second] = layout.glyphs() else {
        panic!("the label must draw one glyph on each line");
    };
    assert_eq!([first.line, second.line], [0, 1]);
    assert!(first.bounds_64[3] <= second.bounds_64[1], "lines overlap");
    [
        first.bounds_64[3] - first.bounds_64[1],
        second.bounds_64[1] - first.bounds_64[1],
    ]
}

#[test]
fn minecraftten_alias_normalizes_native_glyph_height_and_line_pitch() {
    let face = assets::carriers::FONT_TEN.font_face.unwrap();
    let alias = face.json_ui_alias.unwrap();
    let default = fixture_font();
    // Different raster sizes ensure sizing follows the catalog rather than a face-name table.
    for em in [26, 52] {
        let font = default
            .with_named_font(face.name, &native_font(em))
            .unwrap();
        assert_eq!(font.font_named(alias).rendering(), FontRendering::NativeSdf);
        for dpi in [1.0, 1.5] {
            for factor in [0.5, 1.0, 2.0, 13.0] {
                let metrics =
                    TextMetrics::for_viewport([1280, 720], DpiScale::new(dpi).unwrap(), Some(2));
                let px = metrics.scale.get() * FONT_DESIGN_PIXEL_TEXELS as f32;
                let options = TextOptions {
                    font_type: Some(alias.into()),
                    ..Default::default()
                };
                let dest = [0.0, 0.0, 800.0, 800.0];
                let mut layouts = TextLayoutCache::new(4, 1 << 20);
                let bitmap = layouts
                    .layout(painted_label_request(
                        metrics,
                        "0\n0",
                        dest,
                        &font,
                        factor,
                        &TextOptions::default(),
                        TextAlign::Left,
                        px,
                    ))
                    .unwrap();
                let native = layouts
                    .layout(painted_label_request(
                        metrics,
                        "0\n0",
                        dest,
                        &font,
                        factor,
                        &options,
                        TextAlign::Left,
                        px,
                    ))
                    .unwrap();
                let bitmap_geometry = line_geometry(&bitmap);
                let native_geometry = line_geometry(&native);
                if dpi == 1.0 && factor == 13.0 {
                    assert_eq!(bitmap_geometry, native_geometry);
                }
                for (bitmap, native) in bitmap_geometry.into_iter().zip(native_geometry) {
                    // Layout rounds scale to 1/1024 and glyph edges to 1/64 pixels.
                    assert!(
                        (bitmap - native).abs() <= 3,
                        "em {em}, DPI {dpi}, factor {factor}: {bitmap} != {native}"
                    );
                }
                let shape = LabelShape {
                    scale: f64::from(factor),
                    line_padding: 0.0,
                    hide_hyphen: false,
                };
                let measured = Measure {
                    layouts: &RefCell::new(&mut layouts),
                    font: &font,
                    metrics,
                    px,
                    translate: &|_| None,
                }
                .named_label("0\n0", alias, None, shape);
                assert_eq!(
                    measured,
                    native
                        .size_64()
                        .map(|size| f64::from(size) / 64.0 / f64::from(px))
                );
            }
        }
    }
}

#[test]
fn absent_alias_keeps_default_bitmap_label_metrics() {
    let font = fixture_font();
    let metrics = TextMetrics::for_viewport([1280, 720], DpiScale::new(1.0).unwrap(), Some(2));
    let options = TextOptions {
        font_type: assets::carriers::FONT_TEN
            .font_face
            .unwrap()
            .json_ui_alias
            .map(str::to_owned),
        ..Default::default()
    };
    let request = |options| {
        painted_label_request(
            metrics,
            "0\n0",
            [0.0, 0.0, 400.0, 400.0],
            &font,
            1.0,
            options,
            TextAlign::Left,
            metrics.scale.get() * FONT_DESIGN_PIXEL_TEXELS as f32,
        )
    };
    let mut layouts = TextLayoutCache::new(4, 1 << 20);
    let bitmap = layouts.layout(request(&TextOptions::default())).unwrap();
    let fallback = layouts.layout(request(&options)).unwrap();
    assert_eq!(bitmap.key(), fallback.key());
    assert_eq!(bitmap.glyphs(), fallback.glyphs());
    assert_eq!(
        bitmap.key().line_height_64,
        TEXT_PITCH * FONT_DESIGN_PIXEL_TEXELS * 64
    );
    assert_eq!(bitmap.key().baseline_64, metrics.baseline_64);
}
