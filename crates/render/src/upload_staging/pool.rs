use super::{BufferWrite, Shape, Stats};
use bevy::render::renderer::RenderDevice;
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

#[cfg(test)]
#[path = "pool_tests.rs"]
mod tests;

const READY: u8 = 0;
const ACTIVE: u8 = 1;
const PENDING: u8 = 2;
const FAILED: u8 = 3;

struct Slot {
    buffer: wgpu::Buffer,
    state: Arc<AtomicU8>,
    offset: u64,
}

struct Copy {
    slot: usize,
    source: u64,
    target: wgpu::Buffer,
    offset: u64,
    bytes: u64,
}

/// Where one staged piece lands and whether it extends the newest copy.
struct Piece {
    slot: usize,
    start: u64,
    bytes: u64,
    extends: bool,
}

pub(super) struct Pool {
    shape: Shape,
    slots: Box<[Slot]>,
    copies: Vec<Copy>,
    /// Slot offsets and states before the batch being admitted, for an atomic refusal.
    admitted: Vec<(u64, u8)>,
    pub(super) stats: Stats,
}

impl Pool {
    /// Allocates fixed mapped slots and copy metadata once, before recurring preparation.
    pub(super) fn new(device: &RenderDevice, shape: Shape) -> Self {
        Self {
            slots: (0..shape.slots)
                .map(|_| Slot {
                    buffer: device.wgpu_device().create_buffer(&wgpu::BufferDescriptor {
                        label: Some(shape.name),
                        size: shape.slot_bytes,
                        usage: wgpu::BufferUsages::MAP_WRITE | wgpu::BufferUsages::COPY_SRC,
                        mapped_at_creation: true,
                    }),
                    state: Arc::new(AtomicU8::new(READY)),
                    offset: 0,
                })
                .collect(),
            copies: Vec::with_capacity(shape.max_copies),
            admitted: Vec::with_capacity(shape.slots),
            stats: Stats::default(),
            shape,
        }
    }

    /// Names the pool in trace zones.
    #[cfg(feature = "tracy")]
    pub(super) fn name(&self) -> &'static str {
        self.shape.name
    }

    /// Admits a complete batch without allocating GPU storage, polling, or partially staging it.
    pub(super) fn try_stage(&mut self, writes: &[BufferWrite<'_>]) -> bool {
        self.admitted.clear();
        self.admitted.extend(
            self.slots
                .iter()
                .map(|slot| (slot.offset, slot.state.load(Ordering::Acquire))),
        );
        let copies = self.copies.len();
        let newest_bytes = self.copies.last().map(|copy| copy.bytes);
        let stats = self.stats;
        for &(target, offset, bytes) in writes.iter().filter(|write| !write.2.is_empty()) {
            if !self.stage(target, offset, bytes) {
                self.refuse(copies, newest_bytes, stats);
                return false;
            }
        }
        true
    }

    /// Restores the admission snapshot; only mapped slots can have changed since it was taken.
    fn refuse(&mut self, copies: usize, newest_bytes: Option<u64>, stats: Stats) {
        for (slot, &(offset, state)) in self.slots.iter_mut().zip(&self.admitted) {
            if matches!(state, READY | ACTIVE) {
                slot.offset = offset;
                slot.state.store(state, Ordering::Release);
            }
        }
        self.copies.truncate(copies);
        if let (Some(copy), Some(bytes)) = (self.copies.last_mut(), newest_bytes) {
            copy.bytes = bytes;
        }
        self.stats = stats;
    }

    /// Copies `bytes` into as few pieces as fit, extending the newest copy when it abuts.
    fn stage(&mut self, target: &wgpu::Buffer, mut offset: u64, mut bytes: &[u8]) -> bool {
        while !bytes.is_empty() {
            let Some(piece) = self.reserve(target, offset, bytes.len() as u64) else {
                return false;
            };
            if !piece.extends && self.copies.len() == self.shape.max_copies {
                return false;
            }
            let (head, rest) = bytes.split_at(piece.bytes as usize);
            let slot = &mut self.slots[piece.slot];
            slot.state.store(ACTIVE, Ordering::Release);
            slot.offset = piece.start + piece.bytes;
            // An extending piece may start mid-word of the mapping granularity.
            let mapped = piece.start - piece.start % wgpu::MAP_ALIGNMENT;
            slot.buffer
                .slice(mapped..slot.offset)
                .get_mapped_range_mut()
                .expect("staging range is mapped and unaliased")
                .slice((piece.start - mapped) as usize..)
                .copy_from_slice(head);
            if piece.extends {
                let newest = self
                    .copies
                    .last_mut()
                    .expect("an extending piece follows a copy");
                newest.bytes += piece.bytes;
            } else {
                self.copies.push(Copy {
                    slot: piece.slot,
                    source: piece.start,
                    target: target.clone(),
                    offset,
                    bytes: piece.bytes,
                });
            }
            self.stats.staged_bytes += piece.bytes;
            offset += piece.bytes;
            bytes = rest;
        }
        self.stats.staged_writes += 1;
        true
    }

    /// Reports pending command work without creating an encoder for an empty frame.
    pub(super) fn has_copies(&self) -> bool {
        !self.copies.is_empty()
    }

    /// Samples ready, active, pending and failed slot counts alongside queued copies.
    #[cfg(any(test, feature = "tracy"))]
    pub(super) fn diagnostic_counts(&self) -> ([usize; 4], usize) {
        let mut counts = [0; 4];
        for slot in self.slots.iter() {
            counts[usize::from(slot.state.load(Ordering::Acquire))] += 1;
        }
        (counts, self.copies.len())
    }

    /// Uses only mapped slots; incomplete and failed mappings cannot be reused.
    fn cursor(slot: &Slot) -> Option<u64> {
        match slot.state.load(Ordering::Acquire) {
            READY => Some(0),
            ACTIVE => Some(slot.offset),
            _ => None,
        }
    }

    /// Continues the newest copy in place, else packs a whole write at WebGPU's mapping
    /// alignment preferring active slots, else splits it into the slot with the most room.
    fn reserve(&self, target: &wgpu::Buffer, offset: u64, bytes: u64) -> Option<Piece> {
        let slot_bytes = self.shape.slot_bytes;
        if let Some(newest) = self.copies.last()
            && newest.target == *target
            && newest.offset + newest.bytes == offset
            && self.slots[newest.slot].offset == newest.source + newest.bytes
        {
            let start = newest.source + newest.bytes;
            let room = slot_bytes.saturating_sub(start) & !(wgpu::COPY_BUFFER_ALIGNMENT - 1);
            if room > 0 {
                return Some(Piece {
                    slot: newest.slot,
                    start,
                    bytes: room.min(bytes),
                    extends: true,
                });
            }
        }
        let aligned = |slot: &Slot| {
            Self::cursor(slot).map(|cursor| {
                let occupied = cursor > 0;
                (
                    occupied,
                    cursor.div_ceil(wgpu::MAP_ALIGNMENT) * wgpu::MAP_ALIGNMENT,
                )
            })
        };
        for occupied in [true, false] {
            for (index, slot) in self.slots.iter().enumerate() {
                if let Some((used, start)) = aligned(slot)
                    && used == occupied
                    && start
                        .checked_add(bytes)
                        .is_some_and(|end| end <= slot_bytes)
                {
                    return Some(Piece {
                        slot: index,
                        start,
                        bytes,
                        extends: false,
                    });
                }
            }
        }
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| {
                let (_, start) = aligned(slot)?;
                let room = slot_bytes.checked_sub(start)? & !(wgpu::COPY_BUFFER_ALIGNMENT - 1);
                (room > 0).then_some((index, start, room))
            })
            .max_by_key(|&(index, _, room)| (room, std::cmp::Reverse(index)))
            .map(|(slot, start, room)| Piece {
                slot,
                start,
                bytes: room.min(bytes),
                extends: false,
            })
    }

    /// Finds writes that would otherwise execute before an older staged update to the same bytes.
    pub(super) fn overlaps(&self, writes: &[BufferWrite<'_>]) -> bool {
        self.copies.iter().any(|copy| {
            writes
                .iter()
                .filter(|write| !write.2.is_empty())
                .any(|(buffer, offset, bytes)| {
                    **buffer == copy.target
                        && *offset < copy.offset + copy.bytes
                        && copy.offset < offset.saturating_add(bytes.len() as u64)
                })
        })
    }

    /// Encodes copies in issue order and remaps each used slot only after GPU completion.
    pub(super) fn encode(&mut self, encoder: &mut wgpu::CommandEncoder) {
        #[cfg(feature = "tracy")]
        let _span = bevy::log::info_span!(
            "uploads.encode",
            pool = self.shape.name,
            copies = self.copies.len(),
            bytes = self.copies.iter().map(|copy| copy.bytes).sum::<u64>(),
            total_fallback_writes = self.stats.fallback_writes,
            total_fallback_bytes = self.stats.fallback_bytes,
        )
        .entered();
        for slot in self.slots.iter() {
            if slot.state.load(Ordering::Acquire) == ACTIVE {
                slot.buffer.unmap();
                slot.state.store(PENDING, Ordering::Release);
            }
        }
        self.stats.encoded_copies += self.copies.len() as u64;
        for copy in self.copies.drain(..) {
            encoder.copy_buffer_to_buffer(
                &self.slots[copy.slot].buffer,
                copy.source,
                &copy.target,
                copy.offset,
                copy.bytes,
            );
        }
        for slot in self.slots.iter_mut() {
            if slot.offset == 0 || slot.state.load(Ordering::Acquire) != PENDING {
                continue;
            }
            let state = Arc::clone(&slot.state);
            encoder.map_buffer_on_submit(&slot.buffer, wgpu::MapMode::Write, .., move |result| {
                state.store(
                    if result.is_ok() { READY } else { FAILED },
                    Ordering::Release,
                );
            });
            slot.offset = 0;
        }
    }
}
