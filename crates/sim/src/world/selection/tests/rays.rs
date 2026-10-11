use super::*;
use assets::block_shapes::{PickContext, state_identity::state_identity};

#[test]
fn camera_visibility_respects_compiled_pick_context() {
    for space in [CollisionIdSpace::Sequential, CollisionIdSpace::Hashed] {
        for (rule, context, depth, admitted) in [
            (PickRule::Never, PickContext::default(), None, false),
            (PickRule::Always, PickContext::default(), None, true),
            (PickRule::Liquid, PickContext::default(), Some(0), false),
            (
                PickRule::LiquidSource,
                PickContext::default(),
                Some(0),
                false,
            ),
            (PickRule::HeldLight, PickContext::default(), None, false),
            (PickRule::Creative, PickContext::default(), None, false),
            (
                PickRule::HeldLight,
                PickContext {
                    holding_light: true,
                    ..Default::default()
                },
                None,
                true,
            ),
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
                PickRule::Liquid,
                PickContext {
                    liquids: true,
                    ..Default::default()
                },
                Some(1),
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
            let (mut store, mut registry, id) = fixture(space, &target);
            if let Some(depth) = depth {
                registry.set_flow_facts(
                    id,
                    crate::FlowBlockFacts {
                        blocks_motion: false,
                        is_solid: false,
                        blocked_faces: 0,
                        allowed_faces: 63,
                        liquid_depth: Some(depth),
                    },
                );
            }
            let mut behind = shapes(SelectionContext::Static, vec![UNIT], vec![]);
            behind.state_identity = [9; 32];
            registry.register(9, [box_to_aabb(UNIT)]).unwrap();
            registry.set_compiled_selection(9, &behind);
            set_block(&mut store, [1, 0, 0], 9);
            let world = PaletteWorld::new(&store, &registry, 0).with_pick_context(context);
            let origin = Vec3::new(-2.0, 0.5, 0.5);
            let direction = Vec3::new(1.0, 0.0, 0.0);
            let authority = world
                .block_interaction_ray_current(origin, direction, 4.0)
                .unwrap()
                .unwrap();
            assert_eq!(
                authority.block_pos,
                if admitted { [0; 3] } else { [1, 0, 0] }
            );
            let camera = world
                .camera_visibility_ray(origin, direction, 4.0)
                .unwrap()
                .unwrap();
            assert_eq!(
                camera.block_pos, authority.block_pos,
                "{space:?} {rule:?} {context:?}"
            );
            assert_eq!(camera.distance, authority.distance);
            assert_eq!(camera.face, authority.face);
            assert_eq!(camera.runtime_id, authority.runtime_id);
        }
    }
}

#[test]
fn camera_visibility_reaches_translated_actor_selection() {
    check_moving_rays(|world, origin, direction| {
        world
            .camera_visibility_ray(origin, direction, 4.0)
            .unwrap()
            .map(|hit| hit.block_pos)
    });
}

#[test]
fn outline_aim_reaches_translated_actor_selection() {
    check_moving_rays(|world, origin, direction| {
        world
            .camera_aim_ray(origin, origin + direction * 4.0, 8, true, false)
            .unwrap()
            .map(|hit| hit.block_pos)
    });
}

#[test]
fn camera_visibility_reaches_selection_only_extents() {
    let extent = ShapeBox::new([0.0; 3], [1.5, 1.0, 1.0]);
    for space in [CollisionIdSpace::Sequential, CollisionIdSpace::Hashed] {
        let target = shapes(SelectionContext::Static, vec![extent], vec![]);
        let (store, registry, _) = fixture(space, &target);
        let world = PaletteWorld::new(&store, &registry, 0);
        let origin = Vec3::new(1.25, 0.5, 2.0);
        let direction = Vec3::new(0.0, 0.0, -1.0);
        let authority = world
            .block_interaction_ray_current(origin, direction, 4.0)
            .unwrap()
            .unwrap();
        let camera = world
            .camera_visibility_ray(origin, direction, 4.0)
            .unwrap()
            .unwrap();
        assert_eq!(camera.block_pos, authority.block_pos);
        assert_eq!(camera.distance, authority.distance);
        let aim = world
            .camera_aim_ray(origin, origin + direction * 4.0, 8, true, false)
            .unwrap()
            .unwrap();
        assert_eq!(aim.block_pos, authority.block_pos);
    }
}

/// Checks displaced carried boxes in both ID spaces, all axes and both motion directions.
fn check_moving_rays(mut inspect: impl FnMut(&PaletteWorld<'_>, Vec3, Vec3) -> Option<[i32; 3]>) {
    for space in [CollisionIdSpace::Sequential, CollisionIdSpace::Hashed] {
        for (facing, direction) in PISTON_DIRECTIONS.into_iter().enumerate() {
            for expanding in [false, true] {
                let target = shapes(SelectionContext::MovingBlock, vec![], vec![]);
                let (mut store, mut registry, _) = fixture(space, &target);
                let mut carried = shapes(SelectionContext::Static, vec![UNIT], vec![]);
                carried.state_identity = state_identity("minecraft:carried", []);
                registry.register(9, []).unwrap();
                registry.set_compiled_selection(9, &carried);
                let mut piston = shapes(SelectionContext::Static, vec![UNIT], vec![]);
                piston.state_identity = [20; 32];
                piston.actor_facing = Some(facing as u8);
                registry.register(20, []).unwrap();
                registry.set_compiled_selection(20, &piston);
                let owner = [-2, 0, 0];
                set_block(&mut store, owner, 20);
                set_piston(&mut store, owner, 0.5);
                set_moving(&mut store, "minecraft:carried", owner, expanding);
                let axis = direction.iter().position(|value| *value != 0).unwrap();
                let ray_axis = (axis + 1) % 3;
                let mut origin = Vec3::new(0.5, 0.5, 0.5);
                let sign = direction[axis] * if expanding { -1 } else { 1 };
                origin[axis] = if sign > 0 { 1.25 } else { -0.25 };
                origin[ray_axis] = 2.0;
                let mut ray = Vec3::ZERO;
                ray[ray_axis] = -1.0;
                let world = PaletteWorld::new(&store, &registry, 0);
                assert_eq!(
                    world
                        .block_interaction_ray_current(origin, ray, 4.0)
                        .unwrap()
                        .unwrap()
                        .block_pos,
                    [0; 3]
                );
                assert_eq!(
                    inspect(&world, origin, ray),
                    Some([0; 3]),
                    "{space:?} {facing} {expanding}"
                );
            }
        }
    }
}
