use super::{ActorAnimationStore, ActorLifetimeId};

/// A controller state that completed authoritative animation evaluation for one actor lifetime.
#[derive(Clone, Copy, Debug)]
pub struct ActorParticleController<'a> {
    pub actor: ActorLifetimeId,
    pub entity: &'a str,
    pub controller: &'a str,
    pub state: &'a str,
    pub controller_index: u32,
    pub state_index: u32,
    pub entered_tick: u64,
    pub reset_generation: u64,
}

impl ActorAnimationStore {
    pub(crate) fn particle_controllers(&self) -> impl Iterator<Item = ActorParticleController<'_>> {
        self.rigs.iter().flat_map(move |(&actor, rig)| {
            let assets = if rig.pack {
                self.pack.as_ref().map(|pack| pack.assets.as_ref())
            } else {
                self.assets.as_deref()
            };
            rig.controllers.iter().filter_map(move |runtime| {
                if !rig.initialized || rig.reset_pending || !runtime.active {
                    return None;
                }
                let assets = assets?;
                let binding = assets.rig_bindings().get(rig.rig_binding)?;
                let entity = assets.symbols().get(binding.entity_symbol as usize)?;
                let controller = assets.controllers().get(runtime.controller)?;
                let symbol = assets.symbols().get(controller.symbol as usize)?;
                let state = assets
                    .controller_states()
                    .get(controller.first_state as usize + usize::from(runtime.state))?;
                let name = assets.molang_symbols().get(state.name as usize)?;
                Some(ActorParticleController {
                    actor,
                    entity: &entity.identifier,
                    controller: &symbol.identifier,
                    state: &name.identifier,
                    controller_index: runtime.controller as u32,
                    state_index: controller.first_state + u32::from(runtime.state),
                    entered_tick: runtime.entered_tick,
                    reset_generation: rig.reset_generation,
                })
            })
        })
    }
}

/// Retained numeric actor query inputs, with the emitter's frame and controller state time.
pub struct ActorParticleQueries<'a> {
    actor: &'a crate::ActorSnapshot,
    input: super::ActorTickInput,
    context: super::ActorTickContext,
    life_tick: u64,
    completed_tick: u64,
    bone_names: &'a [Box<str>],
    alpha: f32,
    state_time: f32,
}

impl<'a> super::ActorRigSnapshot<'a> {
    /// Retains actor query facts once without copying the owner's script variable map.
    pub fn particle_queries(
        &self,
        actor: &'a crate::ActorSnapshot,
        alpha: f32,
        state_time: f32,
    ) -> ActorParticleQueries<'a> {
        let owner = self.animation_variables;
        let mut context = owner.owner_context().cloned().unwrap_or_default();
        context.frame_alpha = alpha;
        ActorParticleQueries {
            actor,
            input: owner.input().unwrap_or_default(),
            context,
            life_tick: owner.life_tick(),
            completed_tick: self.completed_tick,
            bone_names: self.bone_names,
            alpha,
            state_time,
        }
    }
}

impl ActorParticleQueries<'_> {
    /// Reads a numeric actor query; unknown facts retain the actor evaluator's idle value.
    pub fn query(&self, name: &str) -> f32 {
        if name == "frame_alpha" {
            return self.alpha;
        }
        let inputs = super::query::QueryInputs {
            actor: self.actor,
            input: &self.input,
            context: &self.context,
            anim_tick: self.completed_tick,
            anim_time: None,
            swell_amount: None,
            life_tick: self.life_tick,
            finished: (false, false),
            state_time: self.state_time,
            bones: &[],
            bone_names: self.bone_names,
        };
        super::query::query(&inputs, name, &[]).number()
    }
}
