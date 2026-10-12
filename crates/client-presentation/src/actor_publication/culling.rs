use actor_animation::ActorRigSnapshot;
use bevy::math::Mat4;
use bevy::prelude::{Projection, Transform};
use client_world::ActorSnapshot;
use render_model::ActorCullView;

#[derive(Clone, Copy)]
pub(super) struct ActorPublicationViews {
    pub(super) main: ActorCullView,
    pub(super) publication: ActorCullView,
    pub(super) casts_shadows: bool,
}

impl ActorPublicationViews {
    pub(super) fn new(transform: &Transform, projection: &Projection, casts_shadows: bool) -> Self {
        let main = ActorCullView {
            clip_from_world: projection.get_clip_from_view() * transform.to_matrix().inverse(),
            camera_position: transform.translation,
            max_distance: render_model::MAX_ACTOR_RENDER_DISTANCE_BLOCKS,
        };
        let publication = if casts_shadows {
            let radius = main.max_distance;
            ActorCullView {
                clip_from_world: glam::camera::rh::proj::directx::orthographic(
                    -radius, radius, -radius, radius, -radius, radius,
                ) * Mat4::from_translation(-transform.translation),
                ..main
            }
        } else {
            main
        };
        Self {
            main,
            publication,
            casts_shadows,
        }
    }
}

pub(super) fn rig_may_be_published(
    rig: &ActorRigSnapshot<'_>,
    actor: &ActorSnapshot,
    partial_tick: f32,
    views: Option<ActorPublicationViews>,
    frame: Option<&mut actor_animation::ActorRenderFrame<'_>>,
    occluded: impl Fn([f32; 3], [f32; 3]) -> bool,
) -> bool {
    crate::presentation::actors::rig_may_be_visible(
        rig,
        actor,
        partial_tick,
        views.map(|views| views.publication),
        frame,
        |low, high| !views.is_some_and(|views| views.casts_shadows) && occluded(low, high),
    )
}

pub(super) fn animation_view(
    transform: &Transform,
    projection: &Projection,
    views: Option<ActorPublicationViews>,
) -> Option<actor_animation::ActorAnimationView> {
    let Some(views) = views.filter(|views| views.casts_shadows) else {
        return camera_animation_view(transform, projection);
    };
    let clip = views.publication.clip_from_world;
    let [x, y, z, w] = [0, 1, 2, 3].map(|row| clip.row(row));
    Some(actor_animation::ActorAnimationView {
        planes: [w + x, w - x, w + y, w - y, z, w - z].map(|plane| plane.to_array()),
        camera: transform.translation.to_array(),
        player_distance: views.publication.max_distance,
        entity_radius: render_model::ACTOR_CANDIDATE_RADIUS_BLOCKS,
    })
}

pub(super) fn shadow_only_local(
    mut local: crate::presentation::actors::ActorRigPresentation,
) -> crate::presentation::actors::ActorRigPresentation {
    local.submission.make_shadow_only();
    local
}

pub(super) fn local_shadow_body(
    mut local: crate::presentation::actors::ActorRigPresentation,
    stream: Option<&chunk_pipeline::WorldStream>,
    poses: &mut crate::presentation::actors::PoseConversions,
    partial_tick: f32,
) -> Option<crate::presentation::actors::ActorRigPresentation> {
    match local.submission.route {
        render_model::ActorRigRoute::NoDraw => return None,
        render_model::ActorRigRoute::Diagnostic => {}
        _ => {
            let rig = stream?
                .authority()
                .actor_world_body_rig(local.submission.input.identity.runtime_id)?;
            let actor = stream?.authority().actor(rig.actor.runtime_id)?;
            if !poses.apply_pose(&mut local, &rig, actor, partial_tick) {
                return None;
            }
        }
    }
    Some(shadow_only_local(local))
}

/// Keeps all drawable layers of a shadow-only body behind visible actors in capacity admission.
pub(super) fn shadow_only_caster_layers(
    batch: &mut crate::presentation::actors::ActorPresentationBatch,
) {
    let mut casters: Vec<_> = batch
        .submissions
        .iter()
        .filter(|submission| {
            submission.input.identity.layer == render_model::ACTOR_LAYER_BODY
                && submission.route == render_model::ActorRigRoute::ShadowOnly
        })
        .map(|submission| submission.input.identity.runtime_id)
        .collect();
    casters.sort_unstable();
    for submission in &mut batch.submissions {
        if casters
            .binary_search(&submission.input.identity.runtime_id)
            .is_ok()
        {
            submission.make_shadow_only();
        }
    }
}

pub(super) fn world_rig<'a>(
    stream: &'a chunk_pipeline::WorldStream,
    runtime_id: u64,
    local_runtime_id: u64,
    local_shadows: bool,
) -> Option<ActorRigSnapshot<'a>> {
    if local_shadows && runtime_id == local_runtime_id {
        stream.authority().actor_world_body_rig(runtime_id)
    } else {
        stream.authority().actor_rig(runtime_id)
    }
}

/// Rigs this far outside the view on every side still animate, so only a turn faster than this
/// in one tick shows a rig its held pose for that tick.
const ANIMATION_GUARD_DEGREES: f32 = 30.0;

/// The camera's frustum widened by the guard band, with the render distances.
fn camera_animation_view(
    transform: &Transform,
    projection: &Projection,
) -> Option<actor_animation::ActorAnimationView> {
    let Projection::Perspective(perspective) = projection else {
        return None;
    };
    let (guard, limit) = (ANIMATION_GUARD_DEGREES.to_radians(), 85f32.to_radians());
    let half_vertical = perspective.fov * 0.5;
    let half_horizontal = (half_vertical.tan() * perspective.aspect_ratio).atan();
    let (half_vertical, half_horizontal) = (
        (half_vertical + guard).min(limit),
        (half_horizontal + guard).min(limit),
    );
    let clip = glam::camera::rh::proj::directx::perspective_infinite_reverse(
        half_vertical * 2.0,
        half_horizontal.tan() / half_vertical.tan(),
        perspective.near,
    ) * transform.to_matrix().inverse();
    let [x, y, z, w] = [0, 1, 2, 3].map(|row| clip.row(row));
    let planes = [w + x, w - x, w + y, w - y, z, w - z].map(|plane| plane.to_array());
    planes
        .iter()
        .flatten()
        .all(|value| value.is_finite())
        .then(|| actor_animation::ActorAnimationView {
            planes,
            camera: transform.translation.to_array(),
            player_distance: render_model::MAX_ACTOR_RENDER_DISTANCE_BLOCKS,
            entity_radius: render_model::ACTOR_CANDIDATE_RADIUS_BLOCKS,
        })
}

#[cfg(test)]
mod tests;
