//! Retained section descriptors in the exact order used by the per-section commands.

use bevy::render::sync_world::MainEntityHashMap;
use render_model::terrain_units::TerrainUnit;

use crate::chunk::pipeline::batches::DrawableTerrain;
use crate::chunk::*;

/// A section's units for one pipeline, still subject to this frame's occlusion and facing.
pub(super) struct SectionUnits {
    pub(super) entity: Entity,
    pub(super) key: SubChunkKey,
    pub(super) units: Vec<TerrainUnit>,
}

/// The ordinary pass keeps Bevy's bin insertion order and each bin's entity hash order.
#[derive(Default)]
pub(in crate::chunk) struct UnitSources {
    pub(super) streams: [Vec<SectionUnits>; 4],
    pub(in crate::chunk) order: Vec<usize>,
}

impl UnitSources {
    /// Recreates fresh NonMesh bins only when the retained terrain tracker invalidates a list.
    pub(in crate::chunk) fn build(
        drawable: &[DrawableTerrain],
        allocations: &Query<&GpuChunkAllocation>,
    ) -> Self {
        #[cfg(feature = "tracy")]
        let _span =
            bevy::log::info_span!("terrain.unit_descriptors", sections = drawable.len()).entered();
        let mut bins: [MainEntityHashMap<Entity>; 4] = default();
        let mut result = Self::default();
        for terrain in drawable {
            for (stream, enabled) in [
                terrain.solid,
                terrain.cutout,
                terrain.model,
                terrain.depth_liquid,
            ]
            .into_iter()
            .enumerate()
            {
                if enabled {
                    if bins[stream].is_empty() {
                        result.order.push(stream);
                    }
                    bins[stream].insert(terrain.main, terrain.entity);
                }
            }
        }
        for (stream, bin) in bins.iter().enumerate() {
            result.streams[stream] = bin
                .values()
                .filter_map(|&entity| {
                    let allocation = allocations.get(entity).ok()?;
                    Some(SectionUnits {
                        entity,
                        key: allocation.key,
                        units: allocation_units(allocation, stream),
                    })
                })
                .collect();
        }
        result
    }
}

/// Splits validated commands without crossing section, stream or directional-run boundaries.
pub(super) fn allocation_units(allocation: &GpuChunkAllocation, stream: usize) -> Vec<TerrainUnit> {
    let bounds = if stream == 2 {
        bounds::MODEL_BOUNDS
    } else {
        bounds::FULL_BOUNDS
    };
    let build = |range, facing| {
        TerrainUnit::build(
            range,
            allocation.metadata_index,
            bounds,
            facing,
            allocation.generation,
        )
    };
    if stream == 0 {
        let Some((cube, layout, _)) = cube_draw_base(allocation) else {
            return Vec::new();
        };
        return CubeQuadLayout::SOLID_FACE_ORDER
            .into_iter()
            .flat_map(|face| {
                let range = layout.solid_range(face);
                build(
                    cube.start + range.start..cube.start + range.end,
                    Some(face as u8),
                )
            })
            .collect();
    }
    let command = match stream {
        1 => cutout_indirect_command(allocation),
        2 => model_direct_draw_command(allocation),
        3 => depth_liquid_direct_draw_command(allocation),
        _ => unreachable!("opaque terrain has four streams"),
    };
    command
        .into_iter()
        .flat_map(|command| {
            build(
                command.first_instance..command.first_instance + command.instance_count,
                None,
            )
        })
        .collect()
}
