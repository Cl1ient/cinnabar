//! Door pose selectors retained independently of each half's artwork.

/// Every facing/open/hinge pose for both texture halves in one class-owned group.
pub const TEMPLATE_COUNT: u32 = 32;
/// Door selectors occupy bits above the two GPU quarter-turn bits.
pub const VARIANT_MASK: u32 = (TEMPLATE_COUNT - 1) << 2;
/// Block-local native door thickness, shared by visual and interaction geometry.
pub const THICKNESS: f32 = 0.1825;

/// Independent server fields; world rendering combines the two positional halves.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DoorPose {
    pub direction: u32,
    pub open: bool,
    pub hinge_right: bool,
    pub upper: bool,
}

impl DoorPose {
    /// Decodes a checked template offset in facing/open/hinge/half order.
    pub const fn from_offset(offset: u32) -> Option<Self> {
        if offset >= TEMPLATE_COUNT {
            return None;
        }
        Some(Self {
            direction: offset & 3,
            open: offset & 4 != 0,
            hinge_right: offset & 8 != 0,
            upper: offset & 16 != 0,
        })
    }

    /// Returns this state's position in its class-owned template group.
    pub const fn offset(self) -> u32 {
        self.direction
            | ((self.open as u32) << 2)
            | ((self.hinge_right as u32) << 3)
            | ((self.upper as u32) << 4)
    }

    /// Returns the blocked south/west/north/east plane after open/hinge rotation.
    pub const fn plane(self) -> u32 {
        if self.open {
            (self.direction + (self.hinge_right as u32) * 2) & 3
        } else {
            (self.direction + 3) & 3
        }
    }

    /// Decodes the carrier's selector bits without admitting GPU transforms.
    pub const fn from_variant(variant: u32) -> Option<Self> {
        if variant & !VARIANT_MASK != 0 {
            return None;
        }
        Self::from_offset(variant >> 2)
    }

    /// Resolves lower direction/open and upper hinge, retaining the queried half.
    pub const fn with_neighbor(self, neighbor: Self) -> Self {
        let (lower, upper) = if self.upper {
            (neighbor, self)
        } else {
            (self, neighbor)
        };
        Self {
            direction: lower.direction,
            open: lower.open,
            hinge_right: upper.hinge_right,
            upper: self.upper,
        }
    }
}
