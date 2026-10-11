use serde::{Deserialize, Serialize};

/// Armour facts admitted for an item; unresolved data is never a zero-value claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case", tag = "support", deny_unknown_fields)]
pub enum ItemArmour {
    Unresolved,
    NotArmour,
    Wearable {
        slot: ItemArmourSlot,
        protection: u32,
    },
}

/// Player armour slots, independent of numeric inventory container slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemArmourSlot {
    Head,
    Chest,
    Legs,
    Feet,
}

impl ItemArmour {
    /// Returns admitted protection, with absence of armour distinct from unresolved data.
    pub const fn protection(self) -> Option<u32> {
        match self {
            Self::Unresolved => None,
            Self::NotArmour => Some(0),
            Self::Wearable { protection, .. } => Some(protection),
        }
    }
}
