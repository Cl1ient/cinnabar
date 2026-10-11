use super::{
    BlockEntityKey, BlockEntityNbt, ChunkStore, CollisionIdSpace, NbtCompound, NbtValue,
    PISTON_DIRECTIONS, PaletteWorld, PickRule, SelectionContext, ShapeBox, ShapeFlags, UNIT, Vec3,
    box_to_aabb, fixture, set_block, set_piston, shapes,
};
use crate::WorldQueryError;
use assets::block_shapes::PickContext;
use assets::block_shapes::{CompiledFlow, state_identity::state_identity};

#[test]
fn moving_pick_identity_includes_a_remote_piston_owner() {
    for space in [CollisionIdSpace::Sequential, CollisionIdSpace::Hashed] {
        let target = shapes(SelectionContext::MovingBlock, vec![], vec![]);
        let (mut store, mut registry, _) = fixture(space, &target);
        let mut carried = shapes(SelectionContext::Static, vec![UNIT], vec![]);
        carried.state_identity = state_identity("minecraft:carried", []);
        registry.register(9, [box_to_aabb(UNIT)]).unwrap();
        registry.set_compiled_selection(9, &carried);
        let mut piston = shapes(SelectionContext::Static, vec![UNIT], vec![]);
        piston.state_identity = [20; 32];
        piston.actor_facing = Some(4);
        registry.register(20, [box_to_aabb(UNIT)]).unwrap();
        registry.set_compiled_selection(20, &piston);
        let owner = [20, 0, 0];
        set_block(&mut store, owner, 20);
        set_piston(&mut store, owner, 0.5);
        set_moving(&mut store, "minecraft:carried", owner, false);
        let origin = Vec3::new(0.75, 0.5, 2.0);
        let direction = Vec3::new(0.0, 0.0, -1.0);
        let hit = PaletteWorld::new(&store, &registry, 0)
            .block_interaction_ray_current(origin, direction, 4.0)
            .unwrap()
            .unwrap();
        let owner_chunk = world::ChunkKey::new(0, 1, 0);
        assert!(
            hit.identity
                .chunks
                .iter()
                .any(|revision| revision.chunk == owner_chunk)
        );
        let historical = store.collision_snapshot();
        set_piston(&mut store, owner, 0.25);
        let current = PaletteWorld::new(&store, &registry, 0).block_interaction_ray(
            origin,
            direction,
            4.0,
            &hit.identity,
        );
        assert!(
            matches!(current, Err(WorldQueryError::StaleCollisionIdentity { chunk }) if chunk == owner_chunk),
            "{current:?}"
        );
        assert!(
            PaletteWorld::new(&historical, &registry, 0)
                .block_interaction_ray(origin, direction, 4.0, &hit.identity)
                .unwrap()
                .is_some()
        );
    }
}

/// Ingests a moving-block snapshot with a typed carried-state reference.
fn set_moving(store: &mut ChunkStore, name: &str, piston: [i32; 3], expanding: bool) {
    let mut block = NbtCompound::default();
    block.insert("name", NbtValue::String(name.into()));
    block.insert("states", NbtValue::Compound(NbtCompound::default()));
    let mut root = NbtCompound::default();
    root.insert("id", NbtValue::String("MovingBlock".into()));
    root.insert("movingBlock", NbtValue::Compound(block));
    root.insert("expanding", NbtValue::Byte(i8::from(expanding)));
    for (key, value) in ["pistonPosX", "pistonPosY", "pistonPosZ"]
        .into_iter()
        .zip(piston)
    {
        root.insert(key, NbtValue::Int(value));
    }
    let actor = BlockEntityNbt::decode_prefix(&root.encode_root().unwrap())
        .unwrap()
        .0;
    store
        .commit_block_entity_update(BlockEntityKey::new(0, 0, 0, 0), actor)
        .unwrap();
}

#[test]
fn moving_blocks_resolve_carried_static_visuals_in_both_id_spaces() {
    let carried = ShapeBox::new([0.25; 3], [0.75; 3]);
    for space in [CollisionIdSpace::Sequential, CollisionIdSpace::Hashed] {
        let target = shapes(SelectionContext::MovingBlock, vec![], vec![]);
        let (mut store, mut registry, id) = fixture(space, &target);
        let mut carried_state = shapes(SelectionContext::Static, vec![UNIT], vec![]);
        carried_state.state_identity = state_identity("minecraft:carried", []);
        carried_state.actor_visual = Some(carried);
        registry.register(9, [box_to_aabb(UNIT)]).unwrap();
        registry.set_compiled_selection(9, &carried_state);
        let physics = registry.physics(id).unwrap();
        assert!(
            PaletteWorld::new(&store, &registry, 0)
                .selection_shapes_at([0; 3], physics)
                .unwrap()
                .is_empty()
        );
        set_moving(&mut store, "minecraft:unknown", [0, -1, 0], true);
        assert!(
            PaletteWorld::new(&store, &registry, 0)
                .selection_shapes_at([0; 3], physics)
                .unwrap()
                .is_empty()
        );
        set_moving(&mut store, "minecraft:carried", [0, -1, 0], true);
        assert_eq!(
            PaletteWorld::new(&store, &registry, 0)
                .selection_shapes_at([0; 3], physics)
                .unwrap()
                .as_ref(),
            [box_to_aabb(carried)]
        );
        for (facing, direction) in PISTON_DIRECTIONS.into_iter().enumerate() {
            let mut owner_state = shapes(SelectionContext::Static, vec![UNIT], vec![]);
            owner_state.state_identity = [20 + facing as u8; 32];
            owner_state.actor_facing = Some(facing as u8);
            registry.register(20, [box_to_aabb(UNIT)]).unwrap();
            registry.set_compiled_selection(20, &owner_state);
            let owner = [-2, 0, 0];
            set_block(&mut store, owner, 20);
            set_piston(&mut store, owner, 0.5);
            for expanding in [false, true] {
                set_moving(&mut store, "minecraft:carried", owner, expanding);
                let offset =
                    direction.map(|value| f64::from(value) * if expanding { -0.5 } else { 0.5 });
                let expected =
                    box_to_aabb(carried).translated(Vec3::new(offset[0], offset[1], offset[2]));
                let actual = PaletteWorld::new(&store, &registry, 0)
                    .selection_shapes_at([0; 3], registry.physics(id).unwrap())
                    .unwrap();
                assert_eq!(actual.as_ref(), [expected]);
            }
        }
    }
}

#[test]
fn scaffold_and_unknown_portal_choose_only_their_compiled_alternates() {
    let base = ShapeBox::new([0.0; 3], [1.0, 0.125, 1.0]);
    let alternate = ShapeBox::new([0.0, 0.875, 0.0], [1.0; 3]);
    for (context, flag, neighbour, alternate_without_support) in [
        (
            SelectionContext::ScaffoldSupport,
            ShapeFlags::FULL_TOP_SUPPORT,
            [0, -1, 0],
            true,
        ),
        (
            SelectionContext::PortalAxis,
            ShapeFlags::PORTAL,
            [1, 0, 0],
            false,
        ),
    ] {
        for space in [CollisionIdSpace::Sequential, CollisionIdSpace::Hashed] {
            let target = shapes(context, vec![base], vec![alternate]);
            let (mut store, mut registry, id) = fixture(space, &target);
            let mut support = shapes(SelectionContext::Static, vec![UNIT], vec![]);
            support.flags = flag;
            support.state_identity = [9; 32];
            registry.register(9, [box_to_aabb(UNIT)]).unwrap();
            registry.set_compiled_selection(9, &support);
            for supported in [false, true] {
                set_block(&mut store, neighbour, if supported { 9 } else { 0 });
                let expected = if supported != alternate_without_support {
                    alternate
                } else {
                    base
                };
                assert_eq!(
                    PaletteWorld::new(&store, &registry, 0)
                        .selection_shapes_at([0; 3], registry.physics(id).unwrap())
                        .unwrap()
                        .as_ref(),
                    [box_to_aabb(expected)]
                );
            }
        }
    }
}

#[test]
fn interaction_ray_enforces_pick_context_instead_of_movement_occupancy() {
    for space in [CollisionIdSpace::Sequential, CollisionIdSpace::Hashed] {
        for (rule, context, depth, expected) in [
            (PickRule::Never, PickContext::default(), None, false),
            (PickRule::Always, PickContext::default(), None, true),
            (
                PickRule::HeldLight,
                PickContext {
                    creative: true,
                    ..Default::default()
                },
                None,
                false,
            ),
            (
                PickRule::HeldLight,
                PickContext {
                    holding_light: true,
                    ..Default::default()
                },
                None,
                true,
            ),
            (PickRule::Creative, PickContext::default(), None, false),
            (
                PickRule::Creative,
                PickContext {
                    creative: true,
                    ..Default::default()
                },
                None,
                true,
            ),
            (
                PickRule::LiquidSource,
                PickContext {
                    liquids: true,
                    ..Default::default()
                },
                Some(0),
                true,
            ),
            (
                PickRule::LiquidSource,
                PickContext {
                    liquids: true,
                    ..Default::default()
                },
                Some(1),
                false,
            ),
        ] {
            let mut target = shapes(SelectionContext::Static, vec![UNIT], vec![]);
            target.pick_rule = rule;
            if let Some(depth) = depth {
                target.flow = Some(CompiledFlow {
                    blocks_motion: false,
                    is_solid: false,
                    blocked_faces: 0,
                    allowed_faces: 0x3f,
                    liquid_depth: Some(depth),
                });
            }
            let (store, mut registry, id) = fixture(space, &target);
            if let Some(depth) = depth {
                registry.set_flow_facts(
                    id,
                    crate::world::FlowBlockFacts {
                        blocks_motion: false,
                        is_solid: false,
                        blocked_faces: 0,
                        allowed_faces: 0x3f,
                        liquid_depth: Some(depth),
                    },
                );
            }
            let view = PaletteWorld::new(&store, &registry, 0).with_pick_context(context);
            let hit = view
                .block_interaction_ray_current(
                    Vec3::new(-2.0, 0.5, 0.5),
                    Vec3::new(1.0, 0.0, 0.0),
                    4.0,
                )
                .unwrap();
            assert_eq!(hit.is_some(), expected, "{space:?} {rule:?}");
            if let Some(hit) = hit {
                assert_eq!(hit.block_pos, [0; 3]);
            }
        }
    }
}
