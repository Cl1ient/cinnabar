//! Original projection of an unresolved survival admission.

use render_model::{UiRenderBatch, UiRenderInput, UiRenderTextureArray, UiRenderVertex, UiScissor};
use std::sync::Arc;

/// Resolves a placement against checked original registry and physics carriers.
fn prediction() -> Option<gameplay::block_use::PredictedPlacement> {
    use gameplay::{
        block_use::{PlacementContext, UseSurroundings, predicted_placement},
        movement::PhysicsCollisionRegistries,
        placement_state::PlacementInput,
    };
    let breg = assets::pinned_block_registry_bytes();
    let protocol = assets::active_content_registry_protocol();
    let records = assets::read_registry_for_protocol(breg, protocol).unwrap();
    let id = |name: &str| {
        records
            .iter()
            .find(|record| record.name.as_ref() == name && record.canonical_state.as_ref() == "{}")
            .unwrap()
            .sequential_id
    };
    let air = id("minecraft:air");
    let stone = id("minecraft:stone");
    let dirt = id("minecraft:dirt");
    let collisions = PhysicsCollisionRegistries::from_assets(
        breg,
        &records,
        include_bytes!("../../../../crates/assets/data/block-physics-v2193.bin"),
        protocol,
    )
    .unwrap();
    let mut store = world::ChunkStore::new();
    let key = world::SubChunkKey::new(0, 0, 0, 0);
    store.mark_sub_chunk_loaded(key).unwrap();
    store
        .update_block(key, world::BlockUpdate::new(8, 8, 8, 0, stone), air)
        .unwrap();
    let around = UseSurroundings {
        clicked_identifier: Some("minecraft:stone".into()),
        clicked_canonical_state: Some("{}".into()),
        held_block_identifier: Some("minecraft:dirt".into()),
        neighbor_identifier: Some("minecraft:air".into()),
        player_box: ([0.0; 3], [0.6, 1.8, 0.6]),
        actor_boxes: Vec::new(),
        sneaking: false,
        placed_boxes: None,
    };
    let context = PlacementContext {
        clicked: [8, 8, 8],
        input: PlacementInput {
            face: 1,
            click_position: [0.5, 1.0, 0.5],
            yaw: 0.0,
            pitch: 0.0,
        },
        surroundings: &around,
        build_height: 0..16,
    };
    let world = sim::PaletteWorld::new(
        &store,
        collisions.registry(assets::NetworkIdMode::Sequential),
        0,
        Default::default(),
    );
    predicted_placement(
        &collisions,
        assets::NetworkIdMode::Sequential,
        dirt,
        &world,
        &context,
    )
}

/// Adds one original diagnostic rectangle to the offline render input.
fn rectangle(
    vertices: &mut Vec<UiRenderVertex>,
    indices: &mut Vec<u32>,
    bounds: [f32; 4],
    color: [u8; 4],
) {
    let first = vertices.len() as u32;
    for position in [
        [bounds[0], bounds[1]],
        [bounds[2], bounds[1]],
        [bounds[2], bounds[3]],
        [bounds[0], bounds[3]],
    ] {
        vertices.push(UiRenderVertex {
            position,
            clip_z: 0.0,
            clip_w: 1.0,
            uv: [0.0; 2],
            color,
            style_flags: 0,
            alpha_cutoff: -1.0,
            model_light: 1.0,
            overlay_color: [0.0; 4],
        });
    }
    indices.extend([first, first + 1, first + 2, first, first + 2, first + 3]);
}

/// Projects the actual placement admission through the offline UI rasterizer.
fn snapshot(predicted: bool) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    rectangle(
        &mut vertices,
        &mut indices,
        [0.0, 0.0, 128.0, 128.0],
        [22, 27, 34, 255],
    );
    rectangle(
        &mut vertices,
        &mut indices,
        [28.0, 96.0, 100.0, 110.0],
        [230, 160, 45, 255],
    );
    if predicted {
        let feet = 96.0;
        rectangle(
            &mut vertices,
            &mut indices,
            [56.0, feet - 28.0, 72.0, feet],
            [80, 190, 230, 255],
        );
    }
    let input = UiRenderInput {
        revision: 1,
        viewport_size: [128; 2],
        safe_area: [0; 4],
        batches: Arc::from([UiRenderBatch::new(
            0,
            UiScissor::new(0, 0, 128, 128),
            0,
            indices.len() as u32,
            0,
        )]),
        vertices: vertices.into(),
        indices: indices.into(),
        textures: Arc::new(
            UiRenderTextureArray::new(
                vec![render_model::UiTexturePage::owned([1; 2], Arc::from([255; 4])).unwrap()],
                1,
            )
            .unwrap(),
        ),
    };
    input.validate().unwrap();
    let image = client_ui::ui_runtime::presentation::forms::snapshot::rasterize(&input);
    assert_eq!(
        image.pixels().any(|pixel| pixel.0 == [80, 190, 230, 255]),
        predicted
    );
    client_ui::ui_runtime::presentation::forms::snapshot::write(
        &input,
        "unadmitted-block-survival",
    );
}

#[test]
fn unadmitted_survival_does_not_create_a_predicted_block() {
    let predicted = prediction();
    snapshot(predicted.is_some());
    assert!(predicted.is_none());
}
