use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Larger than all eight 64-byte slots together, so it always takes the fallback path.
const OVERSIZED: usize = 600;

/// Uses NOOP's real buffer copies to check final bytes without driver scheduling assumptions.
fn setup() -> (RenderDevice, RenderQueue, Staging, wgpu::Buffer) {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let device = RenderDevice::from(device);
    let queue = RenderQueue::new(queue);
    let staging = Staging::new(
        &device,
        Shape {
            slot_bytes: 64,
            ..FRAME
        },
    );
    let target = device.wgpu_device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("upload ordering destination"),
        size: 1024,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    (device, queue, staging, target)
}

/// Submits the upload prefix, then reads actual destination bytes after a nonblocking NOOP poll.
fn finish(
    device: &RenderDevice,
    queue: &RenderQueue,
    staging: &Staging,
    target: &wgpu::Buffer,
) -> Vec<u8> {
    let mut encoder = device.create_command_encoder(&Default::default());
    staging.encode_pending(&mut encoder);
    let mapped = Arc::new(AtomicBool::new(false));
    let complete = mapped.clone();
    encoder.map_buffer_on_submit(target, wgpu::MapMode::Read, .., move |result| {
        result.unwrap();
        complete.store(true, Ordering::Release);
    });
    queue.submit([encoder.finish()]);
    device.poll(wgpu::PollType::Poll).unwrap();
    assert!(mapped.load(Ordering::Acquire));
    let bytes = target
        .slice(..)
        .get_mapped_range()
        .expect("readback buffer is mapped")
        .to_vec();
    target.unmap();
    bytes
}

#[test]
fn overlapping_fallback_preserves_pooled_fallback_and_later_pooled_bytes() {
    let (device, queue, staging, target) = setup();
    let oversized = [2; OVERSIZED];
    staging.write_batch(&device, &queue, &[(&target, 0, &[1; 16])]);
    staging.write_batch(&device, &queue, &[(&target, 8, &oversized)]);
    staging.write_batch(&device, &queue, &[(&target, 16, &[3; 4])]);
    let bytes = finish(&device, &queue, &staging, &target);
    assert_eq!(&bytes[..8], &[1; 8]);
    assert_eq!(&bytes[8..16], &[2; 8]);
    assert_eq!(&bytes[16..20], &[3; 4]);
    assert!(bytes[20..8 + OVERSIZED].iter().all(|byte| *byte == 2));
    assert!(bytes[8 + OVERSIZED..].iter().all(|byte| *byte == 0));
    let stats = staging.lock().stats;
    assert_eq!(stats.staged_writes, 2);
    assert_eq!(stats.fallback_writes, 1);
    assert_eq!(stats.fallback_bytes, OVERSIZED as u64);
    assert_eq!(stats.overflow_submissions, 1);
}

#[test]
fn empty_overlaps_do_not_flush_disjoint_fallback_work() {
    let (device, queue, staging, target) = setup();
    let oversized = [2; OVERSIZED];
    staging.write_batch(&device, &queue, &[(&target, 0, &[1; 16])]);
    staging.write_batch(
        &device,
        &queue,
        &[(&target, 8, &[]), (&target, 32, &oversized)],
    );
    assert_eq!(staging.lock().stats.overflow_submissions, 0);
    let bytes = finish(&device, &queue, &staging, &target);
    assert_eq!(&bytes[..16], &[1; 16]);
    assert!(bytes[32..32 + OVERSIZED].iter().all(|byte| *byte == 2));
    assert_eq!(staging.lock().stats.fallback_writes, 1);
}

#[test]
fn concurrent_fallback_keeps_older_flushes_under_the_same_lock() {
    let (device, queue, staging, target) = setup();
    let staging = Arc::new(staging);
    let oversized = [2; OVERSIZED];
    staging.write_batch(&device, &queue, &[(&target, 0, &[1; 16])]);
    let callback_staging = Arc::clone(&staging);
    let callback_ran = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&callback_ran);
    queue.on_submitted_work_done(move || {
        assert!(matches!(
            callback_staging.0.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        ));
        observed.store(true, Ordering::Release);
    });
    staging.write_batch(&device, &queue, &[(&target, 8, &oversized)]);
    assert!(callback_ran.load(Ordering::Acquire));
    let bytes = finish(&device, &queue, &staging, &target);
    assert_eq!(&bytes[..8], &[1; 8]);
    assert!(bytes[8..8 + OVERSIZED].iter().all(|byte| *byte == 2));
}

/// A refused batch runs the owner's own fallback once, after older overlapping copies.
#[test]
fn custom_fallback_runs_after_older_overlapping_copies() {
    let (device, queue, staging, target) = setup();
    let oversized = [2; OVERSIZED];
    staging.write_batch(&device, &queue, &[(&target, 0, &[1; 16])]);
    let mut fallbacks = 0;
    staging.write_batch_or(&device, &queue, &[(&target, 8, &oversized)], || {
        fallbacks += 1;
        queue.write_buffer(&target, 8, &oversized);
    });
    assert_eq!(fallbacks, 1);
    let bytes = finish(&device, &queue, &staging, &target);
    assert_eq!(&bytes[..8], &[1; 8]);
    assert!(bytes[8..8 + OVERSIZED].iter().all(|byte| *byte == 2));
    assert_eq!(staging.lock().stats.overflow_submissions, 1);
}
