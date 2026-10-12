use super::*;

#[test]
fn ordered_hitboxes_use_the_first_intersection_within_reach() {
    let actor = actor(9, "minecraft:pig", [0.0; 3], None);
    let boxes = [
        ([5.0, 0.0, -2.0], [6.0, 3.0, -1.0]),
        ([-0.5, 0.0, -9.0], [0.5, 3.0, -8.0]),
        ([-0.5, 0.0, -4.0], [0.5, 3.0, -3.0]),
        ([-0.5, 0.0, -2.0], [0.5, 3.0, -1.0]),
    ];
    let hit = pick_actor_by([&actor].into_iter(), |_| boxes, None, EYE, NORTH, 5.0).unwrap();
    assert!((hit.distance - 2.9).abs() < 1e-6, "{}", hit.distance);
}

#[test]
fn advertised_projectile_base_excludes_picking_and_placement_obstruction() {
    let actor = actor_with_base(
        80,
        "fixture:projectile",
        Some("minecraft:arrow"),
        [0.0, 0.0, -2.0],
        HashMap::from([
            (
                BOUNDING_BOX_WIDTH_METADATA_KEY,
                ActorMetadataValue::Float(0.6),
            ),
            (
                BOUNDING_BOX_HEIGHT_METADATA_KEY,
                ActorMetadataValue::Float(2.0),
            ),
        ]),
    );
    assert_eq!(
        pick_actor(std::iter::once(&actor), None, EYE, NORTH, 5.0),
        None
    );
    assert!(!obstructs_placement(&actor));
}
