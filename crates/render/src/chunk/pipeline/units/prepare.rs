use render_model::terrain_units::{QUADS_PER_UNIT, TerrainUnit};

use super::TerrainUnitBatches;
use crate::chunk::gpu_cull::direct::DirectOcclusion;
use crate::chunk::*;
use crate::upload_staging::TerrainUploadStaging;

/// Compacts the current occlusion results in the original pass order, retaining unchanged lists.
pub(in crate::chunk) fn prepare_units(
    mut batches: ResMut<TerrainUnitBatches>,
    occlusion: Option<Res<DirectOcclusion>>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    cache: Res<PipelineCache>,
    staging: Option<Res<TerrainUploadStaging>>,
) {
    let batches = &mut *batches;
    batches.views.retain(|_, view| view.queued);
    if batches.views.is_empty() {
        return;
    }
    if batches.index_buffer.is_none() {
        batches.index_buffer = Some(device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("terrain unit indices"),
            contents: bytemuck::cast_slice(&super::indices()),
            usage: BufferUsages::INDEX,
        }));
    }
    let mut stats = batches.stats;
    for (&entity, view) in &mut batches.views {
        let facing = view.camera.and_then(meshing::facing_cell);
        for (stream, batch) in view.streams.iter_mut().enumerate() {
            batch.scratch.clear();
            let sections = &view.sources.streams[stream];
            let early = occlusion
                .as_ref()
                .and_then(|state| state.solid_order(entity));
            if stream == 0
                && let Some(order) = early
            {
                stats.phase_items += u64::from(!order.is_empty());
                batch
                    .scratch
                    .extend(order.iter().map(|&(entity, _)| entity));
            } else {
                batch.scratch.extend(
                    sections
                        .iter()
                        .filter(|section| {
                            !occlusion
                                .as_ref()
                                .is_some_and(|state| state.skips(entity, section.entity))
                        })
                        .map(|section| section.entity),
                );
            }
            if view.stale
                || batch.selected != batch.scratch
                || (stream == 0 && view.facing != facing)
            {
                #[cfg(feature = "tracy")]
                let _span = bevy::log::info_span!(
                    "terrain.unit_compact",
                    stream,
                    sections = batch.scratch.len()
                )
                .entered();
                std::mem::swap(&mut batch.selected, &mut batch.scratch);
                batch.visible.clear();
                // The ordinary order is already in `sections`; the early pass supplies its own
                // sorted order. Build this index only when a compaction is needed.
                let by_entity: HashMap<_, _> = sections
                    .iter()
                    .map(|section| (section.entity, section))
                    .collect();
                for entity in &batch.selected {
                    let Some(section) = by_entity.get(entity) else {
                        continue;
                    };
                    let mask = view.camera.map_or(meshing::FaceMask::ALL, |camera| {
                        meshing::sub_chunk_facing_faces(chunk_origin(section.key), camera)
                    });
                    TerrainUnit::compact(
                        &section.units,
                        |unit| {
                            unit.facing
                                .is_none_or(|face| mask.contains(meshing::Face::ALL[face as usize]))
                        },
                        &mut batch.visible,
                    );
                }
                batch.padded_quads = batch
                    .visible
                    .iter()
                    .map(|unit| u64::from(QUADS_PER_UNIT - unit.quad_count))
                    .sum();
                stats.list_bytes_uploaded += super::buffers::upload(
                    &mut batch.pages,
                    &batch.visible,
                    &device,
                    &queue,
                    &cache,
                    staging.as_deref(),
                );
            }
            stats.draws += batch.pages.len() as u64;
            stats.units += batch.visible.len() as u64;
            stats.padded_quads += batch.padded_quads;
        }
        view.stale = false;
        view.facing = facing;
    }
    batches.stats = stats;
    #[cfg(feature = "tracy")]
    let _span = bevy::log::info_span!(
        "terrain.unit_batches",
        phase_items = stats.phase_items,
        draws = stats.draws,
        units = stats.units,
        padded_quads = stats.padded_quads,
        list_bytes_uploaded = stats.list_bytes_uploaded,
    )
    .entered();
}
