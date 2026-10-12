use super::*;
use bevy::render::renderer::RenderQueue;

const SLOTS: usize = super::super::FRAME.slots;
const MAX_COPIES: usize = super::super::FRAME.max_copies;

/// Supplies validation and synchronous buffer copies without a hardware requirement.
fn setup(slot_bytes: u64) -> (RenderDevice, RenderQueue, Pool, wgpu::Buffer) {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let device = RenderDevice::from(device);
    let queue = RenderQueue::new(queue);
    let pool = Pool::new(
        &device,
        Shape {
            slot_bytes,
            ..super::super::FRAME
        },
    );
    let target = device.wgpu_device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("pooled upload destination"),
        size: 1024,
        usage: wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    (device, queue, pool, target)
}

/// Identifies each retained slot buffer so tests can prove none was replaced.
fn buffers(pool: &Pool) -> Vec<wgpu::Buffer> {
    pool.slots.iter().map(|slot| slot.buffer.clone()).collect()
}

/// Reads one copy's staged source bytes while its slot is still mapped.
fn source(pool: &Pool, copy: &Copy) -> Vec<u8> {
    let mapped = copy.source - copy.source % wgpu::MAP_ALIGNMENT;
    pool.slots[copy.slot]
        .buffer
        .slice(mapped..copy.source + copy.bytes)
        .get_mapped_range()
        .expect("staging slot is mapped")[(copy.source - mapped) as usize..]
        .to_vec()
}

#[test]
fn completed_slots_reuse_buffers_and_empty_batches_create_no_work() {
    let (device, queue, mut pool, target) = setup(64);
    let retained = buffers(&pool);
    for value in 0..SLOTS * 2 {
        assert!(pool.try_stage(&[(&target, 0, &[value as u8; 64])]));
        let mut encoder = device.create_command_encoder(&Default::default());
        pool.encode(&mut encoder);
        queue.submit([encoder.finish()]);
        device.poll(wgpu::PollType::Poll).unwrap();
        assert_eq!(buffers(&pool), retained);
        assert!(
            pool.slots
                .iter()
                .all(|slot| slot.state.load(Ordering::Acquire) == READY)
        );
    }
    assert_eq!(pool.stats.staged_writes, (SLOTS * 2) as u64);
    assert_eq!(pool.stats.staged_bytes, (SLOTS * 2 * 64) as u64);
    assert_eq!(pool.diagnostic_counts(), ([SLOTS, 0, 0, 0], 0));
    let stats = pool.stats;
    assert!(pool.try_stage(&[(&target, 0, &[])]));
    assert!(!pool.has_copies());
    assert_eq!(pool.stats, stats);
}

#[test]
fn incomplete_slots_refuse_work_without_growth_or_polling() {
    let (device, queue, mut pool, target) = setup(64);
    let retained = buffers(&pool);
    let bytes = [1; 64];
    let writes = vec![(&target, 0, bytes.as_slice()); SLOTS];
    assert!(pool.try_stage(&writes));
    let mut encoder = device.create_command_encoder(&Default::default());
    pool.encode(&mut encoder);
    let stats = pool.stats;
    assert!(!pool.try_stage(&[(&target, 256, &[5; 4])]));
    assert_eq!(buffers(&pool), retained);
    assert!(
        pool.slots
            .iter()
            .all(|slot| slot.state.load(Ordering::Acquire) == PENDING)
    );
    assert!(!pool.has_copies());
    assert_eq!(pool.stats, stats);
    assert_eq!(pool.copies.capacity(), MAX_COPIES);
    assert_eq!(pool.diagnostic_counts(), ([0, 0, SLOTS, 0], 0));
    queue.submit([encoder.finish()]);
    device.poll(wgpu::PollType::Poll).unwrap();
}

/// A batch larger than every free slot together is refused whole, even after a partial split.
#[test]
fn oversized_batches_preserve_all_slots_without_partial_writes() {
    let (_, _, mut pool, target) = setup(64);
    let total = SLOTS * 64;
    let large = vec![2; total];
    assert!(!pool.try_stage(&[(&target, 0, &[1; 16]), (&target, 16, &large)]));
    assert!(!pool.has_copies());
    assert!(
        pool.slots
            .iter()
            .all(|slot| { slot.offset == 0 && slot.state.load(Ordering::Acquire) == READY })
    );
    assert_eq!(pool.stats, Stats::default());
}

/// Refusal restores an extended newest copy, so a later batch cannot copy refused bytes.
#[test]
fn refused_batches_restore_the_copy_they_extended() {
    let (_, _, mut pool, target) = setup(64);
    assert!(pool.try_stage(&[(&target, 0, &[1; 16])]));
    let large = vec![2; SLOTS * 64];
    assert!(!pool.try_stage(&[(&target, 16, &large)]));
    assert_eq!(pool.copies.len(), 1);
    assert_eq!(pool.copies[0].bytes, 16);
    assert_eq!(pool.slots[0].offset, 16);
    assert_eq!(pool.diagnostic_counts(), ([SLOTS - 1, 1, 0, 0], 1));
}

#[test]
fn copy_metadata_exhaustion_refuses_the_batch_without_growing() {
    let (_, _, mut pool, target) = setup(4096);
    let bytes = [1; 4];
    let writes = vec![(&target, 0, bytes.as_slice()); MAX_COPIES];
    assert!(pool.try_stage(&writes));
    let stats = pool.stats;
    // Not abutting the newest copy, so it needs a record of its own.
    assert!(!pool.try_stage(&[(&target, 8, &[2; 4])]));
    assert_eq!(pool.copies.len(), MAX_COPIES);
    assert_eq!(pool.copies.capacity(), MAX_COPIES);
    assert_eq!(pool.stats, stats);
}

#[test]
fn failed_mapping_slots_are_quarantined_without_replacement() {
    let (_, _, mut pool, target) = setup(64);
    let retained = buffers(&pool);
    pool.slots[0].state.store(FAILED, Ordering::Release);
    let bytes = [1; 64];
    let writes = vec![(&target, 0, bytes.as_slice()); SLOTS - 1];
    assert!(pool.try_stage(&writes));
    assert!(!pool.try_stage(&[(&target, 192, &[4; 4])]));
    assert!(pool.copies.iter().all(|copy| copy.slot != 0));
    assert_eq!(pool.slots[0].state.load(Ordering::Acquire), FAILED);
    assert_eq!(pool.slots[0].offset, 0);
    assert_eq!(pool.diagnostic_counts(), ([0, SLOTS - 1, 0, 1], SLOTS - 1));
    assert_eq!(buffers(&pool), retained);
}

#[test]
fn four_byte_payload_sizes_keep_the_next_mapped_offset_aligned() {
    let (_, _, mut pool, target) = setup(192);
    assert!(pool.try_stage(&[(&target, 0, &[1; 132]), (&target, 136, &[2; 20])]));
    assert_eq!(pool.copies[0].source, 0);
    assert_eq!(pool.copies[1].source, 136);
    for copy in &pool.copies {
        assert_eq!(copy.source % wgpu::MAP_ALIGNMENT, 0);
        let expected = if copy.offset == 0 { 1 } else { 2 };
        assert!(source(&pool, copy).iter().all(|byte| *byte == expected));
    }
}

/// Abutting records become one copy even when a record ends mid mapping granule.
#[test]
fn abutting_writes_share_one_copy() {
    let (_, _, mut pool, target) = setup(256);
    assert!(pool.try_stage(&[
        (&target, 0, &[1; 12]),
        (&target, 12, &[2; 20]),
        (&target, 32, &[3; 8]),
    ]));
    assert_eq!(pool.copies.len(), 1);
    assert_eq!((pool.copies[0].offset, pool.copies[0].bytes), (0, 40));
    let expected: Vec<u8> = [vec![1; 12], vec![2; 20], vec![3; 8]].concat();
    assert_eq!(source(&pool, &pool.copies[0]), expected);
    assert_eq!(pool.stats.staged_writes, 3);
}

/// A write larger than one slot spans free slots instead of falling back.
#[test]
fn large_writes_split_across_slots_in_destination_order() {
    let (_, _, mut pool, target) = setup(64);
    let bytes: Vec<u8> = (0..160_u8).collect();
    assert!(pool.try_stage(&[(&target, 32, &bytes)]));
    assert_eq!(pool.copies.len(), 3);
    let mut joined = Vec::new();
    let mut offset = 32;
    for copy in &pool.copies {
        assert_eq!(copy.offset, offset);
        offset += copy.bytes;
        joined.extend(source(&pool, copy));
    }
    assert_eq!(joined, bytes);
    assert_eq!(pool.stats.staged_writes, 1);
    assert_eq!(pool.stats.staged_bytes, 160);
}
