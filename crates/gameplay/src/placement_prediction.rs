//! Click-dependent numerical state programs and placement validation before local mutation.

use crate::{
    block_use::{BoxBounds, UseSurroundings, overlaps, placement_cell},
    movement::PhysicsCollisionRegistries,
    placement_state::PlacementInput,
    placement_support::{SupportRequirement, support_acceptance, support_requirement},
};
use assets::NetworkIdMode;
use assets::registry::{
    BlockPlacementFlags as Flags, BlockPlacementRule as Rule, BlockSupportKind as Kind,
};
use sim::PaletteWorld;
use std::ops::Range;

/// Frozen click and collision facts used for one admitted placement.
pub struct PlacementContext<'a> {
    pub clicked: [i32; 3],
    pub input: PlacementInput,
    pub surroundings: &'a UseSurroundings,
    /// The session's inclusive minimum and exclusive maximum build heights.
    pub build_height: Range<i32>,
}

/// A validated cell and store identity to commit after transport admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PredictedPlacement {
    pub position: [i32; 3],
    pub block: u32,
    pub additional: Vec<([i32; 3], u32)>,
}

/// Resolves a verified placement state; missing world or survival facts defer to the server.
pub fn predicted_placement(
    collisions: &PhysicsCollisionRegistries,
    mode: NetworkIdMode,
    held: u32,
    world: &PaletteWorld<'_>,
    context: &PlacementContext<'_>,
) -> Option<PredictedPlacement> {
    if context.input.face > 5 {
        return None;
    }
    let held_facts = collisions.block_placement_facts(mode, held);
    if held_facts.rule == Rule::Unresolved {
        return None;
    }
    let clicked = world.primary_runtime_id(context.clicked).ok()?;
    // A ray cannot select air; an air cell here means its observation changed.
    if collisions.block_survival_facts(mode, clicked).support == Kind::Air {
        return None;
    }
    let clicked_facts = collisions.block_placement_facts(mode, clicked);
    if held_facts.rule == Rule::Candle
        && context.input.face == 1
        && held_facts.set == clicked_facts.set
        && clicked_facts.key & 3 >= 3
    {
        return None;
    }
    let destination = if clicked_facts.flags.contains(Flags::REPLACEABLE) {
        context.clicked
    } else {
        placement_cell(context.clicked, context.input.face)
    };
    let destination_block = world.primary_runtime_id(destination).ok()?;
    let free = collisions
        .block_placement_facts(mode, destination_block)
        .flags
        .contains(Flags::REPLACEABLE);
    let merged = [
        (context.clicked, Some(context.input.face)),
        (destination, None),
    ]
    .into_iter()
    .filter(|(position, face)| face.is_some() || *position != context.clicked)
    .find_map(|(position, face)| {
        let existing = world.primary_runtime_id(position).ok()?;
        let facts = collisions.block_placement_facts(mode, existing);
        crate::placement_program::merged(held_facts, facts, face, context.input.face)
            .map(|(set, key, full)| (position, set, key, full))
    });
    let position = merged.map_or(destination, |merged| merged.0);
    if (merged.is_none() && !free) || !context.build_height.contains(&position[1]) {
        return None;
    }
    let existing = world.primary_runtime_id(position).ok()?;
    if crate::placement_program::water_sensitive(held_facts.rule) {
        use sim::CollisionWorld;
        let sample = world.block_physics(position).ok()?;
        if sample.layers.iter().any(|layer| {
            layer.flags.contains(sim::BlockPhysicsFlags::WATER)
                || layer.flags.contains(sim::BlockPhysicsFlags::LAVA)
        }) {
            return None;
        }
    }
    if merged.is_none() && collisions.block_survival_facts(mode, existing).support != Kind::Air {
        return None;
    }
    let support = std::array::from_fn(|face| {
        let block = world
            .primary_runtime_id(placement_cell(position, face as u8))
            .ok()?;
        support_acceptance(
            collisions.block_survival_facts(mode, held).policy,
            collisions.block_survival_facts(mode, block).support,
            collisions.block_is_full_cube(mode, block),
        )
    });
    let (set, key, full_cell) = if let Some((_, set, key, full)) = merged {
        (set, key, full)
    } else {
        let (set, key) = crate::placement_program::placed(
            held_facts,
            context.input,
            position,
            context.surroundings.player_box,
            support,
        )?;
        (set, key, false)
    };
    let mut block = collisions.placement_state_id(mode, set, key)?;
    let mut placed = collisions.block_placement_facts(mode, block);
    if placed.rule == Rule::Connections {
        placed.key =
            crate::placement_neighbours::connection_key(collisions, mode, world, position, block)?;
        block = collisions.placement_state_id(mode, placed.set, placed.key)?;
    }
    let read = |position| {
        let id = world.primary_runtime_id(position).ok()?;
        Some(collisions.block_placement_facts(mode, id))
    };
    placed.key = crate::placement_neighbours::stair_key(placed, position, read)?;
    block = collisions.placement_state_id(mode, placed.set, placed.key)?;
    let requirement = if full_cell {
        SupportRequirement::Independent
    } else {
        support_requirement(collisions.block_survival_facts(mode, block).rule)
    };
    match requirement {
        SupportRequirement::Independent => {}
        SupportRequirement::Face(face) if support[usize::from(face)] == Some(true) => {}
        SupportRequirement::Face(_) | SupportRequirement::Unsupported => return None,
    }
    let mut neighbours = Vec::new();
    let door = placed.rule == Rule::Door;
    if door {
        if context.input.face != 1 {
            return None;
        }
        let upper_position = placement_cell(position, 1);
        let upper_existing = world.primary_runtime_id(upper_position).ok()?;
        if !context.build_height.contains(&upper_position[1])
            || collisions
                .block_survival_facts(mode, upper_existing)
                .support
                != Kind::Air
        {
            return None;
        }
        let key = crate::placement_neighbours::door_upper(placed, position, |position| {
            let id = world.primary_runtime_id(position).ok()?;
            Some((
                collisions.block_placement_facts(mode, id),
                collisions.block_survival_facts(mode, id).support,
            ))
        })?;
        neighbours.push((
            upper_position,
            collisions.placement_state_id(mode, placed.set, key)?,
        ));
    }
    if placed.rule == Rule::Stair {
        for face in 2..=5 {
            let neighbour_position = placement_cell(position, face);
            let neighbour = read(neighbour_position)?;
            if neighbour.rule == Rule::Unresolved {
                return None;
            }
            if neighbour.rule != Rule::Stair {
                continue;
            }
            let key =
                crate::placement_neighbours::stair_key(neighbour, neighbour_position, |cell| {
                    if cell == position {
                        Some(placed)
                    } else {
                        read(cell)
                    }
                })?;
            let id = collisions.placement_state_id(mode, neighbour.set, key)?;
            if world.primary_runtime_id(neighbour_position).ok()? != id {
                neighbours.push((neighbour_position, id));
            }
        }
    }
    let actor_overlap = |cell, local| {
        std::iter::once(&context.surroundings.player_box)
            .chain(&context.surroundings.actor_boxes)
            .any(|actor| overlaps(cell, local, *actor))
    };
    if full_cell && actor_overlap(position, ([0.0; 3], [1.0; 3])) {
        return None;
    }
    neighbours.push((position, block));
    for &(cell, id) in &neighbours {
        if cell != position && !door {
            continue;
        }
        let shapes = world
            .collision_shapes_with_updates(cell, id, &neighbours)
            .ok()?;
        if shapes.iter().any(|shape| {
            let local: BoxBounds = (
                [shape.min.x, shape.min.y, shape.min.z],
                [shape.max.x, shape.max.y, shape.max.z],
            );
            actor_overlap(cell, local)
        }) {
            return None;
        }
    }
    neighbours.pop();
    Some(PredictedPlacement {
        position,
        block,
        additional: neighbours,
    })
}

#[cfg(test)]
mod tests;
