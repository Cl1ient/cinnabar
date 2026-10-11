use crate::{MAX_STORAGE_COUNT, MutationError};

/// One packet-neutral block change inside a 16x16x16 sub-chunk.
///
/// `layer` maps directly to `UpdateBlock.Layer`. Callers handling an
/// `UpdateSubChunkBlocks` packet use layer zero for `Blocks` and layer one for
/// `Extra`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockUpdate {
    pub x: u8,
    pub y: u8,
    pub z: u8,
    pub layer: u32,
    /// Raw network runtime ID or block-state hash.
    pub runtime_id: u32,
}

/// Which neighbouring sub-chunks a set of changed cells can affect through samples that reach
/// one cell across a face, edge or corner: face culling, ambient occlusion, smooth light,
/// connected shapes and liquid heights. A cell inside the sub-chunk reaches no neighbour; a cell
/// on its faces, edges or corners reaches the neighbours across them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NeighbourReach(u32);

impl NeighbourReach {
    /// Every neighbour, for changes whose cells are unknown.
    pub const ALL: Self = Self((1 << 27) - 1);

    /// The neighbours `updates` reach.
    #[must_use]
    pub fn of_updates(updates: &[BlockUpdate]) -> Self {
        updates.iter().fold(Self(0), |reach, update| {
            Self(reach.0 | Self::of_cell(update.x, update.y, update.z).0)
        })
    }

    /// The neighbours one cell at local `x`, `y`, `z` reaches.
    #[must_use]
    pub fn of_cell(x: u8, y: u8, z: u8) -> Self {
        // Offsets -1, 0 and +1 along each axis the cell's position allows.
        let axis = |c: u8| [c == 0, true, c == 15];
        let (ax, ay, az) = (axis(x), axis(y), axis(z));
        let mut bits = 0;
        for (dx, reaches_x) in ax.into_iter().enumerate() {
            for (dy, reaches_y) in ay.into_iter().enumerate() {
                for (dz, reaches_z) in az.into_iter().enumerate() {
                    if reaches_x && reaches_y && reaches_z {
                        bits |= 1 << (dx * 9 + dy * 3 + dz);
                    }
                }
            }
        }
        Self(bits)
    }

    /// Combines the reach of two sets of changed cells.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Whether the neighbour at sub-chunk `offset` is reached; offsets beyond one sub-chunk
    /// count as reached.
    #[must_use]
    pub const fn reaches(self, offset: [i64; 3]) -> bool {
        let [dx, dy, dz] = offset;
        if dx < -1 || dx > 1 || dy < -1 || dy > 1 || dz < -1 || dz > 1 {
            return true;
        }
        let bit = (dx + 1) * 9 + (dy + 1) * 3 + (dz + 1);
        self.0 & (1 << bit) != 0
    }
}

impl BlockUpdate {
    #[must_use]
    pub const fn new(x: u8, y: u8, z: u8, layer: u32, runtime_id: u32) -> Self {
        Self {
            x,
            y,
            z,
            layer,
            runtime_id,
        }
    }

    pub(crate) fn validate(self) -> Result<(), MutationError> {
        if self.x >= 16 || self.y >= 16 || self.z >= 16 {
            return Err(MutationError::LocalCoordinatesOutOfBounds {
                x: self.x,
                y: self.y,
                z: self.z,
            });
        }
        if self.layer >= MAX_STORAGE_COUNT as u32 {
            return Err(MutationError::LayerOutOfBounds {
                layer: self.layer,
                max: MAX_STORAGE_COUNT,
            });
        }
        Ok(())
    }
}
