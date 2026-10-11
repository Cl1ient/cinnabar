use chunk_pipeline::WorldStream;
use particles::{ParticleSystem, SpawnRequest, actor::ActorEffectKey};

#[cfg(test)]
mod tests;

#[derive(Debug)]
pub(super) enum ActorParticleCommand {
    Start(ActorEffectKey, Box<SpawnRequest>),
    Stop(u64),
}

pub(super) fn queue_actor_particles(
    stream: &WorldStream,
    system: &mut ParticleSystem,
    queue: &mut Vec<ActorParticleCommand>,
    alpha: f32,
) {
    system.actor_emitters.begin_frame();
    let mut frame = stream.authority().actor_render_frame(alpha);
    for state in stream.authority().actor_particle_controllers() {
        let Some(actor) = stream.authority().actor(state.actor.runtime_id) else {
            continue;
        };
        let rig = stream
            .authority()
            .actor_rig(actor.runtime_id)
            .map(|rig| frame.sample_rig_scale(rig));
        let layers = frame.layers(actor.runtime_id).unwrap_or_default();
        for (index, effect) in system
            .actor_bindings
            .effects_for(state.entity, state.controller, state.state)
            .enumerate()
        {
            if !system.has_effect(effect.identifier) {
                continue;
            }
            let key = ActorEffectKey {
                session_id: state.actor.session_id,
                dimension: state.actor.dimension,
                runtime_id: state.actor.runtime_id,
                spawn_revision: state.actor.spawn_revision,
                reset_generation: state.reset_generation,
                controller: state.controller_index,
                state: state.state_index,
                entered_tick: state.entered_tick,
                effect_index: index as u16,
            };
            if system.actor_emitters.admit(key) {
                let Some(transform) =
                    client_presentation::presentation::actors::actor_emitter_transform(
                        actor,
                        rig.as_ref(),
                        &layers,
                        effect.locator,
                        alpha,
                    )
                else {
                    continue;
                };
                let query_names = effect
                    .initialization
                    .into_iter()
                    .flat_map(|script| script.query_names())
                    .map(Into::into)
                    .collect();
                let attachment = particles::actor::ActorAttachment {
                    runtime_id: actor.runtime_id,
                    spawn_revision: actor.spawn_revision,
                    locator: effect.locator.map(Into::into),
                    queries: query_names,
                    entered_tick: state.entered_tick,
                    world_offset: [0.0; 3],
                };
                let queries = if effect.bound {
                    actor_queries(actor, rig.as_ref(), &attachment, alpha)
                } else {
                    particles::Queries::default()
                };
                queue.push(ActorParticleCommand::Start(
                    key,
                    Box::new(SpawnRequest {
                        effect: effect.identifier.to_owned(),
                        position: transform.position,
                        basis: effect.bound.then_some(transform.basis),
                        initialization: effect.initialization.cloned(),
                        queries,
                        actor_attachment: effect.bound.then_some(attachment),
                        seed: actor.runtime_id ^ actor.spawn_revision.rotate_left(32),
                        ..SpawnRequest::default()
                    }),
                ));
            }
        }
    }
    system
        .actor_emitters
        .finish_frame(|emitter| queue.push(ActorParticleCommand::Stop(emitter)));
}

pub(super) fn route_actor_particles(
    system: &mut ParticleSystem,
    queue: &mut Vec<ActorParticleCommand>,
) {
    for command in queue.drain(..) {
        match command {
            ActorParticleCommand::Start(key, request) => {
                let id = system.spawn(&request);
                system.actor_emitters.started(key, id);
            }
            ActorParticleCommand::Stop(emitter) => system.stop(emitter),
        }
    }
}

/// Provides emitter-local numeric query inputs from the retained actor context.
fn actor_queries(
    actor: &client_world::ActorSnapshot,
    rig: Option<&client_world::ActorRigSnapshot<'_>>,
    attachment: &particles::actor::ActorAttachment,
    alpha: f32,
) -> particles::Queries {
    let context = rig.map(|rig| {
        rig.particle_queries(
            actor,
            alpha,
            (rig.completed_tick.saturating_sub(attachment.entered_tick) as f32 + alpha)
                / world::TICKS_PER_SECOND as f32,
        )
    });
    let query = |name| context.as_ref().map_or(0.0, |context| context.query(name));
    particles::Queries {
        is_in_water: query("is_in_water"),
        is_in_water_or_rain: query("is_in_water_or_rain"),
        is_baby: query("is_baby"),
        ground_speed: query("ground_speed"),
        frame_alpha: alpha,
        actor: (!attachment.queries.is_empty()).then(|| {
            std::sync::Arc::new(
                attachment
                    .queries
                    .iter()
                    .map(|name| (name.clone(), query(name.as_ref())))
                    .collect(),
            )
        }),
        ..Default::default()
    }
}

/// Updates bound controller emitters from sampled actor poses, guarding actor replacement.
pub(super) fn follow_actor_particles(
    system: &mut ParticleSystem,
    stream: &WorldStream,
    alpha: f32,
) {
    let mut frame = stream.authority().actor_render_frame(alpha);
    system.update_actor_attachments(|attachment| {
        let actor = stream.authority().actor(attachment.runtime_id)?;
        if actor.spawn_revision != attachment.spawn_revision {
            return None;
        }
        let rig = stream
            .authority()
            .actor_rig(actor.runtime_id)
            .map(|rig| frame.sample_rig_scale(rig));
        let layers = frame.layers(actor.runtime_id).unwrap_or_default();
        let mut transform = client_presentation::presentation::actors::actor_emitter_transform(
            actor,
            rig.as_ref(),
            &layers,
            attachment.locator.as_deref(),
            alpha,
        )?;
        transform.position =
            std::array::from_fn(|axis| transform.position[axis] + attachment.world_offset[axis]);
        Some((
            transform,
            actor_queries(actor, rig.as_ref(), attachment, alpha),
        ))
    });
}
