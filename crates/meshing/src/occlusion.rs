//! Bounded proofs that an opaque voxel slab covers a box's entire silhouette.

/// Maximum plane and cell probes shared by one kind of entity in a frame.
pub const ENTITY_OCCLUSION_PROBES: usize = 4096;
const MAX_PLANES: usize = 64;
const ROUNDING_MARGIN: f64 = 0.0001;
const COORDINATE_LIMIT: f64 = 8_000_000.0;

/// Work remaining in this frame. Exhaustion always draws; no old verdict is reused.
#[derive(Debug)]
pub struct OcclusionBudget {
    remaining: usize,
}

impl Default for OcclusionBudget {
    fn default() -> Self {
        Self {
            remaining: ENTITY_OCCLUSION_PROBES,
        }
    }
}

impl OcclusionBudget {
    /// Creates a smaller budget for deterministic workload tests.
    pub fn new(probes: usize) -> Self {
        Self { remaining: probes }
    }

    /// Plane and cell probes still available in the supplied budget.
    pub fn remaining(&self) -> usize {
        self.remaining
    }

    /// Charges work before it happens; a depleted budget cannot prove occlusion.
    fn charge(&mut self) -> bool {
        if self.remaining == 0 {
            return false;
        }
        self.remaining -= 1;
        true
    }

    /// Hides only if one complete opaque slab separates `eye` from every point of the box.
    /// `opaque` must certify current, drawn full cubes, including their depth clipping.
    /// Gaps, missing terrain, invalid bounds and exhausted work all draw. Unlike corner
    /// ray samples, covering the enclosing projected rectangle also covers its interior.
    pub fn hides_box(
        &mut self,
        eye: [f32; 3],
        low: [f32; 3],
        high: [f32; 3],
        mut opaque: impl FnMut([i32; 3]) -> bool,
    ) -> bool {
        let eye = eye.map(f64::from);
        let low = low.map(f64::from);
        let high = high.map(f64::from);
        if eye
            .iter()
            .chain(&low)
            .chain(&high)
            .any(|v| !v.is_finite() || v.abs() > COORDINATE_LIMIT)
            || (0..3).any(|axis| low[axis] > high[axis])
        {
            return false;
        }
        let distance: [f64; 3] =
            std::array::from_fn(|axis| (low[axis] - eye[axis]).max(eye[axis] - high[axis]));
        let axis = (0..3)
            .max_by(|&a, &b| distance[a].total_cmp(&distance[b]))
            .unwrap();
        if distance[axis] <= 2.0 {
            return false;
        }
        let positive = eye[axis] < low[axis];
        let first = if positive {
            eye[axis].ceil() as i32 + 1
        } else {
            eye[axis].floor() as i32 - 2
        };
        let u = (axis + 1) % 3;
        let v = (axis + 2) % 3;
        for step in 0..MAX_PLANES {
            if !self.charge() {
                return false;
            }
            let cell = first
                + if positive {
                    step as i32
                } else {
                    -(step as i32)
                };
            if (positive && f64::from(cell + 1) >= low[axis])
                || (!positive && f64::from(cell) <= high[axis])
            {
                break;
            }
            let plane = f64::from(cell) + 0.5;
            let mut min = [f64::INFINITY; 2];
            let mut max = [f64::NEG_INFINITY; 2];
            for corner in 0..8 {
                let point: [f64; 3] = std::array::from_fn(|a| {
                    if corner & (1 << a) == 0 {
                        low[a]
                    } else {
                        high[a]
                    }
                });
                let t = (plane - eye[axis]) / (point[axis] - eye[axis]);
                for (i, a) in [u, v].into_iter().enumerate() {
                    let projected = eye[a] + t * (point[a] - eye[a]);
                    min[i] = min[i].min(projected);
                    max[i] = max[i].max(projected);
                }
            }
            let min = min.map(|x| (x - ROUNDING_MARGIN).floor() as i32);
            let max = max.map(|x| (x + ROUNDING_MARGIN).floor() as i32);
            let mut covered = true;
            'cells: for x in min[0]..=max[0] {
                for y in min[1]..=max[1] {
                    if !self.charge() {
                        return false;
                    }
                    let mut position = [0; 3];
                    position[axis] = cell;
                    position[u] = x;
                    position[v] = y;
                    if !opaque(position) {
                        covered = false;
                        break 'cells;
                    }
                }
            }
            if covered {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests;
