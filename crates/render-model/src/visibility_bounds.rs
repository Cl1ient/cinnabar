//! Conservative clip-space tests shared by entity preparation and draw submission.

use glam::{Mat4, Vec3, Vec4};

/// The eight corners of finite, ordered world bounds under a finite clip transform.
fn corners(matrix: Mat4, low: [f32; 3], high: [f32; 3]) -> Option<[Vec4; 8]> {
    let determinant = matrix.determinant();
    if !determinant.is_finite()
        || determinant == 0.0
        || !matrix.is_finite()
        || low.iter().chain(&high).any(|x| !x.is_finite())
        || (0..3).any(|a| low[a] > high[a])
    {
        return None;
    }
    let corners: [Vec4; 8] = std::array::from_fn(|i| {
        matrix
            * Vec3::from_array(std::array::from_fn(|a| {
                if i & (1 << a) == 0 { low[a] } else { high[a] }
            }))
            .extend(1.0)
    });
    corners.iter().all(|p| p.is_finite()).then_some(corners)
}

/// Whether any part of a box can intersect the view. Uncertain bounds always draw.
pub fn box_intersects_view(matrix: Mat4, low: [f32; 3], high: [f32; 3]) -> bool {
    let Some(p) = corners(matrix, low, high) else {
        return true;
    };
    !(p.iter().all(|p| p.x < -p.w)
        || p.iter().all(|p| p.x > p.w)
        || p.iter().all(|p| p.y < -p.w)
        || p.iter().all(|p| p.y > p.w)
        || p.iter().all(|p| p.z < 0.0)
        || p.iter().all(|p| p.z > p.w)
        || p.iter().all(|p| p.w <= 0.0))
}

/// Whether `eye` is the origin of perspective rays for this projection. Orthographic
/// and uncertain transforms cannot use proofs projected from a single camera point.
pub fn perspective_eye_matches(matrix: Mat4, eye: [f32; 3]) -> bool {
    let eye = Vec3::from_array(eye);
    let plane = matrix.row(3);
    matrix.is_finite()
        && eye.is_finite()
        && plane.truncate().length_squared() > f32::EPSILON
        && (matrix * eye.extend(1.0)).w.abs() < 0.0001
}

/// Whether a full cube is strictly inside the depth clip interval. A clipped occluder
/// cannot prove another object's invisibility, even when its terrain occupancy is opaque.
pub fn cell_is_inside_depth(matrix: Mat4, cell: [i32; 3]) -> bool {
    let low = cell.map(|x| x as f32);
    let high = cell.map(|x| x as f32 + 1.0);
    corners(matrix, low, high)
        .is_some_and(|p| p.iter().all(|p| p.w > 0.0 && p.z > 0.0 && p.z < p.w))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_eye_proofs_require_a_matching_perspective_origin() {
        assert!(!perspective_eye_matches(Mat4::IDENTITY, [0.0; 3]));
        let projection = Mat4::perspective_infinite_reverse_rh(1.0, 1.0, 0.1);
        assert!(perspective_eye_matches(projection, [0.0; 3]));
        assert!(!perspective_eye_matches(projection, [0.0, 0.0, 2.0]));
    }

    #[test]
    fn clipped_or_uncertain_cells_cannot_occlude() {
        assert!(!cell_is_inside_depth(Mat4::IDENTITY, [0; 3]));
        assert!(!cell_is_inside_depth(Mat4::ZERO, [0; 3]));
        assert!(box_intersects_view(Mat4::ZERO, [f32::NAN; 3], [0.0; 3]));
    }
}
