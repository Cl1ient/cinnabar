use super::*;
use render_model::{
    BeaconModel, UiRenderBatch, UiRenderInput, UiRenderTextureArray, UiRenderVertex, UiScissor,
    UiTexturePage,
};

/// Projects the production beam mesh into the offline UI rasterizer with original white art.
fn beam_snapshot(tint: [f32; 3]) -> UiRenderInput {
    let placement = assets::BlockEntityPlacement {
        name: "textures/entity/beacon_beam".into(),
        x: 0,
        y: 0,
        width: 16,
        height: 16,
    };
    let pixels = vec![255; 16 * 16 * 4];
    let bytes = assets::encode_block_entity_catalog(b"{}", 16, 16, &pixels, &[placement]).unwrap();
    let catalog = RuntimeBlockEntityAssets::decode(&bytes).unwrap();
    let mut scene = BlockEntityScene::default();
    scene.install_assets(&catalog);
    let frame = scene.update(
        SceneClock { ticks: 0. },
        &[],
        &[BlockEntitySubmission {
            block: [0; 3],
            light: 1.0.into(),
            kind: BlockEntityKind::Beacon(BeaconModel { height: 4, tint }),
        }],
    );
    let vertices = frame
        .overlay
        .chunks_exact(3)
        .filter(|triangle| {
            triangle[0].position[2] < 0.5
                && triangle
                    .iter()
                    .all(|v| (v.position[2] - triangle[0].position[2]).abs() < 0.00001)
        })
        .flatten()
        .map(|vertex| UiRenderVertex {
            position: [
                128. + (vertex.position[0] - 0.5) * 80.,
                220. - vertex.position[1] * 40.,
            ],
            clip_z: 0.,
            clip_w: 1.,
            uv: vertex.uv.map(|v| v * 16.),
            color: std::array::from_fn(|axis| {
                let value = vertex.color[axis];
                let value = if axis == 3 {
                    value
                } else if value <= 0.0031308 {
                    value * 12.92
                } else {
                    1.055 * value.powf(1. / 2.4) - 0.055
                };
                (value.clamp(0., 1.) * 255.).round() as u8
            }),
            style_flags: 0,
            alpha_cutoff: -1.,
            model_light: 1.,
            overlay_color: [0.; 4],
        })
        .collect::<Vec<_>>();
    assert!(!vertices.is_empty());
    let indices = (0..vertices.len() as u32).collect::<Vec<_>>();
    let batch = UiRenderBatch::new(
        0,
        UiScissor::new(0, 0, 256, 256),
        0,
        indices.len() as u32,
        render_model::UI_BLEND_ALPHA,
    );
    let textures = UiRenderTextureArray::new(
        vec![UiTexturePage::owned([16, 16], pixels.into()).unwrap()],
        1,
    )
    .unwrap();
    UiRenderInput {
        revision: 1,
        viewport_size: [256, 256],
        safe_area: [0; 4],
        vertices: vertices.into(),
        indices: indices.into(),
        batches: vec![batch].into(),
        textures: Arc::new(textures),
    }
}

#[test]
fn hardened_glass_colours_the_emitted_beacon_mesh() {
    let id =
        protocol::block_state_network_hash("minecraft:hard_red_stained_glass", std::iter::empty());
    let mut store = world::ChunkStore::new();
    store
        .update_block(
            world::SubChunkKey::new(0, 0, 0, 0),
            world::BlockUpdate::new(0, 2, 0, 0, id),
            0,
        )
        .unwrap();
    let mut runtime = BlockEntityRuntime::new();
    let Some(BlockEntityKind::Beacon(model)) = beacon::kind(
        &mut runtime,
        &store,
        0,
        assets::NetworkIdMode::Hashed,
        [0; 3],
    ) else {
        panic!("the fixture emits a beacon");
    };
    let frame = beam_snapshot(model.tint);
    frame.validate().unwrap();
    client_ui::test_support::write_snapshot(&frame, "hardened-glass-beacon");
    assert_eq!(model.tint, assets::banner::color_linear(1));
    let raster = client_ui::test_support::rasterize(&frame);
    assert!(
        raster
            .pixels()
            .any(|pixel| u16::from(pixel.0[0]) > u16::from(pixel.0[1]) * 2)
    );
}
