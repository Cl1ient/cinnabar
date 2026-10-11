//! Body-bound and per-type direction rules for vanilla placement.

use crate::{block_use::BoxBounds, placement_state::yaw_quadrant};

/// Nearby vertical placements use strict body bounds; other placements use the yaw table.
fn body_facing(position: [i32; 3], player: BoxBounds, yaw: f32, rotation: f32) -> Option<u8> {
    if player.0.into_iter().chain(player.1).any(|v| !v.is_finite()) {
        return None;
    }
    let nearby = [0, 2].into_iter().all(|axis| {
        let coordinate = ((player.0[axis] + player.1[axis]) * 0.5).floor();
        (coordinate - f64::from(position[axis])).abs() < 2.0
    });
    let height = f64::from(position[1]);
    if nearby {
        if height < player.0[1] {
            return Some(1);
        }
        if player.1[1] < height {
            return Some(0);
        }
    }
    Some([2, 5, 3, 4][yaw_quadrant(yaw + rotation)?])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn six_faces_use_body_proximity_and_per_type_rotation() {
        let player = ([0.2, 4.0, 0.2], [0.8, 5.8, 0.8]);
        for (position, expected) in [
            ([0, 3, 0], 1),
            ([0, 6, 0], 0),
            ([0, 4, 0], 2),
            ([0, 5, 0], 2),
            ([2, 3, 0], 2),
            ([-2, 6, 0], 2),
        ] {
            assert_eq!(body_facing(position, player, 0.0, 0.0), Some(expected));
        }
        for (yaw, dispenser, piston) in [(0.0, 2, 3), (90.0, 5, 4), (180.0, 3, 2), (-90.0, 4, 5)] {
            assert_eq!(body_facing([4, 4, 0], player, yaw, 0.0), Some(dispenser));
            assert_eq!(body_facing([4, 4, 0], player, yaw, 180.0), Some(piston));
        }
    }

    #[test]
    fn upper_body_boundary_is_horizontal_and_nonfinite_bounds_defer() {
        let player = ([0.2, 4.0, 0.2], [0.8, 6.0, 0.8]);
        assert_eq!(body_facing([0, 6, 0], player, 0.0, 0.0), Some(2));
        assert_eq!(body_facing([0, 7, 0], player, 0.0, 0.0), Some(0));
        let invalid = ([f64::NAN, 4.0, 0.2], player.1);
        assert_eq!(body_facing([0, 7, 0], invalid, 0.0, 0.0), None);
    }
}
