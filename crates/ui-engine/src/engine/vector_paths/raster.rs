//! Admitted half-pixel grids with sparse visible-cell vertex storage.

use std::sync::Arc;

use ui::{UiBlendMode, UiMesh, UiMeshBatch, UiMeshVertex};

use super::{Key, MAX_COVERAGE_WORK, MAX_GRID_CELLS, MAX_MESH_INDICES, coverage::Coverage};

pub(super) struct Domain {
    min: [f32; 2],
    extent: [f32; 2],
    cells: [usize; 2],
}

impl Domain {
    /// Rejects excessive resolution or sample/segment work without silently coarsening the grid.
    pub(super) fn new(coverage: &Coverage) -> Option<Self> {
        let (min, max) = coverage.raster_bounds()?;
        let extent = [max[0] - min[0], max[1] - min[1]];
        if min
            .into_iter()
            .chain(extent)
            .any(|value| !value.is_finite())
            || extent.iter().any(|value| *value <= 0.0)
        {
            return None;
        }
        let requested = extent.map(|value| (value / coverage.pixel * 2.0).ceil());
        if requested
            .iter()
            .any(|value| !value.is_finite() || *value > MAX_GRID_CELLS as f32)
        {
            return None;
        }
        let cells = requested.map(|value| value.max(1.0) as usize);
        let work = (cells[0] + 1)
            .checked_mul(cells[1] + 1)?
            .checked_mul(coverage.segment_count())?;
        if work > MAX_COVERAGE_WORK {
            return None;
        }
        Some(Self { min, extent, cells })
    }

    /// The exact sample position; the chosen grid never spaces samples over half a physical pixel.
    fn point(&self, x: usize, y: usize) -> [f32; 2] {
        [
            self.min[0] + x as f32 / self.cells[0] as f32 * self.extent[0],
            self.min[1] + y as f32 / self.cells[1] as f32 * self.extent[1],
        ]
    }
}

/// Samples admitted coverage and stores only vertices referenced by visible cells.
pub(super) fn mesh(key: &Key) -> Option<UiMesh> {
    let size = key.size.map(f32::from_bits);
    let coverage = Coverage::new(key)?;
    let domain = Domain::new(&coverage)?;
    let [width, height] = domain.cells;
    let count = (width + 1) * (height + 1);
    let mut samples = Vec::with_capacity(count);
    for y in 0..=height {
        for x in 0..=width {
            samples.push((f32::from(key.color[3]) * coverage.at(domain.point(x, y))).round() as u8);
        }
    }
    let mut packed = vec![u32::MAX; count];
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let a = y * (width + 1) + x;
            let corners = [a, a + 1, a + width + 1, a + width + 2];
            if corners.iter().all(|&index| samples[index] == 0) {
                continue;
            }
            if indices.len() + 6 > MAX_MESH_INDICES {
                return None;
            }
            let mut referenced = [0; 4];
            for (corner, index) in corners.into_iter().enumerate() {
                if packed[index] == u32::MAX {
                    let point = domain.point(index % (width + 1), index / (width + 1));
                    let mut color = key.color;
                    color[3] = samples[index];
                    packed[index] = vertices.len() as u32;
                    vertices.push(UiMeshVertex {
                        position: [point[0] / size[0], point[1] / size[1]],
                        clip_z: 0.0,
                        clip_w: 1.0,
                        uv: [0.0; 2],
                        color,
                        model_light: 1.0,
                        overlay_color: [0.0; 4],
                        style_flags: 0,
                        alpha_test: false,
                    });
                }
                referenced[corner] = packed[index];
            }
            indices.extend([
                referenced[0],
                referenced[1],
                referenced[3],
                referenced[0],
                referenced[3],
                referenced[2],
            ]);
        }
    }
    if indices.is_empty() {
        return None;
    }
    let end = indices.len() as u32;
    UiMesh::new(
        vertices.into(),
        indices.into(),
        Arc::from([UiMeshBatch {
            texture_page: key.page,
            index_range: 0..end,
            blend: UiBlendMode::Alpha,
            depth_test: false,
            depth_write: false,
            alpha_cutoff: None,
        }]),
    )
    .ok()
}
