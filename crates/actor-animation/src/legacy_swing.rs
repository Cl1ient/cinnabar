//! Ordinary player attacks, composed in model space so children share their parent's grip.
use super::*;
use render_api::{BEDROCK_112, CombatVisuals};
use render_model::legacy_combat::legacy_swing_pose;

#[cfg(test)]
mod tests;

/// Leaves item-use and special movement postures on their authored animation path.
pub(super) fn ordinary(actor: &dyn AnimatedActor, context: &ActorTickContext, swim: f32) -> bool {
    context.combat_visuals == CombatVisuals::Legacy112
        && matches!(actor.kind(), ActorKind::Player { .. })
        && context.attachable.is_none()
        && !actor.is_using_item()
        && !actor.is_sleeping()
        && swim <= 0.0
        && !context.main_hand_is_spear
        && ![
            query::FLAG_SWIMMING,
            query::FLAG_BLOCKING,
            query::FLAG_GLIDING,
            query::FLAG_CRAWLING,
            query::FLAG_EMOTING,
        ]
        .into_iter()
        .any(|flag| query::actor_flag(actor, flag))
        && !context.main_hand.as_deref().is_some_and(|item| {
            matches!(
                item,
                "minecraft:bow" | "minecraft:crossbow" | "minecraft:filled_map" | "minecraft:map"
            )
        })
        && !context
            .off_hand
            .as_deref()
            .is_some_and(|item| matches!(item, "minecraft:filled_map" | "minecraft:map"))
}

/// Replaces only attack deltas, preserving the baseline walk, head and special skin transforms.
pub(super) fn apply(
    bones: &[RuntimeBone],
    names: &[Box<str>],
    local: &mut [pose::LocalDelta],
    progress: f32,
    head_pitch: f32,
    weight: f32,
) -> Result<(), EvalError> {
    let base = pose::compose_pose(bones, local).ok_or(EvalError::Invalid)?;
    let swing = legacy_swing_pose(progress, head_pitch);
    for (index, name) in names.iter().enumerate() {
        let (rotation, offset) = match name.as_ref() {
            "body" => ([0.0, swing.body_yaw, 0.0], [0.0; 3]),
            "rightarm" => (
                swing.right_rotation.to_array(),
                [
                    swing.right_shoulder.x - BEDROCK_112.swing_shoulder_pixels,
                    0.0,
                    swing.right_shoulder.z,
                ],
            ),
            "leftarm" => (
                swing.left_rotation.to_array(),
                [
                    swing.left_shoulder.x + BEDROCK_112.swing_shoulder_pixels,
                    0.0,
                    swing.left_shoulder.z,
                ],
            ),
            // A body-parented head keeps its original look instead of inheriting the attack twist.
            "head" => ([0.0; 3], [0.0; 3]),
            _ => continue,
        };
        let mut target = *base.get(index).ok_or(EvalError::Invalid)?;
        if name.as_ref() != "head" {
            let [x, y, z, w] = target.rotation;
            let angles = [
                (2.0 * (w * x + y * z)).atan2(1.0 - 2.0 * (x * x + y * y)),
                (2.0 * (w * y - z * x)).clamp(-1.0, 1.0).asin(),
                (2.0 * (w * z + x * y)).atan2(1.0 - 2.0 * (y * y + z * z)),
            ];
            target.rotation = pose::quat_from_euler(std::array::from_fn(|axis| {
                (angles[axis] + rotation[axis] * weight * if axis == 2 { 1.0 } else { -1.0 })
                    .to_degrees()
            }));
        }
        for axis in 0..3 {
            target.translation_scale[axis] += offset[axis] * weight;
        }
        local[index].model_target = Some(target);
    }
    Ok(())
}
