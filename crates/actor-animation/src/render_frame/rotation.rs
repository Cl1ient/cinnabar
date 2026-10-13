use super::*;

/// Rotation queries read the same partial tick before relative yaw is clamped.
pub(super) fn is_query(assets: &RuntimeEntityAssets, symbol: u32) -> bool {
    use assets::MolangQuery as Q;
    matches!(
        assets.molang_query_binding(symbol),
        Some(
            Q::BodyYRotation
                | Q::BodyXRotation
                | Q::TargetYRotation
                | Q::TargetXRotation
                | Q::HeadYRotation
                | Q::HeadXRotation
        )
    )
}

impl ActorTickInput {
    /// Samples each angle along its shortest arc while retaining this endpoint's other inputs.
    pub(super) fn sample_rotation(self, previous: Self, current: Self, alpha: f32) -> Self {
        let sample = |from, to| from + query::wrap_degrees(to - from) * alpha;
        Self {
            body_yaw: sample(previous.body_yaw, current.body_yaw),
            yaw: sample(previous.yaw, current.yaw),
            head_yaw: sample(previous.head_yaw, current.head_yaw),
            pitch: sample(previous.pitch, current.pitch),
            ..self
        }
    }

    /// Whether the two endpoints need a fresh rotation sample between ticks.
    pub(super) fn rotation_changed(self, other: Self) -> bool {
        self.body_yaw != other.body_yaw
            || self.yaw != other.yaw
            || self.head_yaw != other.head_yaw
            || self.pitch != other.pitch
    }
}
