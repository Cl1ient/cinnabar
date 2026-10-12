use super::{BlockFact, Policy, RESERVED, Result, Scalar, check, projection};
use assets::registry::{ModelStateField, MovementSemantics, RegistryRecord};
use assets::{BlockPhysicsFlags as Flags, BlockPhysicsRecord, SurfaceResponse};
use serde::Deserialize;
use std::collections::BTreeMap;

const SCALE: u32 = assets::block_shapes::COLLISION_UNITS_PER_BLOCK;
const DEFAULT_FRICTION: u32 = SCALE * 3 / 5;

/// Normalizes library float noise to the historical six-decimal fixed-point value.
fn friction(value: f64) -> Result<u32> {
    check(value.is_finite() && value > 0., "invalid friction scalar")?;
    let fixed = ((value * 1_000_000.).round() / 1_000_000. * SCALE as f64).round();
    check(
        fixed > 0. && fixed <= u32::MAX as f64,
        "friction scalar outside bounds",
    )?;
    Ok(fixed as u32)
}

/// Resolves fluid fill height from the inherited typed liquid-depth selector.
fn liquid_height(record: &RegistryRecord) -> i32 {
    let depth = record
        .model_state
        .get(ModelStateField::LiquidDepth)
        .unwrap_or(8);
    if depth >= 8 {
        SCALE as i32
    } else {
        ((8 - depth) * SCALE / 9) as i32
    }
}

/// Requires exactly one canonical byte-valued bubble direction.
fn bubble_down(state: &str) -> Result<bool> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Bubble {
        drag_down: Scalar,
    }
    let state: Bubble = serde_json::from_str(state)?;
    let value = state
        .drag_down
        .value
        .as_u64()
        .ok_or("drag_down fact must be byte 0 or 1")?;
    check(
        state.drag_down.kind == "byte" && value <= 1,
        "drag_down fact must be byte 0 or 1",
    )?;
    Ok(value == 1)
}

/// Compiles source collisions and friction with the client movement dispositions.
pub(super) fn compile(
    records: &[RegistryRecord],
    facts: &[BlockFact],
    policy: &Policy,
) -> Result<Vec<BlockPhysicsRecord>> {
    check(
        records.len() == facts.len(),
        "movement source count mismatch",
    )?;
    let mut counts = BTreeMap::new();
    let mut result = Vec::with_capacity(records.len());
    for (record, fact) in records.iter().zip(facts) {
        check(
            record.sequential_id == fact.sequential_id && record.network_hash == fact.network_hash,
            "movement source identity mismatch",
        )?;
        let mut entry = BlockPhysicsRecord {
            sequential_id: record.sequential_id,
            network_hash: record.network_hash,
            boxes: Box::default(),
            shapes: None,
            friction_q1e8: DEFAULT_FRICTION,
            horizontal_speed_q1e8: SCALE,
            vertical_speed_q1e8: SCALE,
            fluid_height_q1e8: 0,
            flags: Flags::empty(),
            surface_response: SurfaceResponse::None,
            movement_semantics: MovementSemantics::Unresolved,
        };
        if record.name.as_ref() == RESERVED {
            entry.flags = Flags::PASSABLE;
            result.push(entry);
            continue;
        }
        check(
            record.name.as_ref() == fact.name,
            "movement source name mismatch",
        )?;
        *counts.entry(fact.name.as_str()).or_insert(0usize) += 1;
        entry.boxes = projection::collision(fact)?.boxes;
        entry.friction_q1e8 = friction(fact.friction)?;
        if entry.boxes.is_empty() {
            entry.flags |= Flags::PASSABLE;
        }
        if let Some(rule) = policy.movement.iter().find(|rule| rule.name == fact.name) {
            entry.movement_semantics = MovementSemantics::Special;
            match rule.behavior.as_str() {
                "Water" | "Lava" => {
                    entry.flags |= if rule.behavior == "Water" {
                        Flags::WATER
                    } else {
                        Flags::LAVA
                    };
                    entry.flags |= Flags::PASSABLE;
                    entry.boxes = Box::default();
                    entry.fluid_height_q1e8 = liquid_height(record);
                }
                "Climbable" => entry.flags |= Flags::CLIMBABLE,
                "Cobweb" => {
                    entry.flags |= Flags::COBWEB | Flags::PASSABLE;
                    entry.horizontal_speed_q1e8 = SCALE / 4;
                    entry.vertical_speed_q1e8 = SCALE / 20;
                }
                "PowderSnow" => {
                    entry.flags |= Flags::POWDER_SNOW | Flags::PASSABLE;
                    entry.horizontal_speed_q1e8 = SCALE * 9 / 10;
                    entry.vertical_speed_q1e8 = SCALE * 3 / 2;
                }
                "Scaffolding" => entry.flags |= Flags::SCAFFOLDING,
                "Slime" => entry.surface_response = SurfaceResponse::Slime,
                "Honey" => entry.surface_response = SurfaceResponse::Honey,
                "InsideSlowdown" => {
                    entry.flags |= Flags::PASSABLE;
                    entry.horizontal_speed_q1e8 = SCALE * 4 / 5;
                    entry.vertical_speed_q1e8 = SCALE * 3 / 4;
                }
                "SoulSand" => {
                    entry.horizontal_speed_q1e8 = 54_300_000;
                    entry.surface_response = SurfaceResponse::SoulSand;
                }
                "Bubble" => {
                    entry.flags |= Flags::WATER | Flags::PASSABLE;
                    entry.boxes = Box::default();
                    entry.fluid_height_q1e8 = SCALE as i32;
                    entry.surface_response = if bubble_down(&record.canonical_state)? {
                        SurfaceResponse::BubbleDown
                    } else {
                        SurfaceResponse::BubbleUp
                    };
                }
                "Bed" => entry.surface_response = SurfaceResponse::Bed,
                unknown => return Err(format!("unknown movement rule {unknown}").into()),
            }
        }
        result.push(entry);
    }
    for rule in &policy.movement {
        check(
            counts.get(rule.name.as_str()).copied().unwrap_or(0) == rule.states,
            format!("movement state count mismatch for {}", rule.name),
        )?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_friction_preserves_float_noise() {
        assert_eq!(friction(0.6000000238418579).unwrap(), 60_000_000);
        assert!(friction(f64::NAN).is_err());
        assert!(friction(0.).is_err());
    }
    #[test]
    fn bubble_direction_requires_one_typed_byte() {
        for state in [
            r#"{}"#,
            r#"{"drag_down":{"type":"byte","value":2}}"#,
            r#"{"drag_down":{"type":"int","value":1}}"#,
            r#"{"drag_down":{"type":"byte","value":0},"drag_down":{"type":"byte","value":1}}"#,
        ] {
            assert!(bubble_down(state).is_err());
        }
        assert!(bubble_down(r#"{"drag_down":{"type":"byte","value":1}}"#).unwrap());
    }
}
