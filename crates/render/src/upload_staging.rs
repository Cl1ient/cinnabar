//! Bounded frame uploads share mapped storage until asynchronous GPU completion permits reuse.
//! Every update to a participating buffer must use its pool before the camera graph runs.

#[path = "upload_staging/pool.rs"]
mod pool;
#[cfg(test)]
#[path = "upload_staging/tests.rs"]
mod tests;

use bevy::{
    prelude::*,
    render::{
        RenderApp, RenderStartup,
        renderer::{RenderContext, RenderDevice, RenderQueue},
    },
};
use pool::Pool;
use render_api::PublicationServiceConfig;
use std::sync::{Mutex, MutexGuard};

/// Retained mapped storage and copy records for one family of participating buffers.
#[derive(Debug, Clone, Copy)]
struct Shape {
    name: &'static str,
    slots: usize,
    slot_bytes: u64,
    max_copies: usize,
}

/// Hand, UI viewport and particle writes.
const FRAME: Shape = Shape {
    name: "retained frame upload staging",
    slots: 8,
    slot_bytes: 64 * 1024,
    max_copies: 128,
};

const TERRAIN_SLOT_BYTES: u64 = 256 * 1024;
/// Three 120 Hz frames of the publication target rate, so slots recycle before streaming
/// exhausts them; larger join bursts take the queue path.
const TERRAIN_STAGED_BYTES: u64 =
    PublicationServiceConfig::PHASE2_GATE.target_bytes_per_second * 3 / 120;
/// Terrain arena writes, including coalesced runs that span several slots.
const TERRAIN: Shape = Shape {
    name: "retained terrain upload staging",
    slots: TERRAIN_STAGED_BYTES.div_ceil(TERRAIN_SLOT_BYTES) as usize,
    slot_bytes: TERRAIN_SLOT_BYTES,
    max_copies: 2048,
};

pub(crate) type BufferWrite<'a> = (&'a wgpu::Buffer, u64, &'a [u8]);

/// Cumulative work one pool has staged, refused and encoded.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Stats {
    pub(crate) staged_writes: u64,
    pub(crate) staged_bytes: u64,
    /// Pool misses: writes that took the queue path, each allocating backend staging.
    pub(crate) fallback_writes: u64,
    pub(crate) fallback_bytes: u64,
    pub(crate) overflow_submissions: u64,
    /// Copy commands encoded; abutting writes share one.
    pub(crate) encoded_copies: u64,
}

/// Pools requested by render owners; the graph prefix encodes every installed pool.
#[derive(Resource, Default)]
struct Installed {
    frame: bool,
    terrain: bool,
}

/// Installs the frame pool regardless of how many render owners request it.
pub(crate) fn install(app: &mut App) {
    request(app, |installed| installed.frame = true);
}

/// Installs the terrain arena pool beside any frame pool.
pub(crate) fn install_terrain(app: &mut App) {
    request(app, |installed| installed.terrain = true);
}

/// Records a requested pool and installs the one upload prefix on first use.
fn request(app: &mut App, select: impl FnOnce(&mut Installed)) {
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
        return;
    };
    if let Some(mut installed) = render_app.world_mut().get_resource_mut::<Installed>() {
        select(&mut installed);
        return;
    }
    let mut installed = Installed::default();
    select(&mut installed);
    // Slots return to their pool through the frame's device poll.
    crate::device_poll::install(render_app);
    render_app
        .insert_resource(installed)
        .add_systems(RenderStartup, initialize);
}

/// Creates retained staging only when a camera graph will consume its pending copies.
fn initialize(world: &mut World) {
    let installed = world
        .try_schedule_scope(bevy::render::renderer::RenderGraph, |_, schedule| {
            schedule.add_systems(
                crate::gpu_timing::profiled(upload_buffers, None, "main/BufferUploads")
                    .before(bevy::core_pipeline::schedule::camera_driver)
                    .in_set(bevy::render::renderer::RenderGraphSystems::Render),
            );
        })
        .is_ok();
    if !installed {
        return;
    }
    let requested = world.resource::<Installed>();
    let (frame, terrain) = (requested.frame, requested.terrain);
    let device = world.resource::<RenderDevice>().clone();
    if frame {
        world.insert_resource(BufferUploadStaging(Staging::new(&device, FRAME)));
    }
    if terrain {
        world.insert_resource(TerrainUploadStaging::new(&device));
    }
}

/// Uses retained storage when installed; isolated owners retain their ordinary queue path.
pub(crate) fn write_batch(
    staging: Option<&BufferUploadStaging>,
    device: &RenderDevice,
    queue: &RenderQueue,
    writes: &[BufferWrite<'_>],
) {
    if let Some(staging) = staging {
        staging.write_batch(device, queue, writes);
    } else {
        fallback(queue, writes);
    }
}

/// Queues every nonempty fallback write without waiting for reusable storage.
fn fallback(queue: &RenderQueue, writes: &[BufferWrite<'_>]) {
    for &(target, offset, bytes) in writes.iter().filter(|write| !write.2.is_empty()) {
        queue.write_buffer(target, offset, bytes);
    }
}

/// Small recurring frame writes.
#[derive(Resource, Deref)]
pub(crate) struct BufferUploadStaging(Staging);

/// Terrain arena writes; a migration slice must encode pending copies before it copies.
#[derive(Resource, Deref)]
pub(crate) struct TerrainUploadStaging(Staging);

impl TerrainUploadStaging {
    pub(crate) fn new(device: &RenderDevice) -> Self {
        Self(Staging::new(device, TERRAIN))
    }
}

/// Preparation owns destination writes; the graph consumes copies before any camera reads them.
pub(crate) struct Staging(Mutex<Pool>);

impl Staging {
    fn new(device: &RenderDevice, shape: Shape) -> Self {
        Self(Mutex::new(Pool::new(device, shape)))
    }

    /// Recovers the pool after a panicking writer; its slot states remain authoritative.
    fn lock(&self) -> MutexGuard<'_, Pool> {
        self.0.lock().unwrap_or_else(|error| error.into_inner())
    }

    /// Stages the batch, or queues each write when the pool cannot hold all of it.
    pub(crate) fn write_batch(
        &self,
        device: &RenderDevice,
        queue: &RenderQueue,
        writes: &[BufferWrite<'_>],
    ) {
        self.write_batch_or(device, queue, writes, || fallback(queue, writes));
    }

    /// Runs `fallback` for a refused batch after older overlapping copies are submitted, under
    /// the same lock, so overlapping updates cannot overtake each other.
    pub(crate) fn write_batch_or(
        &self,
        device: &RenderDevice,
        queue: &RenderQueue,
        writes: &[BufferWrite<'_>],
        fallback: impl FnOnce(),
    ) {
        let mut pool = self.lock();
        if pool.try_stage(writes) {
            return;
        }
        let overlap = pool.overlaps(writes);
        let count = writes.iter().filter(|write| !write.2.is_empty()).count() as u64;
        let bytes = writes.iter().map(|write| write.2.len() as u64).sum::<u64>();
        pool.stats.fallback_writes += count;
        pool.stats.fallback_bytes += bytes;
        #[cfg(feature = "tracy")]
        let ([ready, active, pending, failed], queued_copies) = pool.diagnostic_counts();
        #[cfg(feature = "tracy")]
        let _span = bevy::log::info_span!(
            "uploads.fallback",
            pool = pool.name(),
            writes = count,
            bytes,
            overlap,
            ready,
            active,
            pending,
            failed,
            queued_copies,
            total_writes = pool.stats.fallback_writes,
            total_bytes = pool.stats.fallback_bytes,
        )
        .entered();
        if overlap {
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("ordered upload fallback"),
            });
            pool.encode(&mut encoder);
            pool.stats.overflow_submissions += 1;
            queue.submit([encoder.finish()]);
        }
        fallback();
    }

    /// Cumulative staged, fallback and encoded work since creation.
    #[cfg(test)]
    pub(crate) fn stats(&self) -> Stats {
        self.lock().stats
    }

    /// Encodes pending copies ahead of `encoder`'s later commands, which may read their bytes.
    pub(crate) fn encode_pending(&self, encoder: &mut wgpu::CommandEncoder) {
        let mut pool = self.lock();
        if pool.has_copies() {
            pool.encode(encoder);
        }
    }
}

/// Records all pending copies before any camera reads their destination buffers.
fn upload_buffers(
    frame: Option<Res<BufferUploadStaging>>,
    terrain: Option<Res<TerrainUploadStaging>>,
    mut context: RenderContext,
) {
    let pools = [
        frame.as_deref().map(|staging| &staging.0),
        terrain.as_deref().map(|staging| &staging.0),
    ];
    for staging in pools.into_iter().flatten() {
        let mut pool = staging.lock();
        if pool.has_copies() {
            pool.encode(context.command_encoder());
        }
    }
}
