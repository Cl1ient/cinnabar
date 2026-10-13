//! Provisional placement programs compiled from canonical typed selectors.

use super::{Result, Scalar, check, survival};
use assets::registry::{
    BlockPlacementFacts as Facts, BlockPlacementFlags as Flags, BlockPlacementRule as Rule,
    COMPATIBILITY_REPLACEABLE_BLOCKS, ModelFamily, RegistryRecord,
};
use std::collections::{BTreeMap, BTreeSet};

struct State(BTreeMap<String, Scalar>);

impl State {
    /// Accepts canonical integer and byte values inside the requested domain.
    fn number(&self, name: &str, maximum: u32) -> Option<u32> {
        let field = self.0.get(name)?;
        let value = field.value.as_f64()?;
        (matches!(field.kind.as_str(), "int" | "byte")
            && value >= 0.0
            && value <= f64::from(maximum)
            && value.fract() == 0.0)
            .then_some(value as u32)
    }

    /// Maps a canonical string to its bounded numerical selector.
    fn text(&self, name: &str, values: &[&str]) -> Option<u32> {
        let field = self.0.get(name)?;
        if field.kind != "string" {
            return None;
        }
        let value = field.value.as_str()?;
        values
            .iter()
            .position(|candidate| *candidate == value)
            .map(|index| index as u32)
    }

    /// Refuses extra state fields whose effects have no program admission.
    fn only(&self, keys: &[&str]) -> bool {
        self.0.len() == keys.len() && keys.iter().all(|key| self.0.contains_key(*key))
    }
}

/// Combines validated selectors in their non-overlapping bit ranges.
fn packed(rule: Rule, values: &[(Option<u32>, u32)]) -> Option<(Rule, u32)> {
    let mut key = 0;
    for &(value, shift) in values {
        key |= value? << shift;
    }
    Some((rule, key))
}

/// Retains the supported compatibility transformations without admitting extra fields.
fn program(record: &RegistryRecord, s: &State) -> Option<(Rule, u32)> {
    let name = record.name.as_ref();
    if !name.starts_with("minecraft:") {
        return None;
    }
    let number = |name, maximum, shift| (s.number(name, maximum), shift);
    let text = |name, values: &[&str], shift| (s.text(name, values), shift);
    match name {
        _ if s.0.is_empty() => Some((Rule::Identity, 0)),
        _ if name.ends_with("_stairs")
            && s.only(&["weirdo_direction", "upside_down_bit", "minecraft:corner"]) =>
        {
            packed(
                Rule::Stair,
                &[
                    number("weirdo_direction", 3, 0),
                    number("upside_down_bit", 1, 2),
                    text(
                        "minecraft:corner",
                        &[
                            "none",
                            "inner_left",
                            "inner_right",
                            "outer_left",
                            "outer_right",
                        ],
                        3,
                    ),
                ],
            )
        }
        "minecraft:loom" if s.only(&["direction"]) => {
            packed(Rule::Loom, &[number("direction", 3, 0)])
        }
        _ if name.ends_with("_glazed_terracotta") && s.only(&["facing_direction"]) => {
            packed(Rule::Glazed, &[number("facing_direction", 5, 0)])
        }
        "minecraft:snow_layer" if s.only(&["height", "covered_bit"]) => packed(
            Rule::Snow,
            &[number("height", 7, 0), number("covered_bit", 1, 3)],
        ),
        "minecraft:sea_pickle" if s.only(&["cluster_count", "dead_bit"]) => packed(
            Rule::SeaPickle,
            &[number("cluster_count", 3, 0), number("dead_bit", 1, 2)],
        ),
        _ if (name == "minecraft:candle" || name.ends_with("_candle"))
            && s.only(&["candles", "lit"]) =>
        {
            packed(
                Rule::Candle,
                &[number("candles", 3, 0), number("lit", 1, 2)],
            )
        }
        _ if s.only(&["pillar_axis"]) => {
            packed(Rule::Axis, &[text("pillar_axis", &["x", "y", "z"], 0)])
        }
        _ if name.ends_with("_slab") && s.only(&["minecraft:vertical_half"]) => packed(
            if name.contains("double_") {
                Rule::DoubleSlab
            } else {
                Rule::Slab
            },
            &[text("minecraft:vertical_half", &["bottom", "top"], 0)],
        ),
        _ if (name == "minecraft:trapdoor" || name.ends_with("_trapdoor"))
            && s.only(&["direction", "open_bit", "upside_down_bit"]) =>
        {
            packed(
                Rule::Trapdoor,
                &[
                    number("direction", 3, 0),
                    number("open_bit", 1, 2),
                    number("upside_down_bit", 1, 3),
                ],
            )
        }
        "minecraft:hopper" if s.only(&["facing_direction", "toggle_bit"]) => packed(
            Rule::Hopper,
            &[
                (
                    s.number("facing_direction", 5).filter(|value| *value != 1),
                    0,
                ),
                number("toggle_bit", 1, 3),
            ],
        ),
        _ if (name.ends_with("_fence")
            || name.ends_with("_pane")
            || name == "minecraft:iron_bars")
            && s.only(&[
                "minecraft:connection_north",
                "minecraft:connection_south",
                "minecraft:connection_west",
                "minecraft:connection_east",
            ]) =>
        {
            packed(
                Rule::Connections,
                &[
                    number("minecraft:connection_north", 1, 0),
                    number("minecraft:connection_south", 1, 1),
                    number("minecraft:connection_west", 1, 2),
                    number("minecraft:connection_east", 1, 3),
                ],
            )
        }
        _ if record.model_family == ModelFamily::Torch && s.only(&["torch_facing_direction"]) => {
            packed(
                Rule::Torch,
                &[text(
                    "torch_facing_direction",
                    &["unknown", "top", "north", "south", "west", "east"],
                    0,
                )],
            )
        }
        "minecraft:lever" if s.only(&["lever_direction", "open_bit"]) => packed(
            Rule::Lever,
            &[
                text(
                    "lever_direction",
                    &[
                        "down_east_west",
                        "down_north_south",
                        "up_east_west",
                        "up_north_south",
                        "north",
                        "south",
                        "west",
                        "east",
                    ],
                    0,
                ),
                number("open_bit", 1, 3),
            ],
        ),
        _ if (name == "minecraft:lantern"
            || name == "minecraft:soul_lantern"
            || name.ends_with("copper_lantern"))
            && s.only(&["hanging_bit"]) =>
        {
            packed(Rule::Lantern, &[number("hanging_bit", 1, 0)])
        }
        _ if record.model_family == ModelFamily::Button
            && s.only(&["facing_direction", "button_pressed_bit"]) =>
        {
            packed(
                Rule::Button,
                &[
                    number("facing_direction", 5, 0),
                    number("button_pressed_bit", 1, 3),
                ],
            )
        }
        "minecraft:vine" if s.only(&["vine_direction_bits"]) => {
            packed(Rule::Vine, &[number("vine_direction_bits", 15, 0)])
        }
        "minecraft:glow_lichen" | "minecraft:sculk_vein"
            if s.only(&["multi_face_direction_bits"]) =>
        {
            packed(
                Rule::Multiface,
                &[number("multi_face_direction_bits", 63, 0)],
            )
        }
        _ if name.ends_with("_door")
            && s.only(&[
                "minecraft:cardinal_direction",
                "open_bit",
                "upper_block_bit",
                "door_hinge_bit",
            ]) =>
        {
            packed(
                Rule::Door,
                &[
                    text(
                        "minecraft:cardinal_direction",
                        &["south", "west", "north", "east"],
                        0,
                    ),
                    number("open_bit", 1, 2),
                    number("upper_block_bit", 1, 3),
                    number("door_hinge_bit", 1, 4),
                ],
            )
        }
        _ if name.ends_with("standing_sign") && s.only(&["ground_sign_direction"]) => packed(
            Rule::StandingSign,
            &[number("ground_sign_direction", 15, 0)],
        ),
        _ if name.ends_with("wall_sign") && s.only(&["facing_direction"]) => packed(
            Rule::WallSign,
            &[(
                s.number("facing_direction", 5).filter(|value| *value >= 2),
                0,
            )],
        ),
        "minecraft:dispenser" if s.only(&["facing_direction", "triggered_bit"]) => packed(
            Rule::Dispenser,
            &[
                number("facing_direction", 5, 0),
                number("triggered_bit", 1, 3),
            ],
        ),
        "minecraft:piston" | "minecraft:sticky_piston" if s.only(&["facing_direction"]) => {
            packed(Rule::Piston, &[number("facing_direction", 5, 0)])
        }
        _ => None,
    }
}

/// Finds only an existing alternate identity for a compatible collected state set.
fn alternate(name: &str, rule: Rule, names: &BTreeMap<String, u32>) -> Option<u32> {
    let target = match rule {
        Rule::Slab => {
            if let Some(prefix) = name.strip_suffix("cut_copper_slab") {
                format!("{prefix}double_cut_copper_slab")
            } else {
                format!("{}_double_slab", name.strip_suffix("_slab")?)
            }
        }
        Rule::StandingSign => format!("{}wall_sign", name.strip_suffix("standing_sign")?),
        Rule::WallSign => format!("{}standing_sign", name.strip_suffix("wall_sign")?),
        _ => return None,
    };
    names.get(&target).copied()
}

/// Assigns projection-local numerical sets and recomputes support after projection.
pub(super) fn compile(records: &mut [RegistryRecord]) -> Result<()> {
    let names: BTreeMap<_, _> = records
        .iter()
        .map(|record| record.name.as_ref())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .enumerate()
        .map(|(index, name)| (name.to_owned(), index as u32 + 1))
        .collect();
    let mut keys = BTreeSet::new();
    for record in records {
        let name = record.name.as_ref();
        let mut facts = Facts::default();
        if COMPATIBILITY_REPLACEABLE_BLOCKS.contains(&name) {
            facts.flags |= Flags::REPLACEABLE;
        }
        if name == "minecraft:nether_brick_fence" {
            facts.flags |= Flags::NETHER_FENCE;
        }
        if let Ok(state) = serde_json::from_str::<BTreeMap<String, Scalar>>(&record.canonical_state)
        {
            let state = State(state);
            if state.0.contains_key("minecraft:corner") {
                facts.flags |= Flags::CORNER_STATE;
            }
            if let Some((rule, key)) = program(record, &state) {
                facts.rule = rule;
                facts.key = key;
                facts.set = names[name];
                check(
                    keys.insert((facts.set, key)),
                    format!("duplicate compiled placement state for {name}"),
                )?;
                facts.alternate = alternate(name, rule, &names);
                if matches!(rule, Rule::Lever | Rule::Button)
                    || (rule == Rule::Trapdoor && name != "minecraft:iron_trapdoor")
                {
                    facts.flags |= Flags::HAND_TOGGLE;
                }
            }
        }
        record.model_state.set_placement_facts(facts);
        let support = survival::compile(record);
        record.model_state.set_survival_facts(support);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
