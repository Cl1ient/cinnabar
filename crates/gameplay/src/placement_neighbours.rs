//! Numerical neighbour predicates for paired doors, connections and stair corners.

use crate::{block_use::placement_cell, movement::PhysicsCollisionRegistries};
use assets::registry::{
    BlockPlacementFacts as Facts, BlockPlacementFlags as Flags, BlockPlacementRule as Rule,
};
use assets::{ContributorRole, ModelFamily, NetworkIdMode};
use sim::PaletteWorld;

/// Resolves an upper door state from the same bounded side facts used by the lower half.
pub(crate) fn door_upper(
    lower: Facts,
    position: [i32; 3],
    mut read: impl FnMut([i32; 3]) -> Option<(Facts, assets::registry::BlockSupportKind)>,
) -> Option<u32> {
    let direction = lower.key & 3;
    let step = [[0, 0, 1], [-1, 0, 0], [0, 0, -1], [1, 0, 0]][direction as usize];
    let mut sides = [(0_u8, false); 2];
    for (side, sign) in [-1, 1].into_iter().enumerate() {
        for height in 0..2 {
            let (facts, support) = read([
                position[0] + sign * step[0],
                position[1] + height,
                position[2] + sign * step[2],
            ])?;
            let door = facts.rule == Rule::Door;
            let solid = match support {
                assets::registry::BlockSupportKind::Air => false,
                assets::registry::BlockSupportKind::Stone => true,
                _ if door => false,
                _ => return None,
            };
            sides[side].0 += u8::from(solid);
            sides[side].1 |= door;
        }
    }
    let hinge = (sides[0].1 && !sides[1].1) || sides[0].0 < sides[1].0;
    Some((lower.key & 7) | 8 | (u32::from(hinge) << 4))
}

/// Retains only the already admitted numerical connection inputs.
#[derive(Clone, Copy)]
struct Connection {
    family: ModelFamily,
    role: ContributorRole,
    occludes: bool,
    orientation: Option<u32>,
    nether: bool,
}

/// Reads connection traits without routing any neighbour by its name.
fn connection(
    collisions: &PhysicsCollisionRegistries,
    mode: NetworkIdMode,
    block: u32,
) -> Option<Connection> {
    let (family, role, occludes, orientation) = collisions.block_connection_facts(mode, block)?;
    Some(Connection {
        family,
        role,
        occludes,
        orientation,
        nether: collisions
            .block_placement_facts(mode, block)
            .flags
            .contains(Flags::NETHER_FENCE),
    })
}

/// Selects pane, same-material fence, perpendicular gate and full-face links from typed traits.
fn connects(center: Connection, neighbour: Connection, face: u8) -> Option<bool> {
    if neighbour.occludes {
        return Some(true);
    }
    if neighbour.family == ModelFamily::Unknown {
        return None;
    }
    if neighbour.role != ContributorRole::Primary {
        return Some(false);
    }
    match center.family {
        ModelFamily::Pane => Some(matches!(
            neighbour.family,
            ModelFamily::Pane | ModelFamily::Wall
        )),
        ModelFamily::Fence => match neighbour.family {
            ModelFamily::Fence => Some(center.nether == neighbour.nether),
            ModelFamily::Gate => {
                let orientation = neighbour.orientation?;
                if orientation > 3 {
                    return None;
                }
                Some(match face {
                    2 | 3 => orientation & 1 != 0,
                    4 | 5 => orientation & 1 == 0,
                    _ => return None,
                })
            }
            _ => Some(false),
        },
        _ => None,
    }
}

/// Computes a bounded four-link state key from loaded admitted neighbours.
pub(crate) fn connection_key(
    collisions: &PhysicsCollisionRegistries,
    mode: NetworkIdMode,
    world: &PaletteWorld<'_>,
    position: [i32; 3],
    block: u32,
) -> Option<u32> {
    let center = connection(collisions, mode, block)?;
    if center.role != ContributorRole::Primary {
        return None;
    }
    let mut key = 0;
    for face in 2..=5 {
        let neighbour = world
            .primary_runtime_id(placement_cell(position, face))
            .ok()?;
        key |= u32::from(connects(
            center,
            connection(collisions, mode, neighbour)?,
            face,
        )?) << (face - 2);
    }
    Some(key)
}

/// Converts the compiled stair's legacy orientation to the compatibility cardinal ordinal.
fn direction(facts: Facts) -> u32 {
    [3, 1, 0, 2][(facts.key & 3) as usize]
}

/// Resolves a stair corner from numeric half, orientation, identity and blocker facts.
pub(crate) fn stair_key(
    center: Facts,
    position: [i32; 3],
    mut read: impl FnMut([i32; 3]) -> Option<Facts>,
) -> Option<u32> {
    if center.rule != Rule::Stair {
        return Some(center.key);
    }
    let config = [
        [3, 2, 3, 4, 5, 1, 5, 4],
        [4, 5, 0, 2, 3, 2, 3, 2],
        [2, 3, 1, 5, 4, 3, 4, 5],
        [5, 4, 2, 3, 2, 0, 2, 3],
    ][direction(center) as usize];
    let mut corner = 0;
    'neighbours: for (face, blocker_index, corners) in
        [(config[0], 3, [3, 4]), (config[1], 4, [1, 2])]
    {
        let neighbour = read(placement_cell(position, face))?;
        if neighbour.rule == Rule::Unresolved {
            return None;
        }
        if neighbour.rule != Rule::Stair {
            if neighbour.flags.contains(Flags::CORNER_STATE) {
                return None;
            }
            continue;
        }
        if neighbour.key & 4 != center.key & 4 {
            continue;
        }
        for side in 0..2 {
            if direction(neighbour) != u32::from(config[2 + side * 3]) {
                continue;
            }
            let blocker = read(placement_cell(position, config[blocker_index + side * 3]))?;
            if blocker.rule == Rule::Unresolved
                || (blocker.flags.contains(Flags::CORNER_STATE) && blocker.rule != Rule::Stair)
            {
                return None;
            }
            if blocker.rule != Rule::Stair
                || blocker.set != center.set
                || blocker.key & 7 != center.key & 7
            {
                corner = corners[side];
                break 'neighbours;
            }
        }
    }
    Some((center.key & 7) | (corner << 3))
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Creates one original stair key without parsing JSON or using installed carriers.
    fn stair(set: u32, direction: u32, half: u32) -> Facts {
        Facts {
            rule: Rule::Stair,
            set,
            key: [2, 1, 3, 0][direction as usize] | (half << 2),
            flags: Flags::CORNER_STATE,
            ..Default::default()
        }
    }

    #[test]
    fn connections_require_admitted_negative_neighbour_facts() {
        let center = Connection {
            family: ModelFamily::Pane,
            role: ContributorRole::Primary,
            occludes: false,
            orientation: None,
            nether: false,
        };
        let unknown = Connection {
            family: ModelFamily::Unknown,
            ..center
        };
        assert_eq!(connects(center, unknown, 2), None);
        assert_eq!(
            connects(
                center,
                Connection {
                    family: ModelFamily::Air,
                    ..unknown
                },
                2
            ),
            Some(false)
        );
        assert_eq!(
            connects(
                center,
                Connection {
                    occludes: true,
                    ..unknown
                },
                2
            ),
            Some(true)
        );
    }

    #[test]
    fn typed_corners_preserve_halves_and_same_identity_blockers() {
        let center = stair(1, 0, 0);
        let air = Facts {
            rule: Rule::Identity,
            set: 3,
            ..Default::default()
        };
        for blocked in [false, true] {
            let key = stair_key(center, [0; 3], |cell| {
                Some(if cell == placement_cell([0; 3], 3) {
                    stair(2, 3, 0)
                } else if blocked && cell == placement_cell([0; 3], 4) {
                    center
                } else {
                    air
                })
            })
            .unwrap();
            assert_eq!(key & !7, if blocked { 0 } else { 3 << 3 });
        }
        let key = stair_key(center, [0; 3], |cell| {
            Some(if cell == placement_cell([0; 3], 3) {
                stair(2, 3, 1)
            } else {
                air
            })
        })
        .unwrap();
        assert_eq!(key, center.key);
        assert_eq!(stair_key(center, [0; 3], |_| Some(Facts::default())), None);
        let mut unknown = air;
        unknown.flags = Flags::CORNER_STATE;
        assert_eq!(stair_key(center, [0; 3], |_| Some(unknown)), None);
    }
}
