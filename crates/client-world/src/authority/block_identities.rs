use std::{collections::HashMap, sync::OnceLock};

use super::*;

impl WorldAuthority {
    /// Retains canonical custom state identities independently of visual pack compilation.
    /// Sequential identities require the admitted internal range; hashed identities use their hash.
    pub fn set_custom_block_identities(&mut self, definitions: &CustomBlocks) {
        let mut identities = HashMap::new();
        self.block_names.clear_custom();
        let mut next = self.custom_block_ids.start;
        let mut skipped = 0_usize;
        for (index, block) in definitions.blocks.iter().enumerate() {
            let first = next;
            if self.network_id_mode == NetworkIdMode::Sequential {
                let Some(end) = next.checked_add(block.state_count) else {
                    skipped = skipped.saturating_add(definitions.blocks.len() - index);
                    break;
                };
                next = end;
                if end > self.custom_block_ids.end {
                    skipped = skipped.saturating_add(1);
                    continue;
                }
            }
            let states = block.hashed_states();
            if states.len() != block.state_count as usize {
                skipped = skipped.saturating_add(1);
                continue;
            }
            let mut named = Vec::with_capacity(states.len());
            for (offset, state) in states.into_iter().enumerate() {
                let internal = match self.network_id_mode {
                    NetworkIdMode::Sequential => first + offset as u32,
                    NetworkIdMode::Hashed => state.hash,
                };
                identities.entry(state.hash).or_insert(internal);
                named.push(internal);
            }
            self.block_names
                .custom
                .insert(block.name.clone(), named.into());
        }
        eprintln!(
            "SESSION_CUSTOM_BLOCK_IDENTITIES session={} mode={:?} states={} skipped_definitions={skipped}",
            self.actor_session_id,
            self.network_id_mode,
            identities.len(),
        );
        self.custom_block_identities = Arc::new(identities);
    }
}

/// Names resolve to internal store identities, including all admitted custom states.
#[derive(Debug)]
pub(super) struct BlockNames {
    base: Arc<HashMap<Box<str>, Box<[u32]>>>,
    custom: HashMap<Arc<str>, Box<[u32]>>,
    revision: u64,
}

impl BlockNames {
    /// Removes names from the previous custom range before a new palette is admitted.
    pub(super) fn clear_custom(&mut self) {
        self.custom.clear();
        self.revision = self.revision.wrapping_add(1);
    }

    /// Shares the immutable pinned registry index between sessions of the same ID mode.
    pub(super) fn new(mode: NetworkIdMode) -> Self {
        static SEQUENTIAL: OnceLock<Arc<HashMap<Box<str>, Box<[u32]>>>> = OnceLock::new();
        static HASHED: OnceLock<Arc<HashMap<Box<str>, Box<[u32]>>>> = OnceLock::new();
        let cache = match mode {
            NetworkIdMode::Sequential => &SEQUENTIAL,
            NetworkIdMode::Hashed => &HASHED,
        };
        let base = cache.get_or_init(|| {
            let records = assets::read_registry_for_protocol(
                assets::pinned_block_registry_bytes(),
                assets::active_content_registry_protocol(),
            )
            .unwrap_or_else(|error| {
                eprintln!("block name registry unavailable: {error}");
                Box::default()
            });
            let mut names: HashMap<Box<str>, Vec<u32>> = HashMap::new();
            for record in records {
                let id = match mode {
                    NetworkIdMode::Sequential => record.sequential_id,
                    NetworkIdMode::Hashed => record.network_hash,
                };
                names.entry(record.name).or_default().push(id);
            }
            Arc::new(
                names
                    .into_iter()
                    .map(|(name, ids)| (name, ids.into()))
                    .collect(),
            )
        });
        Self {
            base: Arc::clone(base),
            custom: HashMap::new(),
            revision: 0,
        }
    }
}

impl WorldAuthority {
    /// Resolves a block identifier to the active session's internal store IDs.
    pub fn block_ids_for_name(&self, name: &str) -> &[u32] {
        self.block_names
            .custom
            .get(name)
            .or_else(|| self.block_names.base.get(name))
            .map_or(&[], AsRef::as_ref)
    }

    /// Changes whenever the admitted custom name-to-state mapping is replaced.
    pub const fn block_name_revision(&self) -> u64 {
        self.block_names.revision
    }
}
