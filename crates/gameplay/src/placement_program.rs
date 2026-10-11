//! Click-dependent numerical state keys; every output must select a compiled registered state.

use crate::{block_use::BoxBounds, placement_state::PlacementInput};
use assets::registry::{BlockPlacementFacts as Facts, BlockPlacementRule as Rule};

/// Quantizes finite yaw to the compatibility quarter-turn convention.
pub(crate) fn quadrant(yaw: f32) -> Option<u32> {
    yaw.is_finite()
        .then(|| (yaw / 90.0 + 0.5).floor().rem_euclid(4.0) as u32)
}

/// Resolves top/bottom clicks directly and side clicks using a strict finite midpoint.
fn upper(input: PlacementInput) -> Option<u32> {
    match input.face {
        0 => Some(1),
        1 => Some(0),
        2..=5 if input.click_position[1].is_finite() => {
            Some(u32::from(input.click_position[1] > 0.5))
        }
        _ => None,
    }
}

/// Selects a body-facing direction using the admitted bounds and per-rule yaw rotation.
fn body_facing(position: [i32; 3], player: BoxBounds, yaw: f32, rotation: f32) -> Option<u32> {
    if player
        .0
        .into_iter()
        .chain(player.1)
        .any(|value| !value.is_finite())
    {
        return None;
    }
    let nearby = [0, 2].into_iter().all(|axis| {
        (((player.0[axis] + player.1[axis]) * 0.5).floor() - f64::from(position[axis])).abs() < 2.0
    });
    if nearby {
        if f64::from(position[1]) < player.0[1] {
            return Some(1);
        }
        if player.1[1] < f64::from(position[1]) {
            return Some(0);
        }
    }
    Some([2, 5, 3, 4][quadrant(yaw + rotation)? as usize])
}

/// Chooses a first attachment while preserving the existing order of consulted support facts.
fn attachment(
    rule: Rule,
    key: u32,
    input: PlacementInput,
    supports: [Option<bool>; 6],
) -> Option<u32> {
    let opposite = usize::from(input.face ^ 1);
    Some(match rule {
        Rule::Torch => {
            let support = if input.face != 0 && supports[opposite]? {
                opposite
            } else {
                let mut selected = None;
                for face in [3, 4, 2, 5, 0] {
                    if supports[face]? {
                        selected = Some(face);
                        break;
                    }
                }
                selected?
            };
            match support {
                0 => 1,
                2..=5 => support as u32,
                _ => return None,
            }
        }
        Rule::Button => {
            if !supports[opposite]? {
                return None;
            }
            u32::from(input.face)
        }
        Rule::Lever => {
            if !supports[opposite]? {
                return None;
            }
            let orientation = match input.face {
                0 => {
                    if quadrant(input.yaw)? & 1 == 0 {
                        1
                    } else {
                        0
                    }
                }
                1 => {
                    if quadrant(input.yaw)? & 1 == 0 {
                        3
                    } else {
                        2
                    }
                }
                2..=5 => u32::from(input.face) + 2,
                _ => return None,
            };
            orientation | (key & 8)
        }
        Rule::Lantern => {
            let hanging = !supports[0]? || (input.face == 0 && supports[1]?);
            if hanging && !supports[1]? {
                return None;
            }
            u32::from(hanging)
        }
        Rule::Vine => {
            if !(2..=5).contains(&input.face) || !supports[opposite]? {
                return None;
            }
            [1, 4, 8, 2][usize::from(input.face - 2)]
        }
        Rule::Multiface => {
            let mut selected = None;
            for face in std::iter::once(opposite).chain(
                [
                    [1, 2, 3, 4, 5],
                    [0, 2, 3, 4, 5],
                    [1, 0, 3, 4, 5],
                    [1, 0, 2, 4, 5],
                    [1, 0, 2, 3, 5],
                    [1, 0, 2, 3, 4],
                ][opposite],
            ) {
                if supports[face]? {
                    selected = Some(face);
                    break;
                }
            }
            [1, 2, 16, 4, 8, 32][selected?]
        }
        _ => return None,
    })
}

/// Computes one provisional state transformation using only admitted numerical facts.
pub(crate) fn placed(
    facts: Facts,
    input: PlacementInput,
    position: [i32; 3],
    player: BoxBounds,
    supports: [Option<bool>; 6],
) -> Option<(u32, u32)> {
    if input.face > 5 {
        return None;
    }
    let mut set = facts.set;
    let key = match facts.rule {
        Rule::Unresolved | Rule::DoubleSlab => return None,
        Rule::Identity => facts.key,
        Rule::Axis => [1, 1, 2, 2, 0, 0][usize::from(input.face)],
        Rule::Loom => [2, 3, 0, 1][quadrant(input.yaw)? as usize],
        Rule::Glazed => [2, 5, 3, 4][quadrant(input.yaw)? as usize],
        Rule::Stair => {
            [2, 1, 3, 0][quadrant(input.yaw)? as usize] | ((facts.key & 4) | (upper(input)? << 2))
        }
        Rule::Slab => upper(input)?,
        Rule::Trapdoor => {
            [3, 0, 2, 1][quadrant(input.yaw)? as usize] | (facts.key & 4) | (upper(input)? << 3)
        }
        Rule::Hopper => {
            if input.face < 2 {
                0
            } else {
                u32::from(input.face ^ 1)
            }
        }
        Rule::Connections | Rule::Snow | Rule::Candle => 0,
        Rule::SeaPickle => 4,
        Rule::Torch | Rule::Button | Rule::Lever | Rule::Lantern | Rule::Vine | Rule::Multiface => {
            attachment(facts.rule, facts.key, input, supports)?
        }
        Rule::Door => (quadrant(input.yaw)? + 1) & 3 | (facts.key & 4),
        Rule::StandingSign | Rule::WallSign => {
            if input.face == 0 {
                return None;
            }
            if input.face == 1 {
                if facts.rule == Rule::WallSign {
                    set = facts.alternate?;
                }
                if !input.yaw.is_finite() {
                    return None;
                }
                let rotation = ((input.yaw + 180.0) * 16.0 / 360.0 + 0.5).floor();
                if !rotation.is_finite() {
                    return None;
                }
                rotation.rem_euclid(16.0) as u32
            } else {
                if facts.rule == Rule::StandingSign {
                    set = facts.alternate?;
                }
                u32::from(input.face)
            }
        }
        Rule::Dispenser => body_facing(position, player, input.yaw, 0.0)?,
        Rule::Piston => body_facing(position, player, input.yaw, 180.0)?,
    };
    Some((set, key))
}

/// Merges only matching admitted identities; its result still requires an existing target state.
pub(crate) fn merged(
    held: Facts,
    existing: Facts,
    face: Option<u8>,
    clicked_face: u8,
) -> Option<(u32, u32, bool)> {
    if held.set == 0 || held.set != existing.set || held.rule != existing.rule {
        return None;
    }
    match held.rule {
        Rule::Slab if face.is_none_or(|face| face == u8::from(existing.key == 0)) => {
            Some((held.alternate?, 0, false))
        }
        Rule::SeaPickle
            if (1..=5).contains(&face.unwrap_or(clicked_face)) && existing.key & 3 < 3 =>
        {
            Some((held.set, ((existing.key & 3) + 1) | 4, false))
        }
        Rule::Candle if (1..=5).contains(&face.unwrap_or(clicked_face)) && existing.key & 3 < 3 => {
            Some((held.set, existing.key + 1, false))
        }
        Rule::Snow if face.is_some_and(|face| face <= 5) && existing.key & 7 < 7 => {
            Some((held.set, existing.key + 1, true))
        }
        _ => None,
    }
}

/// Water-sensitive compatibility rules defer when either liquid layer is present.
pub(crate) fn water_sensitive(rule: Rule) -> bool {
    matches!(
        rule,
        Rule::Candle | Rule::SeaPickle | Rule::Vine | Rule::Multiface
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Uses a typed original program whose set identity is independent from its rule.
    fn facts(rule: Rule, key: u32) -> Facts {
        Facts {
            rule,
            set: 1,
            key,
            alternate: Some(2),
            ..Default::default()
        }
    }
    /// Provides a finite side click for the state transformations under test.
    fn input(face: u8, yaw: f32) -> PlacementInput {
        PlacementInput {
            face,
            yaw,
            pitch: 0.0,
            click_position: [0.5; 3],
        }
    }

    #[test]
    fn typed_placement_preserves_axis_half_open_and_attachment_dispositions() {
        let player = ([0.0; 3], [1.0; 3]);
        for face in 0..6 {
            assert_eq!(
                placed(
                    facts(Rule::Axis, 0),
                    input(face, 0.0),
                    [0; 3],
                    player,
                    [Some(true); 6]
                ),
                Some((1, [1, 1, 2, 2, 0, 0][face as usize]))
            );
        }
        assert_eq!(
            placed(
                facts(Rule::Trapdoor, 4),
                input(0, 90.0),
                [0; 3],
                player,
                [Some(true); 6]
            ),
            Some((1, 12))
        );
        assert_eq!(
            placed(
                facts(Rule::Torch, 0),
                input(2, 0.0),
                [0; 3],
                player,
                [Some(true); 6]
            ),
            Some((1, 3))
        );
        assert_eq!(
            placed(
                facts(Rule::Unresolved, 0),
                input(1, 0.0),
                [0; 3],
                player,
                [Some(true); 6]
            ),
            None
        );
        assert_eq!(
            placed(
                facts(Rule::Vine, 0),
                input(1, 0.0),
                [0; 3],
                player,
                [Some(true); 6]
            ),
            None
        );
    }

    #[test]
    fn typed_stack_and_slab_merges_preserve_count_and_matching_identity_limits() {
        let slab = facts(Rule::Slab, 0);
        assert_eq!(merged(slab, slab, Some(1), 1), Some((2, 0, false)));
        assert_eq!(merged(slab, slab, Some(0), 0), None);
        assert_eq!(
            merged(facts(Rule::Candle, 0), facts(Rule::Candle, 3), Some(1), 1),
            None
        );
        assert_eq!(
            merged(facts(Rule::Candle, 0), facts(Rule::Candle, 4), Some(1), 1),
            Some((1, 5, false))
        );
        assert_eq!(
            merged(facts(Rule::Snow, 0), facts(Rule::Snow, 14), Some(1), 1),
            Some((1, 15, true))
        );
        let mut different = slab;
        different.set = 2;
        assert_eq!(merged(slab, different, Some(1), 1), None);
    }
}
