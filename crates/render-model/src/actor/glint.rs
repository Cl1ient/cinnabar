//! Animated actor glint constants shared by armor and player cape textures.

/// Render-owned profile bit, above the authored material state bits.
pub const LEGACY_COMBAT_MATERIAL_FLAG: u32 = assets::EntityRenderMaterialState::DEPTH_ALWAYS << 1;

/// Converts legacy raster UV through rotation, offset and scale in that order.
pub fn legacy_glint_uv(uv: [f32; 2], offsets: [f32; 2], pass: usize) -> [f32; 2] {
    let rules = render_api::BEDROCK_112;
    let (sin, cos) = rules.glint_rotation_radians[pass].sin_cos();
    let [x, y] = uv.map(|component| component - 0.5);
    [
        (cos * x + sin * y + offsets[pass] / rules.glint_uv_scale[0] + 0.5)
            * rules.glint_uv_scale[0],
        (-sin * x + cos * y + 0.5) * rules.glint_uv_scale[1],
    ]
}

/// Accessibility factors and the actor glint clock, in seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActorGlint {
    pub profile: render_api::CombatVisuals,
    pub time_seconds: f32,
    pub strength: f32,
    pub speed: f32,
}

impl Default for ActorGlint {
    fn default() -> Self {
        Self {
            profile: Default::default(),
            time_seconds: 0.0,
            strength: 1.0,
            speed: 1.0,
        }
    }
}

impl ActorGlint {
    /// Native actor glint offsets, each wrapping at its own millisecond period.
    pub(crate) fn parameters(self) -> [f32; 3] {
        let time = if self.time_seconds.is_finite() {
            self.time_seconds.max(0.0)
        } else {
            0.0
        };
        let speed = if self.speed.is_finite() {
            self.speed.clamp(0.0, 1.0)
        } else {
            1.0
        };
        let strength = if self.strength.is_finite() {
            self.strength.clamp(0.0, 1.0)
        } else {
            1.0
        };
        let millis = ((time * 1000.0) as i32 as f32 * speed) as i32;
        let [first, second] = match self.profile {
            render_api::CombatVisuals::Modern => [1375, 3750],
            render_api::CombatVisuals::Legacy112 => render_api::BEDROCK_112.glint_period_ms,
        };
        [
            (millis % first) as f32 / -(first as f32),
            (millis % second) as f32 / second as f32,
            strength,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actor_glint_wraps_each_layer_and_scales_speed_after_millisecond_quantization() {
        let factors = ActorGlint {
            profile: render_api::CombatVisuals::Modern,
            time_seconds: 3.75,
            strength: 0.5,
            speed: 1.0,
        };
        assert_eq!(factors.parameters(), [-1000.0 / 1375.0, 0.0, 0.5]);
        let half_speed = ActorGlint {
            profile: render_api::CombatVisuals::Modern,
            time_seconds: 0.0039,
            speed: 0.5,
            ..Default::default()
        };
        assert_eq!(half_speed.parameters(), [-1.0 / 1375.0, 1.0 / 3750.0, 1.0]);
        assert_eq!(
            ActorGlint {
                time_seconds: f32::NAN,
                ..Default::default()
            }
            .parameters(),
            [0.0, 0.0, 1.0]
        );
    }
}
