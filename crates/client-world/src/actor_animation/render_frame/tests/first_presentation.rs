use super::*;
use crate::actor_store::ActorStore;
use assets::{EntityRenderVisibility, MolangSymbol, MolangSymbolKind};

/// A custom entity with initialized pose inputs and metadata-controlled part visibility.
fn assets() -> Arc<RuntimeEntityAssets> {
    let mut compiled = counting_random_compiled("minecraft:test");
    let mut symbols = compiled.molang_symbols.into_vec();
    let query = 5;
    symbols.insert(
        query as usize,
        MolangSymbol {
            kind: MolangSymbolKind::Query,
            identifier: "query.show_bottom".into(),
        },
    );
    compiled.molang_symbols = symbols.into_boxed_slice();
    let mut ops = compiled.molang_ops.into_vec();
    for op in &mut ops {
        if let MolangOp::LoadVariable(index) | MolangOp::StoreVariable(index) = op {
            *index += 1;
        }
    }
    let mut expressions = compiled.molang_expressions.into_vec();
    let visibility = expressions.len() as u32;
    expressions.push(CompiledMolangExpression {
        first_op: ops.len() as u32,
        op_count: 1,
        max_stack: 1,
    });
    ops.push(MolangOp::LoadQuery(query));
    compiled.render.visibility = vec![EntityRenderVisibility {
        pattern: "rightitem".into(),
        condition: visibility,
    }]
    .into_boxed_slice();
    compiled.render.layers[0].visibility_count = 1;
    compiled.animation_keyframes[0].expressions = [None, Some(1), None];
    compiled.rig_bindings[0].initialize = Some(expressions.len() as u32);
    expressions.push(CompiledMolangExpression {
        first_op: ops.len() as u32,
        op_count: 3,
        max_stack: 1,
    });
    ops.extend([
        MolangOp::Push(EntityGeometryScalar::new(5.0).unwrap()),
        MolangOp::StoreVariable(8),
        MolangOp::Push(EntityGeometryScalar::new(0.0).unwrap()),
    ]);
    compiled.molang_ops = ops.into_boxed_slice();
    compiled.molang_expressions = expressions.into_boxed_slice();
    Arc::new(RuntimeEntityAssets::from_compiled(compiled).unwrap())
}

/// Admits a fresh lifetime with its authoritative visibility flag already installed.
fn spawn(store: &mut ActorStore, sequence: u64, flags: u64) {
    store.apply(
        1,
        sequence,
        protocol::ActorEvent::Spawn(protocol::ActorSpawnEvent {
            dimension: 0,
            unique_id: 17,
            runtime_id: 1,
            kind: protocol::ActorKind::Entity {
                identifier: "minecraft:test".into(),
            },
            position: [0.0; 3],
            velocity: [0.0; 3],
            pitch: 0.0,
            yaw: 0.0,
            head_yaw: 0.0,
            body_yaw: 0.0,
            held_item: Default::default(),
            metadata: Arc::from([protocol::ActorMetadata {
                key: 0,
                value: ActorMetadataValue::Flags(flags),
            }]),
            attributes: Arc::from([]),
            properties: Arc::from([]),
            links: Arc::from([]),
        }),
    );
}

#[test]
fn first_presentation_uses_initialized_pose_and_spawn_metadata_without_a_tick() {
    for pack in [false, true] {
        for show_bottom in [false, true] {
            let assets = assets();
            let mut store = ActorStore::new_with_entity_assets(1, 0, Arc::clone(&assets));
            if pack {
                store.set_pack_entities(Some((assets, vec![0])));
            }
            spawn(&mut store, 1, if show_bottom { 1 << 38 } else { 0 });
            let actor = store.get(1).unwrap().clone();
            store.advance_interpolation_frame(0);
            let rig = store.actor_rig(1).unwrap();
            assert_eq!(
                rig.completed_tick, 0,
                "presentation must not simulate a tick"
            );
            assert_eq!(
                rig.previous, rig.current,
                "never interpolate from bind pose"
            );
            assert_ne!(rig.current, rig.rest, "draw the authored initial pose");
            assert!((rig.current[0].translation_scale[1] - 6.0).abs() < 1e-6);
            assert_eq!(rig.render.len(), 1);
            assert_eq!(rig.render[0].color[0], 6.0);
            assert_eq!(rig.render[0].hidden_bones.is_empty(), show_bottom);
            let pose = rig.current.to_vec();
            let layers = rig.render.to_vec();
            let stats = store.animation_stats();
            store.advance_interpolation_frame(0);
            assert_eq!(store.actor_rig(1).unwrap().current, pose);
            assert_eq!(store.actor_rig(1).unwrap().render, layers);
            assert_eq!(store.animation_stats(), stats, "initialize only once");
            assert_eq!(
                store.get(1).unwrap(),
                &actor,
                "leave actor simulation unchanged"
            );
            store.advance_interpolation_ticks(1);
            assert_eq!(store.actor_rig(1).unwrap().render[0].color[0], 7.0);
        }
    }
}

#[test]
fn a_replaced_lifetime_initializes_its_own_pose_once() {
    let mut store = ActorStore::new_with_entity_assets(1, 0, assets());
    spawn(&mut store, 1, 0);
    store.advance_interpolation_frame(0);
    let first_lifetime = store.actor_rig(1).unwrap().actor;
    store.advance_interpolation_ticks(1);
    spawn(&mut store, 2, 1 << 38);
    store.advance_interpolation_frame(0);
    let rig = store.actor_rig(1).unwrap();
    assert_ne!(rig.actor, first_lifetime);
    assert_eq!(rig.completed_tick, 1);
    assert_eq!(rig.previous, rig.current);
    assert_eq!(rig.render[0].color[0], 6.0);
    assert!(rig.render[0].hidden_bones.is_empty());
    store.apply(
        1,
        3,
        protocol::ActorEvent::Remove(protocol::ActorRemoveEvent {
            dimension: 0,
            unique_id: 17,
        }),
    );
    store.advance_interpolation_frame(0);
    assert!(store.actor_rig(1).is_none());
    spawn(&mut store, 4, 0);
    store.advance_interpolation_frame(0);
    assert_eq!(store.actor_rig(1).unwrap().render[0].color[0], 6.0);
}

/// A fresh rig with the complete zero-time presentation context.
fn animation_fixture() -> (ActorAnimationStore, HashMap<u64, ActorSnapshot>) {
    let mut store = ActorAnimationStore::with_assets(assets());
    let actor = super::super::super::tests::actor_with_metadata(HashMap::new());
    store.insert(1, 0, &actor);
    (store, HashMap::from([(actor.runtime_id, actor)]))
}

/// Supplies a real frame context with no elapsed simulation interval.
fn zero_time_context(_: &ActorSnapshot) -> ActorTickContext {
    ActorTickContext {
        animation_elapsed_ticks: Some(0),
        is_local: true,
        ..Default::default()
    }
}

#[test]
fn first_presentation_preserves_motion_and_clip_time_and_replays_local_view() {
    let (mut store, actors) = animation_fixture();
    let motion = format!("{:?}", store.rigs.values().next().unwrap().motion);
    store.prepare_frame(&actors, None, Some(1), true, zero_time_context);
    let state = store.rigs.values().next().unwrap();
    assert_eq!(format!("{:?}", state.motion), motion);
    assert_eq!(state.history.len(), 1);
    assert!(state.clip_clocks.values().all(|clock| clock.time == 0.0));
    assert_eq!(state.render[0].color[0], 6.0);
    let authored = authored_state(&store);
    store.prepare_frame(&actors, None, Some(1), true, zero_time_context);
    assert_eq!(
        authored_state(&store),
        authored,
        "local refresh replays the first evaluation"
    );
    store.advance_tick(&actors, None, Some(1), true, true, |_| ActorTickContext {
        is_local: true,
        ..Default::default()
    });
    assert_eq!(store.get(1).unwrap().render[0].color[0], 7.0);
    assert!(
        store
            .rigs
            .values()
            .next()
            .unwrap()
            .clip_clocks
            .values()
            .all(|clock| clock.time == world::TICK_DURATION.as_secs_f32())
    );
    store.clear();
    store.insert(2, 0, actors.get(&1).unwrap());
    store.prepare_frame(&actors, None, Some(1), false, zero_time_context);
    assert_eq!(store.get(1).unwrap().render[0].color[0], 6.0);
    assert_eq!(store.get(1).unwrap().completed_tick, 0);
}

#[test]
fn initial_presentation_respects_the_world_budget_and_retries_without_partial_effects() {
    let (mut store, actors) = animation_fixture();
    let authored = authored_state(&store);
    store.schedule.world_budget = 1;
    store.prepare_frame(&actors, None, None, false, zero_time_context);
    assert_eq!(store.completed_tick, 0);
    assert_eq!(store.stats.evaluated_molang_ops, 1);
    assert_eq!(authored_state(&store), authored);
    assert!(store.needs_initial_presentation(&actors, None, None));
    store.schedule.world_budget = MAX_MOLANG_OPS_PER_WORLD_TICK;
    store.prepare_frame(&actors, None, None, false, zero_time_context);
    assert_eq!(store.get(1).unwrap().render[0].color[0], 6.0);
    assert!(!store.needs_initial_presentation(&actors, None, None));
}

#[test]
fn first_presentation_retry_refreshes_snapshot_inputs_without_a_motion_tick() {
    let (mut store, mut actors) = animation_fixture();
    store.schedule.world_budget = 1;
    store.prepare_frame(&actors, None, None, false, zero_time_context);
    let actor = actors.get_mut(&1).unwrap();
    actor.position = [4.0, 5.0, 6.0];
    actor.yaw = 75.0;
    actor.pitch = 30.0;
    store.schedule.world_budget = MAX_MOLANG_OPS_PER_WORLD_TICK;
    store.prepare_frame(&actors, None, None, false, zero_time_context);
    let state = store.rigs.values().next().unwrap();
    let input = state.history.back().unwrap();
    assert_eq!(input.position, actors[&1].position);
    assert_eq!(input.yaw, actors[&1].yaw);
    assert_eq!(input.pitch, actors[&1].pitch);
    assert_eq!(state.history.len(), 1);
    assert_eq!(store.completed_tick, 0);
}

#[test]
fn off_screen_initial_rigs_do_not_build_frame_contexts() {
    let (mut store, mut actors) = animation_fixture();
    let actor = actors.get_mut(&1).unwrap();
    actor.position[0] = 20.0;
    actor.previous_pose.position = actor.position;
    let view = ActorAnimationView {
        planes: [[-1.0, 0.0, 0.0, 10.0]; 6],
        camera: [0.0; 3],
        player_distance: 100.0,
        entity_radius: 72.0,
    };
    let contexts = std::cell::Cell::new(0);
    for _ in 0..2 {
        assert!(!store.needs_initial_presentation(&actors, Some(&view), None));
        store.prepare_frame(&actors, Some(&view), None, false, |actor| {
            contexts.set(contexts.get() + 1);
            zero_time_context(actor)
        });
    }
    assert_eq!(contexts.get(), 0);
    let actor = actors.get_mut(&1).unwrap();
    actor.position = [0.0; 3];
    actor.previous_pose.position = actor.position;
    assert!(store.needs_initial_presentation(&actors, Some(&view), None));
    store.prepare_frame(&actors, Some(&view), None, false, zero_time_context);
    assert_eq!(store.get(1).unwrap().render[0].color[0], 6.0);
}
