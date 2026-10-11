//! Bounded numerical placement programs and their independent state inventory guard.

use super::{BlockPlacementFlags, BlockPlacementRule, ModelState, RegistryRecord};
use crate::AssetError;
use std::collections::{BTreeMap, BTreeSet};

/// Provisional state transformations select an existing state inside one collected identity set.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BlockPlacementFacts {
    pub rule: BlockPlacementRule,
    pub set: u32,
    pub key: u32,
    pub alternate: Option<u32>,
    pub flags: BlockPlacementFlags,
}

impl ModelState {
    /// Reads immutable placement facts without inspecting names or canonical JSON.
    pub const fn placement_facts(self) -> BlockPlacementFacts {
        BlockPlacementFacts {
            rule: self.placement_rule,
            set: self.placement_set,
            key: self.placement_key,
            alternate: if self.placement_alternate == 0 {
                None
            } else {
                Some(self.placement_alternate)
            },
            flags: BlockPlacementFlags::from_bits_retain(self.placement_flags),
        }
    }
}

impl BlockPlacementRule {
    /// Accepts only the packed selectors declared by this rule's bounded state domain.
    pub fn accepts_key(self, key: u32) -> bool {
        use BlockPlacementRule::*;
        match self {
            Unresolved | Identity => key == 0,
            Axis => key <= 2,
            Loom => key <= 3,
            Glazed | Torch | Piston => key <= 5,
            Stair => key <= 39,
            Slab | DoubleSlab | Lantern => key <= 1,
            Trapdoor | Connections | Lever | Snow | Vine | StandingSign => key <= 15,
            Hopper => key & 7 != 1 && key & 7 <= 5 && key <= 13,
            SeaPickle | Candle => key <= 7,
            Multiface => key <= 63,
            Door => key <= 31,
            WallSign => (2..=5).contains(&key),
            Dispenser | Button => key & 7 <= 5 && key <= 13,
        }
    }
}

/// Checks numerical set ownership and unique collected state keys before any program is admitted.
pub(super) fn validate(records: &[RegistryRecord], name_count: usize) -> Result<(), AssetError> {
    let inventory: BTreeMap<_, _> = records
        .iter()
        .map(|record| record.name.as_ref())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .enumerate()
        .map(|(index, name)| (name, index as u32 + 1))
        .collect();
    let mut sets = BTreeMap::new();
    let mut names = BTreeMap::new();
    let mut keys = BTreeSet::new();
    for record in records {
        let facts = record.model_state.placement_facts();
        let valid = facts.rule.accepts_key(facts.key)
            && facts.set as usize <= name_count
            && facts.alternate.is_none_or(|set| set as usize <= name_count);
        if !valid
            || (facts.rule == BlockPlacementRule::Unresolved
                && (facts.set != 0 || facts.key != 0 || facts.alternate.is_some()))
        {
            return Err(AssetError::InvalidRegistryFlags(facts.rule as u8));
        }
        if facts.rule == BlockPlacementRule::Unresolved {
            continue;
        }
        if inventory.get(record.name.as_ref()) != Some(&facts.set)
            || !keys.insert((facts.set, facts.key))
        {
            return Err(AssetError::InvalidRegistryFlags(facts.rule as u8));
        }
        let identity = (record.name.as_ref(), facts.rule);
        if sets
            .insert(facts.set, identity)
            .is_some_and(|previous| previous != identity)
            || names
                .insert(record.name.as_ref(), facts.set)
                .is_some_and(|previous| previous != facts.set)
        {
            return Err(AssetError::InvalidRegistryFlags(facts.rule as u8));
        }
    }
    for record in records {
        let facts = record.model_state.placement_facts();
        if let Some(target) = facts.alternate.and_then(|set| sets.get(&set)) {
            let compatible = matches!(
                (facts.rule, target.1),
                (BlockPlacementRule::Slab, BlockPlacementRule::DoubleSlab)
                    | (
                        BlockPlacementRule::StandingSign,
                        BlockPlacementRule::WallSign
                    )
                    | (
                        BlockPlacementRule::WallSign,
                        BlockPlacementRule::StandingSign
                    )
            );
            if !compatible {
                return Err(AssetError::InvalidRegistryFlags(facts.rule as u8));
            }
        }
    }
    Ok(())
}
