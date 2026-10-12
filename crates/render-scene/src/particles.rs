use bevy::{
    prelude::{Deref, DerefMut, GlobalTransform, Projection, Resource},
    render::extract_resource::ExtractResource,
};
use particles::{
    AtlasPatch, DrawLists, ParticleInstance, ParticleSystem, ParticleView, ParticleWorld,
};
use std::sync::Arc;
/// The particle simulation as a Bevy resource.
#[derive(Resource, Default, Deref, DerefMut)]
pub struct ParticleSimulation(pub ParticleSystem);

/// The frame's particle draw data, extracted to the render world each frame.
#[derive(Resource, ExtractResource, Clone, Default)]
#[extract_app(bevy::render::RenderApp)]
pub struct ParticleGpuFrame {
    pub base: Option<Arc<[u8]>>,
    pub patch_seq: u64,
    pub patches: Arc<[AtlasPatch]>,
    pub opaque: Arc<[ParticleInstance]>,
    pub blend: Arc<[ParticleInstance]>,
    pub add: Arc<[ParticleInstance]>,
}

impl ParticleGpuFrame {
    #[must_use]
    pub fn instance_count(&self) -> usize {
        self.opaque.len() + self.blend.len() + self.add.len()
    }

    /// Publishes the current camera-sorted instance lists for extraction.
    fn set_lists(&mut self, lists: DrawLists) {
        self.blend = lists.blend.into();
        self.add = lists.add.into();
        self.opaque = lists.opaque.into();
    }
}

/// Camera frustum basis from a Bevy camera transform and projection.
#[must_use]
pub fn particle_view(transform: &GlobalTransform, projection: &Projection) -> ParticleView {
    let (right, up, forward) = (transform.right(), transform.up(), transform.forward());
    let half_diagonal = match projection {
        Projection::Perspective(perspective) => {
            let half_height = (perspective.fov * 0.5).tan();
            let half_width = half_height * perspective.aspect_ratio;
            half_width.hypot(half_height).atan()
        }
        _ => std::f32::consts::FRAC_PI_2,
    };
    ParticleView {
        position: transform.translation().to_array(),
        right: right.to_array(),
        up: up.to_array(),
        forward: forward.to_array(),
        half_diagonal,
    }
}

/// Ticks the simulation and refreshes the extracted draw data; call once per frame.
pub fn update_particle_frame(
    system: &mut ParticleSystem,
    frame: &mut ParticleGpuFrame,
    dt: f32,
    view: &ParticleView,
    world: &dyn ParticleWorld,
) {
    system.set_camera(view.position);
    system.tick(dt, world);
    let lists = system.build_draw(view, world);
    frame.set_lists(lists);
    let base = system.atlas_base();
    if frame
        .base
        .as_ref()
        .is_none_or(|old| !Arc::ptr_eq(old, &base))
    {
        frame.base = Some(base);
        frame.patch_seq = 0;
        frame.patches = Arc::default();
    }
    let seq = system.atlas().patch_seq();
    if seq != frame.patch_seq {
        frame.patches = system.atlas().patches().into();
        frame.patch_seq = seq;
    }
}

#[cfg(test)]
mod tests;
