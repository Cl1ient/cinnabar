use super::super::super::server_pack::ServerAtlas;
use super::super::{EngineInputs, EngineOutput, FormEngine, ScreenArt};
use crate::test_support::{fixture_font, mini_carrier};
use crate::ui_runtime::presentation::TextMetrics;
use json_ui::{Catalog, Context, DataSource, ViewState};
use ui::{DpiScale, SafeArea, TextLayoutCache, UiVisual};

/// Paints an authored banner control against independent pack artwork.
fn render() -> (Vec<ui::UiNode>, crate::ui_runtime::forms::EngineFrame) {
    let mut catalog = Catalog::default();
    catalog.overlay_text(
        "ui/loom_test.json",
        &serde_json::json!({
            "namespace":"loom", "loom_screen":{"type":"panel", "controls":[
                {"banner":{"type":"custom", "renderer":"banner_pattern_renderer", "size":[100,200],
                    "anchor_from":"top_left", "anchor_to":"top_left", "offset":[110,20],
                    "property_bag":{"#banner_patterns":"0_34_1", "#banner_colors":"15_1_4"}}},
                {"unknown":{"type":"custom", "renderer":"camera_renderer", "size":[20,40]}}
            ]}
        })
        .to_string(),
    );
    let mut engine = FormEngine::new(mini_carrier(), catalog, 2);
    let files = ["base", "stripe_right", "border"].map(|name| {
        let mut pixels = image::RgbaImage::new(64, 64);
        for y in 1..41 {
            for x in 1..21 {
                if name == "base"
                    || (name == "stripe_right" && x >= 11)
                    || (name == "border" && (x <= 3 || x >= 18 || y <= 3 || y >= 38))
                {
                    pixels.put_pixel(x, y, image::Rgba([255; 4]));
                }
            }
        }
        let mut png = Vec::new();
        pixels
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        (format!("textures/entity/banner/banner_{name}.png"), png)
    });
    engine.set_server_atlas(ServerAtlas::new(&files, None, 1), 3);
    let font = fixture_font();
    let mut layouts = TextLayoutCache::new(256, 1 << 20);
    let mut nodes = Vec::new();
    let mut next = 1;
    let metrics = TextMetrics::for_viewport([320, 240], DpiScale::new(1.0).unwrap(), None);
    let frame = engine
        .render_screen(
            "loom.loom_screen",
            &DataSource::default(),
            &Context::desktop(),
            &ViewState::default(),
            ScreenArt::default(),
            EngineInputs {
                layouts: &mut layouts,
                font: &font,
                metrics,
                solid_page: 0,
                safe_area: SafeArea::ZERO,
                content: [320.0, 240.0],
                translate: &|_| None,
                language: [0; 3],
            },
            EngineOutput {
                nodes: &mut nodes,
                next: &mut next,
                overlay: &[],
            },
        )
        .unwrap()
        .unwrap();
    if std::env::var_os("CINNABAR_FORM_SNAPSHOT_DIR").is_some() {
        use crate::ui_runtime::render_adapter::{UiRenderViewport, adapt_ui_draw_list};
        let blank =
            render_model::UiTexturePage::owned([512, 512], vec![0; 512 * 512 * 4].into()).unwrap();
        let mut pages = vec![blank; 3];
        pages.extend_from_slice(engine.textures.lock().images());
        let static_pages = pages.len();
        let textures = render_model::UiRenderTextureArray::new(pages, static_pages).unwrap();
        let viewport = ui::UiRect::new(
            ui::UiPoint::new(0.0, 0.0).unwrap(),
            ui::UiPoint::new(320.0, 240.0).unwrap(),
        )
        .unwrap();
        let draw = ui::RetainedDraw::build(
            &nodes,
            viewport,
            ui::UiScale::default(),
            SafeArea::ZERO,
            ui::TextEffects::default(),
        )
        .unwrap();
        let input = adapt_ui_draw_list(
            draw.draw_list(),
            std::sync::Arc::new(textures),
            UiRenderViewport {
                physical_size: [320, 240],
                dpi_scale: DpiScale::new(1.0).unwrap(),
                safe_area: SafeArea::ZERO,
            },
        )
        .unwrap();
        super::super::super::snapshot::write(&input, "loom-banner");
    }
    (nodes, frame)
}

#[test]
fn banner_custom_control_paints_ordered_front_layers_and_reports_unsupported_renderers() {
    let (nodes, frame) = render();
    let colors: Vec<_> = nodes
        .iter()
        .filter_map(|node| match node.visual() {
            UiVisual::Sprite { color, uv, .. } => {
                assert_eq!(uv[2] - uv[0], 20);
                assert_eq!(uv[3] - uv[1], 40);
                Some(*color)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        colors,
        [[240, 240, 240, 255], [176, 46, 38, 255], [60, 68, 170, 255]]
    );
    assert_eq!(frame.unsupported_renderers, ["camera_renderer"]);
}
