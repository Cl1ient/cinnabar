//! Spawner contents use the actor evaluator, artwork pages and draw publication.
use crate::runtime::world::ClientWorld;
use bevy::prelude::{Mat4, Real, Res, ResMut, Resource, Time, Vec3};
use client_presentation::{
    actor_clock::ActorFrameClock,
    actor_publication::{ActorFrameState, PreparedActorPublication},
    local_player::LocalViewPose,
    presentation::{
        actors::{ActorPresentationBatch, PoseConversions, entity_rig_presentation_cached},
        entity_layers::{LayerPoseCache, apply_render_layers_cached},
    },
};
use client_world::DisplayActorStore;
use render::{ActorArtworkPages, matrix_rows};
use render_model::spawner::{SpawnerDisplayClock, spawner_actor_transform};
use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};
use world::{BlockEntityKey, BlockEntityNbt, ChunkKey, SUB_CHUNK_SIDE};

use super::system::SCAN_RADIUS_BLOCKS as DISPLAY_SCAN_RADIUS;
const DEFAULT_ACTIVATION_RANGE: f32 = 16.0;
const DEFAULT_SPAWN_DELAY: i32 = 0;

type EntityIndex = std::collections::BTreeMap<BlockEntityKey, Arc<BlockEntityNbt>>;
struct Column {
    source: Arc<EntityIndex>,
    displays: Vec<Request>,
}
struct Request {
    position: [i32; 3],
    identifier: Arc<str>,
    delay: i32,
    activation_range: f32,
    nbt: Arc<BlockEntityNbt>,
}
struct Clock {
    source: Arc<BlockEntityNbt>,
    spin: SpawnerDisplayClock,
}
struct Pool {
    source: Arc<assets::RuntimeEntityAssets>,
    session: (u64, i32),
    pack: Option<Arc<assets::SessionEntityPack>>,
    actors: DisplayActorStore,
    clock: ActorFrameClock,
    columns: HashMap<ChunkKey, Column>,
    spins: HashMap<[i32; 3], Clock>,
    poses: PoseConversions,
    layers: LayerPoseCache,
}

#[derive(Resource, Default)]
pub(crate) struct SpawnerDisplayPublisher(Option<Pool>);

/// Parses configured displays once for an immutable column NBT index.
fn column(source: Arc<EntityIndex>) -> Column {
    let displays = source
        .iter()
        .filter_map(|(key, nbt)| {
            let root = nbt.parse()?;
            if root.string("id") != Some("MobSpawner") {
                return None;
            }
            let identifier = root.string("EntityIdentifier")?;
            if identifier.is_empty() {
                return None;
            }
            Some(Request {
                position: key.position(),
                identifier: Arc::from(identifier),
                delay: root
                    .integer("Delay")
                    .and_then(|value| i32::try_from(value).ok())
                    .unwrap_or(DEFAULT_SPAWN_DELAY)
                    .max(-1),
                activation_range: root
                    .integer("RequiredPlayerRange")
                    .map_or(DEFAULT_ACTIVATION_RANGE, |value| value.clamp(0, 128) as f32),
                nbt: Arc::clone(nbt),
            })
        })
        .take(client_world::MAX_DISPLAY_ACTORS)
        .collect();
    Column { source, displays }
}

/// Publishes client-owned display actors into this frame's normal actor draw batch.
pub(crate) fn prepare_spawner_actors(
    world: Res<ClientWorld>,
    view: Res<LocalViewPose>,
    time: Res<Time<Real>>,
    collisions: Res<crate::movement::PhysicsCollisionRegistries>,
    artwork: Res<ActorArtworkPages>,
    state: Res<ActorFrameState>,
    mut pending: ResMut<PreparedActorPublication>,
    mut publisher: ResMut<SpawnerDisplayPublisher>,
) {
    let (Some(stream), Some(assets)) = (world.stream.as_ref(), world.entity_assets.as_ref()) else {
        publisher.0 = None;
        return;
    };
    let session = (
        stream.authority().actor_session_id(),
        stream.current_dimension(),
    );
    if publisher
        .0
        .as_ref()
        .is_none_or(|pool| pool.session != session || !Arc::ptr_eq(&pool.source, assets))
    {
        publisher.0 = Some(Pool {
            source: Arc::clone(assets),
            session,
            pack: None,
            actors: DisplayActorStore::new(session.1, Arc::clone(assets)),
            clock: Default::default(),
            columns: Default::default(),
            spins: Default::default(),
            poses: Default::default(),
            layers: Default::default(),
        });
    }
    let pool = publisher.0.as_mut().unwrap();
    let pack_changed = match (&pool.pack, &world.pack_entities) {
        (None, None) => false,
        (Some(old), Some(new)) => !Arc::ptr_eq(old, new),
        _ => true,
    };
    if pack_changed {
        pool.actors
            .set_pack(world.pack_entities.as_ref().map(|pack| {
                (
                    Arc::clone(&pack.assets),
                    pack.bindings
                        .iter()
                        .map(|binding| binding.geometry_candidate)
                        .collect(),
                )
            }));
        pool.pack = world.pack_entities.clone();
        pool.poses = Default::default();
        pool.layers = Default::default();
    }
    let step = pool.clock.advance(time.delta());
    let eye = view.eye_translation();
    let authority = stream.authority();
    let local = authority.actor(authority.local_player_runtime_id());
    let mut players: Vec<Vec3> = authority
        .render_players()
        .into_iter()
        .filter(|(actor, _)| authority.camera_aim_assist_eligible(actor) == Some(true))
        .filter_map(|(actor, _)| {
            actor
                .interpolated_position(step.partial_tick)
                .map(Vec3::from_array)
        })
        .collect();
    if local.is_some_and(|actor| authority.camera_aim_assist_eligible(actor) == Some(true)) {
        players.push(view.feet_translation());
    }
    let range = |center: f32| {
        ((center - DISPLAY_SCAN_RADIUS) / SUB_CHUNK_SIDE as f32).floor() as i32
            ..=((center + DISPLAY_SCAN_RADIUS) / SUB_CHUNK_SIDE as f32).floor() as i32
    };
    let side = SUB_CHUNK_SIDE as i32;
    let mut columns = BTreeSet::new();
    let mut visible = BTreeSet::new();
    let mut displays = Vec::new();
    for x in range(eye.x) {
        for z in range(eye.z) {
            let key = ChunkKey::new(session.1, x, z);
            let Some(chunk) = stream.collision_store().chunk(key) else {
                continue;
            };
            columns.insert(key);
            let source = chunk.shared_block_entities();
            if pool
                .columns
                .get(&key)
                .is_none_or(|old| !Arc::ptr_eq(&old.source, &source))
            {
                pool.columns.insert(key, column(source));
            }
            for request in &pool.columns[&key].displays {
                let [x, y, z] = request.position;
                let backing = chunk
                    .sub_chunk(y.div_euclid(side))
                    .and_then(|part| {
                        part.runtime_id(
                            0,
                            x.rem_euclid(side) as u8,
                            y.rem_euclid(side) as u8,
                            z.rem_euclid(side) as u8,
                        )
                    })
                    .and_then(|id| collisions.block_identifier(stream.network_id_mode(), id));
                if backing != Some("minecraft:mob_spawner") {
                    continue;
                }
                let center =
                    Vec3::from_array(request.position.map(|value| value as f32)) + Vec3::splat(0.5);
                if center.distance_squared(eye) > DISPLAY_SCAN_RADIUS * DISPLAY_SCAN_RADIUS {
                    continue;
                }
                if visible.len() >= client_world::MAX_DISPLAY_ACTORS {
                    break;
                }
                visible.insert(request.position);
                let clock = pool.spins.entry(request.position).or_insert_with(|| Clock {
                    source: Arc::clone(&request.nbt),
                    spin: SpawnerDisplayClock {
                        delay: request.delay,
                        ..Default::default()
                    },
                });
                if !Arc::ptr_eq(&clock.source, &request.nbt) {
                    clock.source = Arc::clone(&request.nbt);
                    clock.spin.delay = request.delay;
                }
                clock.spin.advance(
                    step.ticks,
                    players.iter().any(|player| {
                        center.distance_squared(*player)
                            <= request.activation_range * request.activation_range
                    }),
                );
                if let Some(runtime) = pool.actors.resolve(request.position, &request.identifier) {
                    displays.push((runtime, request.position, clock.spin));
                }
            }
        }
    }
    pool.columns.retain(|key, _| columns.contains(key));
    pool.spins.retain(|position, _| visible.contains(position));
    pool.actors.retain(&visible);
    pool.actors.advance(step.ticks);
    let artwork = state.actor_artwork(&artwork);
    pending.extend_entities(display_batch(
        pool,
        stream,
        artwork,
        step.partial_tick,
        displays,
    ));
}

/// Converts and places display poses, then resolves their lighting and controller layers.
fn display_batch(
    pool: &mut Pool,
    stream: &chunk_pipeline::WorldStream,
    artwork: &ActorArtworkPages,
    partial_tick: f32,
    displays: Vec<(u64, [i32; 3], SpawnerDisplayClock)>,
) -> ActorPresentationBatch {
    let mut batch = ActorPresentationBatch {
        submissions: Vec::new(),
        skin_layers: Vec::new(),
        artwork: HashMap::new(),
    };
    pool.poses.begin_frame();
    let mut sampled = pool.actors.frame(partial_tick);
    for (runtime, position, spin) in displays {
        let (Some(actor), Some(rig)) = (pool.actors.actor(runtime), pool.actors.rig(runtime))
        else {
            continue;
        };
        let rig = sampled.sample_rig_scale(rig);
        let Some(mut presentation) = entity_rig_presentation_cached(
            &rig,
            actor,
            artwork,
            partial_tick,
            Some(&mut pool.poses),
        ) else {
            continue;
        };
        let dimensions = presentation
            .submission
            .culling_bounds
            .half_extents
            .map(|axis| axis * 2.0);
        let Some(placement) = spawner_actor_transform(position, dimensions, spin, partial_tick)
        else {
            continue;
        };
        let body = &mut presentation.submission;
        let [x, y, z] = body.world_from_actor;
        let normal = Mat4::from_cols_array(&[
            x[0], y[0], z[0], 0.0, x[1], y[1], z[1], 0.0, x[2], y[2], z[2], 0.0, 0.0, 0.0, 0.0, 1.0,
        ]);
        body.world_from_actor = matrix_rows(placement * normal);
        if let Some(location) = presentation.artwork {
            batch.artwork.insert(body.input.identity, location);
        }
        batch.submissions.push(presentation.submission);
    }
    for body in &mut batch.submissions {
        let feet = body.world_from_actor.map(|row| row[3]);
        let position = pool
            .actors
            .actor(body.input.identity.runtime_id)
            .map_or(feet, |actor| actor.brightness_sample_position(feet));
        if let Some((block, sky)) = stream.solved_light_at(position) {
            body.light = render::pack_actor_light(block, sky);
        }
    }
    pool.layers.begin_frame();
    apply_render_layers_cached(
        &mut batch,
        |runtime| sampled.layers(runtime),
        artwork,
        &mut pool.layers,
    );
    batch
}

#[cfg(test)]
mod tests;
