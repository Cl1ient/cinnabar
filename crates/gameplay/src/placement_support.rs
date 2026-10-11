//! Attachment state and support requirements for local block placement.

#[cfg(test)]
use crate::placement_state::PlacementInput;
#[cfg(test)]
use serde_json::{Map, Value};

/// A support face is the offset from the destination toward its supporting block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SupportRequirement {
    Independent,
    Face(u8),
    Unsupported,
}

/// Chooses attachment states only when every support consulted by vanilla is known.
#[cfg(test)]
pub(crate) fn attachment_states(
    identifier: &str,
    canonical: &Map<String, Value>,
    input: PlacementInput,
    supports: [Option<bool>; 6],
) -> Option<Map<String, Value>> {
    if input.face > 5 || !identifier.starts_with("minecraft:") {
        return None;
    }
    let mut states = canonical.clone();
    if identifier == "minecraft:vine" {
        return crate::placement_vines::attachment_states(canonical, input.face, supports);
    }
    if crate::placement_multiface::is_multiface(identifier) {
        return crate::placement_multiface::attachment_states(canonical, input.face, supports);
    }
    if is_torch(identifier) {
        if !known_keys(canonical, &["torch_facing_direction"]) {
            return None;
        }
        if !["unknown", "top", "north", "south", "west", "east"]
            .contains(&state_value(canonical, "torch_facing_direction")?.as_str()?)
        {
            return None;
        }
        let clicked_support = input.face ^ 1;
        let support = if input.face != 0 && supports[usize::from(clicked_support)]? {
            clicked_support
        } else {
            let mut selected = None;
            for face in [3, 4, 2, 5, 0] {
                if supports[face]? {
                    selected = Some(face as u8);
                    break;
                }
            }
            selected?
        };
        let name = match support {
            0 => "top",
            2 => "north",
            3 => "south",
            4 => "west",
            5 => "east",
            _ => return None,
        };
        set_state(&mut states, "torch_facing_direction", Value::from(name))?;
    } else if identifier == "minecraft:lever" {
        if !known_keys(canonical, &["lever_direction", "open_bit"])
            || state_bit(canonical, "open_bit").is_none()
            || ![
                "down_east_west",
                "east",
                "west",
                "south",
                "north",
                "up_north_south",
                "up_east_west",
                "down_north_south",
            ]
            .contains(&state_value(canonical, "lever_direction")?.as_str()?)
            || !supports[usize::from(input.face ^ 1)]?
        {
            return None;
        }
        let direction = match input.face {
            0 => {
                if crate::placement_state::yaw_quadrant(input.yaw)? & 1 == 0 {
                    "down_north_south"
                } else {
                    "down_east_west"
                }
            }
            1 => {
                if crate::placement_state::yaw_quadrant(input.yaw)? & 1 == 0 {
                    "up_north_south"
                } else {
                    "up_east_west"
                }
            }
            2 => "north",
            3 => "south",
            4 => "west",
            5 => "east",
            _ => return None,
        };
        crate::placement_state::set_value(&mut states, "lever_direction", Value::from(direction))?;
    } else if identifier.ends_with("_button") {
        if !known_keys(canonical, &["facing_direction", "button_pressed_bit"])
            || state_value(canonical, "facing_direction")?.as_u64()? > 5
            || state_bit(canonical, "button_pressed_bit").is_none()
        {
            return None;
        }
        if !supports[usize::from(input.face ^ 1)]? {
            return None;
        }
        set_state(&mut states, "facing_direction", Value::from(input.face))?;
        set_state(&mut states, "button_pressed_bit", Value::from(0))?;
    } else if is_lantern(identifier) {
        if !known_keys(canonical, &["hanging_bit"]) || state_bit(canonical, "hanging_bit").is_none()
        {
            return None;
        }
        let below = supports[0]?;
        let hanging = !below || (input.face == 0 && supports[1]?);
        if hanging && !supports[1]? {
            return None;
        }
        set_state(&mut states, "hanging_bit", Value::from(u8::from(hanging)))?;
    } else {
        return None;
    }
    Some(states)
}

/// Refuses additional state keys whose placement behavior has not been established.
#[cfg(test)]
fn known_keys(states: &Map<String, Value>, allowed: &[&str]) -> bool {
    states.len() == allowed.len() && states.keys().all(|key| allowed.contains(&key.as_str()))
}

/// Reads a compiled state disposition; unresolved states wait for the server.
pub(crate) fn support_requirement(rule: assets::registry::BlockSurvivalRule) -> SupportRequirement {
    use assets::registry::BlockSurvivalRule as Rule;
    match rule {
        Rule::Unresolved => SupportRequirement::Unsupported,
        Rule::Independent => SupportRequirement::Independent,
        Rule::Down | Rule::Up | Rule::North | Rule::South | Rule::West | Rule::East => {
            SupportRequirement::Face(rule as u8 - Rule::Down as u8)
        }
    }
}

/// Evaluates the admitted support policy independently from placement state or block names.
pub(crate) fn support_acceptance(
    policy: assets::registry::BlockSupportPolicy,
    support: assets::registry::BlockSupportKind,
    full_cube: bool,
) -> Option<bool> {
    use assets::registry::{BlockSupportKind as Kind, BlockSupportPolicy as Policy};
    match policy {
        Policy::Unresolved => None,
        Policy::NonAir => match support {
            Kind::Unresolved => None,
            Kind::Air => Some(false),
            _ => Some(true),
        },
        Policy::Vine => match support {
            Kind::Stone => Some(true),
            Kind::Air => Some(false),
            _ => None,
        },
        Policy::Snow => match support {
            Kind::Stone | Kind::Leaves => Some(true),
            Kind::Snow if full_cube => Some(true),
            Kind::Air | Kind::Fluid => Some(false),
            _ => None,
        },
        Policy::FullSolid => match support {
            Kind::Stone | Kind::FullCube if full_cube => Some(true),
            Kind::Air | Kind::Fluid | Kind::Leaves | Kind::PowderSnow | Kind::Rejected => {
                Some(false)
            }
            _ => None,
        },
    }
}

/// Changes the value while retaining the palette's serialized state type.
#[cfg(test)]
fn set_state(states: &mut Map<String, Value>, key: &str, value: Value) -> Option<()> {
    let entry = states.get_mut(key)?.as_object_mut()?;
    *entry.get_mut("value")? = value;
    Some(())
}

/// Reads the typed palette entry without accepting a malformed state envelope.
#[cfg(test)]
fn state_value<'a>(states: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    states.get(key)?.as_object()?.get("value")
}

/// Accepts boolean state entries and byte-encoded booleans from the vanilla palette.
#[cfg(test)]
fn state_bit(states: &Map<String, Value>, key: &str) -> Option<bool> {
    let value = state_value(states, key)?;
    value.as_bool().or_else(|| match value.as_u64()? {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    })
}

/// These variants share the wall and upright torch placement rule.
#[cfg(test)]
fn is_torch(identifier: &str) -> bool {
    matches!(
        identifier,
        "minecraft:torch"
            | "minecraft:soul_torch"
            | "minecraft:redstone_torch"
            | "minecraft:unlit_redstone_torch"
    )
}

/// Copper and soul variants keep lantern support and hanging-state behavior.
#[cfg(test)]
fn is_lantern(identifier: &str) -> bool {
    identifier == "minecraft:lantern"
        || identifier == "minecraft:soul_lantern"
        || identifier.ends_with("copper_lantern")
}

/// Ordinary candles share count and support rules; candle cakes use another placement path.
#[cfg(test)]
pub(crate) fn is_candle(identifier: &str) -> bool {
    identifier == "minecraft:candle"
        || (identifier.starts_with("minecraft:") && identifier.ends_with("_candle"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(test)]
    use serde_json::json;

    /// Provides one placement input with only the tested face differing.
    fn input(face: u8) -> PlacementInput {
        PlacementInput {
            face,
            click_position: [0.5; 3],
            yaw: 0.0,
            pitch: 0.0,
        }
    }

    /// Uses the same typed envelope as the block palette.
    fn torch_states() -> Map<String, Value> {
        serde_json::from_value(
            json!({"torch_facing_direction":{"type":"string","value":"unknown"}}),
        )
        .unwrap()
    }

    #[test]
    fn lever_faces_and_yaw_parity_preserve_the_open_bit() {
        let states = serde_json::from_value(serde_json::json!({
            "lever_direction":{"type":"string","value":"east"},
            "open_bit":{"type":"byte","value":1}
        }))
        .unwrap();
        for (yaw, expected) in [
            (
                0.0,
                [
                    "down_north_south",
                    "up_north_south",
                    "north",
                    "south",
                    "west",
                    "east",
                ],
            ),
            (
                90.0,
                [
                    "down_east_west",
                    "up_east_west",
                    "north",
                    "south",
                    "west",
                    "east",
                ],
            ),
            (
                180.0,
                [
                    "down_north_south",
                    "up_north_south",
                    "north",
                    "south",
                    "west",
                    "east",
                ],
            ),
            (
                -90.0,
                [
                    "down_east_west",
                    "up_east_west",
                    "north",
                    "south",
                    "west",
                    "east",
                ],
            ),
        ] {
            for (face, direction) in expected.into_iter().enumerate() {
                let mut click = input(face as u8);
                click.yaw = yaw;
                let placed =
                    attachment_states("minecraft:lever", &states, click, [Some(true); 6]).unwrap();
                assert_eq!(
                    state_value(&placed, "lever_direction"),
                    Some(&Value::from(direction))
                );
                assert_eq!(state_bit(&placed, "open_bit"), Some(true));
                let mut unsupported = [Some(true); 6];
                unsupported[face ^ 1] = Some(false);
                assert!(
                    attachment_states("minecraft:lever", &states, click, unsupported).is_none()
                );
            }
        }
    }

    #[test]
    fn torch_faces_point_toward_support() {
        for (face, expected, support) in [
            (1, "top", 0),
            (2, "south", 3),
            (3, "north", 2),
            (4, "east", 5),
            (5, "west", 4),
        ] {
            let mut supports = [Some(false); 6];
            supports[support] = Some(true);
            let states =
                attachment_states("minecraft:torch", &torch_states(), input(face), supports)
                    .unwrap();
            assert_eq!(
                state_value(&states, "torch_facing_direction"),
                Some(&Value::from(expected))
            );
        }
    }

    #[test]
    fn torch_fallback_uses_first_supported_wall_before_floor() {
        let supports = [
            Some(true),
            Some(false),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
        ];
        let states =
            attachment_states("minecraft:torch", &torch_states(), input(0), supports).unwrap();
        assert_eq!(
            state_value(&states, "torch_facing_direction"),
            Some(&Value::from("south"))
        );
        let unknown = [
            Some(true),
            Some(false),
            Some(true),
            None,
            Some(true),
            Some(true),
        ];
        assert!(attachment_states("minecraft:torch", &torch_states(), input(0), unknown).is_none());
    }

    #[test]
    fn invalid_torch_support_and_invalid_faces_do_not_predict() {
        for face in 0..=6 {
            assert!(
                attachment_states(
                    "minecraft:torch",
                    &torch_states(),
                    input(face),
                    [Some(false); 6]
                )
                .is_none()
            );
        }
    }

    #[test]
    fn buttons_require_the_opposite_support_face() {
        let states = serde_json::from_value(json!({"facing_direction":{"type":"int","value":0},"button_pressed_bit":{"type":"byte","value":1}})).unwrap();
        for face in 0..=5 {
            let mut supports = [Some(false); 6];
            supports[usize::from(face ^ 1)] = Some(true);
            let placed =
                attachment_states("minecraft:stone_button", &states, input(face), supports)
                    .unwrap();
            assert_eq!(
                state_value(&placed, "facing_direction"),
                Some(&Value::from(face))
            );
            assert_eq!(
                state_value(&placed, "button_pressed_bit"),
                Some(&Value::from(0))
            );
        }
    }

    #[test]
    fn lantern_prefers_floor_except_when_clicking_a_supported_ceiling() {
        let states =
            serde_json::from_value(json!({"hanging_bit":{"type":"byte","value":0}})).unwrap();
        for (below, above, face, hanging) in [
            (true, true, 1, false),
            (true, true, 0, true),
            (true, false, 0, false),
            (false, true, 1, true),
        ] {
            let mut supports = [Some(false); 6];
            supports[0] = Some(below);
            supports[1] = Some(above);
            let placed =
                attachment_states("minecraft:lantern", &states, input(face), supports).unwrap();
            assert_eq!(state_bit(&placed, "hanging_bit"), Some(hanging));
        }
        assert!(
            attachment_states("minecraft:lantern", &states, input(1), [Some(false); 6]).is_none()
        );
    }

    #[test]
    fn typed_support_policies_distinguish_unknown_air_and_partial_support() {
        use assets::registry::{BlockSupportKind as Kind, BlockSupportPolicy as Policy};
        assert_eq!(
            support_acceptance(Policy::NonAir, Kind::Fluid, false),
            Some(true)
        );
        assert_eq!(
            support_acceptance(Policy::NonAir, Kind::Unresolved, false),
            None
        );
        assert_eq!(
            support_acceptance(Policy::FullSolid, Kind::Leaves, true),
            Some(false)
        );
        assert_eq!(
            support_acceptance(Policy::FullSolid, Kind::Stone, true),
            Some(true)
        );
        assert_eq!(
            support_acceptance(Policy::FullSolid, Kind::Unresolved, false),
            None
        );
    }

    #[test]
    fn unadmitted_survival_family_waits_for_the_server() {
        assert_eq!(
            support_requirement(assets::registry::BlockSurvivalRule::Unresolved),
            SupportRequirement::Unsupported
        );
    }
}
