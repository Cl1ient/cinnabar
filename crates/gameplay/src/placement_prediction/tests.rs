/// Resolves a placement against checked original registry and physics carriers.
fn prediction() -> Option<crate::block_use::PredictedPlacement> {
    use crate::{
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
        include_bytes!("../../../assets/data/block-physics-v2193.bin"),
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

#[test]
fn unadmitted_survival_does_not_create_a_predicted_block() {
    assert!(prediction().is_none());
}
