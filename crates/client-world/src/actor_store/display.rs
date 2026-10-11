//! Client-owned display actors share admission and rig evaluation without entering network state.
use super::*;
use std::{collections::BTreeMap, sync::Arc};

pub const MAX_DISPLAY_ACTORS: usize = 256;

/// One bounded lifetime namespace for a dimension's displayed entities.
pub struct DisplayActorStore {
    store: ActorStore,
    positions: BTreeMap<[i32; 3], u64>,
    sequence: u64,
    next_runtime: u64,
}

impl DisplayActorStore {
    /// Creates the normal actor evaluator with bounded display-only storage.
    pub fn new(dimension: i32, assets: Arc<assets::RuntimeEntityAssets>) -> Self {
        let session = crate::authority::allocate_actor_session_id();
        let mut store = ActorStore::with_capacity(session, dimension, MAX_DISPLAY_ACTORS, 0);
        store.animation = crate::actor_animation::ActorAnimationStore::with_assets(assets);
        Self {
            store,
            positions: BTreeMap::new(),
            sequence: 0,
            next_runtime: 0,
        }
    }

    /// Rebinds the same displayed lifetimes when the active pack changes.
    pub fn set_pack(&mut self, pack: Option<(Arc<assets::RuntimeEntityAssets>, Vec<u32>)>) {
        self.store.set_pack_entities(pack);
    }

    /// Reuses an unchanged display actor, replacing it when its configured entity changes.
    pub fn resolve(&mut self, position: [i32; 3], identifier: &str) -> Option<u64> {
        if identifier.is_empty()
            || identifier.len() > protocol::MAX_ACTOR_IDENTIFIER_BYTES
            || identifier.chars().any(char::is_control)
        {
            return None;
        }
        if let Some(&runtime) = self.positions.get(&position) {
            if self.store.get(runtime).is_some_and(|actor| {
                matches!(&actor.kind,
                ActorKind::Entity { identifier: current } if current.as_ref() == identifier)
            }) {
                return Some(runtime);
            }
            self.remove(position);
        }
        if self.positions.len() >= MAX_DISPLAY_ACTORS {
            return None;
        }
        self.next_runtime = self.next_runtime.checked_add(1)?;
        self.sequence = self.sequence.checked_add(1)?;
        let runtime_id = self.next_runtime;
        let event = ActorSpawnEvent {
            dimension: self.store.dimension,
            unique_id: i64::try_from(runtime_id).ok()?,
            runtime_id,
            kind: ActorKind::Entity {
                identifier: Arc::from(identifier),
            },
            position: position.map(|axis| axis as f32),
            velocity: [0.0; 3],
            pitch: 0.0,
            yaw: 0.0,
            head_yaw: 0.0,
            body_yaw: 0.0,
            held_item: Default::default(),
            metadata: Arc::default(),
            attributes: Arc::default(),
            properties: Arc::default(),
            links: Arc::default(),
        };
        let result = self.store.apply(
            self.store.session_id,
            self.sequence,
            ActorEvent::Spawn(event),
        );
        if result != ActorApplyResult::Inserted {
            return None;
        }
        self.positions.insert(position, runtime_id);
        Some(runtime_id)
    }

    /// Removes only the client-owned lifetime for this block position.
    fn remove(&mut self, position: [i32; 3]) {
        if let Some(runtime) = self.positions.remove(&position) {
            let Some(sequence) = self.sequence.checked_add(1) else {
                return;
            };
            self.sequence = sequence;
            self.store.apply(
                self.store.session_id,
                sequence,
                ActorEvent::Remove(protocol::ActorRemoveEvent {
                    dimension: self.store.dimension,
                    unique_id: runtime as i64,
                }),
            );
        }
    }

    /// Retires displays whose blocks are no longer admitted for this frame.
    pub fn retain(&mut self, positions: &std::collections::BTreeSet<[i32; 3]>) {
        let removed = self
            .positions
            .keys()
            .filter(|position| !positions.contains(*position))
            .copied()
            .collect::<Vec<_>>();
        for position in removed {
            self.remove(position);
        }
    }

    /// Feeds the resolved camera's degrees and world position before evaluating display poses.
    pub fn set_camera(&mut self, rotation: [f32; 2], position: [f32; 3]) {
        self.store.set_camera_rotation(rotation);
        self.store.set_camera_position(position);
    }

    /// Advances ordinary actor visual state once for the elapsed simulation ticks.
    pub fn advance(&mut self, ticks: u32) {
        self.store.advance_interpolation_frame(ticks);
    }

    /// Borrows the displayed snapshot for the normal presentation adapter.
    pub fn actor(&self, runtime: u64) -> Option<&ActorSnapshot> {
        self.store.get(runtime)
    }

    /// Borrows the normal rig, including its admitted render controllers.
    pub fn rig(&self, runtime: u64) -> Option<crate::ActorRigSnapshot<'_>> {
        self.store.actor_rig(runtime)
    }

    /// Gives display rendering the same bounded sampled expression context as world actors.
    pub fn frame(&self, alpha: f32) -> crate::ActorRenderFrame<'_> {
        crate::ActorRenderFrame::new(&self.store, alpha)
    }
}
