use super::ItemMeshVertex;
use glam::{Mat4, Vec3, Vec4};

/// Covers rounding in the products and sums used by affine and clip-space transforms.
const ROUNDING_ALLOWANCE: f32 = 8.0 * f32::EPSILON;

/// Bounds of the actual uploaded item mesh, in model or world coordinates.
#[derive(Clone, Copy, Debug)]
pub struct DroppedItemBounds {
    min: Vec3,
    max: Vec3,
}

/// The current view's reverse-Z near plane and its clip-transform rounding allowance.
#[derive(Clone, Copy, Debug)]
pub struct DroppedItemCullView {
    near_plane: Vec4,
    camera_position: Vec3,
    rounding: Vec4,
}

impl DroppedItemCullView {
    /// Reads the near half-space `clip.w - clip.z > 0`; invalid projections retain rear faces.
    pub fn new(clip_from_world: Mat4, camera_position: Vec3) -> Option<Self> {
        let near_plane = clip_from_world.row(3) - clip_from_world.row(2);
        let rounding =
            (clip_from_world.row(3).abs() + clip_from_world.row(2).abs()) * ROUNDING_ALLOWANCE;
        let length = near_plane.truncate().length_squared();
        (clip_from_world.is_finite()
            && camera_position.is_finite()
            && rounding.is_finite()
            && length.is_finite()
            && length > 0.0)
            .then_some(Self {
                near_plane,
                camera_position,
                rounding,
            })
    }
}

impl DroppedItemBounds {
    /// Encloses all emitted positions; an empty or non-finite mesh cannot justify culling.
    pub fn from_mesh(mesh: &[ItemMeshVertex]) -> Option<Self> {
        let mut min = Vec3::from_array(mesh.first()?.position);
        let mut max = min;
        for vertex in mesh {
            let position = Vec3::from_array(vertex.position);
            if !position.is_finite() {
                return None;
            }
            min = min.min(position);
            max = max.max(position);
        }
        Some(Self { min, max })
    }

    /// Transforms the box by intervals, padding for rounding in the shader's affine dot products.
    pub fn transformed(self, rows: [[f32; 4]; 3]) -> Option<Self> {
        let magnitude = self.min.abs().max(self.max.abs()).extend(1.0);
        let intervals = rows.map(|row| {
            let row = Vec4::from_array(row);
            let first = row.truncate() * self.min;
            let second = row.truncate() * self.max;
            let error = row.abs().dot(magnitude) * ROUNDING_ALLOWANCE;
            [
                first.min(second).element_sum() + row.w - error,
                first.max(second).element_sum() + row.w + error,
            ]
        });
        if intervals.iter().flatten().any(|value| !value.is_finite()) {
            return None;
        }
        Some(Self {
            min: Vec3::from_array(intervals.map(|range| range[0].next_down())),
            max: Vec3::from_array(intervals.map(|range| range[1].next_up())),
        })
    }

    /// Requires the entire world bound beyond this view's near plane.
    /// Intersecting bounds, including ones containing the camera, retain two-sided drawing.
    pub fn allows_back_face_culling(self, view: &DroppedItemCullView) -> bool {
        if view.camera_position.cmpge(self.min).all() && view.camera_position.cmple(self.max).all()
        {
            return false;
        }
        let nearest = Vec3::from_array(std::array::from_fn(|axis| {
            if view.near_plane[axis] >= 0.0 {
                self.min[axis]
            } else {
                self.max[axis]
            }
        }));
        let distance = view.near_plane.dot(nearest.extend(1.0));
        let error = view
            .rounding
            .dot(self.min.abs().max(self.max.abs()).extend(1.0));
        distance.is_finite() && error.is_finite() && distance > error
    }
}

#[cfg(test)]
mod tests;
