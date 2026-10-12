//! Ordered terrain units for ordinary instanced vertex-pulling draws.

use std::ops::Range;

/// Quads covered by one indexed instance, including its unused tail.
pub const QUADS_PER_UNIT: u32 = 64;

/// An allocation's contiguous stream fragment. Bounds are conservative section-local bounds;
/// visibility must also validate the allocation generation before using a retained descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainUnit {
    pub stream_start: u32,
    pub quad_count: u32,
    pub origin_slot: u32,
    pub bounds: [[i32; 3]; 2],
    /// Packed cube face number, or `None` for a stream without directional culling.
    pub facing: Option<u8>,
    pub allocation_generation: u64,
}

impl TerrainUnit {
    /// Splits a valid stream range without crossing an allocation or facing boundary.
    pub fn build(
        range: Range<u32>,
        origin_slot: u32,
        bounds: [[i32; 3]; 2],
        facing: Option<u8>,
        allocation_generation: u64,
    ) -> impl Iterator<Item = Self> {
        (range.start..range.end)
            .step_by(QUADS_PER_UNIT as usize)
            .map(move |stream_start| Self {
                stream_start,
                quad_count: (range.end - stream_start).min(QUADS_PER_UNIT),
                origin_slot,
                bounds,
                facing,
                allocation_generation,
            })
    }

    /// Appends visible descriptors in their original order; callers own allocation visibility.
    pub fn compact(
        units: &[Self],
        mut visible: impl FnMut(&Self) -> bool,
        output: &mut Vec<VisibleTerrainUnit>,
    ) {
        output.extend(
            units
                .iter()
                .filter(|unit| visible(unit))
                .map(|unit| VisibleTerrainUnit {
                    stream_start: unit.stream_start,
                    quad_count: unit.quad_count,
                    origin_slot: unit.origin_slot,
                    padding: 0,
                }),
        );
    }
}

/// Shader-facing part of a CPU-selected unit. Each storage record matches a WGSL vec4.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct VisibleTerrainUnit {
    pub stream_start: u32,
    pub quad_count: u32,
    pub origin_slot: u32,
    pub padding: u32,
}

impl VisibleTerrainUnit {
    /// Resolves a quad only after rejecting padding, before any geometry fetch.
    #[must_use]
    pub fn quad(self, vertex_index: u32) -> Option<(u32, u32)> {
        let quad = vertex_index / 4;
        (quad < self.quad_count).then(|| (self.stream_start + quad, vertex_index % 4))
    }
}

/// Consecutive shards that fit both the buffer binding and the draw's instance count.
pub fn shards(unit_count: usize, max_bytes: u64) -> impl Iterator<Item = Range<usize>> {
    let capacity =
        (max_bytes / size_of::<VisibleTerrainUnit>() as u64).min(u64::from(u32::MAX)) as usize;
    assert!(
        capacity > 0,
        "unit batching requires room for one descriptor"
    );
    (0..unit_count)
        .step_by(capacity)
        .map(move |start| start..start.saturating_add(capacity).min(unit_count))
}

#[cfg(test)]
mod tests;
